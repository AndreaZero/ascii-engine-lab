"""Serialized R14 occupancy evidence. Build first; no overlapping heavy jobs.

Frozen R13 binaries belong in evidence/r14/baseline. Raw evidence stays local.
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
OUT = ROOT / "evidence/r14"
EXT = ".exe" if os.name == "nt" else ""
BIN = {r: (OUT / "baseline" if r == "r13" else ROOT / "target/release") / ("ascii-material-room" + EXT) for r in ("r13", "r14")}
ASSETS = ROOT / "clients/material-room/assets"
SCENES = {"v1": ASSETS / "material-room.json", "v2": ASSETS / "material-room-v2.json", "coverage": ASSETS / "coverage-room-v2.json", "dynamic": ASSETS / "dynamic-room-v2.json"}
UPDATES = ASSETS / "dynamic-updates-v1.json"
MODES = ("first", "third", "overhead")
POLICIES = ("center", "conservative9")

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def run(phase, name, binary, args, records):
    command = [str(binary), *map(str, args)]
    before = time.perf_counter()
    with (OUT / f"{phase}-{name}.log").open("w", encoding="utf-8") as log:
        result = subprocess.run(command, cwd=ROOT, stdout=log, stderr=subprocess.STDOUT,
            timeout=1200, creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
    records.append({"case": name, "command": command, "exit_code": result.returncode,
        "seconds": time.perf_counter()-before, "binary_sha256": digest(binary)})
    (OUT / f"runs-{phase}.json").write_text(json.dumps(records, indent=2), encoding="utf-8")
    if result.returncode:
        raise RuntimeError((OUT/f"{phase}-{name}.log").read_text(encoding="utf-8")[-4000:])
    print(name, flush=True)

def room(scene, output, workers=8, frames=180, backend="vulkan"):
    return ["--scene", SCENES[scene], "--headless", "--camera", "all", "--frames", frames,
        "--workers", workers, "--profile", "detail", "--backend", backend, "--output", output]

def compare(left, right):
    a = {str(p.relative_to(left)): digest(p) for p in left.rglob("*.png")}
    b = {str(p.relative_to(right)): digest(p) for p in right.rglob("*.png")}
    assert a and a == b, f"PNG mismatch: {left}, {right}"
    return len(a)

def validate_dynamic(doc, policy, workers, cpu_only=False):
    scene = json.loads(SCENES["dynamic"].read_text(encoding="utf-8"))
    plan = json.loads(UPDATES.read_text(encoding="utf-8"))
    assert doc["input_scene"] == scene and doc["input_updates"] == plan
    assert doc["grid"] == [240,135] and doc["output"] == [1920,1080]
    assert doc["primary_sampling"] == policy and doc["workers"] == workers
    assert doc["camera_path"] == "static" and doc["camera_orbit_speed"] == 0 and doc["time_start"] == 0
    assert doc["stage"] == "light" and doc["simulation_ticks_per_second"] == 30
    assert doc["cpu_only"] == cpu_only and len(doc["runs"]) == 3
    assert sorted(r["camera"] for r in doc["runs"]) == sorted(MODES)
    for r in doc["runs"]:
        assert r["frame_count"] == 180 and r["static_changed_cells"] == 0
        assert len(r["frames"]) == 180
        assert r["final_dynamic_state"]["committed_updates"] == len(plan["events"])
        live = {o["id"]: i+1 for i,o in enumerate(sorted(scene["objects"], key=lambda o:o["id"]))}
        generation = len(live)
        revision = 0
        for i, frame in enumerate(r["frames"]):
            events = [e for e in plan["events"] if e["tick"] == i]
            for event in events:
                revision += 1
                for op in event["operations"]:
                    if op["op"] == "remove": del live[op["id"]]
                    if op["op"] == "insert":
                        generation += 1
                        live[op["object"]["id"]] = generation
            state = frame["dynamic_state"]
            assert frame["frame"] == i and state["tick"] == i
            assert state["revision"] == revision and state["completed_events"] == revision
            assert state["objects"] == len(live)
            assert {h["id"]:h["generation"] for h in state["identities"]} == live
            assert frame["commits"] == len(events)
            if events: assert frame["matched"] == 0, (r["camera"], i, "history not reset")
        if cpu_only: assert r["submit_wait_ms"] is None

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--phase", required=True, choices=("baseline", "legacy", "audit", "dynamic", "measure", "windows", "summary"))
    phase = parser.parse_args().phase
    OUT.mkdir(parents=True, exist_ok=True)
    records = []
    if phase in ("baseline", "legacy"):
        revision = "r13" if phase == "baseline" else "r14"
        for scene in SCENES:
            for policy in POLICIES:
                for backend in ("vulkan", "dx12"):
                    name = f"{scene}-{policy}-{backend}"
                    run(phase, name, BIN[revision], room(scene, OUT/phase/name, frames=3, backend=backend)+["--sampling",policy], records)
        lab = BIN[revision].with_name("ascii-engine-lab"+EXT)
        for backend in ("vulkan","dx12"):
            name = "lab-"+backend
            run(phase, name, lab, ["--benchmark","--frames",1,"--profile","balanced","--workers",8,"--backend",backend,"--output",OUT/phase/name], records)
    elif phase == "audit":
        run(phase,"occupancy",ROOT/("target/release/examples/occupancy_audit"+EXT),
            ["--cols",240,"--rows",135,"--output",OUT/"occupancy-audit"],records)
    elif phase == "dynamic":
        for policy in POLICIES:
            for backend in ("vulkan","dx12"):
                name = f"{policy}-{backend}"
                run(phase,name,BIN["r14"],room("dynamic",OUT/phase/name,backend=backend)+["--updates",UPDATES,"--sampling",policy,"--sequence",180],records)
    elif phase == "measure":
        for repeat in range(1,4):
            # Default overhead against frozen R13: identical v2 data/path/density.
            for policy in POLICIES:
                for revision in (("r13","r14") if repeat != 2 else ("r14","r13")):
                    name = f"r{repeat}-legacy-{policy}-w8-{revision}"
                    run(phase,name,BIN[revision],room("v2",OUT/phase/name,frames=90)+["--sampling",policy,"--cpu-only"],records)
            # Dynamic workload cost is reported separately, not as a speedup.
            for policy in POLICIES:
                for workers in (1,8):
                    name = f"r{repeat}-dynamic-{policy}-w{workers}-r14"
                    run(phase,name,BIN["r14"],room("dynamic",OUT/phase/name,workers=workers)+["--updates",UPDATES,"--sampling",policy,"--cpu-only"],records)
    elif phase == "windows":
        for policy in POLICIES:
            for mode in MODES:
                name = f"{mode}-{policy}"
                run(phase,name,BIN["r14"],["--scene",SCENES["dynamic"],"--updates",UPDATES,"--camera",mode,"--sampling",policy,"--profile","detail","--workers",8,"--backend","vulkan","--seconds",7,"--output",OUT/phase/name],records)
        run(phase,"lab",BIN["r14"].with_name("ascii-engine-lab"+EXT),["--seconds",2,"--workers",8,"--backend","vulkan"],records)
    else:
        counts = {"baseline":18,"legacy":18,"audit":1,"dynamic":4,"measure":24,"windows":7}
        for p,count in counts.items():
            runs = json.loads((OUT/f"runs-{p}.json").read_text(encoding="utf-8"))
            assert len(runs) == count and len({r["case"] for r in runs}) == count
            if p in ("baseline","legacy"):
                binary = BIN["r13" if p == "baseline" else "r14"]
                expected_runs = {f"{s}-{q}-{b}":binary for s in SCENES for q in POLICIES for b in ("vulkan","dx12")}
                expected_runs.update({f"lab-{b}":binary.with_name("ascii-engine-lab"+EXT) for b in ("vulkan","dx12")})
            elif p == "audit": expected_runs = {"occupancy":ROOT/("target/release/examples/occupancy_audit"+EXT)}
            elif p == "dynamic": expected_runs = {f"{q}-{b}":BIN["r14"] for q in POLICIES for b in ("vulkan","dx12")}
            elif p == "windows":
                expected_runs = {f"{m}-{q}":BIN["r14"] for m in MODES for q in POLICIES}
                expected_runs["lab"] = BIN["r14"].with_name("ascii-engine-lab"+EXT)
            else:
                expected_runs = {f"r{i}-legacy-{q}-w8-{r}":BIN[r] for i in (1,2,3) for q in POLICIES for r in ("r13","r14")}
                expected_runs.update({f"r{i}-dynamic-{q}-w{w}-r14":BIN["r14"] for i in (1,2,3) for q in POLICIES for w in (1,8)})
            assert {r["case"] for r in runs} == set(expected_runs)
            for row in runs:
                assert row["exit_code"] == 0
                assert Path(row["command"][0]).resolve() == expected_runs[row["case"]].resolve()
                assert digest(Path(row["command"][0])) == row["binary_sha256"]
        assert (OUT/"baseline/source-commit.txt").read_text(encoding="utf-8-sig").strip() == "ac2dfd22c8b690cfca79d4949f058830f061efcc"
        # Published R13 artifacts copied before any R14 build. Do not regenerate
        # these expected hashes from whichever binaries happen to be present.
        assert digest(BIN["r13"]) == "d0775df145a60a03ceeda44eba005117ab771c68d113cdd33ea93f5bcbd630d6"
        assert digest(BIN["r13"].with_name("ascii-engine-lab"+EXT)) == "c631383ee91fd76b38db5e22b234910766b812e3819f012556c136caa8409d24"
        audit = json.loads((OUT/"occupancy-audit/audit.json").read_text(encoding="utf-8"))
        assert audit["schema_version"] == 1 and audit["timed"] is False
        assert audit["input_scene"] == json.loads(SCENES["dynamic"].read_text(encoding="utf-8"))
        assert audit["input_updates"] == json.loads(UPDATES.read_text(encoding="utf-8"))
        ar = audit["room"]
        assert (ar["cols"],ar["rows"],ar["events"],ar["composition_records"]) == (240,135,11,420)
        assert ar["retained_snapshots"] == 12 and ar["retained_snapshot_view_checks"] == 72
        ticks = {0,179} | {e["tick"]+d for e in audit["input_updates"]["events"] for d in (-1,0,1)}
        expected = {(t,m,p,w) for t in ticks for m in ("First","Third","Overhead") for p in ("Center","Conservative9") for w in (1,8)}
        assert len(ar["records"]) == 420 and {(r["tick"],r["mode"],r["sampling"],r["workers"]) for r in ar["records"]} == expected
        assert all(r["bitwise_equal"] and r["visible"] > 0 for r in ar["records"])
        transport = audit["transport"]
        assert transport["record_count"] == 48 and len(transport["records"]) == 48 and transport["removal_checks"] == 48
        assert {(r["path"],r["mode"],r["sampling"],r["workers"]) for r in transport["records"]} == {(path,m,p,w) for path in ("primary","shadow","diffuse_normal","local_specular") for m in ("First","Third","Overhead") for p in ("Center","Conservative9") for w in (1,8)}
        assert all(r["path_observed"] and r["bitwise_equal"] and r["before_rgb"] != r["after_rgb"] for r in transport["records"])
        legacy = {name:compare(OUT/"baseline"/name, OUT/"legacy"/name)
            for name in [f"{s}-{q}-{b}" for s in SCENES for q in POLICIES for b in ("vulkan","dx12")]+["lab-vulkan","lab-dx12"]}
        assert sum(legacy.values()) == 120
        backend_pairs = {}
        for policy in POLICIES:
            backend_pairs[policy] = compare(OUT/"dynamic"/(policy+"-vulkan"),OUT/"dynamic"/(policy+"-dx12"))
            assert backend_pairs[policy] == 540
            for backend in ("vulkan","dx12"):
                folder = OUT/"dynamic"/f"{policy}-{backend}"
                doc = json.loads((folder/"run.json").read_text(encoding="utf-8"))
                validate_dynamic(doc,policy,8)
                assert doc["gpu"]["adapter"]["backend"] == {"vulkan":"Vulkan","dx12":"Dx12"}[backend]
                for mode in MODES:
                    assert digest(folder/mode/"frame-0000.png") == digest(OUT/"baseline"/f"dynamic-{policy}-{backend}"/mode/"frame-0000.png")
                    assert digest(folder/mode/"frame-0029.png") != digest(folder/mode/"frame-0121.png")
        measurements = []
        for row in json.loads((OUT/"runs-measure.json").read_text(encoding="utf-8")):
            name = row["case"]
            doc = json.loads((OUT/"measure"/name/"run.json").read_text(encoding="utf-8"))
            _, kind, policy, worker, revision = name.split("-")
            workers = int(worker[1:])
            if kind == "dynamic": validate_dynamic(doc,policy,workers,True)
            else:
                assert doc["input_scene"] == json.loads(SCENES["v2"].read_text(encoding="utf-8"))
                assert doc["grid"] == [240,135] and doc["workers"] == 8 and doc["cpu_only"]
                assert doc["primary_sampling"] == policy and doc["camera_path"] == "static" and doc["time_start"] == 0
                assert len(doc["runs"]) == 3 and all(r["frame_count"] == 90 for r in doc["runs"])
                assert sorted(r["camera"] for r in doc["runs"]) == sorted(MODES)
            for r in doc["runs"]:
                measurements.append({"run":name,"kind":kind,"policy":policy,"workers":workers,"revision":revision,"camera":r["camera"],
                    "cpu_ms":r["cpu_snapshot_and_compose_ms"],"total_ms":r.get("cpu_update_snapshot_and_compose_ms",r["cpu_snapshot_and_compose_ms"]),
                    "snapshot_ms":r["snapshot_ms"],"commit_ms":r.get("commit_frame_update_ms")})
        windows = []
        for mode in MODES:
            for policy in POLICIES:
                doc = json.loads((OUT/"windows"/f"{mode}-{policy}"/"window-run.json").read_text(encoding="utf-8"))
                assert doc["actual_output"] == [1920,1080] and doc["grid"] == [240,135]
                assert doc["configuration_changes"] == 0 and doc["frames"] > 12
                assert doc["input_scene"] == json.loads(SCENES["dynamic"].read_text(encoding="utf-8"))
                assert doc["input_updates"] == json.loads(UPDATES.read_text(encoding="utf-8"))
                assert doc["warmup_frames_excluded"] == 12 and doc["wall_seconds"] >= 7
                assert doc["camera_path"] == "static" and doc["requested_output"] == [1920,1080]
                assert doc["camera"] == mode and doc["primary_sampling"] == policy
                assert doc["workers"] == 8 and doc["gpu"]["adapter"]["backend"] == "Vulkan"
                assert doc["final_dynamic_state"]["completed_events"] == 11 and doc["final_dynamic_state"]["revision"] == 11
                windows.append(doc)
        summary = {"legacy_png_equal":legacy,"dynamic_backend_pairs":backend_pairs,"measurements":measurements,"windows":windows,
            "binary_sha256":{r:digest(p) for r,p in BIN.items()},"scene_sha256":digest(SCENES["dynamic"]),"updates_sha256":digest(UPDATES)}
        (OUT/"summary.json").write_text(json.dumps(summary,indent=2),encoding="utf-8")
        lines = ["# R14 compact evidence", "", "Fixed 240x135 cells / 1920x1080, same scene and update plan across three views.",
            "Single non-isolated Windows host. CPU and window callbacks are not GPU timestamps or guaranteed FPS.", "",
            "Frozen baseline source: ac2dfd22c8b690cfca79d4949f058830f061efcc. Binary hashes are local Windows build provenance, not portable build requirements.",
            f"R13 Material Room SHA-256: `{digest(BIN['r13'])}`.",
            f"R14 Material Room SHA-256: `{digest(BIN['r14'])}`.",
            f"R14 occupancy audit SHA-256: `{digest(ROOT/('target/release/examples/occupancy_audit'+EXT))}`.", "",
            "Independent audit: 420 composition records at 240x135, 217,539 ray checks; 12 retained snapshots / 72 view-time checks; 48 transport probes and 48 removal checks. Compiled input scene/plan match runtime assets.", "",
            f"Legacy PNG equality to frozen R13: {sum(legacy.values())}. Dynamic Vulkan/DX12 pairs: {sum(backend_pairs.values())}.",
            "All 11 commits occur in order; survivor generations remain stable, the recreated panel receives a new generation, and matched history is zero on commit frames.", "",
            "## CPU cost", "", "Median of three p50 values (ms), first / third / overhead. Legacy compares identical v2 content; dynamic is a different workload, not an optimization comparison.", "",
            "| Workload | Policy | Workers | Revision | CPU including updates | p95 >16.7 ms | Worst p95 |", "| --- | --- | ---: | --- | --- | ---: | ---: |"]
        for kind,worker,revisions in (("legacy",8,("r13","r14")),("dynamic",1,("r14",)),("dynamic",8,("r14",))):
            for policy in POLICIES:
                for revision in revisions:
                    rows = [m for m in measurements if (m["kind"],m["workers"],m["policy"],m["revision"]) == (kind,worker,policy,revision)]
                    medians = [statistics.median(r["total_ms"]["p50"] for r in rows if r["camera"] == mode) for mode in MODES]
                    lines.append(f"| {kind} | {policy} | {worker} | {revision} | "+" / ".join(f"{v:.3f}" for v in medians)+f" | {sum(r['total_ms']['p95']>16.7 for r in rows)}/9 | {max(r['total_ms']['p95'] for r in rows):.3f} |")
        dynamic = [m for m in measurements if m["kind"] == "dynamic"]
        lines += ["", f"Commit-frame update p50 range: {min(m['commit_ms']['p50'] for m in dynamic):.4f}–{max(m['commit_ms']['p50'] for m in dynamic):.4f} ms; worst commit-frame p95 {max(m['commit_ms']['p95'] for m in dynamic):.4f} ms.",
            "Preflight and initial catch-up are excluded from frame timers. Each later update includes candidate construction, validation and atomic publication. Query snapshots stay immutable.",
            "", "## Native windows", "", "Seven seconds per case, 12 warmups excluded, all events completed. No configuration changes.", "",
            "| View | Policy | Frames | Callback p50 / p95 ms | CPU total p50 ms |", "| --- | --- | ---: | --- | ---: |"]
        for r in windows:
            lines.append(f"| {r['camera']} | {r['primary_sampling']} | {r['frames']} | {r['frame_interval_ms']['p50']:.3f} / {r['frame_interval_ms']['p95']:.3f} | {r['cpu_update_snapshot_and_compose_ms']['p50']:.3f} |")
        lines += ["", "Raw measurements, generated captures and binaries remain local. Transport probes and independent oracle results are described in DYNAMIC_OCCUPANCY.md.", ""]
        (OUT/"summary.md").write_text("\n".join(lines),encoding="utf-8")
        print("Summary PASS", sum(legacy.values()), sum(backend_pairs.values()), len(measurements), flush=True)

if __name__ == "__main__":
    main()
