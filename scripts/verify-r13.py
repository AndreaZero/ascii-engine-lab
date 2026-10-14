"""R13 serialized comparisons against the preserved, unmodified R12 executable.

Build first. Never overlap these phases with a build, audit or encoder.
Raw logs and captures stay local under evidence/r13; no quality auto-scaling.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import statistics
import subprocess
import time

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "evidence/r13"
MODES = ("first", "third", "overhead")
SAMPLING = ("center", "conservative9")
SCENES = {name: ROOT / "clients/material-room/assets" / file for name, file in (
    ("v1", "material-room.json"), ("v2", "material-room-v2.json"),
    ("coverage", "coverage-room-v2.json"))}
EXE = "ascii-material-room" + (".exe" if os.name == "nt" else "")
BINARIES = {"r12": OUT / "baseline" / EXE, "r13": ROOT / "target/release" / EXE}


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(phase, name, revision, args, records):
    command = [str(BINARIES[revision]), *map(str, args)]
    start = time.perf_counter()
    log = OUT / f"{phase}-{name}.log"
    with log.open("w", encoding="utf-8") as stream:
        result = subprocess.run(command, cwd=ROOT, stdout=stream,
            stderr=subprocess.STDOUT, timeout=1200,
            creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
    records.append({"case": name, "command": command, "exit_code": result.returncode,
        "wall_seconds": time.perf_counter()-start,
        "executable_sha256": digest(BINARIES[revision])})
    (OUT/f"runs-{phase}.json").write_text(json.dumps(records, indent=2), encoding="utf-8")
    if result.returncode:
        raise RuntimeError(log.read_text(encoding="utf-8")[-4000:])
    print(name, flush=True)


def room(scene, output, frames=120, workers=8, backend="vulkan"):
    return ["--headless", "--scene", SCENES[scene], "--camera", "all",
        "--frames", frames, "--profile", "detail", "--workers", workers,
        "--backend", backend, "--output", output]


def compare(left, right):
    a = {str(p.relative_to(left)): digest(p) for p in left.rglob("*.png")}
    b = {str(p.relative_to(right)): digest(p) for p in right.rglob("*.png")}
    assert a and a == b, f"PNG mismatch: {left}, {right}"
    return len(a)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--phase", required=True, choices=["baseline", "runtime", "pilot", "measure", "windows", "cost-summary", "summary"])
    phase = parser.parse_args().phase
    OUT.mkdir(parents=True, exist_ok=True)
    records = []
    if phase in ("baseline", "runtime"):
        rev = "r12" if phase == "baseline" else "r13"
        for scene in SCENES:
            for sampling in SAMPLING:
                for backend in ("vulkan", "dx12"):
                    name = f"{scene}-{sampling}-{backend}"
                    run(phase, name, rev, room(scene, OUT/phase/name, frames=3, backend=backend)
                        + ["--sampling", sampling, "--camera-path", "coverage"], records)
        # Six-second motion, identical simulation clock/path and native detail grid.
        for sampling in SAMPLING:
            name = f"motion-{sampling}"
            run(phase, name, rev, room("coverage", OUT/phase/name, frames=180)
                + ["--sampling", sampling, "--camera-path", "coverage", "--sequence", 180], records)
    elif phase == "pilot":
        # Short diagnostic only; excluded from the final repeated comparison.
        for scene in ("v2", "coverage"):
            for sampling in SAMPLING:
                for rev in ("r12", "r13"):
                    name = f"{scene}-{sampling}-w8-{rev}"
                    run(phase, name, rev, room(scene, OUT/phase/name, frames=60)
                        + ["--cpu-only", "--sampling", sampling, "--camera-path", "coverage"], records)
    elif phase == "measure":
        for repeat in range(1, 4):
            order = ("r12", "r13") if repeat != 2 else ("r13", "r12")
            for scene in ("v2", "coverage"):
                for sampling in SAMPLING:
                    for workers in (1, 8):
                        for rev in order:
                            name = f"r{repeat}-{scene}-{sampling}-w{workers}-{rev}"
                            run(phase, name, rev, room(scene, OUT/phase/name, workers=workers)
                                + ["--cpu-only", "--sampling", sampling, "--camera-path", "coverage"], records)
    elif phase == "windows":
        for mode in MODES:
            for sampling in SAMPLING:
                name = f"{mode}-{sampling}"
                run(phase, name, "r13", ["--scene", SCENES["coverage"], "--camera", mode,
                    "--sampling", sampling, "--camera-path", "coverage", "--seconds", 6,
                    "--workers", 8, "--profile", "detail", "--backend", "vulkan",
                    "--output", OUT/phase/name], records)
    else:
        # Refuse a matrix assembled from different candidate executables.
        hashes = {}
        for checked_phase in ("runtime", "measure"):
            phase_runs = json.loads((OUT/f"runs-{checked_phase}.json").read_text(encoding="utf-8"))
            assert len(phase_runs) == (14 if checked_phase == "runtime" else 48)
            assert len({r["case"] for r in phase_runs}) == len(phase_runs)
            for record in phase_runs:
                assert record["exit_code"] == 0
                revision = "r12" if record["case"].endswith("-r12") else "r13"
                hashes.setdefault(revision, set()).add(record["executable_sha256"])
        assert all(values == {digest(BINARIES[revision])} for revision, values in hashes.items())
        expected_captures = {f"{s}-{q}-{b}": 6 for s in SCENES
            for q in SAMPLING for b in ("vulkan", "dx12")}
        expected_captures.update({f"motion-{q}": 540 for q in SAMPLING})
        parity = {name: compare(OUT/"baseline"/name, OUT/"runtime"/name)
            for name in expected_captures}
        assert parity == expected_captures
        backend = {f"{s}-{q}": compare(OUT/"runtime"/f"{s}-{q}-vulkan",
            OUT/"runtime"/f"{s}-{q}-dx12") for s in SCENES for q in SAMPLING}
        measurements = []
        expected_runs = {f"r{r}-{s}-{q}-w{w}-{v}": (s, q, w)
            for r in range(1, 4) for s in ("v2", "coverage") for q in SAMPLING
            for w in (1, 8) for v in ("r12", "r13")}
        for name, (scene, sampling, workers) in expected_runs.items():
            path = OUT/"measure"/name/"run.json"
            doc = json.loads(path.read_text(encoding="utf-8"))
            assert doc["cpu_only"] and doc["grid"] == [240, 135]
            assert doc["input_scene"] == json.loads(SCENES[scene].read_text(encoding="utf-8"))
            assert doc["camera_path"] == "coverage"
            assert doc["primary_sampling"] == sampling and doc["workers"] == workers
            assert doc["stage"] == "light" and doc["time_start"] == 0
            assert doc["simulation_ticks_per_second"] == 30 and doc["camera_orbit_speed"] == 0
            assert sorted(c["camera"] for c in doc["runs"]) == sorted(MODES)
            for case in doc["runs"]:
                assert case["frame_count"] == 120
                assert case["static_changed_cells"] == 0 and case["submit_wait_ms"] is None
                measurements.append({"run": path.parent.name, "camera": case["camera"],
                    "cpu_ms": case["cpu_snapshot_and_compose_ms"],
                    "snapshot_ms": case["snapshot_ms"], "compose_ms": case["compose_ms"]})
        assert len(measurements) == 144
        if phase == "cost-summary":
            (OUT/"cost-summary.json").write_text(json.dumps({"measurements": measurements,
                "baseline_png_equal": parity, "backend_png_equal": backend}, indent=2), encoding="utf-8")
            for scene in ("v2", "coverage"):
                for sampling in SAMPLING:
                    for workers in (1, 8):
                        for rev in ("r12", "r13"):
                            rows = [m for m in measurements if m["run"].endswith(f"{scene}-{sampling}-w{workers}-{rev}")]
                            print(scene, sampling, workers, rev,
                                [round(statistics.median(m["cpu_ms"]["p50"] for m in rows if m["camera"] == c), 3) for c in MODES],
                                "p95_exceed", sum(m["cpu_ms"]["p95"] > 16.7 for m in rows))
            return
        windows = []
        window_runs = json.loads((OUT/"runs-windows.json").read_text(encoding="utf-8"))
        assert len(window_runs) == 6
        assert {r["case"] for r in window_runs} == {f"{c}-{q}" for c in MODES for q in SAMPLING}
        assert all(r["exit_code"] == 0 and r["executable_sha256"] == digest(BINARIES["r13"])
            for r in window_runs)
        for name in (f"{c}-{q}" for c in MODES for q in SAMPLING):
            path = OUT/"windows"/name/"window-run.json"
            doc = json.loads(path.read_text(encoding="utf-8"))
            assert doc["frames"] > 12 and doc["configuration_changes"] == 0
            assert doc["actual_output"] == [1920, 1080]
            assert doc["requested_output"] == [1920, 1080]
            assert doc["workers"] == 8 and doc["grid"] == [240, 135]
            assert doc["camera_path"] == "coverage" and doc["warmup_frames_excluded"] == 12
            assert doc["input_scene"] == json.loads(SCENES["coverage"].read_text(encoding="utf-8"))
            assert doc["gpu"]["adapter"]["backend"] == "Vulkan" and doc["wall_seconds"] >= 6
            assert f'{doc["camera"]}-{doc["primary_sampling"]}' == name
            windows.append(doc)
        assert len(windows) == 6
        summary = {"baseline_png_equal": parity, "backend_png_equal": backend,
            "measurements": measurements, "windows": windows,
            "scene_sha256": {s: digest(p) for s, p in SCENES.items()},
            "limits": ["Single host, not isolated; CPU-only is not FPS",
                "Window callback intervals and submission waits are not GPU timestamps",
                "Finite corpus and sampled visual inspection do not prove universal equivalence"]}
        (OUT/"summary.json").write_text(json.dumps(summary, indent=2), encoding="utf-8")
        print(json.dumps({k:v for k,v in summary.items() if k not in ("measurements", "windows")}, indent=2))
        for scene in ("v2", "coverage"):
            for sampling in SAMPLING:
                for workers in (1, 8):
                    for rev in ("r12", "r13"):
                        rows = [m for m in measurements if m["run"].endswith(f"{scene}-{sampling}-w{workers}-{rev}")]
                        print(scene, sampling, workers, rev, [round(statistics.median(m["cpu_ms"]["p50"] for m in rows if m["camera"] == c), 3) for c in MODES],
                            "p95_exceed", sum(m["cpu_ms"]["p95"] > 16.7 for m in rows))


if __name__ == "__main__":
    main()
