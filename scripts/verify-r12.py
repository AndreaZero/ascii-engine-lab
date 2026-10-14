"""Serialized R12 checks. Build first, then run one phase at a time.

Keep preserved R11 binaries in evidence/r12/baseline. Dense audits, counters,
captures and encoding never run concurrently with cost measurements.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "evidence/r12"
MODES = ("first", "third", "overhead")
SAMPLING = ("center", "conservative9")
BACKENDS = ("vulkan", "dx12")
V1 = ROOT / "clients/material-room/assets/material-room.json"
V2 = ROOT / "clients/material-room/assets/material-room-v2.json"
COVERAGE = ROOT / "clients/material-room/assets/coverage-room-v2.json"
SUFFIX = ".exe" if os.name == "nt" else ""


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(name, binary, arguments, records, phase):
    command = [str(binary), *map(str, arguments)]
    log = OUT / f"{phase}-{name}.log"
    start = time.perf_counter()
    with log.open("w", encoding="utf-8") as stream:
        result = subprocess.run(command, cwd=ROOT, stdout=stream,
            stderr=subprocess.STDOUT, timeout=1200,
            creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
    records.append({"case": name, "command": command, "exit_code": result.returncode,
        "wall_seconds": time.perf_counter()-start, "executable_sha256": digest(binary)})
    (OUT / f"runs-{phase}.json").write_text(json.dumps(records, indent=2), encoding="utf-8")
    if result.returncode:
        raise RuntimeError(f"{name}: {log.read_text(encoding='utf-8')[-4000:]}")
    print(name, flush=True)


def room(scene, destination, backend="vulkan", frames=2, profile="balanced", workers=8):
    return ["--headless", "--scene", scene, "--camera", "all", "--frames", frames,
        "--profile", profile, "--workers", workers, "--backend", backend, "--output", destination]


def compare(left, right):
    a = {str(p.relative_to(left)): digest(p) for p in left.rglob("*.png")}
    b = {str(p.relative_to(right)): digest(p) for p in right.rglob("*.png")}
    assert a and a == b, f"missing/different PNGs: {left}, {right}"
    return len(a)


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--phase", required=True,
        choices=["baseline", "runtime", "measure", "baseline-cost", "windows", "motion", "summary"])
    phase = p.parse_args().phase
    OUT.mkdir(parents=True, exist_ok=True)
    records = []
    base = OUT / "baseline" if phase == "baseline" else ROOT / "target/release"
    client = base / f"ascii-material-room{SUFFIX}"
    lab = base / f"ascii-engine-lab{SUFFIX}"
    env = (base if phase == "baseline" else base / "examples") / f"environment_study{SUFFIX}"
    # Initial diagnostic captures used zero specular radiance. Preserve them in
    # runtime/ as pilot evidence; final data isolate geometry above the dark cutoff.
    dest = OUT / ("runtime-final" if phase == "runtime" else phase)
    if phase in ("baseline", "runtime"):
        for backend in BACKENDS:
            for name, scene in (("v1", V1), ("v2", V2)):
                run(f"{name}-{backend}", client, room(scene, dest/f"{name}-{backend}", backend), records, phase)
            run(f"lab-{backend}", lab, ["--benchmark", "--frames", 1, "--profile", "balanced",
                "--workers", 8, "--backend", backend, "--output", dest/f"lab-{backend}"], records, phase)
            run(f"environment-{backend}", env, ["--variant", "pair", "--camera", "all",
                "--path", "static", "--frames", 2, "--backend", backend, "--out", dest/f"environment-{backend}"], records, phase)
            if phase == "runtime":
                for sampling in SAMPLING:
                    run(f"coverage-{sampling}-{backend}", client,
                        room(COVERAGE, dest/f"coverage-{sampling}-{backend}", backend, frames=3)
                        + ["--sampling", sampling, "--camera-path", "coverage"], records, phase)
    elif phase == "measure":
        # Preserve the R11 v2 workload. Quality and snapshot clock stay fixed.
        # Three interleaved repeats, 12 warmups then 120 frames/view; CPU-only.
        for repeat in range(1, 4):
            samplings = SAMPLING if repeat != 2 else tuple(reversed(SAMPLING))
            workers_order = [1, 8] if repeat != 2 else [8, 1]
            for sampling in samplings:
                for workers in workers_order:
                    name = f"r{repeat}-{sampling}-w{workers}"
                    run(name, client, room(V2, dest/name, frames=120, profile="detail", workers=workers)
                        + ["--cpu-only", "--sampling", sampling], records, phase)
    elif phase == "baseline-cost":
        for repeat in range(1, 4):
            order = ("r11", "r12") if repeat != 2 else ("r12", "r11")
            for revision in order:
                binary = OUT/"baseline"/f"ascii-material-room{SUFFIX}" if revision == "r11" else client
                name = f"r{repeat}-{revision}"
                run(name, binary, room(V2, dest/name, frames=120, profile="detail", workers=8), records, phase)
    elif phase == "windows":
        for mode in MODES:
            for sampling in SAMPLING:
                name = f"{mode}-{sampling}"
                run(name, client, ["--scene", COVERAGE, "--camera", mode,
                    "--sampling", sampling, "--camera-path", "coverage", "--seconds", 6,
                    "--workers", 8, "--profile", "detail", "--backend", "vulkan",
                    "--output", dest/name], records, phase)
        run("lab", lab, ["--seconds", 2, "--workers", 8, "--backend", "vulkan"], records, phase)
    elif phase == "motion":
        for sampling in SAMPLING:
            run(sampling, client, room(COVERAGE, dest/sampling, frames=180, profile="detail")
                + ["--sampling", sampling, "--camera-path", "coverage", "--sequence", 180], records, phase)
    else:
        legacy = {f"{name}-{backend}": compare(OUT/"baseline"/f"{name}-{backend}",
            OUT/"runtime-final"/f"{name}-{backend}") for backend in BACKENDS
            for name in ("v1", "v2", "lab", "environment")}
        parity = {s: compare(OUT/"runtime-final"/f"coverage-{s}-vulkan", OUT/"runtime-final"/f"coverage-{s}-dx12") for s in SAMPLING}
        measures = []
        for path in sorted((OUT/"measure").glob("*/run.json")):
            doc = json.loads(path.read_text(encoding="utf-8"))
            assert doc["cpu_only"] and doc["grid"] == [240,135]
            for case in doc["runs"]:
                assert case["static_changed_cells"] == 0 and case["submit_wait_ms"] is None
                measures.append({"run":path.parent.name,"sampling":doc["primary_sampling"],
                    "workers":doc["workers"],"camera":case["camera"],
                    "cpu_ms":case["cpu_snapshot_and_compose_ms"],"snapshot_ms":case["snapshot_ms"],
                    "compose_ms":case["compose_ms"],"frames":case["frame_count"]})
        assert len(measures) == 36
        windows = []
        for path in sorted((OUT/"windows").glob("*/window-run.json")):
            doc = json.loads(path.read_text(encoding="utf-8"))
            assert doc["frames"] > 12 and doc["configuration_changes"] == 0
            windows.append({k:doc[k] for k in ("camera","primary_sampling","frames",
                "frame_interval_ms","cpu_snapshot_and_compose_ms","acquire_submit_present_ms","actual_output")})
        assert len(windows) == 6
        movies = []
        for sampling in SAMPLING:
            path = OUT/"motion"/sampling/"run.json"
            doc = json.loads(path.read_text(encoding="utf-8"))
            assert doc["grid"] == [240,135] and doc["output"] == [1920,1080]
            for case in doc["runs"]:
                assert case["frame_count"] == 180 and case["static_changed_cells"] == 0
                assert len(list((path.parent/case["camera"]).glob("frame-*.png"))) == 180
                movies.append({"sampling":sampling,"camera":case["camera"],
                    "cpu_ms":case["cpu_snapshot_and_compose_ms"],"submit_wait_ms":case["submit_wait_ms"]})
        baseline_cost = []
        for path in sorted((OUT/"baseline-cost").glob("*/run.json")):
            doc = json.loads(path.read_text(encoding="utf-8"))
            assert doc["grid"] == [240,135] and doc["workers"] == 8
            for case in doc["runs"]:
                baseline_cost.append({"run":path.parent.name,"camera":case["camera"],
                    "cpu_ms":case["cpu_snapshot_and_compose_ms"],"submit_wait_ms":case["submit_wait_ms"]})
        assert len(baseline_cost) == 18
        audits = []
        current_scene = json.loads(COVERAGE.read_text(encoding="utf-8"))
        for directory in ("coverage-audit-final", "coverage-audit-detail"):
            doc = json.loads((OUT/directory/"coverage-audit.json").read_text(encoding="utf-8"))
            assert doc["input_scene"] == current_scene
            for view in doc["views"]:
                for frame in view["frames"]:
                    for variant in frame["variants"]:
                        assert variant["fresh_history_full_cell_mismatches"] == 0
                        assert all(o["border_cells"] == 0 for o in variant["objects"] if o["object_id"] >= 10)
                for variant in view["temporal_summary"]:
                    subjects = [o for o in variant["objects"] if o["object_id"] >= 10]
                    audits.append({"audit":directory,"grid":doc["grid"],"samples":doc["sample_count"],
                        "camera":view["camera"],"sampling":variant["sampling"],
                        **{key:sum(o[key] for o in subjects) for key in (
                            "missing_samples_while_reference_positive", "missed_area_cell_samples",
                            "excess_area_cell_samples", "full_cell_history_mismatches")}})
        summary = {"legacy_png_equal":legacy,"coverage_backend_png_equal":parity,
            "baseline_cost":baseline_cost,
            "measurements":measures,"windows":windows,"motion":movies,"audits":audits,
            "limits":["Single host, no isolated machine, CPU-only is not FPS",
                "FIFO window intervals are not GPU timestamps or scanout timing",
                "Finite dense geometric reference and sampled visual inspection are distinct from final quality"]}
        (OUT/"summary.json").write_text(json.dumps(summary,indent=2),encoding="utf-8")
        print(json.dumps(summary,indent=2))


if __name__ == "__main__":
    main()
