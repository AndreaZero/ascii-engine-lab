"""Serialized R11 acceptance runs. Build first; never overlap builds/encoders.

Baseline requires local R10 binaries in evidence/r11/baseline. Runtime/motion
use release binaries. Raw logs/media remain local; summary records comparisons.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import time

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "evidence/r11"
MODES = ("first", "third", "overhead")
BACKENDS = ("vulkan", "dx12")
V1 = ROOT / "clients/material-room/assets/material-room.json"
V2 = ROOT / "clients/material-room/assets/material-room-v2.json"
SUFFIX = ".exe" if __import__("os").name == "nt" else ""


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(name, binary, arguments, records, phase):
    command = [str(binary), *map(str, arguments)]
    start = time.perf_counter()
    log = OUT / f"{phase}-{name}.log"
    with log.open("w", encoding="utf-8") as stream:
        result = subprocess.run(command, cwd=ROOT, stdout=stream,
                                stderr=subprocess.STDOUT, timeout=600,
                                creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
    records.append({"case": name, "command": command,
                    "exit_code": result.returncode,
                    "wall_seconds": time.perf_counter() - start,
                    "executable_sha256": digest(binary)})
    (OUT / f"runs-{phase}.json").write_text(json.dumps(records, indent=2), encoding="utf-8")
    if result.returncode:
        raise RuntimeError(f"{name} failed: {log.read_text(encoding='utf-8')[-3000:]}")
    print(name, flush=True)


def room_args(scene, destination, backend="vulkan", frames=2, profile="balanced", workers=8):
    return ["--headless", "--scene", scene, "--camera", "all", "--frames", frames,
            "--profile", profile, "--workers", workers, "--backend", backend,
            "--output", destination]


def compare(left, right):
    a = {str(p.relative_to(left)): digest(p) for p in left.rglob("*.png")}
    b = {str(p.relative_to(right)): digest(p) for p in right.rglob("*.png")}
    if not a or a != b:
        raise AssertionError(f"PNG mismatch/missing: {left} versus {right}")
    return len(a)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--phase", required=True,
                        choices=["baseline", "runtime", "measure", "motion", "summary"])
    args = parser.parse_args()
    phase = args.phase
    OUT.mkdir(parents=True, exist_ok=True)
    records = []
    base = OUT / "baseline" if phase == "baseline" else ROOT / "target/release"
    room = base / f"ascii-material-room{SUFFIX}"
    lab = base / f"ascii-engine-lab{SUFFIX}"
    environment = (base if phase == "baseline" else base / "examples") / f"environment_study{SUFFIX}"
    destination = OUT / ("baseline" if phase == "baseline" else "runtime")
    if phase in ("baseline", "runtime"):
        for backend in BACKENDS:
            run(f"room-{backend}", room, room_args(V1, destination / f"room-{backend}", backend), records, phase)
            run(f"lab-{backend}", lab,
                ["--benchmark", "--frames", 1, "--profile", "balanced", "--workers", 8,
                 "--backend", backend, "--output", destination / f"lab-{backend}"], records, phase)
            run(f"environment-{backend}", environment,
                ["--variant", "pair", "--camera", "all", "--path", "static", "--frames", 2,
                 "--backend", backend, "--out", destination / f"environment-{backend}"], records, phase)
            if phase == "runtime":
                run(f"v2-{backend}", room, room_args(V2, destination / f"v2-{backend}", backend), records, phase)
        if phase == "runtime":
            for mode in MODES:
                run(f"window-v2-{mode}", room, ["--scene", V2, "--camera", mode,
                    "--seconds", 2, "--backend", "vulkan", "--workers", 8], records, phase)
            run("window-lab", lab, ["--seconds", 2, "--backend", "vulkan", "--workers", 8], records, phase)
            # Change scene/material/camera data while using the same executable.
            changed = json.loads(V2.read_text(encoding="utf-8"))
            changed["name"] = "R11 authored data edit"
            changed["materials"][0]["pigment"] = [0.15, 0.48, 0.24]
            changed["materials"][0]["surface"]["roughness"] = 0.25
            changed["objects"][-1]["position"][0] -= 0.5
            for camera in changed["cameras"].values():
                camera["eye"][0] += 0.3
            path = OUT / "changed-v2.json"
            path.write_text(json.dumps(changed, indent=2), encoding="utf-8")
            run("v2-edited", room, room_args(path, destination / "v2-edited"), records, phase)
    elif phase == "measure":
        # Fixed content/density; alternating order, no CPU speed or FPS claim.
        for repeat in range(1, 4):
            for workers in ([8, 1] if repeat == 2 else [1, 8]):
                run(f"r{repeat}-w{workers}", room,
                    room_args(V2, OUT / f"measure/r{repeat}-w{workers}", frames=120,
                              profile="detail", workers=workers), records, phase)
    elif phase == "motion":
        run("v2", room,
            room_args(V2, OUT / "motion", frames=180, profile="detail")
            + ["--sequence", 180, "--orbit-speed", 0.025], records, phase)
    else:
        legacy = {}
        for backend in BACKENDS:
            for name in ("room", "lab", "environment"):
                case = f"{name}-{backend}"
                legacy[case] = compare(OUT / "baseline" / case, OUT / "runtime" / case)
        parity = compare(OUT / "runtime/v2-vulkan", OUT / "runtime/v2-dx12")
        edited = []
        for mode in MODES:
            a = OUT / "runtime/v2-vulkan" / mode / "frame-0000.png"
            b = OUT / "runtime/v2-edited" / mode / "frame-0000.png"
            assert digest(a) != digest(b), f"data edit invisible in {mode}"
            edited.append(mode)
        measurements = []
        for path in sorted((OUT / "measure").glob("*/run.json")):
            manifest = json.loads(path.read_text(encoding="utf-8"))
            for case in manifest["runs"]:
                assert case["static_changed_cells"] == 0
                measurements.append({"run": path.parent.name, "camera": case["camera"],
                    "workers": manifest["workers"], "grid": manifest["grid"],
                    "cpu_ms": case["cpu_snapshot_and_compose_ms"],
                    "submit_wait_ms": case["submit_wait_ms"]})
        assert len(measurements) == 18, "expected 3 repeats x 2 worker counts x 3 views"
        motion = json.loads((OUT / "motion/run.json").read_text(encoding="utf-8"))
        assert motion["grid"] == [240, 135] and motion["output"] == [1920, 1080]
        coverage = []
        for case in motion["runs"]:
            assert case["static_changed_cells"] == 0 and case["frame_count"] == 180
            assert len(list((OUT / "motion" / case["camera"]).glob("frame-*.png"))) == 180
            coverage.append({"camera": case["camera"], "audits": case["coverage_audits"]})
        runtime_records = json.loads((OUT / "runs-runtime.json").read_text(encoding="utf-8"))
        room_hashes = {record["executable_sha256"] for record in runtime_records
                       if record["case"].startswith("v2-")}
        assert len(room_hashes) == 1, "data edits must use the same binary"
        result = {"legacy_png_equal": legacy, "v2_backend_png_equal": parity,
                  "data_edit_changed_views": edited, "measurements": measurements,
                  "motion_coverage": coverage,
                  "limits": ["Same physical host; offscreen and capture scheduling apply",
                             "Submission/wait is not GPU timestamp time or display FPS",
                             "Technical samples; no final artistic quality claim"]}
        (OUT / "summary.json").write_text(json.dumps(result, indent=2), encoding="utf-8")
        print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
