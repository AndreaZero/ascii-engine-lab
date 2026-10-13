pub mod actor;
pub mod budget;
pub mod camera;
pub mod core;
pub mod frame;
pub mod gpu;
pub mod world;

/// Laboratory content adapter. Another application can supply its own Scene.
pub struct LabScene;
impl LabScene {
    /// Capture dynamic fixture data once; share this immutable value with every
    /// primary, shadow and reflected query of a composition.
    pub fn snapshot(time: f32, stage: core::Stage) -> world::WorldSnapshot {
        world::WorldSnapshot::new(time, stage)
    }
}
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

impl core::Scene for world::WorldSnapshot {
    fn trace(&self, r: core::Ray, t: f32, s: core::Stage, w: bool) -> Option<core::Hit> {
        if t.to_bits() == self.time().to_bits() && s == self.stage() {
            self.trace(r, w)
        } else {
            // Preserve the Scene contract even if a caller reuses a snapshot
            // with a different tick/stage. The normal app never takes this path.
            world::trace(r, t, s, w)
        }
    }
    fn occluded(&self, r: core::Ray, t: f32, s: core::Stage, max_distance: f32) -> bool {
        if t.to_bits() == self.time().to_bits() && s == self.stage() {
            self.occluded(r, max_distance)
        } else {
            // Match the existing trace fallback when a client requests a new tick.
            core::Scene::occluded(&LabScene, r, t, s, max_distance)
        }
    }
    fn light_position(&self, t: f32) -> glam::Vec3 {
        if t.to_bits() == self.time().to_bits() {
            self.light_position()
        } else {
            world::light_position(t)
        }
    }
    fn pigment(&self, m: u32) -> glam::Vec3 {
        world::pigment(m)
    }
    fn glyph(&self, m: u32, l: glam::Vec3, n: glam::Vec3, i: f32) -> u8 {
        world::glyph(m, l, n, i)
    }
}
