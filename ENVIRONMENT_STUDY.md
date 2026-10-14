# R10 — common scene and bounded environment transport

14 October 2026. Bounded study implemented and verified on the development host.
This is an experimental transport contract, not final art or a production engine.

## Engine contribution and fixture

The shared `ascii-render` crate now has an opt-in environment-light contract.
It is used by a common scene containing the existing carved stone, metal shield
and animated figure/cape. The demonstration composes previously separate material
studies; it is not a new set of final art assets or an expanded game.

`EnvironmentQuery` supplies hit, normal, reflection direction, filtered roughness,
time and stage. `EnvironmentSample` returns linear diffuse irradiance E, directional
specular radiance and a bounded local query range. `Scene::environment_sample`
defaults to `None`, retaining the previous result. The environment replaces the
old local-material ambient term; it is not added on top of it.

The renderer applies the material's diffuse/conductor split and Fresnel response.
Diffuse irradiance is divided by pi. A normal-direction visibility query is a
deliberately coarse proxy for diffuse occlusion. A reflected query samples the
same immutable scene. A local hit replaces the distant source entirely and is
shaded with direct light and a diffuse environment term, without further
environment rays. Geometry is never rendered into a framebuffer for conversion.
wgpu continues to draw the resulting glyph cells with the existing shader/font.

The sample reuses R8 stone and R9 worked/oxidized metal functions. The R5 actor
is rigidly rotated to show the cape, preserving material coordinates. Its local
roughness and armor conductor parameters are new authored approximations for
this fixture, not a measured fabric model. Stone dampness is a fixed spatial
region, not water simulation. A floor and three wall blocks participate in
primary/shadow/reflection queries. No new simulation or gameplay was added.

## Explicit approximations

- Analytic cool sky plus broad directional light lobe; no HDR image/probe asset,
  sharp sun, physical daylight calibration or exact convolution.
- One geometric-normal sample approximates diffuse visibility. It may switch
  abruptly at blockers and does not integrate a hemisphere.
- One specular ray, local radiance attenuated by `(1 - roughness)^2`; rough local
  reflections may be too sharp or too dark. The distant sky is not blended
  through local blockers to conceal this limitation.
- One environment bounce. Secondary diffuse lighting has no additional ambient
  visibility query. No global illumination, multiscattering, multiple reflection
  bounces, anisotropy or general translucent/fire model.
- Query range is 12 scene units in the fixture, clamped to 0–100 by the core;
  geometry beyond that range is treated as distant environment. Invalid colors
  become zero, finite channels are bounded to 0–16.
- `FrameStats.reflected` still counts the historical water path, not these new
  environment rays. Existing history-change counters are not perceptual metrics.
- Water remains in its original laboratory; it is not added to the common scene
  in this tranche. The first/third/overhead cameras are technical observers, not
  character controllers or a complete game camera system.

