# R16 — reusable visual capacity experiment

15 October 2026. **Bounded experiment complete; visual ambition not met.** Baseline R15
`e15adc683860d18960de4b80a9d3a9c7ab82ccae`, clean master/tracking/remote verified.
This contract precedes implementation. Gameplay remains paused.

## Diagnosis and hypothesis

R5 first/third/overhead frame-0000 captures were inspected locally. The figure
is recognizable but the cape dominates, marks repeat in columns and clothing
separation is weak. These are observations, not an aesthetic approval. No private
reference image was found by the initial workspace reference-name search; no
technique is inferred from earlier descriptions of screenshots.

R5 frame-zero metadata confirms subject bounds of 34x62, 50x95 and 49x78 cells
(first/third/overhead), with 1,108/2,861/2,483 center-ray figure hits. The cape
accounts for 277/2,365/1,840 of those hits. These old views have different framing;
they are diagnostic evidence, not a matched comparison for the new figures.

Content: the 27-part figure and pose are hardcoded. Representation: ellipsoids
and one procedural sheet constrain authored cut and silhouette. Glyph selection:
repeated region grammars dominate broad surfaces. Sampling: even Conservative9
selects one representative hit and one ASCII glyph per cell; it cannot encode
every surface inside that cell. Lighting: darkness and sparse ink can conceal
geometry that is present. Cost: extra parts, sheets and primary samples add work;
R13's rejected acceleration remains rejected.

Hypothesis: separately authored garment panels and oriented body components,
with persistent local coordinates and explicit feature scale, improve clothing
separation without changing the renderer. This does not assume ellipsoids/sheets
are a universal solution or that the target can be achieved by content alone.

## Bounded content and implementation contract

Original custodian: human proportions, defined head/neck, shoulders, separated
arms, belt, boots, two front coat tails and a back panel; no copied character.
Courier variant changes shoulder/body proportions and coat cut/length, not only
color. JSON definitions instantiate the same producer. Maximum 48 oriented
ellipsoids and six bounded analytic graph panels; no meshes or importers.
Panels have tapered width, folds and bounded procedural sway; no cloth physics,
self-collision, arbitrary overhangs or general clothing construction claimed.

Producer lives in `examples/support/authored_figure.rs`, assets in
`examples/assets/figures/`. Core and existing clients remain unchanged.
Host `examples/visual_capacity.rs` uses the existing Scene/Composer/Gpu APIs.
The environment is a plain floor, fixed point light and bounded constant ambient
term, without texture noise. R5 is a diagnostic baseline under the same host
lighting/material override, not a claim of identical workload.

## Frozen comparison and budget

240x135 cells, 1920x1080 output, existing 8x8 ASCII atlas, eight workers;
Center primary baseline and separately labelled Conservative9 comparison.
Three observers: eye-height first, third and overhead. Near/middle/far framing
targets approximately 80/48/24 subject cells in height; record actual geometric
bounds and occupied cells per view rather than assert identical projection.
No density adaptation. Camera/pose/light parameters saved in run metadata.
Static, figure rotation, camera displacement and deformation use separate
clocks; 60 frames at simulated 30 Hz per initial path, longer comparisons only
if needed. Same snapshot supplies primary, shadow and reflection queries.

Passes: uniform clay (shape), structured surface (only necessary seam/hem/fold
marks and material differences). Compare at the same distance/view/time/light.
CPU-only budget: exploratory p95 <=16.7 ms, eight workers, fixed density,
three serial repetitions of 120 measured frames after 12 warmup. Record failures,
snapshot and composition separately; submission/wait is not GPU time. Native
windows and GPU captures are separate from CPU measurements and run serially.

## Exit criteria and decision

- Reusable data path and a changed-cut variant work without artwork in core.
- At near/middle scales, head/neck, arm gaps, belt, split hem and boots have
  observable geometric support in relevant views. At far scale, judge silhouette
  and variant identity, not microseams. Record losses rather than inventing detail.
- Static state is deterministic; deformation and rotation preserve local identity;
  region/LOD transitions release history. No clipping or unreported solver holes.
- Inspect GPU images from all three views and distances and motion sequences;
  separately judge silhouette, detail, character identity and temporal continuity.
  Backend parity alone does not certify artwork; user aesthetic approval is pending.
- Legacy tests/captures remain unchanged. Run fmt, workspace/all-target tests,
  clippy and release, new producer behavior tests and real native windows.
- If shape/detail or budget fail, retain the experiment and identify the limiting
  layer. A working data loader is not closure of general visual capacity.

Second physical host remains unavailable/unverified and does not block this
experiment. No completion percentage, guaranteed FPS or site deployment follows.

## Pilot calibration, before the final matrix

The first Vulkan near captures and JSON remain in `evidence/r16/pilot-clay`
and `pilot-surface`. The initial linear illumination-to-ink ramp selected mostly
dots, making shape judgment confounded by very sparse ink. Clay now uses constant
`o` with ordinary RGB shading and darkness cutoff unchanged. Structured density
thresholds are 0.035/0.12 instead of 0.28/0.65, retaining seam/hem classification.
Light, geometry, cell grid and camera are unchanged by this calibration. This is
an ink-coverage correction, not evidence of richer geometry or increased lighting.

## Result and independent visual review

The reusable producer and changed-cut variant work, without changes to the shared
renderer or existing clients. Both definitions contain 29 oriented ellipsoids and
three analytic panels. Runtime JSON is the source; see the bounded schema and
commands in [Authored figures](docs/AUTHORED_FIGURES.md).

