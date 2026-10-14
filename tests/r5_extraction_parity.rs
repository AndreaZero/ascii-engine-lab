//! Extraction must preserve the R5 courtyard exactly, including history.
pub use ascii_engine_lab::{core, world, LabScene};
#[path = "../evidence/r6/baseline/src/camera.rs"]
pub mod camera;
#[allow(dead_code)]
#[path = "../evidence/r6/baseline/src/frame.rs"]
mod r5_frame;
use ascii_engine_lab::{camera as current_camera, core::Stage, frame::Composer};

#[test]
fn extracted_renderer_preserves_r5_cells_and_history() {
    for mode in [
        camera::Mode::First,
        camera::Mode::Third,
        camera::Mode::Overhead,
    ] {
        for stage in [Stage::Core, Stage::Light, Stage::Water, Stage::Cloth] {
            for workers in [1, 2, 8] {
                for (cols, rows) in [(31, 19), (53, 31)] {
                    let mut reference = r5_frame::Composer::with_threads(workers).unwrap();
                    let mut extracted = Composer::with_threads(workers).unwrap();
                    for (frame, time) in [0., 0.17, 1., 1., 2.6].into_iter().enumerate() {
                        if frame == 3 {
                            reference.reset();
                            extracted.reset();
                        }
                        let mut old = camera::Camera::new(mode);
                        old.orbit = frame as f32 * 0.019;
                        old.zoom = 0.81;
                        let current = current_camera::Camera {
                            mode: current_camera::Mode::parse(mode.name()).unwrap(),
                            orbit: old.orbit,
                            zoom: old.zoom,
                            offset: old.offset,
                            view: None,
                        };
                        let scene = LabScene::snapshot(time, stage);
                        let (a, sa) =
                            reference.compose(&scene, old, time, stage, cols, rows, 16. / 9.);
                        let (b, sb) =
                            extracted.compose(&scene, current, time, stage, cols, rows, 16. / 9.);
                        assert_eq!(
                            a,
                            b,
                            "{} {} workers={workers} frame={frame}",
                            mode.name(),
                            stage.name()
                        );
                        assert_eq!(
                            (
                                sa.visible,
                                sa.reflected,
                                sa.matched,
                                sa.glyph_changes,
                                sa.mean_color_delta.to_bits()
                            ),
                            (
                                sb.visible,
                                sb.reflected,
                                sb.matched,
                                sb.glyph_changes,
                                sb.mean_color_delta.to_bits()
                            )
                        );
                    }
                }
            }
        }
    }
}
