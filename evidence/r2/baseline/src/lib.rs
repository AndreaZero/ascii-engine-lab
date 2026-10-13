pub mod actor;
pub mod camera;
pub mod core;
pub mod frame;
pub mod gpu;
pub mod world;

/// Laboratory content adapter. Another application can supply its own Scene.
pub struct LabScene;
impl core::Scene for LabScene {
    fn trace(&self, r: core::Ray, t: f32, s: core::Stage, w: bool) -> Option<core::Hit> {
        world::trace(r, t, s, w)
    }
    fn light_position(&self, t: f32) -> glam::Vec3 {
        world::light_position(t)
    }
    fn pigment(&self, m: u32) -> glam::Vec3 {
        world::pigment(m)
    }
    fn glyph(&self, m: u32, l: glam::Vec3, n: glam::Vec3, i: f32) -> u8 {
        world::glyph(m, l, n, i)
    }
}
