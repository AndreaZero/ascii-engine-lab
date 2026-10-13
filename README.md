# ASCII Engine Lab

An experimental ASCII rendering laboratory built with **Rust + wgpu**. It explores a reusable renderer in which light, materials, reflections and animation are expressed through dynamic characters.

The current demo is a small environment with a moving light, water reflections and a procedural character with an animated cloak. First-person, third-person and overhead cameras observe the same world state.

The project is under active development. Its APIs and visual language are experimental; it is not yet a production-ready game engine.

## Rendering approach

The image is built directly from semantic character cells:

```text
Immutable world snapshot
        |
Analytic visibility, lighting and reflection queries
        |
Material-driven ASCII glyphs and colors
        |
wgpu glyph atlas -> window or PNG capture
```

CPU queries return surface position, depth, normal, material and persistent identity. These determine the character and its color. wgpu draws the resulting glyph cells using an embedded font atlas.

There is no mesh-rendered world image or RGB-to-ASCII post-processing pass. Spatial data supports perspective and occlusion; the fullscreen display triangle is only a way to draw the characters.

Current capabilities include:

- Three camera projections over a shared scene and simulation time.
- Moving point lighting, analytic shadows and a dark ambient lighting model.
- Planar water with animated normals and single-bounce reflections.
- An analytic character, procedural idle motion and a deforming cloak.
- Material-specific glyph patterns and conservative temporal stabilization.
- Configurable output resolution, character density and CPU worker budgets.
- Windowed rendering, GPU PNG capture and reproducible measurements.

## Getting started

Requirements:

- A Rust toolchain with Cargo.
- A native linker/build toolchain for the target platform. Windows MSVC builds require the Visual Studio C++ Build Tools.
- Graphics drivers and an adapter compatible with a supported wgpu backend.
- Access to crates.io for the first dependency download.

```sh
git clone https://github.com/AndreaZero/ascii-engine-lab.git
cd ascii-engine-lab
cargo run --locked --release
```

The font is embedded. The demo does not require external assets or another game engine.

On Windows, `Avvia-laboratorio.cmd` runs an existing release build or builds one if it is missing. After changing source code, rebuild with Cargo: the launcher does not check whether an existing executable is stale.

### Controls

| Key | Action |
| --- | --- |
| 1 / 2 / 3 | First-person / third-person / overhead camera |
| F1 / F2 / F3 / F4 | Core / lighting / water / animated cloth stage |
| Q / E | Rotate the view |
| W / A / S / D | Move the observer |
| Z / X | Lower / raise the observer |
| Space | Pause simulation time |
| H | Toggle the diagnostic overlay |
| P | Capture a PNG in `evidence/` |
| R | Reset the current view and simulation time |
| Esc | Exit |

These are laboratory observer cameras. Gameplay movement, camera collision, player tracking and first-person hands are not implemented.

## Quality and execution budgets

| Profile | Output pixels | Character grid | Automatic worker cap |
| --- | --- | --- | --- |
| `economy` | 960 x 544 | 120 x 68 | 2 |
| `balanced` (default) | 1280 x 720 | 160 x 90 | 8 |
| `detail` | 1920 x 1080 | 240 x 135 | 16 |

```sh
cargo run --locked --release -- --profile economy
cargo run --locked --release -- --profile detail --workers auto
cargo run --locked --release -- --workers 1
cargo run --locked --release -- --help
```

Automatic worker selection uses the runtime's available-parallelism estimate and the profile cap, leaving one execution slot outside the pool when possible. This is a capacity estimate, not a CPU-speed measurement or a reservation of physical cores. Explicit worker counts must fit the detected capacity and the application limit of 64.

Output pixels and character density can be changed independently with `--width`, `--height`, `--cols` and `--rows`. Profiles remain fixed during a run; they do not guarantee a frame rate or silently reduce detail.

Backend selection accepts `--backend auto|vulkan|dx12|metal|gl`. An explicit backend fails if unavailable rather than silently switching. Automatic selection also respects `WGPU_BACKEND`. Adapter options include `--adapter-power auto|low|high` and `--fallback-adapter`.

Runtime execution has been checked on **Windows x64 with Vulkan and DirectX 12**. Other hardware and operating systems still need validation. Backend selection alone is not proof that a platform has been tested. See [PORTABILITY.md](PORTABILITY.md) for the existing evidence and limitations.

## Captures and measurements

Create a GPU capture, record a sequence or collect a selected benchmark case:

```sh
cargo run --locked --release -- --headless --camera third --stage cloth --output evidence/capture
cargo run --locked --release -- --sequence 90 --camera overhead --stage cloth --output evidence/sequence
cargo run --locked --release -- --benchmark-case --camera third --stage cloth --frames 120 --output evidence/benchmark
cargo run --locked --release -- --diagnostics --output evidence/diagnostics
```

`--benchmark` visits all four stages and all three cameras. `--benchmark-case` measures the selected camera/stage. Both use 12 warm-up frames and produce per-frame CSV data and aggregate JSON. `--diagnostics` records the runtime configuration and actual adapter capabilities.

