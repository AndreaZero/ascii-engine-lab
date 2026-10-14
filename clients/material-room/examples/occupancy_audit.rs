//! Untimed, independent R14 acceptance. Expected states are rebuilt from JSON
//! with the frozen R12 producer, without using the runtime transaction logic.
//! The probes verify actual Composer query paths, not just changed pictures.
use ascii_material_room::{
    runtime::{Operation, RuntimeScene, RuntimeSnapshot, Transform},
    SceneDocument,
};
#[cfg(test)]
pub const DEFAULT_SCENE_JSON: &str = ascii_material_room::DEFAULT_SCENE_JSON;
#[cfg(test)]
pub const DEFAULT_SCENE_V2_JSON: &str = ascii_material_room::DEFAULT_SCENE_V2_JSON;
use ascii_render::{
    camera::{Camera, CameraView, Mode},
    core::{
        EnvironmentQuery, EnvironmentSample, GlyphCell, Hit, MaterialSample, MetallicSample, Ray,
        Scene, Stage,
    },
    frame::{Composer, PrimarySampling},
};
use glam::Vec3;
use serde_json::{json, Value};
use std::{collections::BTreeSet, path::PathBuf, sync::Mutex};

#[allow(dead_code)]
#[path = "../tests/support/r12_content.rs"]
mod reference;

pub const ROOM: &str = include_str!("../assets/dynamic-room-v2.json");
pub const UPDATES: &str = include_str!("../assets/dynamic-updates-v1.json");
const MODES: [Mode; 3] = [Mode::First, Mode::Third, Mode::Overhead];
const POLICIES: [PrimarySampling; 2] = [PrimarySampling::Center, PrimarySampling::Conservative9];
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

