//! Independent traversal acceptance against the exact linear R12 producer.
//! Core primitive arithmetic is deliberately unchanged in R13: this oracle
//! freezes selection, transforms and hooks, not a new intersection definition.
//! In particular, legacy floating-point hits outside ideal bounds must survive.
use ascii_material_room::{coverage_study::coverage_camera, SceneDocument};
pub use ascii_material_room::{DEFAULT_SCENE_JSON, DEFAULT_SCENE_V2_JSON};
use ascii_render::{
    camera::Mode,
    core::{EnvironmentQuery, GlyphCell, Hit, Ray, Scene, Stage},
    frame::{Composer, FrameStats, PrimarySampling},
};
use glam::Vec3;
use serde_json::{json, Value};

#[allow(dead_code)]
#[path = "support/r12_content.rs"]
mod r12_content;

const COVERAGE: &str = include_str!("../assets/coverage-room-v2.json");
const SOURCES: [&str; 3] = [DEFAULT_SCENE_JSON, DEFAULT_SCENE_V2_JSON, COVERAGE];
const MODES: [Mode; 3] = [Mode::First, Mode::Third, Mode::Overhead];
const OFFSETS: [[f32; 2]; 9] = [
    [0., 0.],
    [-1. / 3., -1. / 3.],
    [0., -1. / 3.],
    [1. / 3., -1. / 3.],
    [-1. / 3., 0.],
    [1. / 3., 0.],
    [-1. / 3., 1. / 3.],
    [0., 1. / 3.],
    [1. / 3., 1. / 3.],
];

struct Pair {
    actual: SceneDocument,
    reference: r12_content::SceneDocument,
}
impl Pair {
    fn text(source: &str) -> Self {
        Self {
            actual: SceneDocument::from_json(source).unwrap(),
            reference: r12_content::SceneDocument::from_json(source).unwrap(),
        }
    }
    fn value(value: &Value) -> Self {
        Self::text(&value.to_string())
    }
}

fn hit_bits(hit: Option<Hit>) -> Option<[u32; 12]> {
    hit.map(|h| {
        [
            h.distance.to_bits(),
            h.position.x.to_bits(),
            h.position.y.to_bits(),
            h.position.z.to_bits(),
            h.normal.x.to_bits(),
            h.normal.y.to_bits(),
            h.normal.z.to_bits(),
            h.material,
            h.object_id,
            h.local.x.to_bits(),
            h.local.y.to_bits(),
            h.local.z.to_bits(),
        ]
    })
}
fn cell_bits(cells: &[GlyphCell]) -> Vec<[u32; 8]> {
    cells
        .iter()
        .map(|c| {
            [
                c.glyph,
                c.flags,
                c.id,
                c.pad,
                c.color[0].to_bits(),
                c.color[1].to_bits(),
                c.color[2].to_bits(),
                c.color[3].to_bits(),
            ]
        })
        .collect()
}
fn stats_bits(s: &FrameStats) -> (usize, usize, usize, usize, u32, usize, usize) {
    (
        s.visible,
        s.reflected,
        s.matched,
        s.glyph_changes,
        s.mean_color_delta.to_bits(),
        s.primary_rays,
        s.alternate_primary,
    )
}
fn vector_bits(v: Vec3) -> [u32; 3] {
    v.to_array().map(f32::to_bits)
}

