"""Compact, publishable R13 results; run verify-r13.py --phase summary first."""
import json
from pathlib import Path
import statistics

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "evidence/r13"
summary = json.loads((OUT / "summary.json").read_text(encoding="utf-8"))
audit = json.loads((OUT / "query-audit/query-audit.json").read_text(encoding="utf-8"))
views = ("first", "third", "overhead")
assert audit["grid"] == [240, 135] and audit["workers"] == 1 and audit["stage"] == "light"
assert audit["times_s"] == [0, 3, 6] and not audit["synthetic_stress"]
assert audit["path"] == "coverage_camera: fixed six-second combined motion"
assert audit["audit"] == "r13_bounded_experiment_query_work"
assert audit["measured_query_path"] == "bounded_experiment"
assert audit["comparison_query_path"] == "runtime_r12_linear" and not audit["ordinary_runtime_counters"]
scene_files = {"packaged-v1": "material-room.json", "packaged-v2": "material-room-v2.json", "coverage-v2": "coverage-room-v2.json"}
assert len(audit["scenes"]) == 3 and {s["source_label"] for s in audit["scenes"]} == set(scene_files)
for scene in audit["scenes"]:
    assert scene["input_scene"] == json.loads((ROOT/"clients/material-room/assets"/scene_files[scene["source_label"]]).read_text(encoding="utf-8"))
    assert {(r["camera"], r["sampling"], r["time_s"]) for r in scene["records"]} == {
        (v, q, t) for v in views for q in ("center", "conservative9") for t in (0, 3, 6)}
lines = ["# R13 compact evidence", "",
    "Paired R12/bounded-experiment binaries, unchanged scene JSON, coverage camera path, 240x135 cells.",
    "The bounded path was not adopted as the runtime default. Final Material Room retains R12 traversal.",
    f"Experiment source: `{summary['candidate_source_commit']}`.",
    "CPU-only, 12 warmups + 120 frames per view, three interleaved repetitions.",
    "Single non-isolated Windows host; these numbers are not FPS or GPU timestamps.", "",
    f"Experiment PNG equality to R12: {sum(summary['baseline_png_equal'].values())}; backend pairs: {sum(summary['backend_png_equal'].values())}.",
    f"Final default PNG equality to R12: {sum(summary['default_png_equal'].values())}.", "",
    "## CPU snapshot/camera/composition", "",
    "Median of three p50 values (ms); p95 failures count individual runs above 16.7 ms.", "",
    "| Scene | Sampling | Workers | Revision | First | Third | Overhead | p95 >16.7 | Worst p95 |",
    "| --- | --- | ---: | --- | ---: | ---: | ---: | ---: | ---: |"]
for scene in ("v2", "coverage"):
    for sampling in ("center", "conservative9"):
        for workers in (1, 8):
            for revision in ("r12", "r13"):
                rows = [m for m in summary["measurements"]
                    if m["run"].endswith(f"{scene}-{sampling}-w{workers}-{revision}")]
                assert len(rows) == 9
                medians = [statistics.median(m["cpu_ms"]["p50"] for m in rows if m["camera"] == v) for v in views]
                failures = sum(m["cpu_ms"]["p95"] > 16.7 for m in rows)
                worst = max(m["cpu_ms"]["p95"] for m in rows)
                label = "R12" if revision == "r12" else "bounded experiment"
                lines.append(f"| {scene} | {sampling} | {workers} | {label} | "
                    + " | ".join(f"{n:.3f}" for n in medians) + f" | {failures}/9 | {worst:.3f} |")
lines += ["", "Snapshot timer p50 ranges across the 72 runs of each revision (ms):", ""]
for revision in ("r12", "r13"):
    values = [m["snapshot_ms"]["p50"] for m in summary["measurements"] if m["run"].endswith(revision)]
    label = "R12" if revision == "r12" else "bounded experiment"
    lines.append(f"- {label}: {min(values):.6f} to {max(values):.6f}.")
lines += ["", "## Untimed bounded-experiment work", "",
    "These counters describe the explicit bounded diagnostic, not the final default traversal.", "",
    "Totals per scene across three views, both sampling modes, t=0/3/6. R12 primitive/rotation counts are derived N*calls, not instrumented timings.", "",
    "| Scene | Category | Calls | R12 derived kernels | Actual kernels | Actual direction transforms | Full Hit builds | Candidate hits | Box axes | Unbounded axes, visited boxes |",
    "| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |"]
for scene in audit["scenes"]:
    assert len(scene["records"]) == 18
    assert all(r["bounded_vs_runtime_cells_bitwise_equal"] and r["bounded_vs_runtime_frame_stats_bitwise_equal"] for r in scene["records"])
    for category, total in scene["totals_across_all_18_records"].items():
        fields = ("calls", "derived_r12_full_scan_primitive_tests", "primitive_tests", "direction_transforms", "full_hit_builds", "primitive_hits", "box_axes_examined", "box_axes_unbounded_visited")
        lines.append(f"| {scene['source_label']} | {category} | " + " | ".join(str(total[k]) for k in fields) + " |")
lines += ["", "Primary/secondary still visit every object. Box axes include parallel-axis checks, not division counts. The unbounded comparison only covers the same visited boxes; shadow early exits hide later candidates, whose previous work cannot be inferred. Counts are not elapsed-time percentages.",
    "", "## Bounded-experiment window callback pacing", "",
    "Six seconds per case, 12 warmups, unchanged configuration, requested/actual 1920x1080. Wall-clock callback intervals are not GPU time or scanout latency.", "",
    "| View | Sampling | Frames | Interval p50 ms | Interval p95 ms | CPU p50 ms |",
    "| --- | --- | ---: | ---: | ---: | ---: |"]
for row in summary["windows"]:
    lines.append(f"| {row['camera']} | {row['primary_sampling']} | {row['frames']} | {row['frame_interval_ms']['p50']:.3f} | {row['frame_interval_ms']['p95']:.3f} | {row['cpu_snapshot_and_compose_ms']['p50']:.3f} |")
lines += ["", "## Final default window callback pacing", "",
    "Same window protocol, separately verified current executable. These are not the paired CPU-only experiment measurements.", "",
    "| View | Sampling | Frames | Interval p50 ms | Interval p95 ms | CPU p50 ms |",
    "| --- | --- | ---: | ---: | ---: | ---: |"]
for row in summary["default_windows"]:
    lines.append(f"| {row['camera']} | {row['primary_sampling']} | {row['frames']} | {row['frame_interval_ms']['p50']:.3f} | {row['frame_interval_ms']['p95']:.3f} | {row['cpu_snapshot_and_compose_ms']['p50']:.3f} |")
lines += ["", "Rejected deferred-hit/direction-cache variants and their negative measurements remain separate local evidence. No pilot is pooled with final measurements. Raw JSON/CSV, binaries and generated media are excluded from Git.", ""]
target = OUT / "summary.md"
target.write_text("\n".join(lines), encoding="utf-8")
print(target)