pub fn hit_bits(hit: Option<Hit>) -> Option<[u32; 12]> {
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
pub fn cell_bits(cells: &[GlyphCell]) -> Vec<[u32; 8]> {
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
fn vector(value: &Value) -> Vec3 {
    Vec3::new(
        value[0].as_f64().unwrap() as f32,
        value[1].as_f64().unwrap() as f32,
        value[2].as_f64().unwrap() as f32,
    )
}
fn parse_operation(runtime: &RuntimeScene, operation: &Value) -> Operation {
    match operation["op"].as_str().unwrap() {
        "insert" => Operation::Insert {
            object: operation["object"].clone(),
        },
        "remove" => Operation::Remove {
            handle: runtime
                .handle(operation["id"].as_u64().unwrap() as u32)
                .unwrap(),
        },
        "transform" => Operation::SetTransform {
            handle: runtime
                .handle(operation["id"].as_u64().unwrap() as u32)
                .unwrap(),
            transform: Transform {
                position: vector(&operation["position"]),
                half_extents: vector(&operation["half_extents"]),
                rotation_y: operation["rotation_y"].as_f64().unwrap() as f32,
            },
        },
        _ => panic!("invalid checked-in audit operation"),
    }
}

// Deliberately independent canonical reconstruction: this never calls apply_transaction,
// consumes no receipt and does not obtain the updated document from RuntimeScene.
fn reference_event(source: &mut Value, event: &Value) {
    let objects = source["objects"].as_array_mut().unwrap();
    for operation in event["operations"].as_array().unwrap() {
        match operation["op"].as_str().unwrap() {
            "insert" => objects.push(operation["object"].clone()),
            "remove" => objects.retain(|o| o["id"] != operation["id"]),
            "transform" => {
                let object = objects
                    .iter_mut()
                    .find(|o| o["id"] == operation["id"])
                    .unwrap();
                object["position"] = operation["position"].clone();
                object["half_extents"] = operation["half_extents"].clone();
                object["rotation_y"] = operation["rotation_y"].clone();
                object.as_object_mut().unwrap().remove("motion");
            }
            _ => unreachable!(),
        }
    }
}

fn assert_hooks(actual: &impl Scene, expected: &impl Scene, hit: Hit, time: f32) {
    assert_eq!(actual.local_materials(), expected.local_materials());
    assert_eq!(
        actual.pigment(hit.material).to_array().map(f32::to_bits),
        expected.pigment(hit.material).to_array().map(f32::to_bits)
    );
    assert_eq!(
        actual.light_position(time).to_array().map(f32::to_bits),
        expected.light_position(time).to_array().map(f32::to_bits)
    );
    assert_eq!(
        actual.light_color().to_array().map(f32::to_bits),
        expected.light_color().to_array().map(f32::to_bits)
    );
    for footprint in [0., 0.01, 0.3, 3.] {
        let material = |scene: &dyn Scene| {
            scene.material_sample(hit, footprint).map(|m| {
                [
                    m.albedo.x.to_bits(),
                    m.albedo.y.to_bits(),
                    m.albedo.z.to_bits(),
                    m.normal.x.to_bits(),
                    m.normal.y.to_bits(),
                    m.normal.z.to_bits(),
                    m.roughness.to_bits(),
                    m.f0.to_bits(),
                    m.occlusion.to_bits(),
                ]
            })
        };
        let metal = |scene: &dyn Scene| {
            scene.metallic_sample(hit, footprint).map(|m| {
                [
                    m.f0.x.to_bits(),
                    m.f0.y.to_bits(),
                    m.f0.z.to_bits(),
                    m.metalness.to_bits(),
                ]
            })
        };
        assert_eq!(material(actual), material(expected));
        assert_eq!(metal(actual), metal(expected));
        assert_eq!(
            actual.glyph_sample(hit, 0.2, footprint),
            expected.glyph_sample(hit, 0.2, footprint)
        );
        assert_eq!(
            actual.glyph_sample_key(hit, footprint),
            expected.glyph_sample_key(hit, footprint)
        );
    }
    assert_eq!(
        actual.glyph(hit.material, hit.local, hit.normal, 0.2),
        expected.glyph(hit.material, hit.local, hit.normal, 0.2)
    );
    assert_eq!(
        actual.glyph_history_key(hit.material, hit.local, hit.normal),
        expected.glyph_history_key(hit.material, hit.local, hit.normal)
    );
    let query = EnvironmentQuery {
        hit,
        normal: hit.normal,
        reflected_direction: Vec3::Y,
        roughness: 0.3,
        time,
        stage: Stage::Light,
    };
    let environment = |scene: &dyn Scene| {
        scene.environment_sample(query).map(|e| {
            [
                e.diffuse_irradiance.x.to_bits(),
                e.diffuse_irradiance.y.to_bits(),
                e.diffuse_irradiance.z.to_bits(),
                e.specular_radiance.x.to_bits(),
                e.specular_radiance.y.to_bits(),
                e.specular_radiance.z.to_bits(),
                e.max_distance.to_bits(),
            ]
        })
    };
    assert_eq!(environment(actual), environment(expected));
}

pub fn compare_queries(
    actual: &impl Scene,
    expected: &impl Scene,
    camera: Camera,
    time: f32,
    cols: u32,
    rows: u32,
) -> u64 {
    let mut count = 0;
    for row in 0..rows {
        for col in 0..cols {
            for offset in OFFSETS {
                let ray = camera
                    .ray_sample(col, row, cols, rows, 16. / 9., offset)
                    .unwrap();
                let expected_hit = expected.trace(ray, time, Stage::Light, true);
                assert_eq!(
                    hit_bits(actual.trace(ray, time, Stage::Light, true)),
                    hit_bits(expected_hit),
                    "ray={ray:?}, time={time}"
                );
                assert_eq!(
                    hit_bits(actual.trace(ray, time, Stage::Core, false)),
                    hit_bits(expected.trace(ray, time, Stage::Core, false))
                );
                let mut limits = vec![0., 0.1, 2., 20., f32::INFINITY];
                if let Some(hit) = expected_hit {
                    limits.extend([
                        f32::from_bits(hit.distance.to_bits() - 1),
                        hit.distance,
                        f32::from_bits(hit.distance.to_bits() + 1),
                    ]);
                    // Sample hooks once per center hit, not eight redundant copies.
                    if offset == [0., 0.] {
                        assert_hooks(actual, expected, hit, time);
                    }
                }
                for limit in limits {
                    assert_eq!(
                        actual.occluded(ray, time, Stage::Light, limit),
                        expected.occluded(ray, time, Stage::Light, limit)
                    );
                }
                count += 1;
            }
        }
    }
    count
}

struct ComposerSlot {
    mode: Mode,
    policy: PrimarySampling,
    workers: usize,
    composer: Composer,
}

pub fn room_audit(cols: u32, rows: u32) -> Value {
    let document = SceneDocument::from_json(ROOM).unwrap();
    let mut runtime = RuntimeScene::new(document.clone()).unwrap();
    let mut canonical: Value = serde_json::from_str(ROOM).unwrap();
    let updates: Value = serde_json::from_str(UPDATES).unwrap();
    let events = updates["events"].as_array().unwrap();
    let mut ticks = BTreeSet::from([0_u32, 179]);
    for event in events {
        let tick = event["tick"].as_u64().unwrap() as u32;
        ticks.extend([tick - 1, tick, tick + 1]);
    }
    let mut slots = Vec::new();
    for mode in MODES {
        for policy in POLICIES {
            for workers in [1, 8] {
                let mut composer = Composer::with_threads(workers).unwrap();
                composer.set_primary_sampling(policy);
                slots.push(ComposerSlot {
                    mode,
                    policy,
                    workers,
                    composer,
                });
            }
        }
    }
    let mut retained: Vec<(RuntimeSnapshot, Value, f32)> = Vec::new();
    let mut records = Vec::new();
    let mut ray_count = 0_u64;
    let mut event_index = 0;
    for tick in ticks {
        let time = tick as f32 / 30.;
        let mut changed = false;
        while event_index < events.len()
            && events[event_index]["tick"].as_u64().unwrap() <= tick as u64
        {
            let event = &events[event_index];
            let operations: Vec<_> = event["operations"]
                .as_array()
                .unwrap()
                .iter()
                .map(|op| parse_operation(&runtime, op))
                .collect();
            let receipt = runtime
                .apply_transaction(runtime.revision(), &operations)
                .unwrap();
            assert!(
                receipt.changed,
                "Every authored event must change occupancy"
            );
            reference_event(&mut canonical, event);
            changed = true;
            event_index += 1;
        }
        let actual = runtime.snapshot(time);
        let expected_document =
            reference::SceneDocument::from_json(&canonical.to_string()).unwrap();
        let expected = expected_document.snapshot(time);
        for mode in MODES {
            ray_count += compare_queries(
                &actual,
                &expected,
                document.camera(mode),
                time,
                cols.min(17),
                rows.min(11),
            );
        }
        for slot in &mut slots {
            let camera = document.camera(slot.mode);
            let (cells, stats) =
                slot.composer
                    .compose(&actual, camera, time, Stage::Light, cols, rows, 16. / 9.);
            let mut fresh = Composer::default();
            fresh.set_primary_sampling(slot.policy);
            let (reference_cells, reference_stats) =
                fresh.compose(&expected, camera, time, Stage::Light, cols, rows, 16. / 9.);
            assert!(
                stats.visible > 0,
                "Room comparison must not be an empty image"
            );
            assert_eq!(
                cell_bits(&cells),
                cell_bits(&reference_cells),
                "tick={tick}, mode={:?}, sampling={:?}, workers={}",
                slot.mode,
                slot.policy,
                slot.workers
            );
            assert_eq!(
                (
                    stats.visible,
                    stats.reflected,
                    stats.primary_rays,
                    stats.alternate_primary
                ),
                (
                    reference_stats.visible,
                    reference_stats.reflected,
                    reference_stats.primary_rays,
                    reference_stats.alternate_primary
                )
            );
            if changed {
                assert_eq!(
                    stats.matched, 0,
                    "Successful commit must invalidate even unchanged receivers"
                );
            }
            let glyph_count = |glyph: u8| {
                cells
                    .iter()
                    .filter(|cell| cell.glyph == glyph as u32)
                    .count()
            };
            if runtime.handle(3).is_none() {
                assert_eq!(glyph_count(b'D'), 0, "Removed door must leave no glyphs");
            }
            records.push(json!({"tick":tick,"time":time,"mode":format!("{:?}",slot.mode),"sampling":format!("{:?}",slot.policy),
                "workers":slot.workers,"epoch":actual.history_epoch(),"visible":stats.visible,"matched":stats.matched,
                "primary_rays":stats.primary_rays,"alternate_primary":stats.alternate_primary,"bitwise_equal":true,
                "door_cells":glyph_count(b'D'),"red_target_cells":glyph_count(b'R'),"reflector_cells":glyph_count(b'M'),"blocker_cells":glyph_count(b'B')}));
        }
        if changed || retained.is_empty() {
            retained.push((actual, canonical.clone(), time));
        }
    }
    // Old snapshots must never observe the newest occupancy, even when the legacy
    // query-time fallback prepares a different time from their stored tick.
    let mut old_checks = 0;
    for (snapshot, source, stored_time) in retained.iter().rev() {
        let reference_document = reference::SceneDocument::from_json(&source.to_string()).unwrap();
        for time in [*stored_time, *stored_time + 0.37] {
            let expected = reference_document.snapshot(time);
            for mode in MODES {
                ray_count +=
                    compare_queries(snapshot, &expected, document.camera(mode), time, 9, 7);
                old_checks += 1;
            }
        }
    }
    json!({"cols":cols,"rows":rows,"events":events.len(),"composition_records":records.len(),"query_rays":ray_count,
        "retained_snapshots":retained.len(),"retained_snapshot_view_checks":old_checks,"records":records})
}

#[derive(Clone, Copy)]
struct TraceRecord {
    ray: Ray,
    time: f32,
    primary: bool,
    object: Option<u32>,
}
struct ShadowRecord {
    ray: Ray,
    time: f32,
    blocked: bool,
}
struct Logged<'a, S> {
    scene: &'a S,
    traces: Mutex<Vec<TraceRecord>>,
    shadows: Mutex<Vec<ShadowRecord>>,
}
impl<'a, S: Scene> Logged<'a, S> {
    fn new(scene: &'a S) -> Self {
        Self {
            scene,
            traces: Mutex::default(),
            shadows: Mutex::default(),
        }
    }
}
impl<S: Scene + Sync> Scene for Logged<'_, S> {
    fn trace(&self, ray: Ray, time: f32, stage: Stage, primary: bool) -> Option<Hit> {
        let hit = self.scene.trace(ray, time, stage, primary);
        self.traces.lock().unwrap().push(TraceRecord {
            ray,
            time,
            primary,
            object: hit.map(|h| h.object_id),
        });
        hit
    }
    fn occluded(&self, ray: Ray, time: f32, stage: Stage, max: f32) -> bool {
        let blocked = self.scene.occluded(ray, time, stage, max);
        self.shadows
            .lock()
            .unwrap()
            .push(ShadowRecord { ray, time, blocked });
        blocked
    }
    fn light_position(&self, time: f32) -> Vec3 {
        self.scene.light_position(time)
    }
    fn pigment(&self, material: u32) -> Vec3 {
        self.scene.pigment(material)
    }
    fn glyph(&self, material: u32, local: Vec3, normal: Vec3, illumination: f32) -> u8 {
        self.scene.glyph(material, local, normal, illumination)
    }
    fn local_materials(&self) -> bool {
        self.scene.local_materials()
    }
    fn material_sample(&self, hit: Hit, footprint: f32) -> Option<MaterialSample> {
        self.scene.material_sample(hit, footprint)
    }
    fn metallic_sample(&self, hit: Hit, footprint: f32) -> Option<MetallicSample> {
        self.scene.metallic_sample(hit, footprint)
    }
    fn environment_sample(&self, query: EnvironmentQuery) -> Option<EnvironmentSample> {
        self.scene.environment_sample(query)
    }
    fn glyph_sample(&self, hit: Hit, illumination: f32, footprint: f32) -> u8 {
        self.scene.glyph_sample(hit, illumination, footprint)
    }
    fn glyph_sample_key(&self, hit: Hit, footprint: f32) -> u32 {
        self.scene.glyph_sample_key(hit, footprint)
    }
    fn glyph_history_key(&self, material: u32, local: Vec3, normal: Vec3) -> u32 {
        self.scene.glyph_history_key(material, local, normal)
    }
    fn light_color(&self) -> Vec3 {
        self.scene.light_color()
    }
    fn history_epoch(&self) -> u64 {
        self.scene.history_epoch()
    }
}

