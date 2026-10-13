# Project status — source for the website and documentation

Updated 13 October 2026. This repository is the source of truth for the ASCII engine laboratory. Website copy should link to the corresponding source revision and report; an experiment in progress is not a released capability.

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
| CPU configuration | Serial path and configurable private worker pool, explicit density profiles | [Performance](PERFORMANCE.md), [portability](PORTABILITY.md); one machine does not establish minimum system requirements |

## Work in progress

R9 has completed its bounded runtime study. Its report and this status are versioned with the implementation; use their containing Git revision when citing the result. The next planned study concerns environment lighting/reflections and a common scene with stone, metal and cloth. Those extensions are not implemented in R9. Do not use implementation drafts as proof of completion.

Fire, richer water dynamics, destructible environments, gameplay integration, more content and tools remain separate future work. Their inclusion in the design does not imply implementation.

## Performance and compatibility wording

Measured on one Windows x64 host, Ryzen 7 3800X and Radeon RX 6700 XT, with Vulkan and DX12 runs. These are test conditions, not required hardware. Other CPUs, GPUs and operating systems remain to be validated. Rust + wgpu was selected for a concrete prototype; superiority over Godot has not been measured.

R8 CPU-only composition at 240×135 glyphs and 8 workers had median p50 values of 3.92 / 4.21 / 5.00 ms across the three views. Other recordings had significant spikes, retained in the report. These numbers exclude GPU work and capture; they are not FPS guarantees. Submission/wait measurements are not GPU timestamps. Videos use a simulated 30 Hz cadence and do not prove realtime performance at that cadence.

R9 CPU-only median p50 values span 4.40–6.35 ms across the two finishes and three views with 8 workers. The 16.7 ms exploratory p95 budget was exceeded in 2/18 CPU runs, 19/72 matrix cases and 5/6 full recordings. Host activity was not isolated. Verification PASS means the recorded checks succeeded, not that every performance target or the visual ambition has been reached.

## Website/documentation coordination

The engine coordinator owns this status file, engine source and verification reports. The website project owns page structure, layout, navigation and presentation. Link technical claims to reports and label each capability as verified, experimental or planned. Preserve the distinction between intended visual quality and achieved quality.

No website repository or conversation identifier is currently available to the engine tools: the user is working in an internal side conversation. Direct two-way synchronization has not been established. This file is the common handoff; the website can read the local repository or the published version. The engine coordinator will update it with each verified milestone. No autonomous watcher or schedule is configured.

Generated captures and movies remain local and are excluded from Git, as are private visual references. The website should not promise downloadable binaries or embed unavailable media. Request a specific delivery of selected original captures when needed; do not publish the user's reference screenshots as project assets.

Primary links: [repository](https://github.com/AndreaZero/ascii-engine-lab), [getting started](README.md), [package metadata](Cargo.toml). The package is version 0.1.0; R1–R9 are development tranche labels, not semantic version releases.
