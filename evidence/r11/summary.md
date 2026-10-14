# R11 verification summary

Implementation `4808d7571f8a286d07dad7eb9fb80ce426ff49db`, 14 October 2026.
Full interpretation and limits: [ALPHA_DATA](../../ALPHA_DATA.md).

- 204 workspace/all-target test executions PASS, including frozen fixtures.
- fmt, clippy `-D warnings`, release build PASS; cache hardlink warning retained.
- 60 old PNGs identical to preserved executables; 6 v2 Vulkan/DX12 pairs identical.
- Data-only edit changes all three views with the same binary.
- Real windows: v2 three modes and original courtyard PASS.
- 540 native GPU PNGs, 18 s / 1080p / simulated 30 Hz; full video decode PASS.
- 21 sampled coverage audits: all seven non-floor subjects visible, no border hits.
- CPU detail 240×135, median of three p50: 1 worker 25.88/24.99/37.48 ms;
  8 workers 10.70/7.30/7.73 ms (first/third/overhead).
- 4/9 runs at 8 workers exceed p95 16.7 ms, worst 36.47 ms. Variability retained.
- Motion CPU p95 9.10/6.29/6.49 ms does not override those failures.
- Clean-source build/run guide PASS at 4808d75, new target with installed toolchain
  and registry cache; both formats validate, v2 renders 180 frames, original client
  captures, three initial v2 PNGs match, Git source remains clean.

One Windows x64 PC, Ryzen 7 3800X / RX 6700 XT, Rust/Cargo 1.97.1. Host not
isolated. CPU snapshot/composition measured inside synchronized GPU runs;
submission/wait is not GPU timestamp time or FPS. Technical visual samples,
constant authored environment and bounded one-bounce transport, no full GI or
final art claim. Raw data and media remain local and ignored by Git.
