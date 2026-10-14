//! Independent semantic acceptance for the opt-in primary visibility policy.
//! Frozen camera/composer sources are exact copies of engine 9866c35 (R11).
//! Geometry below uses analytic planes and strips, not the production selector.
pub use ascii_render::core;
#[allow(dead_code)]
#[path = "support/r11/camera.rs"]
mod camera;
#[allow(dead_code)]
#[path = "support/r11/frame.rs"]
mod r11_frame;

use ascii_render::{
    camera::{Camera, CameraView, Mode},
    core::{GlyphCell, Hit, MaterialSample, Ray, Scene, Stage},
    frame::{Composer, FrameStats, PrimarySampling},
};
use glam::Vec3;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Mutex,
};

const MODES: [Mode; 3] = [Mode::First, Mode::Third, Mode::Overhead];

#[derive(Clone, Copy)]
struct Patch {
    z: f32,
    x: f32,
    y: f32,
    half_x: f32,
    half_y: f32,
    diagonal: f32,
    material: u32,
    object: u32,
}
impl Patch {
    fn wall(z: f32, material: u32) -> Self {
        Self {
            z,
            x: 0.,
            y: 0.,
            half_x: 100.,
            half_y: 100.,
            diagonal: 0.,
            material,
            object: material,
        }
    }

    fn hit(self, ray: Ray) -> Option<Hit> {
        if ray.dir.z.abs() < 1e-8 {
            return None;
        }
        let distance = (self.z - ray.origin.z) / ray.dir.z;
        if !distance.is_finite() || distance <= 0.0001 {
            return None;
        }
        let point = ray.origin + ray.dir * distance;
        if (point.x - self.diagonal * point.y - self.x).abs() > self.half_x
            || (point.y - self.y).abs() > self.half_y
        {
            return None;
        }
        Some(Hit {
            distance,
            position: point,
            normal: Vec3::Z,
            material: self.material,
            object_id: self.object,
            local: Vec3::new(point.x, point.y, 0.),
        })
    }
}

struct Planes {
    patches: Vec<Patch>,
    coordinate_glyph: bool,
    record_material: bool,
    traces: AtomicUsize,
    footprints: Mutex<Vec<f32>>,
}
impl Planes {
    fn new(patches: Vec<Patch>) -> Self {
        Self {
            patches,
            coordinate_glyph: false,
            record_material: false,
            traces: AtomicUsize::new(0),
            footprints: Mutex::new(Vec::new()),
        }
    }
}
impl Scene for Planes {
    fn trace(&self, ray: Ray, _: f32, _: Stage, _: bool) -> Option<Hit> {
        self.traces.fetch_add(1, Ordering::Relaxed);
        self.patches
            .iter()
            .filter_map(|patch| patch.hit(ray))
            .min_by(|a, b| a.distance.total_cmp(&b.distance))
    }
    fn light_position(&self, _: f32) -> Vec3 {
        Vec3::new(0., 4., 3.)
    }
    fn pigment(&self, _: u32) -> Vec3 {
        Vec3::splat(0.8)
    }
    fn glyph(&self, material: u32, local: Vec3, _: Vec3, _: f32) -> u8 {
        if self.coordinate_glyph {
            if local.x > 0.043 {
                b'X'
            } else {
                b'Y'
            }
        } else {
            match material {
                32 => b'F',
                33 => b'B',
                34 => b'O',
                _ => b'Q',
            }
        }
    }
    fn local_materials(&self) -> bool {
        self.record_material
    }
    fn material_sample(&self, hit: Hit, footprint: f32) -> Option<MaterialSample> {
        self.footprints.lock().unwrap().push(footprint);
        Some(MaterialSample {
            albedo: self.pigment(hit.material),
            normal: hit.normal,
            roughness: 0.5,
            f0: 0.04,
            occlusion: 1.,
        })
    }
}

fn view(mode: Mode) -> Camera {
    Camera::with_view(
        mode,
        CameraView::new(Vec3::new(0., 0., 4.), Vec3::ZERO, 90., 1.).unwrap(),
    )
}

fn bits(cells: &[GlyphCell]) -> Vec<[u32; 8]> {
    cells
        .iter()
        .map(|cell| {
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
        })
        .collect()
}

fn render(scene: &Planes, camera: Camera, sampling: PrimarySampling) -> (GlyphCell, FrameStats) {
    let mut composer = Composer::default();
    composer.set_primary_sampling(sampling);
    let (cells, stats) = composer.compose(scene, camera, 0., Stage::Core, 1, 1, 1.);
    (cells[0], stats)
}