pub fn probe_source() -> Value {
    json!({"version":2,"name":"Independent occupancy transport probe",
        "cameras":{
            "first":{"eye":[0.,2.,4.],"target":[0.,0.,0.],"fov_y_degrees":1.,"ortho_half_height":0.01},
            "third":{"eye":[0.,2.,4.],"target":[0.,0.,0.],"fov_y_degrees":1.,"ortho_half_height":0.01},
            "overhead":{"eye":[0.,2.,4.],"target":[0.,0.,0.],"fov_y_degrees":1.,"ortho_half_height":0.01}},
        "environment":{"diffuse_irradiance":[0.6,0.6,0.6],"specular_radiance":[0.15,0.2,0.3],"max_distance":12.},
        "materials":[
            {"id":32,"name":"Receiver","pigment":[0.5,0.5,0.5],"pattern":{"kind":"plain","scale":1.,"palette":"F"},"surface":{"roughness":0.18,"f0":0.25,"occlusion":1.}},
            {"id":33,"name":"Red target","pigment":[0.9,0.04,0.02],"pattern":{"kind":"plain","scale":1.,"palette":"T"},"surface":{"roughness":0.6,"f0":0.04,"occlusion":1.}}],
        "objects":[{"id":1,"name":"Receiver floor","shape":"box","material":32,"position":[0.,-0.1,0.],"half_extents":[8.,0.1,8.]}],
        "light":{"position":[3.,4.,0.],"color":[1.,1.,1.]}})
}

