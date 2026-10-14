//! Independently authored content for the reusable ASCII renderer.
//! No BLACKWATER fixture, actor, runtime, or image texture is loaded here.
pub mod content;

pub use content::{SceneDocument, Snapshot};

pub const DEFAULT_SCENE_JSON: &str = include_str!("../assets/material-room.json");
pub const DEFAULT_SCENE_V2_JSON: &str = include_str!("../assets/material-room-v2.json");