fn side_at_plane(mode: Mode) -> f32 {
    // Analytic projected x at z=0 for a +1/3-cell horizontal sample.
    if mode == Mode::Overhead {
        2. / 3.
    } else {
        8. / 3.
    }
}

#[test]
fn foreground_between_centers_is_recovered_even_with_a_background_hit() {
    for mode in MODES {
        let stripe = Patch {
            x: side_at_plane(mode),
            half_x: 0.04,
            ..Patch::wall(0., 32)
        };
        let scene = Planes::new(vec![stripe, Patch::wall(-2., 33)]);
        let (center, center_stats) = render(&scene, view(mode), PrimarySampling::Center);
        let (nine, nine_stats) = render(&scene, view(mode), PrimarySampling::Conservative9);
        assert_eq!(center.glyph, b'B' as u32);
        assert_eq!(
            nine.glyph,
            b'F' as u32,
            "{} must recover the strip",
            mode.name()
        );
        assert_eq!(
            (center_stats.primary_rays, center_stats.alternate_primary),
            (1, 0)
        );
        assert_eq!(
            (nine_stats.primary_rays, nine_stats.alternate_primary),
            (9, 1)
        );
        assert_eq!(
            scene.traces.load(Ordering::Relaxed),
            10,
            "Core fixture has no secondary rays"
        );
    }
}

#[test]
fn diagonal_corner_feature_is_sampled_and_a_full_occluder_never_leaks_it() {
    for mode in MODES {
        let diagonal = Patch {
            x: 2. * side_at_plane(mode),
            half_x: 0.04,
            diagonal: 1.,
            ..Patch::wall(0., 32)
        };
        let visible = Planes::new(vec![diagonal, Patch::wall(-2., 33)]);
        assert_eq!(
            render(&visible, view(mode), PrimarySampling::Center)
                .0
                .glyph,
            b'B' as u32
        );
        assert_eq!(
            render(&visible, view(mode), PrimarySampling::Conservative9)
                .0
                .glyph,
            b'F' as u32
        );
        let blocked = Planes::new(vec![diagonal, Patch::wall(1., 34), Patch::wall(-2., 33)]);
        let (cell, stats) = render(&blocked, view(mode), PrimarySampling::Conservative9);
        assert_eq!(cell.glyph, b'O' as u32);
        assert_eq!(stats.alternate_primary, 0);
    }
}

#[test]
fn foreground_priority_uses_forward_depth_instead_of_oblique_ray_length() {
    for mode in [Mode::First, Mode::Third] {
        let foreground = Patch {
            x: 8. / 3.,
            half_x: 0.04,
            ..Patch::wall(0., 32)
        };
        let background = Patch::wall(-0.1, 33);
        let oblique = Ray {
            origin: Vec3::new(0., 0., 4.),
            dir: Vec3::new(2. / 3., 0., -1.).normalize(),
        };
        assert!(foreground.hit(oblique).unwrap().distance > 4.1);
        let scene = Planes::new(vec![foreground, background]);
        assert_eq!(
            render(&scene, view(mode), PrimarySampling::Conservative9)
                .0
                .glyph,
            b'F' as u32
        );
    }
}

#[test]
fn exact_depth_ties_use_semantic_ids_instead_of_sample_or_scene_order() {
    let left = Patch {
        x: -2. / 3.,
        half_x: 0.04,
        object: 91,
        ..Patch::wall(0., 32)
    };
    let right = Patch {
        x: 2. / 3.,
        half_x: 0.04,
        object: 23,
        ..Patch::wall(0., 35)
    };
    for patches in [vec![left, right], vec![right, left]] {
        let scene = Planes::new(patches);
        let (cell, stats) = render(&scene, view(Mode::Overhead), PrimarySampling::Conservative9);
        assert_eq!(cell.glyph, b'Q' as u32);
        assert_eq!(stats.alternate_primary, 1);
    }
}

#[test]
fn a_uniform_surface_keeps_its_center_sample_and_full_cell_material_footprint() {
    for mode in MODES {
        let mut scene = Planes::new(vec![Patch::wall(0., 32)]);
        scene.record_material = true;
        let (a, _) = render(&scene, view(mode), PrimarySampling::Center);
        let center_width = scene.footprints.lock().unwrap().pop().unwrap();
        let (b, stats) = render(&scene, view(mode), PrimarySampling::Conservative9);
        let nine_width = scene.footprints.lock().unwrap().pop().unwrap();
        assert_eq!(bits(&[a]), bits(&[b]));
        assert_eq!(center_width.to_bits(), nine_width.to_bits());
        assert_eq!(stats.alternate_primary, 0);
    }
    let mut side_scene = Planes::new(vec![Patch {
        x: 2. / 3.,
        half_x: 0.04,
        ..Patch::wall(0., 32)
    }]);
    side_scene.record_material = true;
    render(
        &side_scene,
        view(Mode::Overhead),
        PrimarySampling::Conservative9,
    );
    let widths = side_scene.footprints.lock().unwrap();
    assert_eq!(
        widths.len(),
        1,
        "Only the selected semantic surface is shaded"
    );
    assert!(
        (widths[0] - 2.).abs() < 1e-5,
        "The footprint is one full cell, not one subcell"
    );
}

