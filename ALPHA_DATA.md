# R11 — a bounded alpha through the independent data client

14 October 2026. Material Room now consumes the renderer's local-surface,
conductor and environment contracts from validated client data. This closes a
specific integration gap; it does not declare the renderer production-ready.
Runtime and clean-checkout verification passed within the declared data scope.

## Delivered boundary

Version 1 remains supported and its original sample remains unchanged. Version 2
requires local surface parameters and three authored camera views; conductor
parameters, direct-light color and a constant environment are optional. The
[client guide](clients/material-room/README.md) describes every field, bounds,
error behavior and explicit migration. The file remains a small client format,
not a renderer asset standard or a universal editor format.

The renderer adds validated `CameraView` and `Camera::with_view`. Explicit views
provide eye, target, vertical FOV and orthographic half-height. `Camera::new`
keeps the legacy poses and arithmetic. Camera struct literals now need
`view: None` for the old behavior; this experimental API is not semver-stable.
The new view computes its orientation from a relative vector before world
translation, avoiding a reproduced near-vertical precision failure at large
coordinates. World-space origins still have ordinary f32 precision.

No lighting, material, compositor or GPU algorithm was changed for this tranche.
Material Room implements the existing R7/R9/R10 hooks. Surface parameters are
uniform per material, with albedo from pigment and the geometric shading normal.
Its existing glyph patterns use three discrete footprint-dependent detail levels
in v2; glyph choice and temporal structure use the same classification. This is
not full geometric antialiasing or a new realistic texture library.

The same immutable snapshot supplies primary, shadow and reflection queries.
Version 2 sorts objects by explicit ID so array reordering does not change
equal-distance tie priority. Version 1 retains original array order. IDs do not
come from array positions. On successful reload, the host replaces the complete
document and current camera together and resets glyph history; time continues.
Invalid reload leaves document, camera, time and history untouched. Old snapshots
keep their old content even after the document is replaced.

The sample has eight analytic objects and five materials: the earlier room plus
a simple steel ellipsoid. It imports no courtyard actor/world modules or material
artwork from the isolated studies. Camera controls, content, simulation, JSON and
reload policy remain in the client. The existing shared budget/camera/compositor/
GPU code is reused; the two hosts have sufficiently different lifecycle and
measurement behavior that no additional universal host layer was introduced.

## Reproduce

From the repository root with Rust and the native linker installed:

```sh
cargo build --locked --release --workspace --bins --example environment_study
cargo run --locked --release -p ascii-material-room -- --scene clients/material-room/assets/material-room-v2.json --validate
cargo run --locked --release -p ascii-material-room -- --scene clients/material-room/assets/material-room-v2.json
cargo run --locked --release -p ascii-material-room -- --scene clients/material-room/assets/material-room-v2.json --headless --camera all --frames 60 --profile balanced --workers 8 --output evidence/my-alpha
cargo run --locked --release -p ascii-engine-lab -- --headless --camera third --stage cloth --output evidence/my-courtyard
```

Windows: [Avvia-alpha.cmd](Avvia-alpha.cmd) always rebuilds and opens v2. The old
launcher/default CLI still uses the v1 sample. `1/2/3` select views, `Q/E` rotate,
`WASD` move, `Z/X` change height, Space pauses, `R` reloads scene and current
camera, `H` toggles labels, Escape closes. JSON edits require no Rust rebuild.

`scripts/verify-r11.py` serializes baseline/runtime/measure/motion/summary phases.
Baseline requires preserved R10 executables under `evidence/r11/baseline`; their
absence in a fresh checkout does not prevent the source tests or normal run.
`scripts/encode-r11.py` encodes native GPU captures with labels only, using the
documented Windows ffmpeg location. All media and raw logs stay local.

## Verification

Implementation source: `4808d7571f8a286d07dad7eb9fb80ce426ff49db`. The report
and final status follow in a documentation commit. Tranche R11 is not a semantic
version release; Cargo packages remain at 0.1.0 with experimental APIs.