pub fn transport_audit() -> Value {
    let mut records = Vec::new();
    let mut removal_checks = 0;
    for (name, position, stage) in [
        ("primary", [0., 1., 2.], Stage::Light),
        ("shadow", [1.5, 2., 0.], Stage::Light),
        ("diffuse_normal", [0., 1., 0.], Stage::Core),
        ("local_specular", [0., 1., -2.], Stage::Core),
    ] {
        let source = probe_source();
        let mut runtime =
            RuntimeScene::new(SceneDocument::from_json(&source.to_string()).unwrap()).unwrap();
        let before = runtime.snapshot(0.);
        let object = json!({"id":130,"name":"Dynamic probe blocker","shape":"box","material":33,"position":position,"half_extents":[0.2,0.2,0.2]});
        runtime
            .apply_transaction(
                runtime.revision(),
                &[Operation::Insert {
                    object: object.clone(),
                }],
            )
            .unwrap();
        let after = runtime.snapshot(0.);
        let mut expected_source = source;
        expected_source["objects"]
            .as_array_mut()
            .unwrap()
            .push(object);
        let expected = reference::SceneDocument::from_json(&expected_source.to_string())
            .unwrap()
            .snapshot(0.);
        for mode in MODES {
            for policy in POLICIES {
                for workers in [1, 8] {
                    let view =
                        CameraView::new(Vec3::new(0., 2., 4.), Vec3::ZERO, 1., 0.01).unwrap();
                    let camera = Camera::with_view(mode, view);
                    let mut composer = Composer::with_threads(workers).unwrap();
                    composer.set_primary_sampling(policy);
                    let logged_before = Logged::new(&before);
                    let (old, _) = composer.compose(&logged_before, camera, 0., stage, 1, 1, 1.);
                    let logged_after = Logged::new(&after);
                    let (new, stats) = composer.compose(&logged_after, camera, 0., stage, 1, 1, 1.);
                    let mut fresh = Composer::default();
                    fresh.set_primary_sampling(policy);
                    let (expected_cells, _) = fresh.compose(&expected, camera, 0., stage, 1, 1, 1.);
                    assert_eq!(cell_bits(&new), cell_bits(&expected_cells));
                    assert_eq!(stats.matched, 0);
                    assert_ne!(
                        old[0].color.map(f32::to_bits),
                        new[0].color.map(f32::to_bits),
                        "{name} must affect real RGB"
                    );
                    assert_eq!(old[0].glyph, b'F' as u32);
                    assert_eq!(
                        new[0].glyph,
                        if name == "primary" { b'T' } else { b'F' } as u32
                    );
                    let traces = logged_after.traces.lock().unwrap();
                    assert!(traces
                        .iter()
                        .all(|record| record.time.to_bits() == 0_f32.to_bits()));
                    let hit_path = |direction: Vec3| {
                        traces.iter().any(|record| {
                            !record.primary
                                && record.object == Some(130)
                                && record.ray.dir.dot(direction) > 0.999
                                && (record.ray.origin - Vec3::new(0., 0.012, 0.)).length() < 0.001
                        })
                    };
                    match name {
                        "primary" => assert!(traces
                            .iter()
                            .any(|record| record.primary && record.object == Some(130))),
                        "shadow" => {
                            assert!(!logged_before.shadows.lock().unwrap().is_empty());
                            assert!(logged_before
                                .shadows
                                .lock()
                                .unwrap()
                                .iter()
                                .all(|record| !record.blocked));
                            assert!(logged_after.shadows.lock().unwrap().iter().any(
                                |record| record.blocked
                                    && record.time.to_bits() == 0_f32.to_bits()
                                    && record.ray.dir.dot(Vec3::new(3., 4., 0.).normalize())
                                        > 0.999
                                    && (record.ray.origin - Vec3::new(0., 0.012, 0.)).length()
                                        < 0.001
                            ));
                        }
                        "diffuse_normal" => assert!(hit_path(Vec3::Y)),
                        "local_specular" => assert!(hit_path(Vec3::new(0., 2., -4.).normalize())),
                        _ => unreachable!(),
                    }
                    records.push(json!({"path":name,"mode":format!("{mode:?}"),"sampling":format!("{policy:?}"),"workers":workers,
                "before_rgb":old[0].color,"after_rgb":new[0].color,"trace_calls":traces.len(),
                "shadow_calls":logged_after.shadows.lock().unwrap().len(),"path_observed":true,"bitwise_equal":true}));
                }
            }
        }
        // Removal must undo all four effects and old snapshots must retain them.
        runtime
            .apply_transaction(
                runtime.revision(),
                &[Operation::Remove {
                    handle: runtime.handle(130).unwrap(),
                }],
            )
            .unwrap();
        let removed = runtime.snapshot(0.);
        let fresh_before = reference::SceneDocument::from_json(&probe_source().to_string())
            .unwrap()
            .snapshot(0.);
        for mode in MODES {
            let camera = Camera::with_view(
                mode,
                CameraView::new(Vec3::new(0., 2., 4.), Vec3::ZERO, 1., 0.01).unwrap(),
            );
            compare_queries(&removed, &fresh_before, camera, 0., 3, 3);
            compare_queries(&after, &expected, camera, 0.37, 3, 3);
            for policy in POLICIES {
                for workers in [1, 8] {
                    let mut composer = Composer::with_threads(workers).unwrap();
                    composer.set_primary_sampling(policy);
                    composer.compose(&after, camera, 0., stage, 1, 1, 1.);
                    let (actual_cells, stats) =
                        composer.compose(&removed, camera, 0., stage, 1, 1, 1.);
                    let mut fresh = Composer::default();
                    fresh.set_primary_sampling(policy);
                    let (reference_cells, _) =
                        fresh.compose(&fresh_before, camera, 0., stage, 1, 1, 1.);
                    assert_eq!(
                        cell_bits(&actual_cells),
                        cell_bits(&reference_cells),
                        "Removing {name} must restore original transport"
                    );
                    assert_eq!(stats.matched, 0);
                    removal_checks += 1;
                }
            }
        }
    }
    json!({"records":records,"record_count":records.len(),"removal_checks":removal_checks,"note":"Same-axis one-cell probes isolate transport; the independent room audit uses three distinct authored cameras. No water counter or visual reflection-recognition claim."})
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut output = PathBuf::from("evidence/r14/occupancy-audit");
    let (mut cols, mut rows) = (80_u32, 45_u32);
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        if argument == "--help" {
            println!("occupancy_audit --output DIR --cols N --rows N\nUntimed R14 audit of the checked-in dynamic room/timeline: three views, both sampling policies, 1/8 workers; frozen R12 reconstruction, retained snapshots and four non-vacuous transport probes. Default80x45; bounds1..240 by1..135.");
            return Ok(());
        }
        let value = args.next().ok_or("missing option value")?;
        match argument.as_str() {
            "--output" => output = value.into(),
            "--cols" => cols = value.parse()?,
            "--rows" => rows = value.parse()?,
            _ => return Err(format!("unknown option {argument}").into()),
        }
    }
    if !(1..=240).contains(&cols) || !(1..=135).contains(&rows) {
        return Err("audit dimensions outside1..240 by1..135".into());
    }
    let report = json!({"schema_version":1,"timed":false,"reference":"Frozen R12 producer with independently reconstructed JSON states",
        "input_scene":serde_json::from_str::<Value>(ROOM)?,"input_updates":serde_json::from_str::<Value>(UPDATES)?,
        "room":room_audit(cols, rows),"transport":transport_audit(),
        "limits":"Finite semantic samples; no exact coverage, GI, reflection artwork or realtime performance claim. Global epoch changes reset history."});
    std::fs::create_dir_all(&output)?;
    std::fs::write(
        output.join("audit.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!(
        "occupancy audit PASS: {} composition records, {} transport records; {}",
        report["room"]["composition_records"],
        report["transport"]["record_count"],
        output.display()
    );
    Ok(())
}
