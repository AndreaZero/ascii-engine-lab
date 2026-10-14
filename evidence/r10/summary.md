# R10 verification summary

Full methodology, commands, visual observations and limits: [ENVIRONMENT_STUDY](../../ENVIRONMENT_STUDY.md).

- 173 workspace/all-target test executions passed (including reused fixtures);
  fmt, clippy and release examples passed. One unused import fixed before final build.
- All 12 metal and 18 stone legacy PNG pairs match R9; 12 common-scene PNG pairs
  match across Vulkan/DX12. Window run passed. These checks concern one PC/GPU.
- Matrix: 72 cases / 5,400 frames, 3 grids × 3 views × 4 paths × off/on.
  Static frames unchanged, all three subjects present, no subject border hits.
- Video: 540 wgpu PNGs, 18 s, 1920×1080, simulated 30 Hz; full decode passed.
  Only labels added. Capture cadence is not measured runtime FPS.

| CPU median of 3 p50 runs, 240×135, 8 workers | First | Third | Overhead |
| --- | ---: | ---: | ---: |
| Environment off | 4.45 ms | 4.14 ms | 4.02 ms |
| Environment on | 6.34 ms | 7.36 ms | 7.55 ms |

No 8-worker CPU run exceeded p95 16.7 ms (worst on p95 15.25 ms). One of 72
matrix cases exceeded it (off, 18.50 ms). No motion case exceeded it (worst
13.71 ms). Serial on-path medians 37.74–46.23 ms; worst serial p95 107.82 ms.
Host load was not isolated, no FPS guarantee or hardware minimum inferred.

Process working set: CPU-only 18.21–19.27 MiB, matrix 248.86–256.04 MiB,
motion 271.90 MiB. This is not dedicated GPU memory.

The cool environment response is visible, especially on metal. Remaining limits:
coarse diffuse visibility, one-ray rough reflections, dark bands/lower shield,
regular glyphs and demonstrative actor silhouette. Reviewed captures do not show
a clear mirror image of another subject; bounded reflection behavior is unit
tested. Static equality and glyph-change counters do not certify perceptual
stability. Full GI, final art, multiple hardware validation and data-driven
consumption of the new API by Material Room remain open.

Raw local evidence: `summary.json`, `runs-*.json`, tests/build logs, per-case
JSON/CSV and PNGs, original R9 executables/source archive and encoded video.
Those generated files are excluded from source control.