fn compare_ray(actual: &impl Scene, reference: &impl Scene, ray: Ray, time: f32) {
    assert_eq!(
        hit_bits(actual.trace(ray, time, Stage::Light, true)),
        hit_bits(reference.trace(ray, time, Stage::Light, true)),
        "ray={ray:?} time={time}"
    );
}
fn compare_occlusion(actual: &impl Scene, reference: &impl Scene, ray: Ray, time: f32) {
    let hit = reference.trace(ray, time, Stage::Light, false);
    let mut limits = vec![
        f32::NEG_INFINITY,
        -1.,
        -0.,
        0.,
        0.0001,
        0.5,
        100.,
        f32::INFINITY,
        f32::NAN,
    ];
    if let Some(h) = hit {
        assert!(h.distance.is_finite() && h.distance > 0.);
        limits.extend([
            f32::from_bits(h.distance.to_bits() - 1),
            h.distance,
            f32::from_bits(h.distance.to_bits() + 1),
        ]);
    }
    for limit in limits {
        let expected = hit.is_some_and(|h| h.distance < limit);
        // This client admits material IDs >=32, so the emissive exception is
        // absent here. The general Scene contract must still preserve it.
        assert_eq!(reference.occluded(ray, time, Stage::Light, limit), expected);
        assert_eq!(
            actual.occluded(ray, time, Stage::Light, limit),
            expected,
            "ray={ray:?} time={time} limit={limit}"
        );
    }
}

fn object(id: u32, shape: &str, position: [f32; 3], half: [f32; 3], yaw: f32) -> Value {
    json!({"id":id,"name":format!("Object {id}"),"shape":shape,"material":32 + id % 3,
        "position":position,"half_extents":half,"rotation_y":yaw})
}
fn document(version: u64, objects: Vec<Value>) -> Value {
    let mut value = json!({"version":version,"name":"Independent R13 traversal fixture",
        "materials": [
            {"id":32,"name":"Stone","pigment":[0.4,0.3,0.2],"pattern":{"kind":"stone","scale":2.,"palette":"#.:|"}},
            {"id":33,"name":"Wood","pigment":[0.6,0.4,0.2],"pattern":{"kind":"wood","scale":3.,"palette":"|:-."}},
            {"id":34,"name":"Metal","pigment":[0.5,0.5,0.5],"pattern":{"kind":"cloth","scale":1.,"palette":"/\\|."}}
        ],"objects":objects,"light":{"position":[0.,4.,3.]}});
    if version == 2 {
        for material in value["materials"].as_array_mut().unwrap() {
            material["surface"] = json!({"roughness":0.3,"f0":0.04,"occlusion":0.85});
        }
        value["materials"][2]["surface"]["metallic"] = json!({"f0":[0.7,0.75,0.8],"metalness":0.8});
        value["light"]["color"] = json!([1., 0.8, 0.6]);
        value["environment"] = json!({"diffuse_irradiance":[0.3,0.4,0.5],"specular_radiance":[0.2,0.3,0.4],"max_distance":12.});
        let view = json!({"eye":[0.,2.,6.],"target":[0.,0.,0.],"fov_y_degrees":55.,"ortho_half_height":4.});
        value["cameras"] = json!({"first":view,"third":view,"overhead":view});
    }
    value
}

