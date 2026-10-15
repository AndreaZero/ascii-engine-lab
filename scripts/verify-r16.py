"""R16 serialized evidence runner. Build first; no concurrent builds/benchmarks.

Generated media, logs and frozen executables remain local in evidence/r16.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "evidence/r16"
BIN = ROOT / "target/release"
EXT = ".exe" if os.name == "nt" else ""
MODES = ("first", "third", "overhead")
POLICIES = ("center", "conservative9")


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write(path, data):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")


def compare(a, b):
    left = {str(p.relative_to(a)): digest(p) for p in a.rglob("*.png")}
    right = {str(p.relative_to(b)): digest(p) for p in b.rglob("*.png")}
    assert left and left == right, (a, b)
    return len(left)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--phase", required=True, choices=("legacy", "static", "motion", "cpu", "windows", "summary"))
    phase = parser.parse_args().phase
    records = []
    OUT.mkdir(parents=True, exist_ok=True)

    def run(name, binary, arguments):
        command = [str(binary), *map(str, arguments)]
        start = time.perf_counter()
        result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True,
                                encoding="utf-8", timeout=900,
                                creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
        (OUT / f"{phase}-{name}.log").write_text(result.stdout + result.stderr, encoding="utf-8")
        records.append({"name": name, "command": command, "exit_code": result.returncode,
                        "seconds": time.perf_counter() - start, "binary_sha256": digest(binary)})
        write(OUT / f"runs-{phase}.json", records)
        assert result.returncode == 0, result.stdout + result.stderr
        print(name, flush=True)

    study = BIN / "examples" / ("visual_capacity" + EXT)

    def visual(name, figure, surface="structured", distance="middle", path="static", frames=1,
               policy="center", backend="vulkan", extra=()):
        run(name, study, ["--figure", figure, "--surface", surface, "--distance", distance,
                         "--path", path, "--frames", frames, "--sampling", policy, "--backend", backend,
                         "--workers", 8, "--out", OUT / phase / name, *extra])

    if phase == "legacy":
        for revision, folder in (("r15", OUT / "baseline"), ("r16", BIN)):
            for scene in ("material-room", "material-room-v2", "dynamic-room-v2"):
                for policy in POLICIES:
                    assets = ROOT / "clients/material-room/assets"
                    args = ["--scene", assets / (scene + ".json"), "--frames", 3]
                    if scene == "dynamic-room-v2":
                        args += ["--updates", assets / "dynamic-updates-v1.json", "--time", 3]
                    name = f"{revision}-{scene}-{policy}"
                    run(name, folder / ("ascii-material-room" + EXT), args + [
                        "--headless", "--camera", "all", "--profile", "detail", "--workers", 8,
                        "--backend", "vulkan", "--sampling", policy, "--output", OUT / "legacy" / name])
            run(revision + "-lab", folder / ("ascii-engine-lab" + EXT), [
                "--benchmark", "--frames", 1, "--profile", "balanced", "--workers", 8,
                "--backend", "vulkan", "--output", OUT / "legacy" / (revision + "-lab")])
        count = sum(compare(OUT / "legacy" / f"r15-{s}-{p}", OUT / "legacy" / f"r16-{s}-{p}")
                    for s in ("material-room", "material-room-v2", "dynamic-room-v2") for p in POLICIES)
        count += compare(OUT / "legacy/r15-lab", OUT / "legacy/r16-lab")
        write(OUT / "legacy-summary.json", {"identical_png": count})
        print(f"Legacy identical PNG: {count}")
    elif phase == "static":
        for figure in ("baseline", "warden", "courier"):
            for surface in (("clay",) if figure == "baseline" else ("clay", "structured")):
                for distance in ("near", "middle", "far"):
                    for backend in ("vulkan", "dx12"):
                        visual(f"{figure}-{surface}-{distance}-{backend}", figure, surface, distance, backend=backend)
        for figure in ("warden", "courier"):
            visual(f"{figure}-nine-middle", figure, policy="conservative9")
    elif phase == "motion":
        for figure in ("warden", "courier"):
            for path in ("rotate", "camera", "deform"):
                for backend in ("vulkan", "dx12"):
                    visual(f"{figure}-{path}-{backend}", figure, path=path, frames=60, backend=backend)
    elif phase == "cpu":
        # Alternate ordering each repetition; timings exclude PNG and geometry diagnostics.
        cases = [(f, p) for f in ("baseline", "warden", "courier") for p in POLICIES]
        for repeat in range(3):
            for figure, policy in (cases if repeat % 2 == 0 else reversed(cases)):
                visual(f"r{repeat + 1}-{figure}-{policy}", figure,
                       surface="clay" if figure == "baseline" else "structured",
                       path="deform", frames=120, policy=policy, extra=("--benchmark",))
    elif phase == "windows":
        for figure in ("warden", "courier"):
            for mode in MODES:
                visual(f"{figure}-{mode}", figure, path="deform",
                       extra=("--window", "--camera", mode, "--seconds", 4))
    else:
        pairs = 0
        for phase_name in ("static", "motion"):
            for p in (OUT / phase_name).glob("*-vulkan"):
                pairs += compare(p, p.with_name(p.name.replace("-vulkan", "-dx12")))
        cpu = []
        for p in sorted((OUT / "cpu").glob("*/run.json")):
            d = json.loads(p.read_text(encoding="utf-8"))
            for r in d["runs"]:
                cpu.append({"run": p.parent.name, "figure": d["figure"], "sampling": d["primary_sampling"],
                            "camera": r["camera"], "total": r["cpu_total_ms"], "snapshot": r["snapshot_ms"]})
        windows = []
        for p in sorted((OUT / "windows").glob("*/window-run.json")):
            d = json.loads(p.read_text(encoding="utf-8"))
            assert d["actual_output"] == [1920, 1080] and d["frames"] > 12
            windows.append({"name": p.parent.name, "frames": d["frames"], "configuration_changes": d["configuration_changes"], "compose": d["compose_ms"], "interval": d["frame_interval_ms"]})
        assert len(cpu) == 54 and len(windows) == 6
        for phase_name in ("legacy", "static", "motion", "cpu", "windows"):
            rows = json.loads((OUT / f"runs-{phase_name}.json").read_text())
            assert rows and all(r["exit_code"] == 0 for r in rows)
            assert all(digest(Path(r["command"][0])) == r["binary_sha256"] for r in rows)
        result = {"backend_pairs_identical": pairs, "legacy": json.loads((OUT / "legacy-summary.json").read_text()),
                  "cpu": cpu, "windows": windows, "scope": "One physical host; no aesthetic acceptance or FPS claim."}
        write(OUT / "summary.json", result)
        print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
