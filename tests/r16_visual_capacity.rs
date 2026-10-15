//! Independent behavior probes for the R16 authored producer.
//! Deliberately avoids reproducing its intersection or region algorithms.
#[path = "../examples/support/authored_figure.rs"]
mod authored_figure;

use ascii_render::{
    camera::{Camera, CameraView, Mode},
    core::{EnvironmentQuery, EnvironmentSample, Hit, MaterialSample, Ray, Scene, Stage},
    frame::{surface_id, Composer, PrimarySampling},
};
use authored_figure::{FigureDefinition, FigureSnapshot};
use glam::Vec3;

struct ProbeScene(FigureSnapshot);

impl Scene for ProbeScene {
    fn trace(&self, ray: Ray, _: f32, _: Stage, _: bool) -> Option<Hit> {
        self.0.trace(ray)
    }
    fn light_position(&self, _: f32) -> Vec3 {
        Vec3::new(-2., 4., 4.)
    }
    fn pigment(&self, material: u32) -> Vec3 {
        self.0.pigment(material)
    }
    fn glyph(&self, _: u32, _: Vec3, _: Vec3, _: f32) -> u8 {
        b'?'
    }
    fn local_materials(&self) -> bool {
        true
    }
    fn material_sample(&self, hit: Hit, footprint: f32) -> Option<MaterialSample> {
        self.0.material_sample(hit, footprint)
    }
    fn glyph_sample(&self, hit: Hit, illumination: f32, footprint: f32) -> u8 {
        self.0.glyph_sample(hit, illumination, footprint)
    }
    fn glyph_sample_key(&self, hit: Hit, footprint: f32) -> u32 {
        self.0.glyph_sample_key(hit, footprint)
    }
}

fn assert_same_hit(a: Option<Hit>, b: Option<Hit>) {
    match (a, b) {
        (None, None) => {}
        (Some(a), Some(b)) => {
            assert_eq!(a.distance.to_bits(), b.distance.to_bits());
            assert_eq!(a.position, b.position);
            assert_eq!(a.local, b.local);
            assert_eq!(a.normal, b.normal);
            assert_eq!(a.object_id, b.object_id);
            assert_eq!(a.material, b.material);
        }
        _ => panic!("snapshot visibility changed"),
    }
}

fn fixture() -> serde_json::Value {
    serde_json::json!({
        "version":1,"name":"independent probes",
        "materials":[{"id":32,"color":[0.5,0.5,0.5],"feature_scale":0.1}],
        "parts":[{"id":100,"material":32,"center":[0,0,0],"radii":[1,1,1]}],
        "panels":[]
    })
}

fn definition(value: &serde_json::Value) -> FigureDefinition {
    FigureDefinition::from_json(&value.to_string()).unwrap()
}

#[test]
fn invalid_resource_identity_and_numeric_ranges_are_rejected() {
    let base = fixture();
    for (pointer, value) in [
        ("/version", serde_json::json!(2)),
        ("/materials/0/id", serde_json::json!(3)),
        ("/materials/0/feature_scale", serde_json::json!(0)),
        ("/materials/0/color", serde_json::json!([1.1, 0.5, 0.5])),
        ("/parts/0/id", serde_json::json!(99)),
        ("/parts/0/material", serde_json::json!(33)),
        ("/parts/0/radii", serde_json::json!([-1, 1, 1])),
        ("/parts/0/center", serde_json::json!([5, 0, 0])),
    ] {
        let mut value_json = base.clone();
        *value_json.pointer_mut(pointer).unwrap() = value;
        assert!(
            FigureDefinition::from_json(&value_json.to_string()).is_err(),
            "{pointer}"
        );
    }
    let mut duplicate = base.clone();
    duplicate["parts"] = serde_json::json!([base["parts"][0], base["parts"][0]]);
    assert!(FigureDefinition::from_json(&duplicate.to_string()).is_err());
    let mut oversized = base.clone();
    oversized["parts"] = serde_json::Value::Array(
        (0..49)
            .map(|i| {
                let mut part = base["parts"][0].clone();
                part["id"] = (100 + i).into();
                part
            })
            .collect(),
    );
    assert!(FigureDefinition::from_json(&oversized.to_string()).is_err());
    assert!(FigureDefinition::from_json(&" ".repeat(262145)).is_err());
    let mut unknown = base;
    unknown["texture"] = "unsupported".into();
    assert!(FigureDefinition::from_json(&unknown.to_string()).is_err());
}