#[test]
fn frozen_workloads_and_three_view_subcell_queries_match_r12() {
    // FNV-1a of the exact Git blobs at 8c27e5c, permitting checkout CRLF only.
    // This protects the workload, independently of both producer parsers.
    let signatures = [
        (2539, 0xb194cbf57560edbd_u64),
        (4021, 0xfc34113a302f11d5),
        (7996, 0xe6dd5a0efeb08f83),
    ];
    for (source, (length, signature)) in SOURCES.into_iter().zip(signatures) {
        let normalized = source.replace("\r\n", "\n");
        let hash = normalized
            .bytes()
            .fold(0xcbf29ce484222325_u64, |hash, byte| {
                (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
            });
        assert_eq!((normalized.len(), hash), (length, signature));
        let pair = Pair::text(source);
        for time in [0., 0.17, 1.3, 3., 6.] {
            let actual = pair.actual.snapshot(time);
            let reference = pair.reference.snapshot(time);
            for mode in MODES {
                let camera = coverage_camera(pair.reference.camera(mode), time);
                assert_eq!(
                    pair.actual.camera(mode).pose(),
                    pair.reference.camera(mode).pose()
                );
                for y in 0..19 {
                    for x in 0..31 {
                        for offset in OFFSETS {
                            let ray = camera.ray_sample(x, y, 31, 19, 16. / 9., offset).unwrap();
                            compare_ray(&actual, &reference, ray, time);
                            if (y * 31 + x) % 53 == 0 {
                                compare_occlusion(&actual, &reference, ray, time);
                                for scale in [0.25, 4.] {
                                    let scaled = Ray {
                                        dir: ray.dir * scale,
                                        ..ray
                                    };
                                    compare_ray(&actual, &reference, scaled, time);
                                    compare_occlusion(&actual, &reference, scaled, time);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn grazing_inside_parallel_nonunit_and_invalid_rays_preserve_legacy_results() {
    let below_one = f32::from_bits(1_f32.to_bits() - 1);
    let above_one = f32::from_bits(1_f32.to_bits() + 1);
    let below_parallel = f32::from_bits(1e-8_f32.to_bits() - 1);
    let above_parallel = f32::from_bits(1e-8_f32.to_bits() + 1);
    for shape in ["box", "ellipsoid"] {
        for yaw in [0., 0.37, std::f32::consts::FRAC_PI_4, 1024.] {
            let pair = Pair::value(&document(2, vec![object(1, shape, [0.; 3], [1.; 3], yaw)]));
            let actual = pair.actual.snapshot(0.);
            let reference = pair.reference.snapshot(0.);
            for origin in [
                Vec3::ZERO,
                Vec3::X,
                -Vec3::X,
                Vec3::ONE,
                Vec3::new(0., 0., 4.),
                Vec3::new(1., 0., 4.),
                Vec3::new(1., 1., 4.),
                Vec3::new(below_one, 0., 4.),
                Vec3::new(above_one, 0., 4.),
            ] {
                for dir in [
                    Vec3::X,
                    -Vec3::X,
                    Vec3::Y,
                    -Vec3::Y,
                    Vec3::Z,
                    -Vec3::Z,
                    Vec3::new(-0., 0., -1.),
                    Vec3::new(below_parallel, 0., -1.),
                    Vec3::new(1e-8, 0., -1.),
                    Vec3::new(above_parallel, 0., -1.),
                    Vec3::new(1., 1., -1.),
                ] {
                    for scale in [0.25, 1., 4.] {
                        let ray = Ray {
                            origin,
                            dir: dir * scale,
                        };
                        compare_ray(&actual, &reference, ray, 0.);
                        compare_occlusion(&actual, &reference, ray, 0.);
                    }
                }
            }
            for dir in [
                Vec3::ZERO,
                Vec3::splat(f32::NAN),
                Vec3::splat(f32::INFINITY),
                Vec3::splat(f32::MAX),
                Vec3::splat(f32::MIN_POSITIVE),
            ] {
                let ray = Ray {
                    origin: Vec3::ZERO,
                    dir,
                };
                assert!(reference.trace(ray, 0., Stage::Core, true).is_none());
                compare_ray(&actual, &reference, ray, 0.);
                compare_occlusion(&actual, &reference, ray, 0.);
            }
            for origin in [Vec3::splat(f32::NAN), Vec3::splat(f32::INFINITY)] {
                compare_ray(
                    &actual,
                    &reference,
                    Ray {
                        origin,
                        dir: -Vec3::Z,
                    },
                    0.,
                );
            }
        }
    }
}

#[test]
fn numerical_legacy_hits_outside_ideal_bounds_are_not_pruned() {
    let cases = [
        (
            "ellipsoid",
            Ray {
                origin: Vec3::new(10000., 1.01, 0.),
                dir: -Vec3::X,
            },
        ),
        (
            "box",
            Ray {
                origin: Vec3::new(0., 0., -1e9),
                dir: Vec3::new(5e-9, 0., 1.),
            },
        ),
    ];
    for (shape, ray) in cases {
        // Additional distant primitives prevent a small-object linear shortcut
        // from being the only path exercised by these adversarial cases.
        let mut objects = vec![object(1, shape, [0.; 3], [1.; 3], 0.)];
        for id in 2..=128 {
            objects.push(object(
                id,
                "box",
                [100., id as f32 * 3., 100.],
                [0.2; 3],
                0.,
            ));
        }
        let pair = Pair::value(&document(2, objects));
        let actual = pair.actual.snapshot(0.);
        let reference = pair.reference.snapshot(0.);
        let legacy = reference.trace(ray, 0., Stage::Light, true).unwrap();
        assert_eq!(legacy.object_id, 1);
        assert!(
            legacy.position.abs().max_element() > 1.,
            "Retain the numerical R12 hit"
        );
        compare_ray(&actual, &reference, ray, 0.);
        compare_occlusion(&actual, &reference, ray, 0.);
    }
}

#[test]
fn exact_ties_preserve_v1_author_order_and_v2_id_priority_across_many_objects() {
    let ray = Ray {
        origin: Vec3::new(0., 0., 4.),
        dir: -Vec3::Z,
    };
    for version in [1, 2] {
        for reverse in [false, true] {
            let first = object(99, "box", [0.; 3], [1.; 3], 0.);
            // The ellipsoid is exactly tangent at the box's front plane. Both
            // distances are 3, with normals +Z versus -X and distinct materials.
            let second = object(1, "ellipsoid", [1., 0., 1.], [1.; 3], 0.);
            let mut objects = if reverse {
                vec![second, first]
            } else {
                vec![first, second]
            };
            for id in 2..=128 {
                if id != 99 {
                    objects.push(object(
                        id,
                        "ellipsoid",
                        [10. + id as f32, 5., -5.],
                        [0.2; 3],
                        0.,
                    ));
                }
            }
            let pair = Pair::value(&document(version, objects));
            let actual = pair.actual.snapshot(0.);
            let reference = pair.reference.snapshot(0.);
            let expected = if version == 1 && !reverse { 99 } else { 1 };
            assert_eq!(
                reference
                    .trace(ray, 0., Stage::Light, true)
                    .unwrap()
                    .object_id,
                expected
            );
            for stage in [Stage::Core, Stage::Light, Stage::Water, Stage::Cloth] {
                for include_water in [false, true] {
                    assert_eq!(
                        hit_bits(actual.trace(ray, 0., stage, include_water)),
                        hit_bits(reference.trace(ray, 0., stage, include_water))
                    );
                }
            }
            compare_occlusion(&actual, &reference, ray, 0.);
        }
    }
}

#[test]
fn empty_scenes_and_strict_occlusion_thresholds_keep_parametric_distance() {
    for objects in [
        vec![],
        vec![object(1, "box", [0.; 3], [1.; 3], 0.)],
        vec![
            object(1, "ellipsoid", [0.; 3], [1.; 3], 0.),
            object(2, "box", [0., 0., -4.], [1.; 3], 0.),
        ],
    ] {
        let pair = Pair::value(&document(2, objects));
        let actual = pair.actual.snapshot(0.);
        let reference = pair.reference.snapshot(0.);
        for scale in [0.25, 1., 4.] {
            let ray = Ray {
                origin: Vec3::new(0., 0., 4.),
                dir: -Vec3::Z * scale,
            };
            compare_ray(&actual, &reference, ray, 0.);
            compare_occlusion(&actual, &reference, ray, 0.);
            if let Some(hit) = reference.trace(ray, 0., Stage::Light, false) {
                assert_eq!(hit.distance.to_bits(), (3. / scale).to_bits());
                assert!(!actual.occluded(ray, 0., Stage::Light, hit.distance));
                assert!(actual.occluded(
                    ray,
                    0.,
                    Stage::Light,
                    f32::from_bits(hit.distance.to_bits() + 1)
                ));
            }
        }
    }
}

#[test]
fn data_extremes_and_motion_fallback_rebuild_without_changing_old_snapshots() {
    for shape in ["box", "ellipsoid"] {
        for (position, half) in [
            ([10000., -10000., 10000.], [0.0001, 1., 10000.]),
            ([-10000., 10000., -10000.], [10000., 0.0001, 1.]),
            ([0.; 3], [0.0001; 3]),
        ] {
            let mut primitive = object(1, shape, position, half, 1024.);
            primitive["motion"] = json!({"translation":{"amplitude":[1000.,-1000.,1000.],"speed":64.,"phase":10000.},
                "yaw":{"amplitude":1024.,"speed":-64.,"phase":-10000.}});
            let pair = Pair::value(&document(2, vec![primitive]));
            let actual = pair.actual.snapshot(0.);
            let reference = pair.reference.snapshot(0.);
            let retained = actual.clone();
            let _later = pair.actual.snapshot(3.);
            for time in [-f32::MAX, -2., -0., 0., 0.17, 3., f32::MAX] {
                let fresh = pair.actual.snapshot(time);
                let frozen_fresh = pair.reference.snapshot(time);
                for origin in [
                    Vec3::from_array(position),
                    Vec3::ZERO,
                    Vec3::new(0., 0., 20000.),
                ] {
                    for dir in [
                        Vec3::X,
                        -Vec3::X,
                        Vec3::Y,
                        -Vec3::Y,
                        Vec3::Z,
                        -Vec3::Z,
                        Vec3::new(0.25, -0.5, 1.),
                        Vec3::new(4., 2., -1.),
                    ] {
                        let ray = Ray { origin, dir };
                        compare_ray(&actual, &reference, ray, time);
                        compare_occlusion(&actual, &reference, ray, time);
                        assert_eq!(
                            hit_bits(fresh.trace(ray, time, Stage::Light, true)),
                            hit_bits(frozen_fresh.trace(ray, time, Stage::Light, true))
                        );
                        assert_eq!(
                            hit_bits(actual.trace(ray, time, Stage::Light, true)),
                            hit_bits(fresh.trace(ray, time, Stage::Light, true))
                        );
                        assert_eq!(
                            hit_bits(retained.trace(ray, 0., Stage::Light, true)),
                            hit_bits(reference.trace(ray, 0., Stage::Light, true))
                        );
                    }
                }
                assert_eq!(
                    vector_bits(actual.light_position(time)),
                    vector_bits(reference.light_position(time))
                );
            }
        }
    }
}

#[test]
fn complete_material_hooks_match_the_frozen_producer() {
    let pair = Pair::value(&document(
        2,
        (1..=3)
            .map(|id| object(id, "box", [id as f32 * 3., 0., 0.], [1.; 3], 0.))
            .collect(),
    ));
    let actual = pair.actual.snapshot(0.);
    let reference = pair.reference.snapshot(0.);
    assert_eq!(actual.local_materials(), reference.local_materials());
    assert_eq!(
        vector_bits(actual.light_color()),
        vector_bits(reference.light_color())
    );
    for id in 1..=3 {
        let ray = Ray {
            origin: Vec3::new(id as f32 * 3., 0., 4.),
            dir: -Vec3::Z,
        };
        let hit = reference.trace(ray, 0., Stage::Light, true).unwrap();
        compare_ray(&actual, &reference, ray, 0.);
        assert_eq!(
            vector_bits(actual.pigment(hit.material)),
            vector_bits(reference.pigment(hit.material))
        );
        for footprint in [0., 0.01, 0.04, 0.2, 1., f32::INFINITY] {
            let a = actual.material_sample(hit, footprint).unwrap();
            let b = reference.material_sample(hit, footprint).unwrap();
            assert_eq!(
                (
                    vector_bits(a.albedo),
                    vector_bits(a.normal),
                    a.roughness.to_bits(),
                    a.f0.to_bits(),
                    a.occlusion.to_bits()
                ),
                (
                    vector_bits(b.albedo),
                    vector_bits(b.normal),
                    b.roughness.to_bits(),
                    b.f0.to_bits(),
                    b.occlusion.to_bits()
                )
            );
            let metallic = |m: Option<ascii_render::core::MetallicSample>| {
                m.map(|m| (vector_bits(m.f0), m.metalness.to_bits()))
            };
            assert_eq!(
                metallic(actual.metallic_sample(hit, footprint)),
                metallic(reference.metallic_sample(hit, footprint))
            );
            assert_eq!(
                actual.glyph_sample_key(hit, footprint),
                reference.glyph_sample_key(hit, footprint)
            );
            for illumination in [0., 0.01, 0.3, 1.] {
                assert_eq!(
                    actual.glyph_sample(hit, illumination, footprint),
                    reference.glyph_sample(hit, illumination, footprint)
                );
            }
            let query = EnvironmentQuery {
                hit,
                normal: hit.normal,
                reflected_direction: Vec3::Z,
                roughness: 0.5,
                time: 0.,
                stage: Stage::Light,
            };
            let a = actual.environment_sample(query).unwrap();
            let b = reference.environment_sample(query).unwrap();
            assert_eq!(
                (
                    vector_bits(a.diffuse_irradiance),
                    vector_bits(a.specular_radiance),
                    a.max_distance.to_bits()
                ),
                (
                    vector_bits(b.diffuse_irradiance),
                    vector_bits(b.specular_radiance),
                    b.max_distance.to_bits()
                )
            );
        }
    }
}

#[test]
fn consecutive_rotation_groups_preserve_queries_after_misses_and_motion() {
    for version in [1, 2] {
        let mut objects = vec![
            object(1, "box", [100., 0., 0.], [1.; 3], 0.),
            object(2, "box", [101., 0., 0.], [1.; 3], 0.),
            object(3, "box", [100., 0., 0.], [1.; 3], 0.37),
            object(4, "box", [0.; 3], [1.; 3], 0.37),
            object(5, "ellipsoid", [0., 0., -4.], [1.; 3], 0.),
            object(6, "box", [-100., 0., 0.], [1.; 3], -0.37),
            object(7, "ellipsoid", [0., 0., -8.], [1.; 3], -0.37),
        ];
        objects[3]["motion"] = json!({"yaw":{"amplitude":0.2,"speed":1.,"phase":0.}});
        let pair = Pair::value(&document(version, objects));
        let actual = pair.actual.snapshot(0.);
        let reference = pair.reference.snapshot(0.);
        for time in [0., 0.3, 3., 6.] {
            for x in -8..=8 {
                for y in -4..=4 {
                    for scale in [0.25, 1., 4.] {
                        let ray = Ray {
                            origin: Vec3::new(0., 0., 5.),
                            dir: Vec3::new(x as f32 * 0.05, y as f32 * 0.05, -1.) * scale,
                        };
                        compare_ray(&actual, &reference, ray, time);
                        compare_occlusion(&actual, &reference, ray, time);
                    }
                }
            }
        }
    }
}

#[test]
fn composed_cells_history_and_all_counters_match_r12_in_both_sampling_modes() {
    for source in SOURCES {
        let pair = Pair::text(source);
        for mode in MODES {
            for sampling in [PrimarySampling::Center, PrimarySampling::Conservative9] {
                for workers in [1, 8] {
                    let mut actual = Composer::with_threads(workers).unwrap();
                    let mut reference = Composer::with_threads(workers).unwrap();
                    actual.set_primary_sampling(sampling);
                    reference.set_primary_sampling(sampling);
                    for (frame, time) in [0., 0.17, 1.3, 1.3, 3., 6.].into_iter().enumerate() {
                        if frame == 4 {
                            actual.reset();
                            reference.reset();
                        }
                        let camera = coverage_camera(pair.reference.camera(mode), time);
                        let (a, sa) = actual.compose(
                            &pair.actual.snapshot(time),
                            camera,
                            time,
                            Stage::Cloth,
                            31,
                            19,
                            16. / 9.,
                        );
                        let (b, sb) = reference.compose(
                            &pair.reference.snapshot(time),
                            camera,
                            time,
                            Stage::Cloth,
                            31,
                            19,
                            16. / 9.,
                        );
                        assert_eq!(
                            cell_bits(&a),
                            cell_bits(&b),
                            "{} {sampling:?} workers={workers} frame={frame}",
                            mode.name()
                        );
                        assert_eq!(stats_bits(&sa), stats_bits(&sb));
                        if frame == 3 {
                            assert_eq!(sa.glyph_changes, 0);
                        }
                    }
                }
            }
        }
    }
}
