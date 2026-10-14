"""Independent, stdlib-only integrity checker for a directory or its ZIP archive.

This checks supplied bytes, relative paths and PE architecture; it neither runs
the binaries nor certifies a driver, source/binary provenance or another host.
"""
import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath
import re
import stat
import struct
import zipfile


def checked_path(name):
    if not isinstance(name, str) or not name or "\\" in name or ":" in name:
        raise ValueError(f"Not a portable relative path: {name!r}")
    path = PurePosixPath(name)
    if path.is_absolute() or str(path) != name or any(p in (".", "..") for p in path.parts):
        raise ValueError(f"Unsafe path: {name!r}")
    reserved = re.compile(r"^(CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])(?:\.|$)", re.I)
    for part in path.parts:
        if part.endswith((".", " ")) or reserved.match(part) or any(ord(c) < 32 or c in '<>"|?*' for c in part):
            raise ValueError(f"Invalid Windows path: {name!r}")
    return name


def pe_imports(data, name):
    if len(data) < 64 or data[:2] != b"MZ":
        raise ValueError(f"Not a PE: {name}")
    pe = struct.unpack_from("<I", data, 60)[0]
    if pe + 26 > len(data) or data[pe:pe+4] != b"PE\0\0":
        raise ValueError(f"Invalid PE header: {name}")
    machine, sections = struct.unpack_from("<HH", data, pe+4)
    opt_size = struct.unpack_from("<H", data, pe+20)[0]
    opt = pe + 24
    if machine != 0x8664 or struct.unpack_from("<H", data, opt)[0] != 0x20B:
        raise ValueError(f"Not AMD64 PE32+: {name}")
    if opt_size < 128 or opt + opt_size + sections * 40 > len(data):
        raise ValueError(f"Truncated PE: {name}")
    table = opt + opt_size

    def rva_offset(rva):
        for i in range(sections):
            virtual_size, base, raw_size, raw = struct.unpack_from("<IIII", data, table + 40*i + 8)
            if base <= rva < base + max(virtual_size, raw_size):
                offset = raw + rva - base
                if offset >= len(data) or rva - base >= raw_size:
                    break
                return offset
        raise ValueError(f"PE RVA outside file: {name}")

    import_rva, _ = struct.unpack_from("<II", data, opt+120)
    if import_rva == 0:
        return []
    at = rva_offset(import_rva)
    names = []
    for _ in range(4096):
        entry = struct.unpack_from("<IIIII", data, at)
        if not any(entry):
            return sorted(set(names), key=str.casefold)
        start = rva_offset(entry[3])
        end = data.find(b"\0", start, min(len(data), start+4096))
        if end == -1:
            raise ValueError(f"Unterminated PE import: {name}")
        names.append(data[start:end].decode("ascii"))
        at += 20
    raise ValueError(f"Unbounded PE import table: {name}")