Sequences use a simulation step of 1/30 second and a small camera rotation. Captured playback rate is independent of rendering throughput.

The controlled character study uses the same renderer with an isolated figure, fixed lighting and three views:

```sh
cargo run --locked --release --example character_study -- --camera all --variant current --frames 180 --out evidence/character-study
cargo run --locked --release --example character_study -- --camera all --variant baseline --frames 180 --out evidence/character-baseline
cargo run --locked --release --example character_study -- --benchmark --camera all --frames 120 --out evidence/character-cpu
```

The baseline contains the previous character and materials. This comparison evaluates intentional visual changes, not an optimization at identical quality. The study's `--benchmark` mode skips GPU initialization and captures.

CPU composition time, submission/wait time and synchronized offscreen frame time are separate measurements. **Submission/wait is not a GPU timestamp**, and offscreen timings are not window presentation FPS. Stability metrics cover matching surface IDs in the same screen cell; they are not a complete perceptual flicker measure.

When reporting results, include the revision, operating system, CPU, GPU/backend/driver, profile or explicit dimensions, worker count, camera, stage, frame count and workload conditions.

Generated captures, videos, binaries and raw recordings are excluded from Git. Historical source fixtures needed by tests and compact report summaries are versioned. Some historical scripts expect local recordings that are not included in a clone; use the current CLI and fresh output directories for new measurements.

## Project structure and reuse

| Path | Responsibility |
| --- | --- |
| `src/core.rs` | Rays, hits, glyph cells, scene contract and intersections |
| `src/budget.rs` | Quality profiles and CPU worker budgets |
| `src/camera.rs` | Perspective and overhead projections |
| `src/frame.rs` | Visibility, lighting, reflection and glyph history |
| `src/gpu.rs`, `src/glyph.wgsl` | GPU presentation and capture |
| `src/world.rs`, `src/actor.rs` | Replaceable environment and character fixtures |
| `src/main.rs` | Window, input, CLI and measurements |
| `examples/` | Controlled studies and benchmark programs |
| `tests/` | Regression and traversal comparisons |
| `scripts/` | Measurement and verification utilities |
| `evidence/r*/baseline/` | Frozen sources used by regression tests |

Applications can implement `Scene` and feed the existing `Composer`. Query implementations must be `Sync` and deterministic for one immutable simulation snapshot. Update the simulation before composition and share its state across primary, shadow and reflection queries.

The demo prepares `LabScene::snapshot(time, stage)` once per composition. `Scene::glyph_history_key` lets materials identify structural changes, such as a moving fold or seam, so temporal stabilization can preserve lighting continuity while allowing those characters to change.

These contracts are still evolving. A stable SDK, portable content format and independent application integration remain development goals.

## Limitations and next steps

- One visibility sample per character cell can miss thin features and cause popping during camera movement.
- Lighting uses an artistic ambient term and one point light. General multi-light rendering and global illumination are not implemented.
- Water uses a planar surface and one reflection bounce; refraction and fluid simulation are not implemented.
- Cloth motion is procedural, with anchored regions and local deformation. It is not a physical cloth solver, and grazing intersections need further work.
- Fire, general vegetation animation and destructible environments are future capabilities.
- The project does not yet provide gameplay systems, an editor, asset authoring tools or networking.
- Rendering queries currently run on the CPU. Further acceleration requires profiling and quality comparisons.

Near-term work focuses on visual stability, separating authored content from the demo fixture, exercising the renderer from an independent client and expanding hardware validation.

## Contributing

Bug reports, reproducible measurements, platform testing and focused pull requests are welcome. Describe the expected and observed behavior, relevant commands and environment. For visual changes, include captures or short sequences with comparable camera, lighting and simulation settings.

Before submitting code, run the relevant checks:

```sh
cargo fmt --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo build --locked --release
```

Keep changes scoped to a clear problem. Preserve direct character generation, deterministic snapshot queries and the source fixtures used by regression tests. Avoid committing generated media, build output, credentials or machine-specific configuration. Preserve `Cargo.lock` for reproducible dependencies.

Visual or GPU changes also need a real rendering check; CPU tests alone cannot establish appearance or backend compatibility.

## Development reports

- [REPORT.md](REPORT.md) — initial laboratory and rendering limitations.
- [PERFORMANCE.md](PERFORMANCE.md) — CPU parallelism experiments.
- [PORTABILITY.md](PORTABILITY.md) — execution budgets, snapshots and compatibility checks.
- [QUERY_PERFORMANCE.md](QUERY_PERFORMANCE.md) — shadow-query profiling and comparisons.

These reports describe specific historical revisions and measurement sessions. Their results are not universal performance guarantees.

Core dependency documentation: [wgpu 26.0.1](https://docs.rs/wgpu/26.0.1/wgpu/), [winit 0.30.12](https://docs.rs/winit/0.30.12/winit/), [font8x8](https://docs.rs/font8x8/0.3.1/font8x8/).
