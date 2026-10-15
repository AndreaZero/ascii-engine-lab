# R16 local evidence summary

15 October 2026. Baseline R15 `e15adc683860d18960de4b80a9d3a9c7ab82ccae`.
See [the complete report](../../VISUAL_CAPACITY.md) for interpretation and costs.

## Decision

Reusable bounded JSON producer: demonstrated. General visual capacity / desired
richness: **not demonstrated**. Improved silhouette and clothing cut, but weak
middle-third contrast, repetitive glyphs and lost small features. Independent
review opened ten final static PNGs. No aesthetic acceptance or FPS promise.

## Checks

| Evidence | Result |
| --- | --- |
| Workspace all-target suite | 412 passed, escalated identical retry after R15 save-permission failure |
| Final focused R16 suite | 13 passed, including one later history-key negative-control test |
| fmt / clippy / release | PASS; cache hard-link fallback warning only |
| Legacy R15 comparison | 48 PNGs identical |
| Static matrix | 96 PNGs, 3 views and 3 distances, clay/structured; six Nine captures |
| Motion matrix | 2,160 PNGs, 2 figures x 3 paths x 2 backends x 3 views x 60 frames |
| Backend comparisons | 1,125 identical Vulkan/DX12 pairs |
| Subject clipping counter | Zero border hits in 2,256 captured frame records; center-ray diagnostic only |
| CPU matrix | 54 cases, 6,480 measured frames; eight workers; three serial repetitions |
| New figures Center budget | 0/18 runs above p95 16.7 ms; worst p95 7.704 ms |
| New figures Conservative9 budget | 18/18 above threshold; worst p95 32.539 ms |
| Native integration | Six four-second runs, 412/413 frames each, final 1920x1080 |
| Native configuration caveat | Two changes per run; no event timeline, stable-size native timings not proven |
| Video | 540 frames, 18 seconds, 1920x1080; encoded then decoded and counted successfully |

Static and motion metadata preserve source JSON, camera, light, fixed 240x135
grid, worker count, simulation clock, per-frame bounds and counts. Paths:
`static/`, `motion/`, `cpu/`, `windows/`, `legacy/`, `baseline/`, `video/`.
The `runs-*.json` manifests record exact commands, exit codes and binary SHA256;
`summary.json` aggregates the machine-readable result. Raw material is ignored
by Git and remains local. GPU adapter reported AMD Radeon RX 6700 XT; one
physical host only, no second-host portability validation.

Temporal tests verify static determinism, worker parity, snapshot immutability,
local coordinates, feature cutoff and history invalidation. Motion paths produce
matching backend sequences. Selected frames were visually inspected; continuous
playback quality was not certified. Whole-scene temporal counts include floor
and cannot establish perceptual stability of clothing. Approximate panel solver
can miss tangent roots; no dense-reference geometric completeness proof exists.

No shared core or existing client changes; no R13 runtime acceleration, Godot
expansion, mesh importer, universal editor or site publication.