def check(package):
    def is_link(path):
        return path.is_symlink() or bool(getattr(path.lstat(), "st_file_attributes", 0) & 0x400)

    if package.is_dir():
        if is_link(package):
            raise ValueError("Package root cannot be a link/junction")
        files = {}
        for path in package.rglob("*"):
            if is_link(path):
                raise ValueError(f"Link/junction in package: {path}")
            if path.is_file():
                name = path.relative_to(package).as_posix()
                # Runtime evidence and user-created projects are deliberately outside the sealed payload.
                if name.startswith(("results/", "work/")):
                    continue
                files[checked_path(name)] = path.read_bytes()
    else:
        files = {}
        with zipfile.ZipFile(package) as archive:
            for entry in archive.infolist():
                if entry.is_dir():
                    checked_path(entry.filename.rstrip("/"))
                    continue
                name = checked_path(entry.filename)
                if stat.S_ISLNK(entry.external_attr >> 16) or name in files:
                    raise ValueError(f"Link/duplicate ZIP entry: {name}")
                files[name] = archive.read(entry)
    if len({name.casefold() for name in files}) != len(files):
        raise ValueError("Windows case-insensitive path collision")
    folded = {name.casefold() for name in files}
    directories = {}
    for name in files:
        for parent in PurePosixPath(name).parents:
            if str(parent) == ".":
                continue
            key = str(parent).casefold()
            if key in folded or (key in directories and directories[key] != str(parent)):
                raise ValueError(f"File/directory or directory-case collision: {name}")
            directories[key] = str(parent)
    manifest = json.loads(files.pop("package-manifest.json").decode("utf-8-sig"))
    if manifest["schema_version"] != 1 or manifest["target"] != "x86_64-pc-windows-msvc":
        raise ValueError("Unsupported package manifest")
    listed = {}
    for entry in manifest["files"]:
        name = checked_path(entry["path"])
        if name.casefold() in {p.casefold() for p in listed}:
            raise ValueError(f"Duplicate manifest entry: {name}")
        listed[name] = entry
    if set(files) != set(listed):
        raise ValueError(f"Payload mismatch: missing={sorted(set(listed)-set(files))}, extra={sorted(set(files)-set(listed))}")
    for name, data in files.items():
        entry = listed[name]
        if len(data) != entry["bytes"] or hashlib.sha256(data).hexdigest() != entry["sha256"]:
            raise ValueError(f"Hash/size mismatch: {name}")
    required = {"ascii-material-room.exe", "ascii-engine-lab.exe", "author.exe", "LICENSE", "PORTABLE_PACKAGE.md", "probe-material-room.ps1", "Start-MaterialRoom.cmd", "Start-Laboratory.cmd", "Probe.cmd", "projects/starter.project.json", "third-party-licenses/manifest.json"}
    if not required <= files.keys():
        raise ValueError(f"Missing required payload: {sorted(required-files.keys())}")
    project = json.loads(files[manifest["starter_project"]].decode("utf-8-sig"))
    if set(project) != {"version", "scene", "updates", "camera_controls"} or project["version"] != 1 or project["scene"]["version"] != 2 or project["updates"]["version"] != 1:
        raise ValueError("Unexpected embedded project schema")
    if set(project["camera_controls"]) != {"first", "third", "overhead"}:
        raise ValueError("Missing per-view project camera controls")
    for name in ("Start-MaterialRoom.cmd", "Start-Laboratory.cmd", "Probe.cmd"):
        text = files[name].decode("utf-8-sig")
        if "%~dp0" not in text or re.search(r"(?i)\b(cargo|rustc|python|node)(?:\.exe)?\b|[a-z]:[\\/]", text):
            raise ValueError(f"Launcher has build/absolute-path dependency: {name}")
    license_manifest = json.loads(files["third-party-licenses/manifest.json"].decode("utf-8-sig"))
    if not license_manifest:
        raise ValueError("Empty dependency notices")
    for dependency in license_manifest:
        if not dependency["files"]:
            raise ValueError(f"Missing reviewed dependency notice: {dependency['name']}")
        for notice in dependency["files"]:
            name = checked_path(f"third-party-licenses/{dependency['name']}-{dependency['version']}/{notice.replace(chr(92), '/')}")
            if name not in files:
                raise ValueError(f"Missing notice: {name}")
    imports = {name: pe_imports(files[name], name) for name in ("ascii-material-room.exe", "ascii-engine-lab.exe", "author.exe")}
    return {"status": "PASS", "payload_files": len(files), "source": manifest["source"], "license_packages": len(license_manifest), "pe_imports": imports, "limits": "Integrity/schema/architecture only; no runtime or second-host validation."}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("package", type=Path)
    args = parser.parse_args()
    try:
        print(json.dumps(check(args.package), indent=2))
    except (OSError, ValueError, KeyError, TypeError, struct.error, zipfile.BadZipFile) as error:
        parser.exit(1, f"Portable package check FAILED: {error}\n")