#[test]
fn unit_sphere_has_known_axis_intersections_and_bounds() {
    let snapshot = definition(&fixture()).snapshot(0.);
    assert_eq!(snapshot.bounds(), (-Vec3::ONE, Vec3::ONE));
    for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
        for sign in [-1., 1.] {
            let direction = axis * sign;
            let hit = snapshot
                .trace(Ray {
                    origin: direction * 3.,
                    dir: -direction,
                })
                .unwrap();
            assert!((hit.distance - 2.).abs() < 1e-5);
            assert!((hit.position - direction).length() < 1e-5);
            assert!((hit.normal - direction).length() < 1e-5);
            assert!((hit.local - direction).length() < 1e-5);
            assert_eq!(hit.object_id, 100);
        }
    }
    assert!(snapshot
        .trace(Ray {
            origin: Vec3::new(2., 0., 3.),
            dir: -Vec3::Z
        })
        .is_none());
}

#[test]
fn flat_panel_is_two_sided_and_respects_its_authored_edges() {
    let mut value = fixture();
    value["parts"][0]["center"] = serde_json::json!([3, 0, 0]);
    value["parts"][0]["radii"] = serde_json::json!([0.1, 0.1, 0.1]);
    value["panels"] = serde_json::json!([{
        "id":130,"material":32,"origin":[0,0,0],"height":1,
        "bottom":0.5,"top":0.5,"amplitude":0,"sway":0,"slope":0
    }]);
    let snapshot = definition(&value).snapshot(0.7);
    for sign in [-1., 1.] {
        let ray = Ray {
            origin: Vec3::new(0.2, 0.5, sign * 2.),
            dir: -Vec3::Z * sign,
        };
        let hit = snapshot.trace(ray).unwrap();
        assert_eq!(hit.object_id, 130);
        assert!((hit.distance - 2.).abs() < 1e-4);
        assert!((hit.local - Vec3::new(0.4, 0., 0.)).length() < 1e-4);
        assert!(hit.normal.dot(ray.dir) < -0.999);
    }
    for point in [
        Vec3::new(0.6, 0.5, 2.),
        Vec3::new(0., 1.1, 2.),
        Vec3::new(0., -0.1, 2.),
    ] {
        assert!(snapshot
            .trace(Ray {
                origin: point,
                dir: -Vec3::Z
            })
            .is_none());
    }
}

#[test]
fn old_snapshots_remain_unchanged_after_new_motion_snapshots() {
    let mut value = fixture();
    value["parts"][0]["motion"] = serde_json::json!([0, 0, 0.1]);
    value["parts"][0]["speed"] = serde_json::json!(1.0);
    let definition = definition(&value);
    let old = definition.snapshot(0.);
    let ray = Ray {
        origin: Vec3::new(0., 0., 3.),
        dir: -Vec3::Z,
    };
    let before = old.trace(ray);
    let moved = definition.snapshot(std::f32::consts::FRAC_PI_2);
    assert!((moved.trace(ray).unwrap().distance - 1.9).abs() < 1e-5);
    assert_same_hit(before, old.trace(ray));
    assert_same_hit(before, definition.snapshot(f32::NAN).trace(ray));
    assert_same_hit(before, definition.snapshot(f32::INFINITY).trace(ray));
}

#[test]
fn structural_and_lod_keys_distinguish_required_symbol_transitions() {
    let snapshot = definition(&fixture()).snapshot(0.);
    let edge = snapshot
        .trace(Ray {
            origin: Vec3::Y * 3.,
            dir: -Vec3::Y,
        })
        .unwrap();
    let body = snapshot
        .trace(Ray {
            origin: Vec3::Z * 3.,
            dir: -Vec3::Z,
        })
        .unwrap();
    assert_ne!(
        snapshot.glyph_sample(edge, 0.5, 0.01),
        snapshot.glyph_sample(body, 0.5, 0.01)
    );
    assert_ne!(
        snapshot.glyph_sample_key(edge, 0.01),
        snapshot.glyph_sample_key(body, 0.01)
    );
    assert_ne!(
        snapshot.glyph_sample(edge, 0.5, 0.01),
        snapshot.glyph_sample(edge, 0.5, 0.2)
    );
    assert_ne!(
        snapshot.glyph_sample_key(edge, 0.01),
        snapshot.glyph_sample_key(edge, 0.2)
    );
}

