//! Frozen R1 compositor remains an independent oracle on legacy R4 content.
//! R5 adds structural history invalidation for the new figure; those deliberate
//! changes have focused current-content tests rather than false legacy parity.
//! Keep evidence/r2/baseline/src immutable; it is not linked into the application.
pub use ascii_engine_lab::core;
#[allow(dead_code)]
#[path = "../examples/support/r4_actor.rs"]
pub mod actor;
#[path = "../evidence/r2/baseline/src/camera.rs"]
pub mod camera;
#[allow(dead_code)]
#[path = "../evidence/r5/baseline/src/world.rs"]
mod legacy_world;
#[allow(dead_code)]
#[path = "../evidence/r2/baseline/src/frame.rs"]
mod r1_frame;

use ascii_engine_lab::{camera as current_camera, frame::Composer};
use core::Stage;

pub struct LabScene;
impl core::Scene for LabScene {
    fn trace(&self, r: core::Ray, t: f32, s: Stage, w: bool) -> Option<core::Hit> {
        legacy_world::trace(r, t, s, w)
    }
    fn light_position(&self, t: f32) -> glam::Vec3 {
        legacy_world::light_position(t)
    }
    fn pigment(&self, m: u32) -> glam::Vec3 {
        legacy_world::pigment(m)
    }
    fn glyph(&self, m: u32, l: glam::Vec3, n: glam::Vec3, i: f32) -> u8 {
        legacy_world::glyph(m, l, n, i)
    }
}

#[test]
fn current_compositor_preserves_r1_bytes_and_history_on_legacy_content() {
    for mode in [
        camera::Mode::First,
        camera::Mode::Third,
        camera::Mode::Overhead,
    ] {
        for stage in [Stage::Core, Stage::Light, Stage::Water, Stage::Cloth] {
            for threads in [1, 4, 16] {
                let mut reference = r1_frame::Composer::default();
                let mut actual = Composer::with_threads(threads).unwrap();
                for (frame, time) in [0., 0.016, 0.43, 1., 1., 2.6].into_iter().enumerate() {
                    if frame == 4 {
                        reference.reset();
                        actual.reset();
                    }
                    let mut old_camera = camera::Camera::new(mode);
                    old_camera.orbit = frame as f32 * 0.004;
                    old_camera.zoom = 0.93;
                    let new_camera = current_camera::Camera {
                        mode: current_camera::Mode::parse(mode.name()).unwrap(),
                        orbit: old_camera.orbit,
                        offset: old_camera.offset,
                        zoom: old_camera.zoom,
                    };
                    let (old, old_stats) =
                        reference.compose(&LabScene, old_camera, time, stage, 41, 25, 16. / 9.);
                    let (new, new_stats) =
                        actual.compose(&LabScene, new_camera, time, stage, 41, 25, 16. / 9.);
                    assert!(
                        bytemuck::cast_slice::<_, u8>(&old) == bytemuck::cast_slice::<_, u8>(&new),
                        "R1 parity: {} {} workers={threads} frame={frame}",
                        mode.name(),
                        stage.name()
                    );
                    assert_eq!(
                        (
                            old_stats.visible,
                            old_stats.reflected,
                            old_stats.matched,
                            old_stats.glyph_changes
                        ),
                        (
                            new_stats.visible,
                            new_stats.reflected,
                            new_stats.matched,
                            new_stats.glyph_changes
                        )
                    );
                    assert_eq!(
                        old_stats.mean_color_delta.to_bits(),
                        new_stats.mean_color_delta.to_bits()
                    );
                }
            }
        }
    }
}
