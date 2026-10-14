//! Camera persistence acceptance through the actual public ray generators.
use ascii_material_room::{
    authoring_camera::CameraBank, SceneDocument, DEFAULT_SCENE_JSON, DEFAULT_SCENE_V2_JSON,
};
use ascii_render::{
    camera::{Camera, CameraView, Mode},
    core::Ray,
};
use glam::Vec3;
use serde_json::{json, Value};

const MODES: [Mode; 3] = [Mode::First, Mode::Third, Mode::Overhead];

fn ray_bits(ray: Ray) -> [u32; 6] {
    [
        ray.origin.x.to_bits(),
        ray.origin.y.to_bits(),
        ray.origin.z.to_bits(),
        ray.dir.x.to_bits(),
        ray.dir.y.to_bits(),
        ray.dir.z.to_bits(),
    ]
}

fn assert_camera_bits(actual: Camera, expected: Camera) {
    assert_eq!(actual.mode, expected.mode);
    assert_eq!(actual.orbit.to_bits(), expected.orbit.to_bits());
    assert_eq!(actual.zoom.to_bits(), expected.zoom.to_bits());
    assert_eq!(
        actual.offset.to_array().map(f32::to_bits),
        expected.offset.to_array().map(f32::to_bits)
    );
    for (cols, rows, aspect) in [(1, 1, 1.), (17, 11, 16. / 9.), (240, 135, 1.5)] {
        for (x, y) in [
            (0, 0),
            (cols - 1, 0),
            (0, rows - 1),
            (cols - 1, rows - 1),
            (cols / 2, rows / 2),
        ] {
            assert_eq!(
                ray_bits(actual.ray(x, y, cols, rows, aspect)),
                ray_bits(expected.ray(x, y, cols, rows, aspect))
            );
            for offset in [
                [0., 0.],
                [-1. / 3., 1. / 3.],
                [1. / 3., -1. / 3.],
                [-0.5, -0.5],
                [0.5, 0.5],
            ] {
                assert_eq!(
                    ray_bits(actual.ray_sample(x, y, cols, rows, aspect, offset).unwrap()),
                    ray_bits(
                        expected
                            .ray_sample(x, y, cols, rows, aspect, offset)
                            .unwrap()
                    ),
                    "mode={:?}, grid={cols}x{rows}, cell=({x},{y}), offset={offset:?}",
                    actual.mode
                );
            }
        }
    }
}

fn reloaded(document: &SceneDocument, bank: &CameraBank) -> CameraBank {
    // Exercise both saved scene bases and controls through text, not Value clones.
    let text = serde_json::to_string_pretty(&json!({
        "scene": document.source(),
        "camera_controls": bank.to_json(),
    }))
    .unwrap();
    let parsed: Value = serde_json::from_str(&text).unwrap();
    let restored_document = SceneDocument::from_json(&parsed["scene"].to_string()).unwrap();
    CameraBank::from_json(&restored_document, &parsed["camera_controls"]).unwrap()
}

#[test]
fn defaults_and_controls_roundtrip_exact_rays_in_all_three_views() {
    for text in [DEFAULT_SCENE_JSON, DEFAULT_SCENE_V2_JSON] {
        let document = SceneDocument::from_json(text).unwrap();
        let defaults = CameraBank::defaults(&document);
        let restored = reloaded(&document, &defaults);
        for mode in MODES {
            assert_camera_bits(restored.get(mode), document.camera(mode));
        }
        for (orbit, offset, zoom) in [
            (-0.0, Vec3::new(-0.0, 0., -0.0), 1.),
            (0.37, Vec3::new(0.1234567, -2.75, 6.25), 0.85),
            (-17.25, Vec3::new(-9999.5, 9000., 9999.5), 2.75),
            (-1024., Vec3::splat(-10000.), 0.01),
            (1024., Vec3::splat(10000.), 100.),
        ] {
            let mut bank = defaults.clone();
            for mode in MODES {
                let mut camera = bank.get(mode);
                camera.orbit = orbit;
                camera.offset = offset;
                camera.zoom = zoom;
                bank.set(camera).unwrap();
            }
            let restored = reloaded(&document, &bank);
            for mode in MODES {
                assert_camera_bits(restored.get(mode), bank.get(mode));
            }
        }
    }
}

fn near_vertical_document() -> SceneDocument {
    let mut source: Value = serde_json::from_str(DEFAULT_SCENE_V2_JSON).unwrap();
    for mode in MODES {
        source["cameras"][mode.name()] = json!({
            "eye": [0., 0., 0.],
            "target": [0.0002, 1., 0.],
            "fov_y_degrees": 73.,
            "ortho_half_height": 2.5,
        });
    }
    SceneDocument::from_json(&source.to_string()).unwrap()
}

#[test]
fn large_translation_saves_the_rendered_basis_instead_of_a_rounded_world_pose() {
    let document = near_vertical_document();
    let mut bank = CameraBank::defaults(&document);
    for mode in MODES {
        let mut camera = bank.get(mode);
        camera.offset = Vec3::splat(9999.);
        bank.set(camera).unwrap();
        let (eye, target) = camera.pose();
        assert_eq!(eye.x.to_bits(), target.x.to_bits());
        assert!(
            CameraView::new(eye, target, 73., 2.5).is_err(),
            "baking the displayed world points would destroy this valid view"
        );
        let ray = camera.ray(0, 0, 1, 1, 1.);
        assert!(ray.dir.is_finite() && ray.dir.x > 0.);
    }
    let restored = reloaded(&document, &bank);
    for mode in MODES {
        assert_camera_bits(restored.get(mode), bank.get(mode));
    }
}

