# R12 — bounded primary coverage, motion and measured cost

14 October 2026. R12 addresses a reproduced gap: a single ray at the center of a
character cell can miss a thin object even when it is in front of a visible
background. Material footprint filtering does not query the missing geometry.
The reusable result is an explicit visibility option, not another material study.

## Contract and implementation

`frame::PrimarySampling::{Center, Conservative9}` belongs to `Composer`.
`Center` remains the default. `set_primary_sampling` invalidates history only
when the policy changes, retaining its worker pool and buffers.

Conservative9 queries the center and eight fixed offsets at -1/3, 0, +1/3 cell
units on each axis. Every cell attempts nine primary queries, including sky.
The smallest finite positive camera-forward depth selects an object/material
group; exact ties use object ID, material ID and sample index. If the center
belongs to that group it remains the representative. Otherwise the winning
subcell hit supplies the position, normal, material, glyph and shading.

Only that one semantic hit is shaded. There is no rendered world image, RGB
average, new mesh pipeline or per-sample color blend. Shadow, environment and
water reflection algorithms are unchanged and remain single-ray approximations.
The material footprint still spans a whole cell, with neighboring rays offset
consistently with the selected sample. Geometry inside reflections is not given
the nine-ray primary policy.

History in the optional path additionally checks primary object/material,
sample index and the winning group's nine-bit sample mask. This releases held
glyphs on coverage transitions even inside the same quantized surface bin.
The old Center history behavior and output bits are protected by frozen R11
camera/compositor oracles. `FrameStats.primary_rays` counts attempted primary
queries; `alternate_primary` counts cells represented off-center. Neither is
an area percentage. `FrameStats.reflected` still counts only legacy water.

`Camera::ray_sample` is also available for explicit subcell queries, validating
offsets in [-0.5,0.5], positive grid dimensions and finite positive aspect.
Zero offset is bitwise identical to `ray`. APIs remain experimental 0.1.0.

## Fixed workloads and reproduction

The original R11 `material-room-v2.json` is unchanged and remains the cost
workload. The separate `coverage-room-v2.json` has twelve analytic objects:
floor and backdrop, five vertical rods of total widths 0.015/0.03/0.06/0.12/0.24
world units, three horizontal rails, an ellipsoid and a moving panel. Each has
a unique fixed ASCII glyph, so diagnostics can identify actual output ownership
without repeating the production winner algorithm. There is no new parser,
content format or fixture import.

The diagnostic scene uses constant irradiance, specular radiance 0.1 and zero
direct-light color/local environment range. The small specular term keeps
grazing surfaces above the glyph darkness cutoff, isolating geometric selection.
An initial zero-specular pilot is retained locally: it could count a dark blank
as missing geometry. This diagnostic adjustment does not alter the R11 workload,
the renderer's light model or the original scene's appearance.

`coverage_study::coverage_camera` is shared by the client and audit: a fixed
six-second camera path combines horizontal motion, a retreat of up to 0.9 world
units and a small orbit; overhead additionally widens its extent by up to 25%.
All views observe the same immutable scene tick. The camera path is periodic;
the panel's period is 2*pi seconds, so the complete world is not a six-second
loop. A 180-frame sequence at 30 simulated Hz ends at 5.9667 seconds.

```sh
cargo build --locked --release --workspace --bins --example coverage_audit --example environment_study
cargo run --locked --release -p ascii-material-room -- --scene clients/material-room/assets/coverage-room-v2.json --sampling conservative9 --camera-path coverage --profile detail
cargo run --locked --release -p ascii-material-room -- --scene clients/material-room/assets/material-room-v2.json --cpu-only --camera all --frames 120 --profile detail --workers 8 --sampling conservative9 --output evidence/my-coverage-cost
cargo run --locked --release -p ascii-material-room --example coverage_audit -- --output evidence/my-coverage-audit
```

Windows launcher: [Avvia-copertura.cmd](Avvia-copertura.cmd) rebuilds before
opening the diagnostic scene. `C` switches sampling with history reset, `P`
toggles the scripted camera, `1/2/3` changes view. The existing alpha launcher
and all default clients still use Center. More controls and CLI details are in
the [client guide](clients/material-room/README.md).

`scripts/verify-r12.py` serializes regression, CPU-only, frozen-binary cost,
real-window and motion phases. `scripts/encode-r12.py` assembles native wgpu
frames with labels only. Baseline binaries and raw evidence are local and are
not required to build or run a fresh checkout. No generated media is published.

## Acceptance and limits