// Controlled semantic hits isolate integration of the real producer's region
// rules with Composer history. This is not a real geometric motion trajectory.
struct RegionTransitionScene {
    producer: FigureSnapshot,
    local_y: f32,
    suppress_key: bool,
}

impl RegionTransitionScene {
    fn hit(&self) -> Hit {
        Hit {
            distance: 1.,
            position: Vec3::ZERO,
            normal: Vec3::Z,
            material: 32,
            object_id: 100,
            local: Vec3::new(0., self.local_y, 0.),
        }
    }
}

impl Scene for RegionTransitionScene {
    fn trace(&self, _: Ray, _: f32, _: Stage, _: bool) -> Option<Hit> {
        Some(self.hit())
    }
    fn light_position(&self, _: f32) -> Vec3 {
        Vec3::new(0., 0., 3.)
    }
    fn light_color(&self) -> Vec3 {
        Vec3::ZERO
    }
    fn pigment(&self, material: u32) -> Vec3 {
        self.producer.pigment(material)
    }
    fn glyph(&self, _: u32, _: Vec3, _: Vec3, _: f32) -> u8 {
        panic!("the real producer's local glyph path is required")
    }
    fn local_materials(&self) -> bool {
        true
    }
    fn material_sample(&self, hit: Hit, footprint: f32) -> Option<MaterialSample> {
        self.producer.material_sample(hit, footprint)
    }
    fn glyph_sample(&self, hit: Hit, illumination: f32, footprint: f32) -> u8 {
        self.producer.glyph_sample(hit, illumination, footprint)
    }
    fn glyph_sample_key(&self, hit: Hit, footprint: f32) -> u32 {
        if self.suppress_key {
            0
        } else {
            self.producer.glyph_sample_key(hit, footprint)
        }
    }
    fn environment_sample(&self, _: EnvironmentQuery) -> Option<EnvironmentSample> {
        Some(EnvironmentSample {
            diffuse_irradiance: Vec3::splat(std::f32::consts::PI * 0.5),
            specular_radiance: Vec3::ZERO,
            max_distance: 0.,
        })
    }
}

#[test]
fn producer_region_change_releases_composer_history_inside_the_same_surface_bin() {
    let producer = definition(&fixture()).snapshot(0.);
    let before = RegionTransitionScene {
        producer: producer.clone(),
        local_y: 0.875,
        suppress_key: false,
    };
    let after = RegionTransitionScene {
        producer: producer.clone(),
        local_y: 0.885,
        suppress_key: false,
    };
    assert_eq!(surface_id(before.hit()), surface_id(after.hit()));
    assert_ne!(
        producer.glyph_sample_key(before.hit(), 0.02),
        producer.glyph_sample_key(after.hit(), 0.02)
    );
    // The narrow projection keeps the actual computed footprint in the fine
    // region. No footprint or glyph result is substituted in the proxy hooks.
    let camera = Camera::with_view(
        Mode::First,
        CameraView::new(Vec3::Z, Vec3::ZERO, 1., 1.).unwrap(),
    );
    let compose = |composer: &mut Composer, scene: &RegionTransitionScene| {
        composer
            .compose(scene, camera, 0., Stage::Cloth, 1, 1, 1.)
            .0
    };
    let mut held = Composer::default();
    let old = compose(&mut held, &before);
    let updated = compose(&mut held, &after);
    let fresh = compose(&mut Composer::default(), &after);
    assert_ne!(old[0].glyph, 32);
    assert_eq!(fresh[0].glyph, u32::from(b'='));
    assert_eq!(old[0].id, fresh[0].id);
    assert_eq!(old[0].color, fresh[0].color);
    assert_ne!(old[0].glyph, fresh[0].glyph);
    assert_eq!(updated, fresh);

    // Negative control proves the transition would be incorrectly held if
    // the structural key were absent, rather than passing via another guard.
    let without_key_before = RegionTransitionScene {
        suppress_key: true,
        ..before
    };
    let without_key_after = RegionTransitionScene {
        suppress_key: true,
        ..after
    };
    let mut blind = Composer::default();
    let initial = compose(&mut blind, &without_key_before);
    let wrongly_held = compose(&mut blind, &without_key_after);
    assert_eq!(initial[0].glyph, wrongly_held[0].glyph);
    assert_ne!(wrongly_held[0].glyph, fresh[0].glyph);
}

