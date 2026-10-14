# R12 verification summary

14 October 2026; baseline engine `9866c35`. Full contract, method and limits:
[GEOMETRY_COVERAGE.md](../../GEOMETRY_COVERAGE.md).

- Opt-in Conservative9: nine fixed primary queries, one shaded semantic hit,
  coverage-aware history. Center remains default and bitwise compatible.
- 292 workspace/all-target test executions PASS, including frozen historical
  copies; fmt, clippy `-D warnings` and release build PASS.
- Frozen R11 camera/compositor source oracles unchanged; original scene data,
  analytic producer, lighting contracts and GPU shader unchanged.
- 72 legacy GPU PNGs identical (v1/v2/courtyard/environment, Vulkan and DX12).
  Twelve coverage-image backend pairs identical across both policies/three views.
- Dense audits: 39 scene/view samples at 80x45 with 16x16 reference and 32x32
  convergence at 0/3/6 seconds; nine at 240x135 with 8x8 and 16x16 convergence.
  Both policies checked; zero fresh/history mismatches, no target border hits.
- Missing target object-samples at 80x45: Center 56/45/26, Nine 23/2/5 in
  first/third/overhead. At 240x135, three sparse samples/view: 4/0/1 versus 0/0/0.
  Foreground improvement accompanies increased excess support and loss of small
  background slivers; finite reference and temporal sampling are not exact truth.
- 36 CPU-only cases, each 120 measured frames plus twelve warmups, fixed R11 v2
  scene at 240x135. Three interleaved repeats for both policies and 1/8 workers.
  Eight-worker median p50 Center 4.43/4.96/5.25 ms; Nine 14.84/16.57/15.67 ms.
  p95 >16.7 ms: 0/9 Center, **8/9 Nine**, worst Nine 31.91 ms. All serial cases
  exceed that budget. Earlier R11 timing failures remain documented.
- Separate frozen R11/R12 default comparison: 18 synchronized-GPU cases,
  eight workers. Median p50 R11 4.33/4.29/5.02 versus R12 4.61/4.19/5.13 ms.
  No uniform speedup or zero-overhead claim; all p95 below16.7 in this session.
- Six real coverage windows, 1920x1080, 240x135, eight workers and six seconds,
  with zero interactive configuration changes; original courtyard also ran.
  Callback p50 Center about10 ms versus Nine about20–21 ms. CPU and host
  acquire/submit/present intervals separate; not GPU timestamps or scanout time.
- 1,080 native wgpu frames, Center then Nine per view, 36 seconds at simulated
  30 Hz/1080p. Video frame count and complete decode PASS. Review of sampled
  stills shows recovered rails and thicker/stair-stepped edges, not final art.

Raw JSON/CSV/logs, binaries, the zero-specular pilot, PNGs and video remain local
under `evidence/r12`; excluded from publication. One Windows x64 host, Ryzen7
3800X/RX6700XT, non-isolated activity. No new physical-machine validation or FPS
guarantee. The bounded R12 option is verified; the general geometry/motion/cost
gate remains open. Next: measured query traversal with output/quality preserved.