The test suite covers thin geometry in front of a background, diagonal edges,
full occluders, foreground depth rather than oblique ray length, exact ties,
disocclusion/removal, changing samples and coverage masks, full-cell footprint,
static/reset behavior, worker determinism and exact Center compatibility.
It deliberately includes geometry missed even by nine samples and silhouette
expansion, so these limits cannot silently become universal coverage claims.

The independent dense audit queries a 16x16 midpoint lattice per cell and
compares 32x32 at the first/middle/last sample. It uses the same analytic producer
but a different lattice and no production selection algorithm. It verifies
sampling against a finite reference, not primitive intersection correctness or
exact geometric area. Actual output ownership comes from the authored glyphs.
Missing/excess areas are fractional cell support versus binary output ownership,
not font ink, perceptual brightness or an artistic quality score. A selection
without dense support is a finite-reference discrepancy, not automatically a
visibility leak. Floor/backdrop are kept separate from the ten target objects.

Conservative9 can retain thin foreground at the cost of wider or stair-stepped
silhouettes. It can still miss features between samples, favors foreground over
majority area, and increases primary query work everywhere. It is opt-in; there
is no adaptive density reduction or automatic promotion to a new default.
General anti-aliasing, reflection coverage, arbitrary geometry robustness and
production frame budgets remain open. Measurements below do not establish
minimum hardware requirements or a promise of realtime performance.

## Verification results

Source compilation, the complete 292-execution test suite and all-target clippy
passed. This count includes frozen historical test copies and is not a count
of new features. Both oracle files are byte-for-byte source copies of R11.

| Verification | Result |
| --- | --- |
| fmt, locked workspace/all-target tests, clippy `-D warnings`, release | PASS; 292 test executions including frozen copies |
| Frozen R11 default/Center output | 72 PNGs byte-identical: v1 12, v2 12, courtyard 24, environment 24; Vulkan and DX12 |
| Coverage scene backend parity | 12 corresponding PNG pairs identical across Vulkan/DX12, both sampling modes and three views |
| Dense geometry audits | 39 low-grid and 9 capture-grid scene/view samples, each evaluated with both sampling modes; history and target framing checks PASS |
| Cost matrix | 36 CPU-only cases plus 18 frozen-binary synchronized-GPU cases, failures retained below |
| Windows | Six coverage windows plus courtyard rendered successfully; pacing measured separately |
| Motion/media | 1,080 native wgpu PNGs, six clips assembled as 36 s at simulated 30 Hz, 1920x1080; frame count and full video decode PASS |

Local video: `evidence/r12/video/copertura-confronto-tre-visuali.mp4`, Center then
Nine for each of first/third/overhead. Visual review sampled the static views,
midpoint comparisons in both modes and final Nine frames. It shows recovered
rails/rods and visibly thickened or stair-stepped edges, with deliberately plain
diagnostic letters. It is not an exhaustive perceptual review of every frame,
final artwork or evidence of a 30 FPS runtime budget. Raw logs, captures and
initial pilot results remain local; [the compact summary](evidence/r12/summary.md)
records the public results.

The initial integration needed the frozen oracle's existing rayon/bytemuck
packages declared as client dev-dependencies; no package versions changed.
One test byte-slice style lint was corrected. Existing incremental-cache hardlink
warnings fall back to copying; no remaining code lint was accepted or suppressed.

### Geometry and history

At 80x45 cells, thirteen inclusive samples from 0 to 6 seconds use the 16x16
reference, with 32x32 convergence at 0/3/6 seconds. The ten target objects are
reported separately from floor/backdrop. Values below sum over target objects
and audit samples, not consecutive animation frames or perceptual flicker.

| View | Missing object-samples, Center / Nine | Missed area, Center / Nine | Excess area, Center / Nine |
| --- | ---: | ---: | ---: |
| First | 56 / 23 | 309.56 / 122.79 | 290.67 / 766.90 |
| Third | 45 / 2 | 223.39 / 80.67 | 182.19 / 628.47 |
| Overhead | 26 / 5 | 216.26 / 57.77 | 279.88 / 714.39 |

These are cell-area-samples. Missing object-samples require positive finite
reference area and zero output cells. Nine improves aggregate foreground support
but does not dominate every object: the partly occluded ellipsoid disappears
in 1 versus 0 first-view samples and 2 versus 1 third-view samples. Foreground
priority can erase a small visible background sliver. This is an explicit cost
of conservative silhouette expansion, not stale temporal history.

At the actual capture grid, 240x135, an additional three samples (0/3/6 seconds)
use 8x8 reference and 16x16 convergence. Center misses 4/0/1 target object-samples
in first/third/overhead; Nine misses 0/0/0 in these sparse samples. Missed area
changes from 218.30/171.95/182.36 to 83.88/34.97/37.39, while excess area increases
from 190.06/147.53/179.41 to 590.64/458.55/481.44. The finer display grid and sparser
reference are explicitly different conditions, not a silent change to rendering
quality or proof that all animation frames retain every object.

