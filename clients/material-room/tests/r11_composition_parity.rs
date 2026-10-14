//! Freeze the compositor too: a producer-only oracle cannot detect a shared
//! compositor regression. Sources below are exact R11 copies from 9866c35.
pub use ascii_render::core;
#[allow(dead_code)]
#[path = "../../../crates/ascii-render/tests/support/r11/camera.rs"]
mod camera;
#[allow(dead_code)]
#[path = "../../../crates/ascii-render/tests/support/r11/frame.rs"]
mod r11_frame;

use ascii_material_room::{SceneDocument, DEFAULT_SCENE_JSON, DEFAULT_SCENE_V2_JSON};
use ascii_render::{
    camera::{Camera, Mode},
    core::Stage,
    frame::{Composer, PrimarySampling},
};

fn old_camera(current: Camera) -> camera::Camera {
    let mode = camera::Mode::parse(current.mode.name()).unwrap();
    let mut old = if let Some(view) = current.view {
        camera::Camera::with_view(
            mode,
            camera::CameraView::new(
                view.eye(),
                view.target(),
                view.vertical_fov_degrees(),
                view.orthographic_half_height(),
            )
            .unwrap(),
        )
    } else {
        camera::Camera::new(mode)
    };
    old.orbit = current.orbit;
    old.offset = current.offset;
    old.zoom = current.zoom;
    old
}

#[test]
fn default_r12_matches_r11_with_v1_and_v2_environment_materials_and_history() {
    for source in [DEFAULT_SCENE_JSON, DEFAULT_SCENE_V2_JSON] {
        let document = SceneDocument::from_json(source).unwrap();
        for mode in [Mode::First, Mode::Third, Mode::Overhead] {
            for workers in [1, 8] {
                let mut current = Composer::with_threads(workers).unwrap();
                let mut frozen = r11_frame::Composer::with_threads(workers).unwrap();
                assert_eq!(current.primary_sampling(), PrimarySampling::Center);
                for (frame, time) in [0., 0.31, 1.2, 1.2, 2.4].into_iter().enumerate() {
                    if frame == 3 {
                        current.reset();
                        frozen.reset();
                    }
                    let mut cam = document.camera(mode);
                    cam.orbit += frame as f32 * 0.021;
                    let scene = document.snapshot(time);
                    let (a, sa) =
                        current.compose(&scene, cam, time, Stage::Cloth, 43, 27, 16. / 9.);
                    let (b, sb) = frozen.compose(
                        &scene,
                        old_camera(cam),
                        time,
                        Stage::Cloth,
                        43,
                        27,
                        16. / 9.,
                    );
                    assert_eq!(a.len(), b.len());
                    for (index, (actual, expected)) in a.iter().zip(&b).enumerate() {
                        assert_eq!(
                            cell_bits(actual),
                            cell_bits(expected),
                            "v{} {} workers={workers} frame={frame} cell={index}",
                            document.version(),
                            mode.name()
                        );
                    }
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
                    assert_eq!(sa.primary_rays, 43 * 27);
                    assert_eq!(sa.alternate_primary, 0);
                }
            }
        }
    }
}

fn cell_bits(cell: &core::GlyphCell) -> [u32; 8] {
    [
        cell.glyph,
        cell.flags,
        cell.id,
        cell.pad,
        cell.color[0].to_bits(),
        cell.color[1].to_bits(),
        cell.color[2].to_bits(),
        cell.color[3].to_bits(),
    ]
}