The material split follows the distinction between diffuse irradiance and
specular radiance described in [Filament](https://google.github.io/filament/main/filament.html).
The [light transport equation](https://pbr-book.org/4ed/Light_Transport_I_Surface_Reflection/The_Light_Transport_Equation)
defines the broader integration problem. This bounded implementation is not
Filament's split-sum IBL or a complete solution to that equation.

## Reproduce

```sh
cargo build --locked --release --example environment_study
cargo run --locked --release --example environment_study -- --window --variant on --camera third
cargo run --locked --release --example environment_study -- --variant pair --camera all --path showcase --frames 180 --workers 8 --backend vulkan --out evidence/r10/motion-pair
```

Windows: `Avvia-ambiente.cmd`. It builds only when the executable is absent;
rebuild explicitly after source changes. `1/2/3` select views, `V` toggles the
environment, `O` selects camera/light movement, Space pauses, `R` restarts,
`H` toggles labels and Escape closes. Toggling modes resets glyph history.

`off` retains the old ambient calculation, not a black/unlit scene. `on` and
`off` share geometry, point light, animation tick and camera path. `static`
freezes the complete snapshot; other paths include procedural cloth movement.
The showcase moves camera and light initially, then keeps the camera stationary
while light and cloth continue. The two forms of movement are not fully isolated
in that illustrative clip; `orbit` and `light` are separate measured paths.

The serialized runner is `scripts/verify-r10.py`, phases preview/runtime/matrix/
cpu/motion. It requires existing release binaries and should not overlap a build,
encoder or another benchmark. `scripts/encode-r10.py` encodes native GPU PNGs;
`scripts/finalize-r10.py` checks recorded data. The finalizer also needs local R9
baseline captures; absent local evidence is not a failed source build.

## Verification and measurements

Budget declared before runs: 240×135 glyphs,
1920×1080 output, 8 workers, exploratory CPU p95 <=16.7 ms on the current test
host. CPU timers exclude coverage audits, capture and disk output. Synchronized
submission/wait is not GPU timestamp time. Video cadence is simulated, not FPS.

Windows x64, Ryzen 7 3800X development host, Radeon RX 6700 XT. Vulkan and DX12
were exercised on the same GPU; other machines remain unverified. Host activity
was not isolated. No comparison to another engine is implied.

- `cargo fmt --all -- --check`, full workspace/all-target tests (173 pass
  executions, including reused/frozen fixture tests), clippy with `-D warnings`
  and release examples: PASS. Initial clippy found one unused host import, removed
  before the final build; environmental incremental-cache hardlink warnings remain.
- Seven core tests cover absent-hook compatibility, conductor response, visibility,
  local reflection/bounded recursion, black blockers, invalid inputs and parallel
  determinism. The common fixture tests transforms and immutable snapshots; the
  host checks serial/parallel frames, printable glyphs and static equality.
- R9 executables and source archive preserved locally before edits. All 12 metal
  and 18 stone legacy PNG pairs remain byte-identical. All 12 common-scene static
  PNG pairs are identical between Vulkan and DX12. Window smoke run passed.
- Matrix: 72 cases, 5,400 frames; three density profiles, three views, off/on,
  static/orbit/light/approach. Subjects remain present without touching image
  borders; static scenes have zero changed cells. Walls may extend beyond the
  frame: the border diagnostic is deliberately for the three subjects only.
- CPU: three repetitions, 120 frames/case after 12 warmup compositions; serial/8
  worker order alternates between repetitions. Density fixed at 240×135.
- Motion: 540 native wgpu PNGs encoded into 18 seconds at simulated 30 Hz,
  1920×1080, three views, labels only. Full decode and frame count: PASS. Static
  A/B and sampled moving frames were visually reviewed; counters and sampled
  review do not certify the absence of perceptual flicker in every frame.

| CPU composition, median of three p50 values | First | Third | Overhead |
| --- | ---: | ---: | ---: |
| Environment off, 1 worker | 21.24 ms | 21.30 ms | 22.70 ms |
| Environment on, 1 worker | 37.74 ms | 40.98 ms | 46.23 ms |
| Environment off, 8 workers | 4.45 ms | 4.14 ms | 4.02 ms |
| Environment on, 8 workers | 6.34 ms | 7.36 ms | 7.55 ms |

The added work is measurable: approximately 1.43–1.88 times the off-path CPU
median with 8 workers in these runs. None of the 18 CPU runs with 8 workers
exceeded the exploratory 16.7 ms p95 budget; the worst on-path p95 was 15.25 ms.
One of 72 matrix cases exceeded it (off path, p95 18.50 ms); none of the three
motion recordings did (worst p95 13.71 ms). Serial runs are substantially slower
and show spikes up to p95 107.82 ms. These are retained observations, not a
guarantee of 60 FPS or evidence that the on path suppresses host jitter.

Matched-cell glyph changes across the motion clips average 1.15%, with maximum
frame 3.42%; the matrix has a 20.68% maximum. Those counters include legitimate
lighting/animation changes and the floor; they are not a flicker score. Process
peak working set: CPU-only 18.21–19.27 MiB, matrix 248.86–256.04 MiB, motion
271.90 MiB; runtime runs 217.20–328.05 MiB including legacy apps. Not GPU VRAM.

Visual result: the cool environment separates the upper metal response from the
warm point light and changes shadowed surfaces without globally lifting black.
All three subjects are recognizable. Strong dark bands on the stone, dark lower
shield with isolated rivets, regular glyph patterns and a cape-dominated figure
remain. A clear mirror image of the other subjects is not demonstrated by the
reviewed captures; local single-bounce transport is validated by behavioral tests.
The scene does not meet the final realistic-art ambition.

Compact evidence: [summary](evidence/r10/summary.md). Local raw data are in
`evidence/r10/summary.json`, `runs-*.json`, per-case JSON/CSV and PNGs. Source,
scripts and reports are versioned; raw media and private references are not.

## What follows

Use the [engine path](ENGINE_PATH.md) rather than adding another material for its
own sake. The next proposed product gate is consuming the new contracts through
validated data in the independent Material Room client. Performance or coverage
failures from this study remain explicit prerequisites for performance claims.
