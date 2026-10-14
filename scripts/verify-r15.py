"""Serialized R15 authoring / rendering acceptance. Build before running.

Baseline executables are frozen R14 b69bb16 in evidence/r15/baseline.
No build, benchmark or encoder should run concurrently with this script.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "evidence/r15"
ASSETS = ROOT / "clients/material-room/assets"
BIN = ROOT / "target/release"
MODES = ("first", "third", "overhead")
POLICIES = ("center", "conservative9")
EXT = ".exe" if os.name == "nt" else ""
PROJECT = OUT / "authored.project.json"

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def write(path, data):
    path.write_text(json.dumps(data, indent=2)+"\n", encoding="utf-8")

def run(name, binary, arguments, expected=0):
    command = [str(binary), *map(str, arguments)]
    start = time.perf_counter()
    result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, encoding="utf-8",
        timeout=600, creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
    (OUT / (phase+"-"+name+".log")).write_text(result.stdout+result.stderr,encoding="utf-8")
    records.append({"name":name,"command":command,"exit_code":result.returncode,"expected":expected,
                    "seconds":time.perf_counter()-start,"binary_sha256":digest(binary)})
    write(OUT / ("runs-"+phase+".json"),records)
    assert (result.returncode == 0) == (expected == 0), result.stdout+result.stderr
    print(name,flush=True)

def room(arguments, output, backend="vulkan", policy="center"):
    return [*arguments,"--headless","--camera","all","--profile","detail","--workers",8,
            "--backend",backend,"--sampling",policy,"--output",output]

def compare(a,b):
    left = {str(p.relative_to(a)):digest(p) for p in a.rglob("*.png")}
    right = {str(p.relative_to(b)):digest(p) for p in b.rglob("*.png")}
    assert left and left == right, (a,b)
    return len(left)

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--phase",required=True,choices=("legacy","author","views","windows","summary"))
phase = parser.parse_args().phase
OUT.mkdir(parents=True,exist_ok=True)
records = []
client = BIN / ("ascii-material-room"+EXT)
author = BIN / ("author"+EXT)
if phase == "legacy":
    assert digest(OUT/"baseline"/("ascii-material-room"+EXT)) == "d7b2a1f27b6d490843f05fc09755f5415e45f969e0826081df55dadeaad3e127"
    for revision,folder in (("r14",OUT/"baseline"),("r15",BIN)):
        for scene in ("material-room","material-room-v2","dynamic-room-v2"):
            for policy in POLICIES:
                args = ["--scene",ASSETS/(scene+".json"),"--frames",3]
                if scene == "dynamic-room-v2":
                    args += ["--updates",ASSETS/"dynamic-updates-v1.json","--time",3.]
                name = f"{revision}-{scene}-{policy}"
                run(name,folder/("ascii-material-room"+EXT),room(args,OUT/"legacy"/name,policy=policy))
        run(revision+"-lab",folder/("ascii-engine-lab"+EXT),["--benchmark","--frames",1,"--profile","balanced","--workers",8,"--backend","vulkan","--output",OUT/"legacy"/(revision+"-lab")])
elif phase == "author":
    run("new",author,["new",PROJECT])
    initial = PROJECT.read_bytes()
    run("new-no-clobber",author,["new",PROJECT],expected=1)
    assert PROJECT.read_bytes() == initial
    run("duplicate",author,["duplicate",PROJECT,7])
    run("translate",author,["translate",PROJECT,9,-0.6,0.2,0.8])
    run("material",author,["material",PROJECT,9,36])
    patch = {"version":1,"operations":[
        {"op":"replace","path":"/scene/name","value":"R15 authored room — saved and reopened"},
        {"op":"replace","path":"/scene/objects/8/half_extents","value":[0.3,0.7,0.3]},
        {"op":"replace","path":"/scene/materials/0/pigment","value":[0.45,0.55,0.65]},
        {"op":"replace","path":"/updates/events","value":[
            {"tick":30,"operations":[{"op":"transform","id":9,"position":[0.8,0.8,1.2],"half_extents":[0.3,0.7,0.3],"rotation_y":0.4}]},
            {"tick":60,"operations":[{"op":"remove","id":9}]}]},
    ]}
    for i,mode in enumerate(MODES):
        patch["operations"].append({"op":"replace","path":"/camera_controls/"+mode,
            "value":{"orbit":0.03*(i+1),"offset":[0.05,0.02,-0.04],"zoom":1.0}})
    write(OUT/"edit.patch.json",patch)
    run("apply",author,["apply",PROJECT,OUT/"edit.patch.json"])
    before = PROJECT.read_bytes()
    run("invalid-future-remove",author,["remove",PROJECT,9],expected=1)
    assert PROJECT.read_bytes() == before
    run("inspect",author,["inspect",PROJECT])
    run("save-as",author,["save-as",PROJECT,OUT/"reopened.project.json"])
    assert json.loads(before) == json.loads((OUT/"reopened.project.json").read_text(encoding="utf-8"))
    run("validate",client,["--project",PROJECT,"--validate"])
    write(OUT/"author-summary.json",{"commands":len(records),"expected_rejections":2,"project_sha256":digest(PROJECT),"objects":9,"events":2})
elif phase == "views":
    for policy in POLICIES:
        for backend in ("vulkan","dx12"):
            name = policy+"-"+backend
            run(name,client,room(["--project",PROJECT,"--sequence",90],OUT/"views"/name,backend,policy))
    run("reopened",client,room(["--project",OUT/"reopened.project.json","--frames",90],OUT/"views/reopened"))
elif phase == "windows":
    for policy in POLICIES:
        for mode in MODES:
            name=mode+"-"+policy
            run(name,client,["--project",PROJECT,"--camera",mode,"--sampling",policy,"--profile","detail","--workers",8,"--backend","vulkan","--seconds",4,"--output",OUT/"windows"/name])
    run("lab",BIN/("ascii-engine-lab"+EXT),["--seconds",2,"--workers",8,"--backend","vulkan"])
else:
    for p in ("legacy","author","views","windows"):
        rows=json.loads((OUT/("runs-"+p+".json")).read_text(encoding="utf-8"))
        assert rows
        for row in rows:
            assert (row["exit_code"]==0)==(row["expected"]==0)
            assert digest(Path(row["command"][0])) == row["binary_sha256"]
    count=0
    for scene in ("material-room","material-room-v2","dynamic-room-v2"):
        for policy in POLICIES:
            count+=compare(OUT/"legacy"/f"r14-{scene}-{policy}",OUT/"legacy"/f"r15-{scene}-{policy}")
    count+=compare(OUT/"legacy/r14-lab",OUT/"legacy/r15-lab")
    pairs=sum(compare(OUT/"views"/(p+"-vulkan"),OUT/"views"/(p+"-dx12")) for p in POLICIES)
    project=json.loads(PROJECT.read_text(encoding="utf-8"))
    for policy in POLICIES:
        for backend in ("vulkan","dx12"):
            data=json.loads((OUT/"views"/(policy+"-"+backend)/"run.json").read_text(encoding="utf-8"))
            assert data["input_project"] == project and data["grid"] == [240,135] and data["workers"] == 8
            assert data["primary_sampling"] == policy and data["camera_path"] == "static"
            assert len(data["runs"]) == 3
            for run_data in data["runs"]:
                assert run_data["frame_count"] == 90 and run_data["final_dynamic_state"]["committed_updates"] == 2
                for tick in (30,60):
                    assert run_data["frames"][tick]["commits"] == 1 and run_data["frames"][tick]["matched"] == 0
    for mode in MODES:
        for frame in (0,89):
            name=f"frame-{frame:04}.png"
            assert digest(OUT/"views/center-vulkan"/mode/name)==digest(OUT/"views/reopened"/mode/name)
    windows=[]
    for policy in POLICIES:
        for mode in MODES:
            data=json.loads((OUT/"windows"/(mode+"-"+policy)/"window-run.json").read_text(encoding="utf-8"))
            assert data["frames"]>12 and data["grid"]==[240,135] and data["actual_output"]==[1920,1080]
            assert data["final_dynamic_state"]["committed_updates"]==2
            windows.append({"view":mode,"sampling":policy,"frames":data["frames"],"cpu_ms":data["cpu_update_snapshot_and_compose_ms"],"interval_ms":data["frame_interval_ms"]})
    result={"legacy_png_identical":count,"backend_pairs_identical":pairs,"reopened_png_identical":6,
            "windows":windows,"scope":"One physical Windows host; fixed density. No second-machine or display FPS claim."}
    write(OUT/"summary.json",result)
    print(json.dumps(result,indent=2))
