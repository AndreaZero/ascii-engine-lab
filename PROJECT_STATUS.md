# Project status — source for the website and documentation

Updated 15 October 2026. This repository is the source of truth for the ASCII engine laboratory. Website copy should link to the corresponding source revision and report; an experiment in progress is not a released capability.

## What this project is

A reusable renderer under development in Rust + wgpu. Spatial queries produce characters and their colors directly; wgpu draws those characters. There is no mesh-rendered world image converted into ASCII. Spatial coordinates, normals and depth are used for visibility and cameras. BLACKWATER is the intended first game, with gameplay expansion currently paused. The previous Godot prototype is historical and is not the current engine architecture.

`crates/ascii-render` is the shared renderer. The courtyard laboratory and `clients/material-room` are separate clients. This demonstrates code reuse; it does not mean a stable production engine, universal scene format or complete editor exists.

## Verified state

| Area | What exists | Evidence and limits |
| --- | --- | --- |
| Cameras | First person, third person and overhead views of the same scene | [Laboratory report](REPORT.md); demonstration cameras, not a complete game camera system |
| Characters | Procedural figure and animated cloth with local glyph structure | [Character study](CHARACTER_STUDY.md); demonstrative assets and procedural deformation, not full cloth physics |
| Light and water | Moving light, visibility queries and a water reflection prototype | [Laboratory report](REPORT.md); limited light transport and analytic water, not a fluid simulation |
| Reuse | Shared crate and independent JSON-driven Material Room client | [Reuse](REUSE.md); experimental API and a deliberately small content format |
| Stone | Analytic carved stone, dry/wet local material and filtered glyph detail | [Stone study](STONE_STUDY.md); stylized surface, not the final art target |
| Wet readability, R8 | More legible grain in half shadows through glyph selection | [Wet readability](WET_READABILITY.md), source revision `395fd27`; same cell RGB, geometry and lighting; specular response unchanged |
| Metal, R9 | Analytic convex shield, worked/oxidized steel and optional RGB conductor response | [Metal study](METAL_STUDY.md); verified three-view captures and backend parity, stylized material, no environment reflections or anisotropy |
| Common scene/environment, R10 | Optional irradiance/radiance contract and one local reflection bounce; stone, metal and animated cape together | [Environment study](ENVIRONMENT_STUDY.md); bounded proxies, three-view captures, legacy parity; not full GI or final artwork |
| Alpha data gate, R11 | Independent client consumes validated v2 surfaces, conductors, constant environment and authored cameras; v1 compatibility and atomic reload | [Alpha data](ALPHA_DATA.md); 204 test executions, 60 legacy PNGs unchanged, backend parity and clean-source build/run guide; bounded experimental scope |
| Primary coverage, R12 | Opt-in Conservative9 semantic sampling with coverage-aware history; Center remains default | [Geometry coverage](GEOMETRY_COVERAGE.md); 292 test executions, 72 legacy PNGs unchanged, dense geometry comparisons, three-view windows/motion; silhouette expansion, sample gaps and measured extra cost remain |
| Query work, R13 | Exact distance-limited box helper and explicit bounded diagnostics; client integration rejected for CPU cost, ordinary R12 traversal retained | [Query acceleration](QUERY_ACCELERATION.md); 324 test executions, frozen hit/tie/cell comparisons, 1,152 final-default PNGs unchanged, three-view native windows; fewer axes are not a runtime speedup |
| Dynamic occupancy, R14 | Atomic client-owned insertion/removal/absolute transforms, lifetime handles, immutable retained snapshots and optional global history epoch | [Dynamic occupancy](DYNAMIC_OCCUPANCY.md); 376 workspace test executions, 27 focused release tests, independent reconstruction and transport probes, 120 legacy PNGs unchanged and 1,080 matching backend pairs; bounded updates, not physics or spatial acceleration |
| CPU configuration | Serial path and configurable private worker pool, explicit density profiles | [Performance](PERFORMANCE.md), [portability](PORTABILITY.md); one machine does not establish minimum system requirements |
| Practical authoring, R15 | Self-contained validated projects, CLI edits, native object/camera controls and protected save/reopen; relocatable Windows package/probe | [Authoring and portability](AUTHORING_PORTABILITY.md); 396 test executions, exact saved-camera rays, whole-timeline validation and real CLI checks. Renderer unchanged. One physical host only; combined portability gate remains partial |