**The visual exit criterion fails.** Near clay improves the neck, arm gaps and
head proportions compared with the matched R5 baseline. Courier changes the
shoulder masses, jacket length and side bag, rather than only the palette.
Structured surfaces separate clothing colors, but repeated columns of marks
still flatten the volume. Boots, hands, split hems and convincing folds do not
remain reliably legible at middle distance across views. Third/middle is the
weakest view: dark clothing merges with the dotted floor. Far/first retains
the long cool warden versus the shorter warm courier silhouette, while small
features disappear. No semirealistic quality or general visual capacity is proved.

The coordinator inspected static views at all distances and selected rotation,
camera and deformation frames. An independent reviewer opened ten final PNGs:
matched near-first baseline/warden clay; near-first structured figures;
middle-third, far-first and near-overhead structured pairs. The observations
agree. These are sampled-frame reviews, not a claim of continuous playback
inspection or user aesthetic acceptance. Backend parity is technical evidence only.

Actual center-ray subject heights (first / third / overhead), in cells:

| Figure | Near | Middle | Far |
| --- | --- | --- | --- |
| Baseline | 77 / 76 / 53 | 46 / 45 / 32 | 22 / 22 / 16 |
| Warden | 73 / 70 / 48 | 44 / 42 / 29 | 21 / 21 / 14 |
| Courier | 69 / 66 / 47 | 42 / 40 / 27 | 21 / 20 / 14 |

Nominal 80/48/24 is a framing target, not a measurement. Overhead foreshortening
and different body heights are retained. The geometry counter uses center rays;
it does not describe Conservative9 coverage. Actual nonspace counts include the
floor and must not be used as a character-quality score.

## Measured CPU cost

54 cases, 6,480 measured frames, three serial repeats, 120 measured frames after
12 warmup per view. Values are snapshot plus composition, with no PNG writing,
GPU submission or geometry diagnostics in the timed region. One non-isolated
physical host; these are not an FPS guarantee or cross-machine comparison.

| Figure / sampling | Median of three p50, first / third / overhead (ms) | Worst p95 (ms) | Runs p95 >16.7 ms |
| --- | --- | --- | --- |
| Baseline / Center | 2.469 / 3.442 / 4.314 | 4.806 | 0/9 |
| Warden / Center | 4.783 / 5.212 / 6.838 | 7.704 | 0/9 |
| Courier / Center | 5.038 / 5.227 / 6.388 | 7.335 | 0/9 |
| Baseline / Conservative9 | 8.231 / 10.485 / 10.400 | 11.384 | 0/9 |
| Warden / Conservative9 | 26.410 / 29.720 / 30.684 | 32.539 | 9/9 |
| Courier / Conservative9 | 25.200 / 29.382 / 30.425 | 32.513 | 9/9 |

New-figure snapshot worst p95 is 0.0039 ms. Most cost is composition/query work.
The baseline has different representation and surface work: this table is a
workload comparison, not an isolated attribution to one algorithm. Center stays
default. R13's discarded runtime acceleration was not reactivated.

## Limits and next decision

Content alone improves separability but does not deliver the desired richness.
The experiment identifies a joint bottleneck in glyph vocabulary, lighting/
background contrast and available cells; it does not isolate a single cause.
The current marks classify normalized borders/hems, not fold-directed strokes.
`feature_scale` is a manual world-space cutoff, not a measured seam width.
Graph panels are zero-thickness, two-sided surfaces with a bounded approximate
root search (32 brackets, eight bisections); grazing/tangent roots can be missed.
No dense reference solver or universal hole-free geometry claim is made.

The next proposed question is a controlled ablation of contrast/background,
glyph direction/density and feature scale on these frozen figures, before adding
more parts or optimizing the failed Nine workload. This is a proposal, not an
automatic new tranche. General coverage/motion/cost, visual quality and second-
machine portability remain open. Gameplay stays paused.

## Verification and deliverables

- 412 workspace/all-target test executions passed; the final focused suite has
  13 tests, including an additional end-to-end region/history transition test
  with a negative control that suppresses the key. It was added after the full
  suite; do not describe this as a 413-test full-suite run.
- Formatting, clippy and release bins/examples passed. The first full-suite
  sandbox run failed on the existing R15 atomic-save rename permission; the
  identical command passed with escalation. Clippy's only environmental warning
  was an incremental-cache hard-link fallback, not a code diagnostic.
- 48 legacy PNGs are byte-identical to frozen R15 binaries (Material Room v1,
  v2, dynamic updates and the courtyard). No existing core/client code changed.
- 96 static and 2,160 motion PNGs were generated. All 1,125 comparable Vulkan/
  DX12 pairs are byte-identical; six additional static Nine PNGs are separately
  labelled. No subject center hit touches the image border in these 2,256 records.
- Six four-second native runs completed 412/413 frames each with final actual
  size 1920x1080. Each records two configuration changes. The host did not log
  event sizes/timestamps, so startup resize events cannot be distinguished from
  changes during rendering. Fixed-size native timing is therefore **not proven**;
  use the independent CPU matrix for the budget. The summary retains this
  limitation instead of silently treating the windows as stable configurations.
- An 18-second comparison video contains 540 captured frames, three views and
  separate rotation/camera/deformation paths, with both figures side by side.
  Side-margin cropping and labels only: no scaling, recoloring or interpolation.
  Playback is a simulated 30 Hz presentation, not measured real-time FPS.

Commands and binary hashes, raw metadata, captures and logs are local under
`evidence/r16/`; the [tracked evidence summary](evidence/r16/summary.md) records
the result without publishing generated media. `scripts/verify-r16.py --phase`
selects legacy/static/motion/cpu/windows/summary; `scripts/encode-r16.py` assembles
the captured video. Run performance phases serially. Double-click
`Avvia-capacita-visiva.cmd` for the native example, or follow the authoring guide.
