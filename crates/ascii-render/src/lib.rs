//! Experimental v0.1 extraction; the R5 runtime algorithms remain unchanged.
//!
//! A client supplies immutable [`core::Scene`] queries, materials and glyphs.
//! There is no actor, courtyard, simulation, or asset-format dependency here.
//! The GPU compositor embeds `src/glyph.wgsl` at compile time and the font8x8
//! atlas is bundled; rendering does not read a shader or font file at runtime.
//!
//! Compatibility constraints are intentional: `WATER = 3` and `EMISSIVE = 8`
//! have renderer behavior, `Stage` retains the R5 names, and `Camera` retains
//! its existing observer poses. Other named material constants are legacy IDs,
//! not a universal material system. Lighting/fog/water response remains fixed
//! in the compositor. Extraction alone does not remove these API limitations.

pub mod budget;
pub mod camera;
pub mod core;
pub mod frame;
pub mod gpu;
