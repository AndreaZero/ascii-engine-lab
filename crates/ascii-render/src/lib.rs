//! Experimental v0.1 renderer with an opt-in local material path (R7).
//!
//! A client supplies immutable [`core::Scene`] queries, materials and glyphs.
//! There is no actor, courtyard, simulation, or asset-format dependency here.
//! The GPU compositor embeds `src/glyph.wgsl` at compile time and the font8x8
//! atlas is bundled; rendering does not read a shader or font file at runtime.
//!
//! Compatibility constraints are intentional: `WATER = 3` and `EMISSIVE = 8`
//! have renderer behavior, `Stage` retains the R5 names, and `Camera` retains
//! its existing observer poses. Other named material constants are legacy IDs,
//! not a universal material system. Local materials can supply albedo, shading
//! normal, roughness, scalar F0 and occlusion; legacy scenes retain their prior
//! response. R10 adds opt-in analytic environment irradiance/radiance and a
//! bounded single local reflection. Its one-direction diffuse visibility and
//! rough-reflection blend are approximations, not global illumination or full
//! IBL. Lighting retains one point source and artistic fog; legacy ambient is
//! unchanged without the opt-in. Water retains its specialized path. These
//! contracts remain experimental rather than a stable material API.

pub mod budget;
pub mod camera;
pub mod core;
pub mod frame;
pub mod gpu;