| Check | Result |
| --- | --- |
| Full `cargo test --locked --workspace --all-targets` | 204 passing executions, including historical fixture copies; not 204 new independent features |
| fmt, workspace/all-target clippy with `-D warnings`, release client/lab/environment build | PASS; existing incremental-cache hardlink warning falls back to copying |
| V1 producer and host-camera regression | Frozen R10 source/JSON oracle; cells and history bitwise equal in three views, four stages, multiple times, 1/8 workers and resets |
| V2 contracts | Invalid nested/duplicate/range fields, camera validation and precision, surface/metal/environment changes, immutable snapshots, stable IDs/reordering, footprint history and atomic reload PASS |
| GPU legacy regression | 60 PNGs byte-identical: v1 room 12, courtyard 24, R10 environment 24, covering Vulkan and DX12 |
| V2 backend parity | 6 corresponding PNG pairs byte-identical between Vulkan and DX12 |
| Edit data only | Changed material, object position and cameras with the same binary; all three captures changed |
| Windows | V2 first/third/overhead and original lab created real windows and rendered before timed exit |
| Motion | 540 native wgpu frames, 18 s at simulated 30 Hz, 1920×1080; encoded video fully decoded and frame count checked |
| Clean source checkout | Fresh release target built from committed source; both formats validate, v2 renders 60 frames per view and the original client captures successfully; source checkout stays clean |

The independent oracle copies are unchanged from `a2b3eeb`. The core's material,
frame-composition and GPU sources remain unchanged; only camera.rs needed a new
input contract. No dependency or lockfile change was required.

The clean clone is implementation commit `4808d75`, using the same installed
toolchain and Cargo registry cache but a new target directory. All three initial
v2 PNGs match the main checkout. This verifies the build/run guide from committed
files, not a new physical machine or a toolchain installation from scratch. The
initial sandboxed Git/MSYS clone failed on a Windows named-object permission;
the normal escalation retry succeeded. No workaround changed project scripts.

Static history checks have zero changed cells. The motion run uses fixed density
and a 0.025 rad/s camera path from the authored pose, with the same animated
object/light state in each view. Seven coverage audits per view include all seven
non-floor objects with zero border cells. The floor extends beyond the image by
design. These sampled visibility checks do not prove every-frame geometric
coverage or perceptual stability. Static captures and sampled motion frames were
reviewed: all subjects remain recognizable, with dark lower surfaces, regular
patterns and a sparse technical appearance. The final artistic target is unmet.

### Cost and reproducibility limits

Windows x64, Ryzen 7 3800X, Radeon RX 6700 XT; Vulkan and DX12 on the same physical
GPU. Rust/cargo 1.97.1. Baseline/backend runs use balanced 160×90 / 1280×720;
measurement and motion use detail 240×135 / 1920×1080. Worker/density profiles
remain fixed. Tests, builds, captures, measurements and encoding are serialized;
host activity is not otherwise isolated.

Three measurement repetitions use 120 frames per view after 12 CPU+GPU warmups;
worker order is 1/8, 8/1, 1/8. CPU timings include snapshot and composition within
a synchronized GPU run, not a CPU-only benchmark. Submission/wait is measured
separately and is not GPU timestamp time or display FPS. Captures/coverage audits
are outside both timers but can affect scheduling between frames.

| CPU snapshot + composition, median of three p50 values | First | Third | Overhead |
| --- | ---: | ---: | ---: |
| 1 worker | 25.88 ms | 24.99 ms | 37.48 ms |
| 8 workers | 10.70 ms | 7.30 ms | 7.73 ms |

Four of nine 8-worker runs exceed an exploratory p95 threshold of 16.7 ms; the
worst p95 is 36.47 ms. Across repetitions, the first-view p50 ranges from 5.34 to
12.80 ms and third-view p50 from 4.48 to 15.55 ms. The separate motion recordings
have CPU p95 9.10 / 6.29 / 6.49 ms. Those faster runs do not erase the earlier
exceedances. Content, camera and environment differ from R10, so these numbers
are neither an optimization comparison nor a 60 FPS guarantee.

Compact evidence: [summary](evidence/r11/summary.md). Raw manifests, commands,
hashes, logs, per-frame CSVs and GPU PNGs remain in local `evidence/r11/`.
The local [three-view video](evidence/r11/video/material-room-alpha-tre-visuali.mp4)
contains labels only; no interpolation, resizing or enhancement. Generated media,
build caches and private references are excluded from Git.

## Limits and next gate

The environment is constant authored irradiance/radiance. The existing core uses
one normal-direction visibility proxy and at most one local specular bounce;
rough local reflections can be too dark or sharp. This is not full GI/IBL,
multiple-light transport or calibrated PBR. `FrameStats.reflected` still counts
only legacy water. Material Room exposes neither water nor emissive reserved
roles. The panel is rigid procedural motion, not simulated cloth.

The samples do not establish the final visual target, general geometry coverage,
a complete camera controller, physics, fire, destruction, gameplay or an editor.
The next gate in [ENGINE_PATH](ENGINE_PATH.md) concerns thin geometry, edges,
distance transitions and disocclusions under measured motion/cost. Dynamic
occupancy updates and another physical machine remain later explicit gates.