const WARDEN: &str = include_str!("../examples/assets/figures/warden.json");
const COURIER: &str = include_str!("../examples/assets/figures/courier.json");

#[test]
fn authored_variants_differ_in_hit_geometry_not_only_color() {
    let a = FigureDefinition::from_json(WARDEN).unwrap().snapshot(0.);
    let b = FigureDefinition::from_json(COURIER).unwrap().snapshot(0.);
    let mut differences = 0;
    for y in 0..60 {
        for x in -24..=24 {
            let ray = Ray {
                origin: Vec3::new(x as f32 * 0.025, y as f32 * 0.04, 4.),
                dir: -Vec3::Z,
            };
            let first = a.trace(ray).map(|h| h.distance.to_bits());
            let second = b.trace(ray).map(|h| h.distance.to_bits());
            differences += usize::from(first != second);
        }
    }
    assert!(
        differences > 0,
        "color changes cannot satisfy geometric variation"
    );
}

#[test]
fn authored_hits_are_finite_and_contained_in_derived_bounds() {
    for text in [WARDEN, COURIER] {
        let definition = FigureDefinition::from_json(text).unwrap();
        for time in [0., 0.7, 1.8] {
            let snapshot = definition.snapshot(time);
            let (min, max) = snapshot.bounds();
            let mut hits = 0;
            for side in [-1., 1.] {
                for y in 0..60 {
                    for x in -24..=24 {
                        let ray = Ray {
                            origin: Vec3::new(x as f32 * 0.025, y as f32 * 0.04, side * 4.),
                            dir: -Vec3::Z * side,
                        };
                        if let Some(hit) = snapshot.trace(ray) {
                            hits += 1;
                            assert!(hit.distance.is_finite() && hit.distance > 0.);
                            assert!(
                                hit.position.is_finite()
                                    && hit.normal.is_finite()
                                    && hit.local.is_finite()
                            );
                            assert!((hit.normal.length() - 1.).abs() < 1e-4);
                            assert!(hit.position.cmpge(min - Vec3::splat(1e-4)).all());
                            assert!(hit.position.cmple(max + Vec3::splat(1e-4)).all());
                        }
                    }
                }
            }
            assert!(hits > 0);
        }
    }
}

#[test]
fn static_cells_match_one_and_eight_workers_before_and_after_reset() {
    for text in [WARDEN, COURIER] {
        let scene = ProbeScene(FigureDefinition::from_json(text).unwrap().snapshot(0.7));
        let view =
            CameraView::new(Vec3::new(0., 1.1, 4.), Vec3::new(0., 1., 0.), 45., 1.4).unwrap();
        for mode in [Mode::First, Mode::Third, Mode::Overhead] {
            for sampling in [PrimarySampling::Center, PrimarySampling::Conservative9] {
                let camera = Camera::with_view(mode, view);
                let mut serial = Composer::with_threads(1).unwrap();
                let mut parallel = Composer::with_threads(8).unwrap();
                serial.set_primary_sampling(sampling);
                parallel.set_primary_sampling(sampling);
                let mut frozen = None;
                for pass in 0..3 {
                    if pass == 2 {
                        serial.reset();
                        parallel.reset();
                    }
                    let (a, _) =
                        serial.compose(&scene, camera, 0.7, Stage::Cloth, 48, 27, 16. / 9.);
                    let (b, _) =
                        parallel.compose(&scene, camera, 0.7, Stage::Cloth, 48, 27, 16. / 9.);
                    assert_eq!(a, b);
                    assert!(a.iter().any(|c| c.id != 0));
                    if let Some(previous) = &frozen {
                        assert_eq!(&a, previous);
                    } else {
                        frozen = Some(a);
                    }
                }
            }
        }
    }
}