## Work in progress

R16 completed a bounded reusable visual experiment: validated JSON figures with
oriented ellipsoids and garment panels, including a changed-cut variant. Core and
existing clients are unchanged. [Result and measurements](VISUAL_CAPACITY.md).
The visual exit criterion failed: improved head/neck/arm separation does not yet
deliver reliable clothing detail, convincing folds or sufficient contrast across
views. General visual capacity remains open. Center passed the exploratory CPU
budget on this host; both new figures failed it in all Conservative9 runs.
No completion percentage is supported. The user has no second physical host;
that validation gap remains explicit without blocking visual development.

R15 delivers practical creation/edit/preview/save/reopen for the supported subset
and a Windows package/probe. Another physical machine is still unverified, so
the combined creation/portability gate remains partial. No universal editor,
savegame identity, minimum hardware or production frame rate is implied.

R11 closes the bounded alpha data gate. R12 verifies an optional nine-ray primary coverage policy, fixed scene/camera paths, independent dense-reference comparisons and real-window pacing in three views. It recovers thin foreground but widens silhouettes, can hide small background slivers and still misses between samples. R13 verifies an exact bounded-box helper and compares a client integration against R12 on the same workloads. Fewer axes did not improve measured CPU cost, so the integration was rejected and ordinary traversal remains R12. R14 closes the bounded client occupancy/lifetime gate: transactions publish one validated state, surviving handles remain stable, recreated IDs receive new generations and retained snapshots remain coherent. Effective commits reset all glyph history conservatively; continuous updates reduce history reuse. The general geometry/motion/cost gate remains open: traversal is still linear, and operation counts do not establish universal frame-time gains. Practical creation/save/reopen is now delivered in R15; validation on another physical machine remains next; broader spatial rejection needs explicit numerical semantics. API stability and final art remain open. See [the engine development path](ENGINE_PATH.md).

Fire, richer water dynamics, destructible environments, gameplay integration, more content and tools remain separate future work. Their inclusion in the design does not imply implementation.

## Performance and compatibility wording

Measured on one Windows x64 host, Ryzen 7 3800X and Radeon RX 6700 XT, with Vulkan and DX12 runs. These are test conditions, not required hardware. Other CPUs, GPUs and operating systems remain to be validated. Rust + wgpu was selected for a concrete prototype; superiority over Godot has not been measured.

R8 CPU-only composition at 240×135 glyphs and 8 workers had median p50 values of 3.92 / 4.21 / 5.00 ms across the three views. Other recordings had significant spikes, retained in the report. These numbers exclude GPU work and capture; they are not FPS guarantees. Submission/wait measurements are not GPU timestamps. Videos use a simulated 30 Hz cadence and do not prove realtime performance at that cadence.

R9 CPU-only median p50 values span 4.40–6.35 ms across the two finishes and three views with 8 workers. The 16.7 ms exploratory p95 budget was exceeded in 2/18 CPU runs, 19/72 matrix cases and 5/6 full recordings. Host activity was not isolated. Verification PASS means the recorded checks succeeded, not that every performance target or the visual ambition has been reached.

R10 common-scene CPU-only medians with 8 workers are 6.34 / 7.36 / 7.55 ms with environment enabled, versus 4.45 / 4.14 / 4.02 ms without it (first/third/overhead, 240×135). No 8-worker CPU run exceeded the exploratory p95 budget; one of 72 matrix cases did (off path, 18.50 ms). The three motion runs did not. These are different contents from R9, not a direct optimization comparison. New transport remains a one-ray diffuse-visibility and one-bounce specular approximation, with rough local reflections potentially too dark or sharp.

