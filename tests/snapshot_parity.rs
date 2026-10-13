//! Frozen R2 traversal vs current prepared traversal, using the SAME R5 content.
//! R5 intentionally changes the actor/materials; this verifies preparation,
//! not visual equality to the historical figure. Archived R2 stays untouched.
pub use ascii_engine_lab::{actor, core};
#[allow(dead_code)]
#[path = "../evidence/r3/baseline/src/world.rs"]
mod world;

use ascii_engine_lab::{
    camera::{Camera, Mode},
    core::{Hit, Ray, Scene, Stage},
    frame::Composer,
    LabScene,
};
use glam::Vec3;
struct Reference;
impl Scene for Reference {
    fn trace(&self, ray: Ray, t: f32, s: Stage, w: bool) -> Option<Hit> {
        world::trace(ray, t, s, w)
    }
    fn light_position(&self, t: f32) -> Vec3 {
        world::light_position(t)
    }
    fn pigment(&self, m: u32) -> Vec3 {
        ascii_engine_lab::world::pigment(m)
    }
    fn glyph(&self, m: u32, l: Vec3, n: Vec3, i: f32) -> u8 {
        ascii_engine_lab::world::glyph(m, l, n, i)
    }
    fn glyph_history_key(&self, m: u32, l: Vec3, n: Vec3) -> u32 {
        LabScene.glyph_history_key(m, l, n)
    }
}
#[test]
fn prepared_world_preserves_stateless_traversal_with_current_content() {
    for mode in [Mode::First, Mode::Third, Mode::Overhead] {
        for stage in [Stage::Core, Stage::Light, Stage::Water, Stage::Cloth] {
            for workers in [1, 2, 8] {
                let mut old = Composer::with_threads(workers).unwrap();
                let mut new = Composer::with_threads(workers).unwrap();
                for (frame, t) in [0., 0.17, 0.5, 1., 1.6, 3.2].into_iter().enumerate() {
                    if frame == 4 {
                        old.reset();
                        new.reset();
                    }
                    let mut camera = Camera::new(mode);
                    camera.orbit = frame as f32 * 0.021;
                    camera.zoom = 0.9;
                    let (a, astats) = old.compose(&Reference, camera, t, stage, 47, 29, 16. / 9.);
                    let snapshot = LabScene::snapshot(t, stage);
                    let (b, bstats) = new.compose(&snapshot, camera, t, stage, 47, 29, 16. / 9.);
                    assert!(
                        bytemuck::cast_slice::<_, u8>(&a) == bytemuck::cast_slice::<_, u8>(&b),
                        "{} {} workers={workers} frame={frame}",
                        mode.name(),
                        stage.name()
                    );
                    assert_eq!(
                        (
                            astats.visible,
                            astats.reflected,
                            astats.matched,
                            astats.glyph_changes,
                            astats.mean_color_delta.to_bits()
                        ),
                        (
                            bstats.visible,
                            bstats.reflected,
                            bstats.matched,
                            bstats.glyph_changes,
                            bstats.mean_color_delta.to_bits()
                        )
                    );
                }
            }
        }
    }
}
#[test]
fn snapshot_adapter_preserves_scene_contract_when_reused_at_another_tick() {
    let snapshot = LabScene::snapshot(0., Stage::Core);
    for stage in [Stage::Core, Stage::Cloth] {
        let mut a = Composer::default();
        let mut b = Composer::default();
        let camera = Camera::new(Mode::First);
        let (x, _) = a.compose(&snapshot, camera, 2., stage, 47, 29, 16. / 9.);
        let (y, _) = b.compose(&Reference, camera, 2., stage, 47, 29, 16. / 9.);
        assert_eq!(x, y);
    }
}
