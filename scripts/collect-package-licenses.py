"""Collect registry license/notice files from locked Windows-target metadata.

Run cargo metadata --locked --format-version 1 --filter-platform
x86_64-pc-windows-msvc first. This copies source notices; it does not install
packages, choose a license on behalf of upstream, or certify legal compliance.
"""
import argparse
import json
from pathlib import Path
import shutil

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("metadata", type=Path)
parser.add_argument("output", type=Path)
args = parser.parse_args()
meta = json.loads(args.metadata.read_text(encoding="utf-8-sig"))
nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
packages = {p["id"]: p for p in meta["packages"]}
pending = list(meta["workspace_members"])
visited = set()
while pending:
    key = pending.pop()
    if key in visited:
        continue
    visited.add(key)
    # Include build-time dependencies conservatively; exclude dev-only edges.
    for dep in nodes[key]["deps"]:
        if any(k["kind"] != "dev" for k in dep["dep_kinds"]):
            pending.append(dep["pkg"])
args.output.mkdir(parents=True, exist_ok=False)
manifest = []
missing = []
fallbacks = []
for key in sorted(visited):
    pkg = packages[key]
    if pkg["source"] is None:
        continue
    root = Path(pkg["manifest_path"]).parent
    destination = args.output / (pkg["name"] + "-" + pkg["version"])
    found = []
    for path in root.rglob("*"):
        if path.is_file() and path.name.upper().startswith(("LICENSE", "LICENCE", "COPYING", "NOTICE", "COPYRIGHT", "UNLICENSE")):
            relative = path.relative_to(root)
            target = destination / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(path, target)
            found.append(str(relative))
    if not found:
        # These upstream archives omit a standalone license file. Preserve their
        # whole distributed source (including notices in headers/XML/READMEs)
        # and the standard text allowed by their own Cargo SPDX declaration.
        expression = pkg["license"]
        standard = "Apache-2.0" if expression in ("Apache-2.0", "MIT OR Apache-2.0") else "CC0-1.0" if expression == "CC0-1.0" else None
        if standard:
            shutil.copytree(root, destination / "upstream-source")
            shutil.copyfile(Path(__file__).parent / "license-texts" / (standard+".txt"), destination / (standard+".txt"))
            found = [standard+".txt"] + [str(p.relative_to(destination)) for p in (destination/"upstream-source").rglob("*") if p.is_file()]
            fallbacks.append(pkg["name"] + "-" + pkg["version"])
        else:
            missing.append(pkg["name"] + "-" + pkg["version"])
    manifest.append({"name":pkg["name"], "version":pkg["version"], "license":pkg["license"],
                     "repository":pkg["repository"], "files":found})
(args.output / "manifest.json").write_text(json.dumps(manifest, indent=2)+"\n", encoding="utf-8")
(args.output / "README.txt").write_text(
    "Upstream notices from Cargo.lock-resolved Windows MSVC dependencies.\n"
    "Includes build-time dependencies conservatively. See manifest.json.\n"
    "Project MIT license is distributed separately as LICENSE.\n", encoding="utf-8")
print(json.dumps({"packages":len(manifest), "source_and_standard_text_fallback":fallbacks,"unresolved":missing}))
if missing:
    raise SystemExit("Review missing notices before packaging")