#[test]
fn selected_sample_changes_release_history_inside_the_same_surface_bin() {
    let camera = Camera::with_view(
        Mode::Overhead,
        CameraView::new(
            Vec3::new(0.04, 0.04, 4.),
            Vec3::new(0.04, 0.04, 0.),
            90.,
            0.01,
        )
        .unwrap(),
    );
    let mut old_scene = Planes::new(vec![Patch {
        x: 0.04 + 0.02 / 3.,
        y: 0.04,
        half_x: 0.0005,
        half_y: 0.0005,
        ..Patch::wall(0., 32)
    }]);
    old_scene.coordinate_glyph = true;
    let mut new_scene = Planes::new(vec![Patch {
        x: 0.04,
        y: 0.04 + 0.02 / 3.,
        half_x: 0.0005,
        half_y: 0.0005,
        ..Patch::wall(0., 32)
    }]);
    new_scene.coordinate_glyph = true;
    let mut warmed = Composer::default();
    warmed.set_primary_sampling(PrimarySampling::Conservative9);
    let (before, _) = warmed.compose(&old_scene, camera, 0., Stage::Core, 1, 1, 1.);
    let (after, _) = warmed.compose(&new_scene, camera, 1., Stage::Core, 1, 1, 1.);
    let fresh = render(&new_scene, camera, PrimarySampling::Conservative9).0;
    assert_eq!(
        before[0].id, after[0].id,
        "Both selected points intentionally share a local bin"
    );
    assert_eq!(before[0].glyph, b'X' as u32);
    assert_eq!(after[0].glyph, b'Y' as u32);
    assert_eq!(
        bits(&after),
        bits(&[fresh]),
        "A glyph from the old primary sample must not be held"
    );
}

#[test]
fn disocclusion_and_removal_leave_no_stale_semantic_glyphs() {
    let front = Patch {
        x: 2. / 3.,
        half_x: 0.04,
        ..Patch::wall(0., 32)
    };
    let scenes = [
        Planes::new(vec![front, Patch::wall(-2., 33)]),
        Planes::new(vec![Patch::wall(-2., 33)]),
        Planes::new(vec![]),
        Planes::new(vec![front]),
    ];
    let mut warmed = Composer::default();
    warmed.set_primary_sampling(PrimarySampling::Conservative9);
    for (frame, scene) in scenes.iter().enumerate() {
        let (actual, _) = warmed.compose(
            scene,
            view(Mode::Overhead),
            frame as f32,
            Stage::Core,
            1,
            1,
            1.,
        );
        let fresh = render(scene, view(Mode::Overhead), PrimarySampling::Conservative9).0;
        assert_eq!(bits(&actual), bits(&[fresh]), "transition {frame}");
        assert_eq!(actual[0].glyph, b"FB F"[frame] as u32);
    }
}

#[test]
fn policy_change_resets_history_but_reselecting_the_policy_keeps_it() {
    let scene = Planes::new(vec![Patch::wall(0., 32)]);
    let mut composer = Composer::default();
    assert_eq!(composer.primary_sampling(), PrimarySampling::Center);
    composer.compose(&scene, view(Mode::Overhead), 0., Stage::Core, 1, 1, 1.);
    composer.set_primary_sampling(PrimarySampling::Conservative9);
    assert_eq!(composer.primary_sampling(), PrimarySampling::Conservative9);
    assert_eq!(
        composer
            .compose(&scene, view(Mode::Overhead), 0., Stage::Core, 1, 1, 1.)
            .1
            .matched,
        0
    );
    composer.set_primary_sampling(PrimarySampling::Conservative9);
    assert_eq!(
        composer
            .compose(&scene, view(Mode::Overhead), 0., Stage::Core, 1, 1, 1.)
            .1
            .matched,
        1
    );
    composer.set_primary_sampling(PrimarySampling::Center);
    assert_eq!(
        composer
            .compose(&scene, view(Mode::Overhead), 0., Stage::Core, 1, 1, 1.)
            .1
            .matched,
        0
    );
}

