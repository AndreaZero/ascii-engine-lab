//! Controlled visual probe: same engine, actor-only Scene, fixed light and camera.
//! Separates procedural deformation from the courtyard's moving light/orbit.
use ascii_engine_lab::{
    actor,
    camera::{Camera, Mode},
    core::{Hit, Ray, Scene, Stage},
    frame::Composer,
    gpu::Gpu,
    world,
};
use glam::Vec3;
use std::{error::Error, path::Path};
struct ClothStudy;
impl Scene for ClothStudy {
    fn trace(&self, ray: Ray, time: f32, stage: Stage, _water: bool) -> Option<Hit> {
        actor::trace(ray, time, stage)
    }
    fn light_position(&self, _time: f32) -> Vec3 {
        Vec3::new(1.4, 2.2, -2.8)
    }
    fn pigment(&self, material: u32) -> Vec3 {
        world::pigment(material)
    }
    fn glyph(&self, m: u32, l: Vec3, n: Vec3, i: f32) -> u8 {
        world::glyph(m, l, n, i)
    }
}
fn main() -> Result<(), Box<dyn Error>> {
    let path = Path::new("evidence/cloth-isolated");
    std::fs::create_dir_all(path)?;
    let mut gpu = pollster::block_on(Gpu::new(None, 1280, 720, 160, 90))?;
    let mut camera = Camera::new(Mode::Third);
    camera.orbit = 2.4;
    camera.zoom = 0.35;
    camera.offset = Vec3::new(1., 0., 0.5);
    let mut composer = Composer::default();
    for i in 0..90 {
        let (cells, _) = composer.compose(
            &ClothStudy,
            camera,
            i as f32 / 30.,
            Stage::Cloth,
            160,
            90,
            16. / 9.,
        );
        gpu.capture(&cells, &path.join(format!("sequence-{i:04}.png")))?;
    }
    println!("Cloth probe: 90 GPU frames, fixed light/camera, procedural actor-only Scene.");
    Ok(())
}