R11 Material Room v2 CPU snapshot/composition medians with 8 workers are 10.70 / 7.30 / 7.73 ms at 240×135, measured inside synchronized GPU runs. Four of nine runs exceed p95 16.7 ms (worst 36.47 ms); later motion runs are faster and do not cancel those exceedances. Content differs from R10 and host activity is not isolated. This is neither an optimization comparison nor a realtime performance guarantee. Submission/wait remains separate from GPU timestamp time.

R12 CPU-only medians at 240x135 with 8 workers are 4.43 / 4.96 / 5.25 ms for Center and 14.84 / 16.57 / 15.67 ms for Conservative9. Nine exceeds p95 16.7 ms in 8/9 runs (worst 31.91 ms); Center does in 0/9 during this session. Earlier R11 failures remain recorded. Actual diagnostic-window callback intervals have p50 around 10 ms for Center and 20–21 ms for Nine on this desktop. These are different measurement modes, not GPU timestamps, universal FPS or proof of production readiness. The default remains Center; no silent density reduction occurs.

R13 compares the bounded experiment with frozen R12 on identical moving workloads: 144 CPU cases, 17,280 measured frames and three interleaved repetitions. At 240x135/eight workers, v2 Conservative9 median p50 changes from 15.260 / 15.114 / 15.365 ms to 18.606 / 18.167 / 18.528 ms; coverage Conservative9 changes from 21.944 / 21.721 / 22.361 to 25.644 / 23.207 / 23.871 ms. Across one/eight-worker cases, 53/72 baseline and 54/72 experimental runs exceed p95 16.7 ms. The integration is rejected and the final default retains R12 traversal. The reusable helper preserves the strict distance/tie contract, but reduced axis counts are not an elapsed-time gain. The complete experiment, failed variants, final-default parity and separate native-window checks are recorded in [R13](QUERY_ACCELERATION.md). The general cost gate remains open.

R14 dynamic-room CPU-only total medians at 240x135/eight workers are 4.666 /
3.970 / 5.030 ms with Center and 15.893 / 15.695 / 16.360 ms with Conservative9.
Center exceeds p95 16.7 ms in 0/9 runs; Nine in 9/9 (worst 19.493 ms).
Commit-frame publication p50 spans 0.2185–0.3319 ms, worst p95 0.5759 ms on this
small room. The 72-case/9,720-frame matrix includes a separate unchanged-v2
comparison against R13: legacy Nine exceeds the budget in 8/9 runs for each
revision, with worst p95 increasing from 20.621 to 37.431 ms. This is retained
as measured variability, not a speedup claim. See [R14](DYNAMIC_OCCUPANCY.md)
for full one/eight-worker results, native-window evidence and limitations.

## Website/documentation coordination

The engine coordinator owns this status file, engine source and verification reports. The website project owns page structure, layout, navigation and presentation. Link technical claims to reports and label each capability as verified, experimental or planned. Preserve the distinction between intended visual quality and achieved quality.

The separate website repository is now identified as [ascii-engine-website](https://github.com/AndreaZero/ascii-engine-website). Its local checkout is `ascii-engine-site`, beside this engine checkout. Website content reads a committed engine snapshot; it requires explicit catalogue/snapshot refresh and publication to update the live site. The user reports the site online; local content verification is not verification of the deployed revision. Direct contact with its internal side conversation has not been established. No autonomous watcher or schedule is configured.

Generated captures and movies remain local and are excluded from Git, as are private visual references. The website should not promise downloadable binaries or embed unavailable media. Request a specific delivery of selected original captures when needed; do not publish the user's reference screenshots as project assets.

Primary links: [repository](https://github.com/AndreaZero/ascii-engine-lab), [getting started](README.md), [package metadata](Cargo.toml). The package is version 0.1.0; R1–R15 are development tranche labels, not semantic version releases.