#[test]
fn json_requires_complete_known_records_and_bounded_finite_controls() {
    let document = SceneDocument::from_json(DEFAULT_SCENE_V2_JSON).unwrap();
    let base = CameraBank::defaults(&document).to_json();
    let mut cases = vec![Value::Null, json!([]), json!({})];
    let mut missing_mode = base.clone();
    missing_mode.as_object_mut().unwrap().remove("overhead");
    cases.push(missing_mode);
    let mut extra_mode = base.clone();
    extra_mode["fourth"] = json!({});
    cases.push(extra_mode);
    for mode in MODES {
        for field in ["orbit", "offset", "zoom"] {
            let mut missing = base.clone();
            missing[mode.name()].as_object_mut().unwrap().remove(field);
            cases.push(missing);
        }
        for (field, value) in [
            ("orbit", json!(1024.01)),
            ("orbit", json!(-1024.01)),
            ("orbit", json!("0")),
            ("orbit", Value::Null),
            ("offset", json!([0, 0])),
            ("offset", json!([0, 0, 0, 0])),
            ("offset", json!([10000.0001, 0, 0])),
            ("offset", json!([0, -10000.0001, 0])),
            ("offset", json!([0, 0, "0"])),
            ("offset", json!({"x":0,"y":0,"z":0})),
            ("zoom", json!(0.)),
            ("zoom", json!(-1.)),
            ("zoom", json!(0.009)),
            ("zoom", json!(100.000001)),
            ("zoom", json!(1e100)),
            ("pitch", json!(0.)),
        ] {
            let mut invalid = base.clone();
            invalid[mode.name()][field] = value;
            cases.push(invalid);
        }
    }
    for value in cases {
        let error = CameraBank::from_json(&document, &value).unwrap_err();
        assert!(error.contains("$.camera_controls"), "{error}");
    }
}

#[test]
fn rejected_sets_leave_every_slot_unchanged() {
    let document = SceneDocument::from_json(DEFAULT_SCENE_V2_JSON).unwrap();
    let mut bank = CameraBank::defaults(&document);
    let original = bank.clone();
    for mode in MODES {
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            for field in 0..5 {
                let mut camera = bank.get(mode);
                match field {
                    0 => camera.orbit = bad,
                    1..=3 => camera.offset[field - 1] = bad,
                    _ => camera.zoom = bad,
                }
                assert!(bank.set(camera).is_err());
            }
        }
        for (orbit, offset, zoom) in [
            (1025., Vec3::ZERO, 1.),
            (-1025., Vec3::ZERO, 1.),
            (0., Vec3::splat(10001.), 1.),
            (0., Vec3::splat(-10001.), 1.),
            (0., Vec3::ZERO, 0.),
            (0., Vec3::ZERO, 101.),
        ] {
            let mut camera = bank.get(mode);
            camera.orbit = orbit;
            camera.offset = offset;
            camera.zoom = zoom;
            assert!(bank.set(camera).is_err());
        }
    }
    assert_eq!(bank.to_json(), original.to_json());
    for mode in MODES {
        assert_camera_bits(bank.get(mode), original.get(mode));
    }
}

#[test]
fn changed_base_views_are_rejected_including_signed_zero_and_projection() {
    let document = near_vertical_document();
    let mut bank = CameraBank::defaults(&document);
    let original = bank.to_json();
    for mode in MODES {
        let camera = bank.get(mode);
        let view = camera.view.unwrap();
        for changed_view in [
            None,
            Some(CameraView::new(Vec3::new(-0., 0., 0.), view.target(), 73., 2.5).unwrap()),
            Some(CameraView::new(view.eye(), view.target(), 74., 2.5).unwrap()),
            Some(CameraView::new(view.eye(), view.target(), 73., 3.).unwrap()),
        ] {
            let changed = Camera {
                view: changed_view,
                ..camera
            };
            assert!(bank.set(changed).unwrap_err().contains("base view"));
            assert_eq!(bank.to_json(), original);
        }
    }
}

#[test]
fn editing_one_camera_slot_does_not_reset_the_other_views_or_the_cloned_bank() {
    let document = SceneDocument::from_json(DEFAULT_SCENE_V2_JSON).unwrap();
    let mut bank = CameraBank::defaults(&document);
    let before = bank.clone();
    let mut first = bank.get(Mode::First);
    first.orbit = 0.71;
    first.offset = Vec3::new(1., 2., 3.);
    first.zoom = 0.9;
    bank.set(first).unwrap();
    assert_camera_bits(bank.get(Mode::First), first);
    for mode in [Mode::Third, Mode::Overhead] {
        assert_camera_bits(bank.get(mode), before.get(mode));
    }
    for mode in MODES {
        assert_camera_bits(before.get(mode), document.camera(mode));
    }
}