Across both audits, current-history and fresh composition match exactly for
all sampled cells and both modes. Target objects have no border hits; floor
and backdrop may extend outside the view. The largest per-object summed absolute
16-to-32 lattice difference in the 80x45 audit is 1.28 / 0.32 / 0.58 cells across
the three views. Relative error can matter for very small objects. Dense references
remain finite and their per-object convergence data are retained.

### Query work and real windows

A separate untimed counter wrapper forwards every local-material/environment
hook and verifies its output equals the unwrapped producer. It counts primary,
combined secondary trace and occluded calls separately. At 80x45 and t=3, the
unchanged R11 scene attempts 3,600 versus 32,400 primary rays. Total primitive
intersection attempts from the current full object scan are 73,720 / 72,176 /
91,072 with Center and 306,064 / 304,080 / 322,920 with Nine. These counts explain
added work; they do not assign equal cost to different primitives or measure GPU
time. No spatial acceleration or specialized shadow traversal was added.

Real 1920x1080 windows ran for six seconds per view and sampling mode, with
8 workers, 240x135 cells, fixed path, zero interactive configuration changes and
the first twelve frames excluded from timing. FIFO callback interval p50/p95:

| View | Center milliseconds | Nine milliseconds |
| --- | ---: | ---: |
| First | 10.00 / 10.51 | 20.84 / 22.61 |
| Third | 9.99 / 10.52 | 19.95 / 21.57 |
| Overhead | 9.99 / 10.74 | 19.95 / 21.78 |

Windows rendered 596/597/596 Center frames and 286/302/299 Nine frames. These are
observed callback/presentation pacing on this desktop, including scheduling;
they are not GPU timestamps or scanout measurements. CPU composition and
acquire/submit/present wall time are reported separately. The original courtyard
also opened and rendered successfully.

## CPU cost on the unchanged R11 scene

Windows x64, Ryzen 7 3800X / Radeon RX 6700 XT, same physical host as earlier
tranches, Rust/Cargo 1.97.1. Host activity is not isolated. The fixed workload
uses 240x135 cells, stage Light and the original R11 v2 material/environment
data. Three repeats alternate worker and sampling order; each view has twelve
warmup compositions then 120 measured frames at the same simulated ticks.
CPU-only performs no GPU work, captures or dense audits. Timing includes snapshot
and composition, with those phases also recorded separately.

Median of the three p50 values, in milliseconds:

| Sampling / workers | First | Third | Overhead |
| --- | ---: | ---: | ---: |
| Center / 1 | 21.32 | 25.46 | 31.20 |
| Conservative9 / 1 | 99.80 | 101.89 | 99.20 |
| Center / 8 | 4.43 | 4.96 | 5.25 |
| Conservative9 / 8 | 14.84 | 16.57 | 15.67 |

Center exceeds the exploratory p95 16.7 ms budget in 0/9 eight-worker runs;
Conservative9 exceeds it in **8/9**, with worst p95 **31.91 ms**. All one-worker
runs exceed that budget. This is a measured cost increase for additional coverage,
not an optimization or an FPS claim. R11's earlier 4/9 eight-worker exceedances
remain valid recorded failures; a different session and CPU-only scheduling do
not cancel them.

A separate interleaved comparison uses the frozen R11 executable and R12's
default Center on the identical synchronized-GPU workload: 8 workers, three
repeats, 120 frames/view plus twelve warmups, same backend, grid and data.
R11 median p50 is 4.33 / 4.29 / 5.02 ms; R12 is 4.61 / 4.19 / 5.13 ms.
The difference is not uniform across views and does not establish a speedup or
zero overhead. Every p95 in this bounded comparison is below 16.7 ms. Its CPU
numbers are separate from CPU-only results; submission/wait is never labeled
GPU timestamp time.

## Decision and next bounded step

R12's opt-in coverage contract and reproducible comparison are verified in this
scope. The broader geometry/motion/cost gate remains open: sample gaps, foreground
dilation, partially visible background loss and the nine-ray budget exceedances
are demonstrated limitations. Keep Center as the default.

The next engineering step is a bounded query-cost/acceleration comparison on
these same frozen scenes and paths, preserving hit/tie semantics, history and
quality. The current all-object traversal is measured work, not evidence that a
particular acceleration structure is already the solution. Only after that
choice is validated should dynamic occupancy updates extend its lifetime rules.
No new material showcase, universal editor, gameplay or physics is implied.
