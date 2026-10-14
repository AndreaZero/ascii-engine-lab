# ASCII Engine Lab

[![Rust](https://img.shields.io/badge/Rust-2021_edition-dea584?logo=rust&logoColor=white)](Cargo.toml)
[![wgpu](https://img.shields.io/badge/wgpu-26.0.1-4a90d9)](Cargo.toml)
[![Rendering](https://img.shields.io/badge/Rendering-Native_ASCII-638c73)](#rendering-approach)
[![Status](https://img.shields.io/badge/Status-Experimental-d6a44c)](#limitations-and-next-steps)

**Direct ASCII graphics with dynamic glyphs, lighting, reflections and procedural animation.**

ASCII Engine Lab is an experimental rendering laboratory built with **Rust + wgpu**. It explores how expressive, animated worlds can be constructed directly from characters, with a reusable renderer that treats light, materials and motion as part of the character system.

The current demo is a small environment with a moving light, water reflections and a procedural character with an animated cloak. First-person, third-person and overhead cameras observe the same world state.

The project is under active development. Its APIs and visual language are experimental; it is not yet a production-ready game engine.

R6 extracts the renderer into `crates/ascii-render` and adds [Material Room](clients/material-room/README.md), a separate application that loads its own JSON scene. It demonstrates reuse and data editing; its simple shapes and material patterns are technical samples, not the target visual quality. See [verification and limits](REUSE.md).

R7 adds a [dry/wet stone study](STONE_STUDY.md): carved cracks, chipped edges, local grain and a view-dependent light response, generated directly as glyphs. This is an isolated material experiment; the courtyard and Material Room keep their existing appearance.

R8 refines [wet-stone readability](WET_READABILITY.md): low-light glyph families retain more grain while per-cell colors, geometry and lighting remain identical. The previous wet appearance remains available for a matched comparison.

R9 explores [worked and oxidized steel](METAL_STUDY.md) on the same analytic convex shield, with optional RGB conductor response. See [project status](PROJECT_STATUS.md) for verified capabilities, remaining limits and the source of truth for the website/documentation.

R10 adds a [common-scene environment study](ENVIRONMENT_STUDY.md): stone, metal and animated cloth share a bounded environment-light and reflection contract. Run `Avvia-ambiente.cmd` on Windows, or `cargo run --locked --release --example environment_study -- --window --variant on`. The [engine path](ENGINE_PATH.md) sets the next gate: validated data-driven use from the independent client.

[Quick start](#getting-started) · [How it renders](#rendering-approach) · [Captures and benchmarks](#captures-and-measurements) · [Contributing](#contributing) · [Report an issue](https://github.com/AndreaZero/ascii-engine-lab/issues)

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
- Opt-in local albedo, shading normal, roughness, scalar Fresnel reflectance and occlusion, with footprint-aware glyph detail.
- Optional analytic environment irradiance/radiance and a bounded local specular bounce, with explicit approximations.
- Optional RGB conductor reflectance and metalness, with a dedicated worked/oxidized steel sample.
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

The stone study separates form, surface and lighting, with matched dry/wet camera paths:

```sh
cargo run --locked --release --example stone_study -- --window --variant dry --camera third
cargo run --locked --release --example stone_study -- --variant pair --camera all --frames 180 --out evidence/stone-study
cargo run --locked --release --example stone_study -- --benchmark --variant pair --camera all --frames 180 --workers 8 --out evidence/stone-cpu
```

On Windows, `Avvia-pietra.cmd` opens the study (builds if missing). Rebuild with Cargo after source changes. Study controls: `1/2/3` views, `V` dry/wet, `C` wet R7/R8 comparison, `B` baseline/shape, `O` orbit/light, `Space` pause, `R` restart, `H` labels. Its headless output is fixed at 1920×1080; `--grid` changes only cell count. Use `--variant compare` to capture wet R7/R8 or `--variant trio` for dry/R7 wet/R8 wet. See [original study](STONE_STUDY.md) and [readability refinement](WET_READABILITY.md).

For the metal sample, run `cargo run --locked --release --example metal_study -- --window --variant forged --camera third`, or use `Avvia-metallo.cmd` on Windows. `V` switches worked/oxidized steel, `B` compares a uniform metal surface, and `1/2/3` switch views. See [metal study](METAL_STUDY.md) for capture commands, measurements and lighting limits.

CPU composition time, submission/wait time and synchronized offscreen frame time are separate measurements. **Submission/wait is not a GPU timestamp**, and offscreen timings are not window presentation FPS. Stability metrics cover matching surface IDs in the same screen cell; they are not a complete perceptual flicker measure.

When reporting results, include the revision, operating system, CPU, GPU/backend/driver, profile or explicit dimensions, worker count, camera, stage, frame count and workload conditions.

Generated captures, videos, binaries and raw recordings are excluded from Git. Historical source fixtures needed by tests and compact report summaries are versioned. Some historical scripts expect local recordings that are not included in a clone; use the current CLI and fresh output directories for new measurements.

## Project structure and reuse

| Path | Responsibility |
| --- | --- |
| `crates/ascii-render/src/core.rs` | Rays, hits, glyph cells, scene contract and intersections |
| `crates/ascii-render/src/budget.rs` | Quality profiles and CPU worker budgets |
| `crates/ascii-render/src/camera.rs` | Perspective and overhead projections |
| `crates/ascii-render/src/frame.rs` | Visibility, lighting, reflection and glyph history |
| `crates/ascii-render/src/gpu.rs`, `glyph.wgsl` | GPU presentation and capture |
| `src/world.rs`, `src/actor.rs` | Replaceable environment and character fixtures |
| `src/main.rs` | Window, input, CLI and measurements |
| `src/lib.rs` | Lab adapter and compatibility reexports of the extracted renderer |
| `clients/material-room/` | Independent JSON scene loader, immutable snapshots and application |
| `examples/` | Controlled studies and benchmark programs |
| `tests/` | Regression and traversal comparisons |
| `scripts/` | Measurement and verification utilities |
| `evidence/r*/baseline/` | Frozen sources used by regression tests |

Applications can implement `Scene` and feed the existing `Composer`. Query implementations must be `Sync` and deterministic for one immutable simulation snapshot. Update the simulation before composition and share its state across primary, shadow and reflection queries.

The demo prepares `LabScene::snapshot(time, stage)` once per composition. `Scene::glyph_history_key` lets materials identify structural changes, such as a moving fold or seam, so temporal stabilization can preserve lighting continuity while allowing those characters to change.

`Scene::local_materials` opts into the R7 path. `material_sample(hit, footprint)` supplies local surface properties; `glyph_sample` and `glyph_sample_key` share the same filtered structural classification. The default path preserves existing clients. This material API is experimental and is not yet exposed by Material Room's JSON format.

R9 adds optional `metallic_sample(hit, footprint)` for the RGB conductor endpoint and metalness. `None` or zero metalness preserves the dielectric path. Pure metal has no diffuse component. The R9 sample itself has no environment reflection; anisotropy remains unimplemented. This extension is also outside Material Room's current JSON format.

R10 adds optional `environment_sample(EnvironmentQuery)` for analytic diffuse irradiance, directional specular radiance and a local query range. None preserves the prior path. Local reflections use one bounded bounce and roughness attenuation; diffuse visibility is a one-direction proxy. This is not complete IBL or GI. See [the measured common scene](ENVIRONMENT_STUDY.md).

These contracts are still evolving. Material Room depends only on `ascii-render` and its own scene data. Its bounded, versioned JSON format is client-owned; it is not a universal asset format. Reserved water/emissive IDs, one point light, artistic ambient/fog response and laboratory camera/stage defaults remain API limitations. A stable SDK and broader hardware validation remain development goals.

## Limitations and next steps

- One visibility sample per character cell can miss thin features and cause popping during camera movement.
- Lighting retains one point light, with either legacy ambient or the opt-in approximate environment path. General multi-light rendering and global illumination are not implemented.
- Water uses a planar surface and one reflection bounce; refraction and fluid simulation are not implemented.
- Cloth motion is procedural, with anchored regions and local deformation. It is not a physical cloth solver, and grazing intersections need further work.
- Fire, general vegetation animation and destructible environments are future capabilities.
- The project provides a small JSON editing/reload workflow; it does not yet provide gameplay systems, a visual editor, a general asset pipeline or networking.
- Rendering queries currently run on the CPU. Further acceleration requires profiling and quality comparisons.

The R7 stone specimen is the first richer surface experiment, with static/orbit/light/approach checks across three cell grids and views. It remains an authored block with shallow analytic cracks, filtered procedural grain and approximate wetness. Final art quality, silhouette antialiasing, richer content creation and broader hardware validation remain open. The next product gate is data-driven use of the newer contracts in Material Room, followed by coverage/motion, dynamic updates and practical authoring/portability; see [ENGINE_PATH.md](ENGINE_PATH.md).

## Contributing

Contributions are welcome, especially in **ASCII visual quality**, **animation and temporal stability**, **hardware compatibility** and **reproducible performance testing**.

Use [issues](https://github.com/AndreaZero/ascii-engine-lab/issues) for bug reports and proposals, and [pull requests](https://github.com/AndreaZero/ascii-engine-lab/pulls) for focused changes. Describe the expected and observed behavior, relevant commands and environment. For visual changes, include captures or short sequences with comparable camera, lighting and simulation settings.

Before submitting code, run the relevant checks:

```sh
cargo fmt --all -- --check
cargo test --workspace --all-targets --locked
cargo clippy --workspace --locked --all-targets -- -D warnings
cargo build --workspace --locked --release --bins
```

Keep changes scoped to a clear problem. Preserve direct character generation, deterministic snapshot queries and the source fixtures used by regression tests. Avoid committing generated media, build output, credentials or machine-specific configuration. Preserve `Cargo.lock` for reproducible dependencies.

Visual or GPU changes also need a real rendering check; CPU tests alone cannot establish appearance or backend compatibility.

## Development reports

- [REPORT.md](REPORT.md) — initial laboratory and rendering limitations.
- [PERFORMANCE.md](PERFORMANCE.md) — CPU parallelism experiments.
- [PORTABILITY.md](PORTABILITY.md) — execution budgets, snapshots and compatibility checks.
- [QUERY_PERFORMANCE.md](QUERY_PERFORMANCE.md) — shadow-query profiling and comparisons.
- [CHARACTER_STUDY.md](CHARACTER_STUDY.md) — character/cloak study and motion evidence.
- [REUSE.md](REUSE.md) — extracted renderer, independent client and content-edit verification.
- [STONE_STUDY.md](STONE_STUDY.md) — stone form, dry/wet material, three-view motion and performance.
- [WET_READABILITY.md](WET_READABILITY.md) — matched wet-stone glyph refinement and stone-only diagnostics.
- [METAL_STUDY.md](METAL_STUDY.md) — conductor response, worked/oxidized steel and three-view verification.
- [PROJECT_STATUS.md](PROJECT_STATUS.md) — shared technical status for the website and documentation.

These reports describe specific historical revisions and measurement sessions. Their results are not universal performance guarantees.

Core dependency documentation: [wgpu 26.0.1](https://docs.rs/wgpu/26.0.1/wgpu/), [winit 0.30.12](https://docs.rs/winit/0.30.12/winit/), [font8x8](https://docs.rs/font8x8/0.3.1/font8x8/).
