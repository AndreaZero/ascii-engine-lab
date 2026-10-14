//! Public-client acceptance checks, independent of the parser's private types.
//! The v1 producer and asset in support/ are frozen verbatim from engine commit
//! a2b3eebbaadbd93889c807903c6d3151272344d1 (the R6 client at the end of R10).
//! They protect the producer and client camera migration; the compositor itself
//! has separate historical regression tests in the workspace.
use ascii_material_room::{SceneDocument, DEFAULT_SCENE_JSON, DEFAULT_SCENE_V2_JSON};
use ascii_render::{
    camera::{Camera, Mode},
    core::{EnvironmentQuery, GlyphCell, Hit, Ray, Scene, Stage},
    frame::{Composer, FrameStats},
};
use glam::Vec3;
use serde_json::{json, Value};

#[allow(dead_code)]
#[path = "support/r6_content.rs"]
mod r6_content;

const FROZEN_V1: &str = include_str!("support/r6_material-room.json");
const MODES: [Mode; 3] = [Mode::First, Mode::Third, Mode::Overhead];

fn cells_bits(cells: &[GlyphCell]) -> Vec<[u32; 8]> {
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

fn stats_bits(stats: &FrameStats) -> (usize, usize, usize, usize, u32) {
    (
        stats.visible,
        stats.reflected,
        stats.matched,
        stats.glyph_changes,
        stats.mean_color_delta.to_bits(),
    )
}

fn hit_bits(hit: Option<Hit>) -> Option<[u32; 12]> {
    hit.map(|hit| {
        [
            hit.distance.to_bits(),
            hit.position.x.to_bits(),
            hit.position.y.to_bits(),
            hit.position.z.to_bits(),
            hit.normal.x.to_bits(),
            hit.normal.y.to_bits(),
            hit.normal.z.to_bits(),
            hit.material,
            hit.object_id,
            hit.local.x.to_bits(),
            hit.local.y.to_bits(),
            hit.local.z.to_bits(),
        ]
    })
}

fn load(value: &Value) -> SceneDocument {
    SceneDocument::from_json(&value.to_string()).unwrap()
}

fn image(scene: &(impl Scene + Sync), camera: Camera, time: f32) -> Vec<[u32; 8]> {
    let (cells, _) =
        Composer::default().compose(scene, camera, time, Stage::Cloth, 43, 27, 16. / 9.);
    cells_bits(&cells)
}

fn render(value: &Value, mode: Mode) -> Vec<[u32; 8]> {
    let document = load(value);
    image(&document.snapshot(0.), document.camera(mode), 0.)
}

// Original host setup, deliberately not delegated to SceneDocument::camera.
fn r6_camera(mode: Mode) -> Camera {
    let mut camera = Camera::new(mode);
    match mode {
        Mode::First => {
            camera.offset = Vec3::new(-1., 0., 1.4);
            camera.orbit = -0.194;
        }
        Mode::Third => camera.zoom = 0.85,
        Mode::Overhead => camera.zoom = 1.,
    }
    camera
}

fn specimen() -> Value {
    json!({
        "version": 2, "name": "Independent client contract specimen",
        "materials": [{
            "id": 32, "name": "Local surface", "pigment": [0.42, 0.25, 0.1],
            "pattern": {"kind": "wood", "scale": 2., "palette": "|:-."},
            "surface": {"roughness": 0.3, "f0": 0.04, "occlusion": 0.85}
        }],
        "objects": [{
            "id": 23, "name": "Analytic box", "shape": "box", "material": 32,
            "position": [0., 0., 0.], "half_extents": [1., 1., 0.5]
        }],
        "light": {"position": [0., 3., 4.], "color": [1., 0.8, 0.6]},
        "cameras": {
            "first": {"eye": [0., 0., 4.], "target": [0., 0., 0.], "fov_y_degrees": 54., "ortho_half_height": 3.},
            "third": {"eye": [4., 2., 5.], "target": [0., 0., 0.], "fov_y_degrees": 43., "ortho_half_height": 3.},
            "overhead": {"eye": [3., 6., 4.], "target": [0., 0., 0.], "fov_y_degrees": 54., "ortho_half_height": 3.}
        },
        "environment": {
            "diffuse_irradiance": [0.35, 0.45, 0.55],
            "specular_radiance": [0.2, 0.3, 0.45], "max_distance": 10.
        }
    })
}

#[test]
fn v1_keeps_original_asset_cells_and_history_in_three_views() {
    // Comparing parsed values avoids making checkout newline conversion an API.
    assert_eq!(
        serde_json::from_str::<Value>(DEFAULT_SCENE_JSON).unwrap(),
        serde_json::from_str::<Value>(FROZEN_V1).unwrap()
    );
    let old = r6_content::SceneDocument::from_json(FROZEN_V1).unwrap();
    let current = SceneDocument::from_json(DEFAULT_SCENE_JSON).unwrap();
    assert_eq!(current.version(), 1);
    assert!(!current.snapshot(0.).local_materials());
    for mode in MODES {
        assert_eq!(r6_camera(mode).pose(), current.camera(mode).pose());
        for workers in [1, 8] {
            for stage in [Stage::Core, Stage::Light, Stage::Water, Stage::Cloth] {
                let mut reference = Composer::with_threads(workers).unwrap();
                let mut actual = Composer::with_threads(workers).unwrap();
                for (frame, time) in [0., 0.17, 1.3, 1.3, 2.6, 2.6].into_iter().enumerate() {
                    if frame == 4 {
                        reference.reset();
                        actual.reset();
                    }
                    let mut old_camera = r6_camera(mode);
                    let mut new_camera = current.camera(mode);
                    old_camera.orbit += frame as f32 * 0.013;
                    new_camera.orbit += frame as f32 * 0.013;
                    let (a, sa) = reference.compose(
                        &old.snapshot(time),
                        old_camera,
                        time,
                        stage,
                        43,
                        27,
                        16. / 9.,
                    );
                    let (b, sb) = actual.compose(
                        &current.snapshot(time),
                        new_camera,
                        time,
                        stage,
                        43,
                        27,
                        16. / 9.,
                    );
                    assert_eq!(
                        cells_bits(&a),
                        cells_bits(&b),
                        "{} {} workers={workers} frame={frame}",
                        mode.name(),
                        stage.name()
                    );
                    assert_eq!(stats_bits(&sa), stats_bits(&sb));
                }
            }
        }
    }
}

#[test]
fn v1_retains_author_order_for_exact_distance_ties() {
    let mut value: Value = serde_json::from_str(FROZEN_V1).unwrap();
    value["objects"] = json!([
        {"id": 90, "name": "First", "shape": "box", "material": 33, "position": [0,0,0], "half_extents": [1,1,1]},
        {"id": 2, "name": "Second", "shape": "box", "material": 32, "position": [0,0,0], "half_extents": [1,1,1]}
    ]);
    let ray = Ray {
        origin: Vec3::new(0., 0., 4.),
        dir: -Vec3::Z,
    };
    let current = load(&value).snapshot(0.);
    let reference = r6_content::SceneDocument::from_json(&value.to_string())
        .unwrap()
        .snapshot(0.);
    let hit = current.trace(ray, 0., Stage::Cloth, true);
    assert_eq!(hit.unwrap().object_id, 90);
    assert_eq!(
        hit_bits(hit),
        hit_bits(reference.trace(ray, 0., Stage::Cloth, true))
    );
}

#[test]
fn packaged_v2_has_bitwise_deterministic_workers_and_static_history() {
    let document = SceneDocument::from_json(DEFAULT_SCENE_V2_JSON).unwrap();
    assert_eq!(document.version(), 2);
    assert!(document.snapshot(0.).local_materials());
    for mode in MODES {
        let mut serial = Composer::with_threads(1).unwrap();
        let mut parallel = Composer::with_threads(8).unwrap();
        let mut previous = None;
        for time in [0., 0.3, 1.2, 1.2, 2.4] {
            let snapshot = document.snapshot(time);
            let (a, sa) = serial.compose(
                &snapshot,
                document.camera(mode),
                time,
                Stage::Cloth,
                53,
                31,
                16. / 9.,
            );
            let (b, sb) = parallel.compose(
                &snapshot,
                document.camera(mode),
                time,
                Stage::Cloth,
                53,
                31,
                16. / 9.,
            );
            let bits = cells_bits(&a);
            assert_eq!(bits, cells_bits(&b), "{} time={time}", mode.name());
            assert_eq!(stats_bits(&sa), stats_bits(&sb));
            assert!(sa.visible > 0);
            if let Some((last_time, last_image)) = previous {
                if time == last_time {
                    assert_eq!(bits, last_image);
                    assert_eq!(sa.glyph_changes, 0);
                    assert_eq!(sa.mean_color_delta.to_bits(), 0_f32.to_bits());
                }
            }
            previous = Some((time, bits));
        }
    }
}

#[test]
fn v2_snapshots_own_pose_and_survive_document_replacement() {
    let mut value = specimen();
    value["objects"][0]["motion"] =
        json!({"translation": {"amplitude": [3., 0., 0.], "speed": 1.}});
    value["light"]["motion"] = json!({"translation": {"amplitude": [0., 0., 2.], "speed": 1.}});
    let document = load(&value);
    let camera = document.camera(Mode::First);
    let frozen = document.snapshot(0.);
    let original = image(&frozen, camera, 0.);
    let later = document.snapshot(std::f32::consts::FRAC_PI_2);
    let time = std::f32::consts::FRAC_PI_2;
    assert_ne!(original, image(&later, camera, time));
    assert_ne!(frozen.light_position(0.), later.light_position(time));
    assert_eq!(image(&frozen, camera, time), image(&later, camera, time));
    drop(document);
    value["materials"][0]["pigment"] = json!([0., 1., 0.]);
    let replacement = load(&value);
    assert_ne!(original, image(&replacement.snapshot(0.), camera, 0.));
    assert_eq!(original, image(&frozen, camera, 0.));
}

#[test]
fn v2_reordering_keeps_ids_tie_winner_and_composition_history() {
    let mut value = specimen();
    let mut material = value["materials"][0].clone();
    material["id"] = json!(71);
    material["pigment"] = json!([0.05, 0.8, 0.1]);
    value["materials"].as_array_mut().unwrap().push(material);
    let mut object = value["objects"][0].clone();
    object["id"] = json!(91);
    object["material"] = json!(71);
    value["objects"].as_array_mut().unwrap().insert(0, object);
    let original = load(&value);
    value["objects"].as_array_mut().unwrap().reverse();
    value["materials"].as_array_mut().unwrap().reverse();
    let reordered = load(&value);
    let ray = Ray {
        origin: Vec3::new(0., 0., 4.),
        dir: -Vec3::Z,
    };
    for document in [&original, &reordered] {
        let hit = document
            .snapshot(0.)
            .trace(ray, 0., Stage::Cloth, true)
            .unwrap();
        assert_eq!((hit.object_id, hit.material), (23, 32));
    }
    for mode in MODES {
        let mut unchanged = Composer::default();
        let mut reloaded = Composer::default();
        for frame in 0..3 {
            let time = frame as f32 / 30.;
            let expected_scene = original.snapshot(time);
            let actual_scene = if frame == 0 {
                original.snapshot(time)
            } else {
                reordered.snapshot(time)
            };
            let (a, sa) = unchanged.compose(
                &expected_scene,
                original.camera(mode),
                time,
                Stage::Cloth,
                43,
                27,
                16. / 9.,
            );
            let (b, sb) = reloaded.compose(
                &actual_scene,
                reordered.camera(mode),
                time,
                Stage::Cloth,
                43,
                27,
                16. / 9.,
            );
            assert_eq!(cells_bits(&a), cells_bits(&b));
            assert_eq!(stats_bits(&sa), stats_bits(&sb));
        }
    }
}

#[test]
fn v2_material_light_and_environment_data_change_the_composed_frame() {
    let base = specimen();
    let changes = [
        ("/materials/0/pigment", json!([0.1, 0.5, 0.6])),
        ("/materials/0/surface/roughness", json!(0.9)),
        ("/materials/0/surface/f0", json!(0.7)),
        ("/materials/0/surface/occlusion", json!(0.1)),
        ("/light/color", json!([0.05, 0.15, 0.4])),
        ("/environment/diffuse_irradiance", json!([1.5, 0.1, 0.2])),
        ("/environment/specular_radiance", json!([1.8, 1.2, 0.2])),
    ];
    for mode in MODES {
        let reference = render(&base, mode);
        for (pointer, value) in &changes {
            let mut changed = base.clone();
            *changed.pointer_mut(pointer).unwrap() = value.clone();
            let actual = render(&changed, mode);
            assert_ne!(reference, actual, "{} must consume {pointer}", mode.name());
            // Material-only edits must not change surface identity or coverage.
            assert_eq!(
                reference.iter().map(|cell| cell[2]).collect::<Vec<_>>(),
                actual.iter().map(|cell| cell[2]).collect::<Vec<_>>()
            );
        }
        let mut metal = base.clone();
        metal["materials"][0]["surface"]["metallic"] =
            json!({"f0": [0.85, 0.5, 0.2], "metalness": 1.});
        assert_ne!(reference, render(&metal, mode));
        metal["materials"][0]["surface"]["metallic"]["metalness"] = json!(0.);
        assert_eq!(
            reference,
            render(&metal, mode),
            "zero metalness must preserve dielectric arithmetic"
        );
        let mut no_environment = base.clone();
        no_environment
            .as_object_mut()
            .unwrap()
            .remove("environment");
        assert_ne!(reference, render(&no_environment, mode));
    }
}

#[test]
fn v2_environment_contract_keeps_irradiance_and_radiance_distinct() {
    let value = specimen();
    let scene = load(&value).snapshot(0.);
    let hit = scene
        .trace(
            Ray {
                origin: Vec3::new(0., 0., 4.),
                dir: -Vec3::Z,
            },
            0.,
            Stage::Cloth,
            true,
        )
        .unwrap();
    let sample = scene.material_sample(hit, 0.1).unwrap();
    assert_eq!(sample.albedo, Vec3::new(0.42, 0.25, 0.1));
    assert_eq!(
        (sample.roughness, sample.f0, sample.occlusion),
        (0.3, 0.04, 0.85)
    );
    assert!(scene.metallic_sample(hit, 0.1).is_none());
    let environment = scene
        .environment_sample(EnvironmentQuery {
            hit,
            normal: hit.normal,
            reflected_direction: Vec3::Z,
            roughness: 0.3,
            time: 0.,
            stage: Stage::Cloth,
        })
        .unwrap();
    assert_eq!(environment.diffuse_irradiance, Vec3::new(0.35, 0.45, 0.55));
    assert_eq!(environment.specular_radiance, Vec3::new(0.2, 0.3, 0.45));
    assert_eq!(environment.max_distance, 10.);
}

#[test]
fn v2_camera_pose_and_lens_are_consumed_from_each_named_view() {
    let base = specimen();
    let document = load(&base);
    for mode in MODES {
        let key = mode.name();
        let camera = document.camera(mode);
        let ray = camera.ray(0, 0, 1, 1, 16. / 9.);
        let (eye, target) = camera.pose();
        let authored_eye = &base["cameras"][key]["eye"];
        let expected_eye = Vec3::new(
            authored_eye[0].as_f64().unwrap() as f32,
            authored_eye[1].as_f64().unwrap() as f32,
            authored_eye[2].as_f64().unwrap() as f32,
        );
        assert!(eye.abs_diff_eq(expected_eye, 1e-6));
        assert!(target.abs_diff_eq(Vec3::ZERO, 1e-6));
        assert!(ray.origin.abs_diff_eq(eye, 1e-6));
        assert!(ray.dir.abs_diff_eq((target - eye).normalize(), 1e-6));
        let mut changed = base.clone();
        if mode == Mode::Overhead {
            changed["cameras"][key]["ortho_half_height"] = json!(1.5);
        } else {
            changed["cameras"][key]["fov_y_degrees"] = json!(80.);
        }
        assert_ne!(render(&base, mode), render(&changed, mode));
        changed["cameras"][key]["target"] = json!([0.8, 0.2, 0.]);
        assert_ne!(render(&base, mode), render(&changed, mode));
        for other in MODES.into_iter().filter(|other| *other != mode) {
            assert_eq!(render(&base, other), render(&changed, other));
        }
    }
}

#[test]
fn v2_rejects_invalid_nested_data_with_specific_paths() {
    let cases = [
        (
            "/materials/0/surface/roughness",
            json!(-0.001),
            "$.materials[0].surface.roughness",
        ),
        (
            "/materials/0/surface/f0",
            json!(1.0000000001),
            "$.materials[0].surface.f0",
        ),
        (
            "/materials/0/surface/occlusion",
            json!("1"),
            "$.materials[0].surface.occlusion",
        ),
        (
            "/environment/diffuse_irradiance",
            json!([0., 17., 0.]),
            "$.environment.diffuse_irradiance[1]",
        ),
        (
            "/environment/specular_radiance",
            json!([0., 0.]),
            "$.environment.specular_radiance",
        ),
        (
            "/environment/max_distance",
            json!(100.01),
            "$.environment.max_distance",
        ),
        ("/light/color", json!([0., -0.1, 1.]), "$.light.color[1]"),
        ("/cameras/first/eye", json!([0., 0., 0.]), "$.cameras.first"),
        (
            "/cameras/third/fov_y_degrees",
            json!(180.),
            "$.cameras.third.fov_y_degrees",
        ),
        (
            "/cameras/overhead/ortho_half_height",
            json!(0.),
            "$.cameras.overhead.ortho_half_height",
        ),
        (
            "/cameras/third/target",
            json!([0., 1e100, 0.]),
            "$.cameras.third.target[1]",
        ),
        ("/environment", Value::Null, "$.environment"),
        ("/version", json!(3), "$.version"),
    ];
    for (pointer, invalid, path) in cases {
        let mut value = specimen();
        *value.pointer_mut(pointer).unwrap() = invalid;
        let error = SceneDocument::from_json(&value.to_string())
            .err()
            .unwrap_or_else(|| panic!("accepted {pointer}"));
        assert!(error.contains(path), "{error}; expected {path}");
    }
    for (pointer, key, path) in [
        ("/materials/0", "surface", "$.materials[0].surface"),
        (
            "/materials/0/surface",
            "roughness",
            "$.materials[0].surface.roughness",
        ),
        ("/cameras", "overhead", "$.cameras.overhead"),
        ("/cameras/third", "target", "$.cameras.third.target"),
    ] {
        let mut value = specimen();
        value
            .pointer_mut(pointer)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .remove(key);
        let error = SceneDocument::from_json(&value.to_string()).err().unwrap();
        assert!(error.contains(path), "{error}; expected {path}");
    }
    for (metallic, path) in [
        (
            json!({"f0": [0.7, 0.5, 1.01], "metalness": 1.}),
            "$.materials[0].surface.metallic.f0[2]",
        ),
        (
            json!({"f0": [0.7, 0.5, 0.3], "metalness": -0.01}),
            "$.materials[0].surface.metallic.metalness",
        ),
        (
            json!({"f0": [0.7, 0.5, 0.3]}),
            "$.materials[0].surface.metallic.metalness",
        ),
        (
            json!({"f0": [0.7, 0.5, 0.3], "metalness": 1., "layer": 2}),
            "$.materials[0].surface.metallic.layer",
        ),
    ] {
        let mut value = specimen();
        value["materials"][0]["surface"]["metallic"] = metallic;
        let error = SceneDocument::from_json(&value.to_string()).err().unwrap();
        assert!(error.contains(path), "{error}; expected {path}");
    }
}

#[test]
fn versions_and_nested_duplicate_keys_cannot_silently_drop_data() {
    let raw = specimen().to_string();
    let duplicate = raw.replace(
        "\"roughness\":0.3",
        "\"roughness\":0.3,\"rough\\u006eess\":0.7",
    );
    assert_ne!(duplicate, raw);
    let error = SceneDocument::from_json(&duplicate).err().unwrap();
    assert!(
        error.contains("$.materials[0].surface.roughness: duplicate field"),
        "{error}"
    );
    let mut value: Value = serde_json::from_str(FROZEN_V1).unwrap();
    value["materials"][0]["surface"] = json!({"roughness": 0.3, "f0": 0.04, "occlusion": 1.});
    let error = SceneDocument::from_json(&value.to_string()).err().unwrap();
    assert!(error.contains("$.materials[0].surface"));
    let mut value = specimen();
    value["cameras"]["first"]["projection"] = json!("hidden mesh");
    let error = SceneDocument::from_json(&value.to_string()).err().unwrap();
    assert!(error.contains("$.cameras.first.projection"));
}