#[test]
fn nine_samples_still_miss_features_between_them_and_can_expand_apparent_support() {
    let camera = view(Mode::Overhead);
    // A real strip at x=1/3 lies between the center and x=2/3 samples.
    let missed = Planes::new(vec![Patch {
        x: 1. / 3.,
        half_x: 0.01,
        ..Patch::wall(0., 32)
    }]);
    let ray = Ray {
        origin: Vec3::new(1. / 3., 0., 4.),
        dir: -Vec3::Z,
    };
    assert!(missed.trace(ray, 0., Stage::Core, true).is_some());
    assert_eq!(
        render(&missed, camera, PrimarySampling::Conservative9)
            .0
            .glyph,
        b' ' as u32
    );
    // A 0.02-unit strip occupies 1% of this 2-unit-wide cell, but selects a full
    // semantic glyph. This deliberately records the limit; it is not area AA.
    let narrow = Planes::new(vec![Patch {
        x: 2. / 3.,
        half_x: 0.01,
        ..Patch::wall(0., 32)
    }]);
    assert_eq!(
        render(&narrow, camera, PrimarySampling::Conservative9)
            .0
            .glyph,
        b'F' as u32
    );
}

#[test]
fn moving_inputs_are_bitwise_deterministic_across_workers_and_static_repeats() {
    for mode in MODES {
        let mut serial = Composer::with_threads(1).unwrap();
        let mut parallel = Composer::with_threads(8).unwrap();
        serial.set_primary_sampling(PrimarySampling::Conservative9);
        parallel.set_primary_sampling(PrimarySampling::Conservative9);
        let mut previous = None;
        for position in [-0.4, -0.2, 0., 0.2, 0.2, 0.4] {
            let scene = Planes::new(vec![
                Patch {
                    x: position,
                    half_x: 0.03,
                    diagonal: 0.2,
                    ..Patch::wall(0., 32)
                },
                Patch::wall(-2., 33),
            ]);
            let (a, sa) = serial.compose(&scene, view(mode), 0., Stage::Core, 31, 19, 16. / 9.);
            let (b, sb) = parallel.compose(&scene, view(mode), 0., Stage::Core, 31, 19, 16. / 9.);
            assert_eq!(bits(&a), bits(&b));
            assert_eq!(
                (
                    sa.visible,
                    sa.matched,
                    sa.glyph_changes,
                    sa.mean_color_delta.to_bits(),
                    sa.primary_rays,
                    sa.alternate_primary
                ),
                (
                    sb.visible,
                    sb.matched,
                    sb.glyph_changes,
                    sb.mean_color_delta.to_bits(),
                    sb.primary_rays,
                    sb.alternate_primary
                )
            );
            if let Some((last_position, last_bits)) = previous {
                if position == last_position {
                    assert_eq!(bits(&a), last_bits);
                    assert_eq!(sa.glyph_changes, 0);
                }
            }
            previous = Some((position, bits(&a)));
        }
    }
}

#[test]
fn default_composition_preserves_frozen_r11_cells_and_statistics() {
    for mode in MODES {
        for workers in [1, 8] {
            let mut current = Composer::with_threads(workers).unwrap();
            let mut frozen = r11_frame::Composer::with_threads(workers).unwrap();
            for (frame, position) in [-0.4, 0., 0.3, 0.3].into_iter().enumerate() {
                let scene = Planes::new(vec![
                    Patch {
                        x: position,
                        half_x: 0.1,
                        ..Patch::wall(0., 32)
                    },
                    Patch::wall(-2., 33),
                ]);
                let mut new_camera = view(mode);
                new_camera.orbit = frame as f32 * 0.017;
                let old_view =
                    camera::CameraView::new(Vec3::new(0., 0., 4.), Vec3::ZERO, 90., 1.).unwrap();
                let mut old_camera =
                    camera::Camera::with_view(camera::Mode::parse(mode.name()).unwrap(), old_view);
                old_camera.orbit = new_camera.orbit;
                let (a, sa) =
                    current.compose(&scene, new_camera, 0., Stage::Light, 31, 19, 16. / 9.);
                let (b, sb) =
                    frozen.compose(&scene, old_camera, 0., Stage::Light, 31, 19, 16. / 9.);
                assert_eq!(bits(&a), bits(&b), "{} frame={frame}", mode.name());
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
                assert_eq!(sa.primary_rays, 31 * 19);
                assert_eq!(sa.alternate_primary, 0);
            }
        }
    }
}
