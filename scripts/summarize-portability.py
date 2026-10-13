"""Aggregate R3 evidence without conflating worker budgets with hardware tests."""
import hashlib
import json
import statistics
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "evidence/r3"


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


baseline = OUT / "baseline"
r2 = json.loads((baseline / "r2-manifest.json").read_text(encoding="utf-8-sig"))
baseline_checks = {}
for name, expected in r2["source_sha256"].items():
    path = baseline / name
    if path.is_file():
        baseline_checks[name] = digest(path) == expected
baseline_checks["ascii-engine-lab-r2.exe"] = (
    digest(baseline / "ascii-engine-lab-r2.exe") == r2["executable_sha256"]
)
assert baseline_checks and all(baseline_checks.values()), baseline_checks

cpu = json.loads((OUT / "cpu-paired/benchmark.json").read_text(encoding="utf-8"))
groups = {}
for run in cpu["runs"]:
    key = run["workers"], run["camera"], run["variant"]
    groups.setdefault(key, []).append(run)
rows = []
for (workers, camera, variant), runs in sorted(groups.items()):
    rows.append({"workers": workers, "camera": camera, "variant": variant,
                 "repetitions": len(runs),
                 "median_run_p50_ms": statistics.median(r["p50_ms"] for r in runs),
                 "median_run_p95_ms": statistics.median(r["p95_ms"] for r in runs),
                 "min_run_p95_ms": min(r["p95_ms"] for r in runs),
                 "max_run_p95_ms": max(r["p95_ms"] for r in runs)})

captures = []
for backend in ("vulkan", "dx12"):
    images = sorted((OUT / backend).glob("*.png"))
    assert len(images) == 12, (backend, len(images))
    for path in images:
        reference = ROOT / "evidence/r2/scaling/w8-720p-r1" / path.name
        captures.append({"backend": backend, "file": path.name,
                         "sha256": digest(path),
                         "identical_r2_vulkan": digest(path) == digest(reference)})

data = {"method": "Median of per-run percentiles; ranges show repeat spread, not confidence intervals",
        "cpu": rows, "baseline_hash_checks": baseline_checks, "captures": captures,
        "query_counts": cpu["queries"],
        "limits": cpu["limits"]}
(OUT / "summary.json").write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")
lines = ["# R3 — snapshot e compatibilità", "", data["method"], "",
         "CPU only, 240×135 celle, Cloth, 120 frame misurati per ripetizione dopo 12 warm-up. "
         "Nessun tempo GPU o FPS finestra. Il riferimento R2 e lo snapshot sono ricompilati nello stesso eseguibile. "
         "Varianti alternate all'interno di ciascun frame, ordine invertito a ogni frame/ripetizione. "
         "Il primo esperimento a blocchi, più sensibile alle oscillazioni di carico, è conservato in cpu/.", "",
         "| Worker | Camera | Variante | Rip. | CPU p50 ms | CPU p95 ms | Intervallo p95 ms |",
         "| --- | --- | --- | --- | --- | --- | --- |"]
for r in rows:
    lines.append(f"| {r['workers']} | {r['camera']} | {r['variant']} | {r['repetitions']} | "
                 f"{r['median_run_p50_ms']:.2f} | {r['median_run_p95_ms']:.2f} | "
                 f"{r['min_run_p95_ms']:.2f}–{r['max_run_p95_ms']:.2f} |")
lines += ["", f"Baseline R2: {sum(baseline_checks.values())}/{len(baseline_checks)} hash preservati.",
          f"Catture GPU identiche byte per byte a R2/Vulkan: {sum(c['identical_r2_vulkan'] for c in captures)}/{len(captures)}.",
          "", "Queste prove usano una sola macchina fisica; pochi worker non emulano CPU meno potenti.", ""]
(OUT / "summary.md").write_text("\n".join(lines), encoding="utf-8")
print("\n".join(lines))
