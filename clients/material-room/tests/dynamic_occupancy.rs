//! Independent lifetime/transaction acceptance plus the executable untimed audit.
use ascii_material_room::{
    runtime::{ObjectHandle, Operation, RuntimeScene, RuntimeSnapshot, SceneRevision, Transform},
    SceneDocument,
};
pub use ascii_material_room::{DEFAULT_SCENE_JSON, DEFAULT_SCENE_V2_JSON};
use ascii_render::{
    camera::Mode,
    core::{Ray, Scene, Stage},
    frame::{Composer, PrimarySampling},
};
use glam::Vec3;
use serde_json::{json, Value};

#[allow(dead_code)]
#[path = "../examples/occupancy_audit.rs"]
mod audit;

fn document(source: &Value) -> SceneDocument {
    SceneDocument::from_json(&source.to_string()).unwrap()
}
fn runtime(source: &Value) -> RuntimeScene {
    RuntimeScene::new(document(source)).unwrap()
}
fn object(id: u32, position: Vec3) -> Value {
    json!({"id":id,"name":"Independent transaction object","shape":"box","material":33,
        "position":position.to_array(),"half_extents":[0.2,0.2,0.2],"rotation_y":0.0})
}
fn floor_transform() -> Transform {
    Transform {
        position: Vec3::new(0., -0.1, 0.),
        half_extents: Vec3::new(8., 0.1, 8.),
        rotation_y: 0.,
    }
}
fn center_ray() -> Ray {
    Ray {
        origin: Vec3::new(0., 2., 4.),
        dir: Vec3::new(0., -2., -4.).normalize(),
    }
}
fn assert_rejected_unchanged(
    runtime: &mut RuntimeScene,
    expected: SceneRevision,
    operations: &[Operation],
) {
    let source = runtime.source().clone();
    let revision = runtime.revision();
    let epoch = runtime.history_epoch();
    let handles: Vec<_> = runtime.handles().collect();
    let before = runtime.snapshot(0.);
    assert!(runtime.apply_transaction(expected, operations).is_err());
    assert_eq!(runtime.source(), &source);
    assert_eq!(runtime.revision(), revision);
    assert_eq!(runtime.history_epoch(), epoch);
    assert_eq!(runtime.handles().collect::<Vec<_>>(), handles);
    for time in [0., 0.37, 5.] {
        assert_eq!(
            audit::hit_bits(before.trace(center_ray(), time, Stage::Light, true)),
            audit::hit_bits(
                runtime
                    .snapshot(time)
                    .trace(center_ray(), time, Stage::Light, true)
            )
        );
    }
}

#[test]
fn checked_in_sequence_matches_frozen_reconstruction_in_three_views() {
    let report = audit::room_audit(17, 11);
    assert_eq!(report["events"], 11);
    assert!(report["composition_records"].as_u64().unwrap() >= 300);
    assert_eq!(report["retained_snapshots"], 12);
}

#[test]
fn actual_composer_queries_observe_primary_shadow_diffuse_and_reflected_occupancy() {
    let report = audit::transport_audit();
    assert_eq!(report["record_count"], 48);
    assert_eq!(report["removal_checks"], 48);
}

#[test]
fn failed_batches_publish_neither_partial_state_nor_new_identities() {
    let mut scene = runtime(&audit::probe_source());
    let original_handle = scene.handle(1).unwrap();
    let revision = scene.revision();
    let mut invalid_object = object(130, Vec3::new(0., 1., 2.));
    invalid_object["material"] = json!(999);
    assert_rejected_unchanged(
        &mut scene,
        revision,
        &[
            Operation::Remove {
                handle: original_handle,
            },
            Operation::Insert {
                object: invalid_object,
            },
        ],
    );
    let receipt = scene
        .apply_transaction(
            revision,
            &[Operation::Insert {
                object: object(130, Vec3::new(0., 1., 2.)),
            }],
        )
        .unwrap();
    assert_eq!(receipt.inserted.len(), 1);
    assert_eq!(
        receipt.inserted[0].generation,
        original_handle.generation + 1,
        "Failed insertion consumes no generation"
    );
    assert_eq!(scene.handle(1), Some(original_handle));
    assert!(receipt.changed);
}

#[test]
fn stale_revisions_foreign_handles_duplicate_targets_and_missing_ids_are_atomic_errors() {
    let source = audit::probe_source();
    let mut scene = runtime(&source);
    let other = runtime(&source);
    assert_ne!(scene.revision().namespace, other.revision().namespace);
    assert_ne!(scene.history_epoch(), other.history_epoch());
    let handle = scene.handle(1).unwrap();
    let initial = scene.revision();
    assert_rejected_unchanged(&mut scene, other.revision(), &[]);
    assert_rejected_unchanged(
        &mut scene,
        initial,
        &[Operation::Remove {
            handle: other.handle(1).unwrap(),
        }],
    );
    assert_rejected_unchanged(
        &mut scene,
        initial,
        &[
            Operation::SetTransform {
                handle,
                transform: floor_transform(),
            },
            Operation::Remove { handle },
        ],
    );
    let missing = ObjectHandle { id: 999, ..handle };
    assert_rejected_unchanged(
        &mut scene,
        initial,
        &[Operation::Remove { handle: missing }],
    );
    let wrong_generation = ObjectHandle {
        generation: handle.generation + 1,
        ..handle
    };
    assert_rejected_unchanged(
        &mut scene,
        initial,
        &[Operation::Remove {
            handle: wrong_generation,
        }],
    );
    scene
        .apply_transaction(
            initial,
            &[Operation::Insert {
                object: object(130, Vec3::new(0., 1., 2.)),
            }],
        )
        .unwrap();
    assert_rejected_unchanged(&mut scene, initial, &[]);
}

#[test]
fn remove_and_reinsert_without_an_intermediate_frame_creates_a_new_lifetime() {
    let source = audit::probe_source();
    let mut scene = runtime(&source);
    let camera = document(&source).camera(Mode::First);
    let old_handle = scene.handle(1).unwrap();
    let before = scene.snapshot(0.);
    let mut composers = Vec::new();
    for policy in [PrimarySampling::Center, PrimarySampling::Conservative9] {
        for workers in [1, 8] {
            let mut composer = Composer::with_threads(workers).unwrap();
            composer.set_primary_sampling(policy);
            let old_cells = composer
                .compose(&before, camera, 0., Stage::Core, 1, 1, 1.)
                .0;
            assert_eq!(old_cells[0].glyph, b'F' as u32);
            composers.push((composer, old_cells));
        }
    }
    let initial_revision = scene.revision();
    scene
        .apply_transaction(
            initial_revision,
            &[Operation::Remove { handle: old_handle }],
        )
        .unwrap();
    let absent = scene.snapshot(0.);
    assert!(absent.trace(center_ray(), 0., Stage::Core, true).is_none());
    // No compose is performed while the object is absent.
    let receipt = scene
        .apply_transaction(
            scene.revision(),
            &[Operation::Insert {
                object: source["objects"][0].clone(),
            }],
        )
        .unwrap();
    let new_handle = receipt.inserted[0];
    assert_eq!(old_handle.id, new_handle.id);
    assert_eq!(old_handle.namespace, new_handle.namespace);
    assert_ne!(old_handle.generation, new_handle.generation);
    assert_eq!(before.handle(1), Some(old_handle));
    assert_eq!(absent.handle(1), None);
    let after = scene.snapshot(0.);
    assert_eq!(after.handle(1), Some(new_handle));
    assert_ne!(before.history_epoch(), after.history_epoch());
    for (mut composer, cells) in composers {
        let (new_cells, stats) = composer.compose(&after, camera, 0., Stage::Core, 1, 1, 1.);
        assert_eq!(audit::cell_bits(&cells), audit::cell_bits(&new_cells));
        assert_eq!(
            stats.matched, 0,
            "Same surface bins must not inherit the old lifetime's history"
        );
        assert_eq!(
            composer
                .compose(&after, camera, 0., Stage::Core, 1, 1, 1.)
                .1
                .matched,
            1
        );
        assert_eq!(
            composer
                .compose(&before, camera, 0., Stage::Core, 1, 1, 1.)
                .1
                .matched,
            0,
            "Returning to an older immutable snapshot also changes epoch"
        );
    }
    let current = scene.revision();
    assert_rejected_unchanged(
        &mut scene,
        current,
        &[Operation::Remove { handle: old_handle }],
    );
}

#[test]
fn empty_and_identical_batches_preserve_revision_epoch_handles_and_history() {
    let source = audit::probe_source();
    let mut scene = runtime(&source);
    let handle = scene.handle(1).unwrap();
    let original_source = scene.source().clone();
    let original_epoch = scene.history_epoch();
    let original_revision = scene.revision();
    let camera = document(&source).camera(Mode::First);
    let mut composer = Composer::default();
    composer.compose(&scene.snapshot(0.), camera, 0., Stage::Core, 1, 1, 1.);
    for operations in [
        vec![],
        vec![Operation::SetTransform {
            handle,
            transform: floor_transform(),
        }],
    ] {
        let receipt = scene
            .apply_transaction(original_revision, &operations)
            .unwrap();
        assert!(!receipt.changed);
        assert_eq!(receipt.revision, original_revision);
        assert!(receipt.inserted.is_empty());
        assert_eq!(scene.source(), &original_source);
        assert_eq!(scene.history_epoch(), original_epoch);
        assert_eq!(scene.handle(1), Some(handle));
        assert_eq!(
            composer
                .compose(&scene.snapshot(0.), camera, 0., Stage::Core, 1, 1, 1.)
                .1
                .matched,
            1
        );
    }
    let mut changed_bits = floor_transform();
    changed_bits.rotation_y = -0.0;
    assert!(
        scene
            .apply_transaction(
                original_revision,
                &[Operation::SetTransform {
                    handle,
                    transform: changed_bits
                }]
            )
            .unwrap()
            .changed
    );
    assert_ne!(
        scene.history_epoch(),
        original_epoch,
        "Transform identity is bitwise, including signed zero"
    );
}

#[test]
fn absolute_transform_removes_motion_and_old_snapshot_fallback_keeps_its_original_motion() {
    let mut source = audit::probe_source();
    source["objects"][0]["motion"] = json!({"translation":{"amplitude":[1.0,0.0,0.0],"speed":1.0,"phase":0.0},"yaw":{"amplitude":0.25,"speed":1.0,"phase":0.0}});
    let original_document = document(&source);
    let mut scene = RuntimeScene::new(original_document.clone()).unwrap();
    let old = scene.snapshot(0.);
    let handle = scene.handle(1).unwrap();
    let receipt = scene
        .apply_transaction(
            scene.revision(),
            &[Operation::SetTransform {
                handle,
                transform: floor_transform(),
            }],
        )
        .unwrap();
    assert!(
        receipt.changed,
        "Removing motion is an edit even when the base pose is unchanged"
    );
    assert!(scene.source()["objects"][0].get("motion").is_none());
    assert_eq!(scene.handle(1), Some(handle));
    let mut independent = source.clone();
    independent["objects"][0]
        .as_object_mut()
        .unwrap()
        .remove("motion");
    independent["objects"][0]["rotation_y"] = json!(0.0);
    let expected = document(&independent);
    let current = scene.snapshot(0.);
    for time in [0., 0.37, 2., 100., f32::MAX] {
        for mode in [Mode::First, Mode::Third, Mode::Overhead] {
            audit::compare_queries(
                &old,
                &original_document.snapshot(time),
                original_document.camera(mode),
                time,
                5,
                3,
            );
            audit::compare_queries(
                &current,
                &expected.snapshot(time),
                expected.camera(mode),
                time,
                5,
                3,
            );
        }
    }
}

#[test]
fn invalid_nested_insert_and_transform_values_do_not_publish() {
    let mut scene = runtime(&audit::probe_source());
    let revision = scene.revision();
    let handle = scene.handle(1).unwrap();
    let mut invalid = Vec::new();
    for (field, value) in [
        ("id", json!(0)),
        ("material", json!(999)),
        ("shape", json!("mesh")),
        ("position", json!([0., 10001., 0.])),
        ("half_extents", json!([1., 0., 1.])),
        (
            "motion",
            json!({"yaw":{"amplitude":0.1,"speed":"invalid","phase":0.}}),
        ),
        ("unexpected", json!(true)),
    ] {
        let mut candidate = object(130, Vec3::ZERO);
        candidate[field] = value;
        invalid.push(candidate);
    }
    for object in invalid {
        assert_rejected_unchanged(&mut scene, revision, &[Operation::Insert { object }]);
    }
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 10001.] {
        let mut transform = floor_transform();
        transform.position.x = value;
        assert_rejected_unchanged(
            &mut scene,
            revision,
            &[Operation::SetTransform { handle, transform }],
        );
    }
    for value in [0., -1., 0.00001, 10001., f32::NAN] {
        let mut transform = floor_transform();
        transform.half_extents.z = value;
        assert_rejected_unchanged(
            &mut scene,
            revision,
            &[Operation::SetTransform { handle, transform }],
        );
    }
    for value in [-1025., 1025., f32::NAN] {
        let mut transform = floor_transform();
        transform.rotation_y = value;
        assert_rejected_unchanged(
            &mut scene,
            revision,
            &[Operation::SetTransform { handle, transform }],
        );
    }
}

#[test]
fn active_limit_is_final_state_and_empty_scene_can_be_repopulated() {
    let mut source = audit::probe_source();
    source["objects"] = json!([]);
    let mut scene = runtime(&source);
    assert!(scene
        .snapshot(0.)
        .trace(center_ray(), 0., Stage::Core, true)
        .is_none());
    let operations: Vec<_> = (1..=128)
        .map(|id| Operation::Insert {
            object: object(id, Vec3::new(id as f32, 0., 0.)),
        })
        .collect();
    let receipt = scene
        .apply_transaction(scene.revision(), &operations)
        .unwrap();
    assert_eq!(receipt.inserted.len(), 128);
    assert_eq!(scene.handles().count(), 128);
    let revision = scene.revision();
    assert_rejected_unchanged(
        &mut scene,
        revision,
        &[Operation::Insert {
            object: object(129, Vec3::ZERO),
        }],
    );
    // Temporary candidate array may reach129; only the final active count matters.
    let remove = scene.handle(1).unwrap();
    scene
        .apply_transaction(
            revision,
            &[
                Operation::Insert {
                    object: object(129, Vec3::ZERO),
                },
                Operation::Remove { handle: remove },
            ],
        )
        .unwrap();
    assert_eq!(scene.handles().count(), 128);
    assert_eq!(scene.handle(1), None);
    assert!(scene.handle(129).is_some());
    let revision = scene.revision();
    let too_many: Vec<_> = (200..329)
        .map(|id| Operation::Insert {
            object: object(id, Vec3::ZERO),
        })
        .collect();
    assert_rejected_unchanged(&mut scene, revision, &too_many);
    let removals: Vec<_> = scene
        .handles()
        .map(|handle| Operation::Remove { handle })
        .collect();
    scene.apply_transaction(revision, &removals).unwrap();
    assert_eq!(scene.handles().count(), 0);
    assert!(scene
        .snapshot(0.)
        .trace(center_ray(), 0., Stage::Core, true)
        .is_none());
}

#[test]
fn version_one_order_and_version_two_id_ties_survive_edits() {
    for version in [1, 2] {
        let mut source = audit::probe_source();
        source["version"] = json!(version);
        if version == 1 {
            source.as_object_mut().unwrap().remove("cameras");
            source.as_object_mut().unwrap().remove("environment");
            source["light"].as_object_mut().unwrap().remove("color");
            for material in source["materials"].as_array_mut().unwrap() {
                material.as_object_mut().unwrap().remove("surface");
            }
        }
        source["objects"] = json!([object(50, Vec3::ZERO), object(20, Vec3::ZERO)]);
        let mut scene = runtime(&source);
        let ray = Ray {
            origin: Vec3::new(0., 0., 2.),
            dir: -Vec3::Z,
        };
        let winner = |scene: &RuntimeScene| {
            scene
                .snapshot(0.)
                .trace(ray, 0., Stage::Core, true)
                .unwrap()
                .object_id
        };
        assert_eq!(winner(&scene), if version == 1 { 50 } else { 20 });
        scene
            .apply_transaction(
                scene.revision(),
                &[Operation::Insert {
                    object: object(10, Vec3::ZERO),
                }],
            )
            .unwrap();
        assert_eq!(winner(&scene), if version == 1 { 50 } else { 10 });
        let remove = scene.handle(50).unwrap();
        scene
            .apply_transaction(scene.revision(), &[Operation::Remove { handle: remove }])
            .unwrap();
        assert_eq!(winner(&scene), if version == 1 { 20 } else { 10 });
    }
}

#[test]
fn untracked_snapshot_retains_zero_epoch_and_transitions_reset_history_both_directions() {
    let source = audit::probe_source();
    let document = document(&source);
    let scene = RuntimeScene::new(document.clone()).unwrap();
    let tracked = scene.snapshot(0.);
    let untracked = RuntimeSnapshot::untracked(document.snapshot(0.));
    assert_eq!(untracked.history_epoch(), 0);
    assert!(untracked.handle(1).is_none());
    assert_eq!(untracked.handles().count(), 0);
    for policy in [PrimarySampling::Center, PrimarySampling::Conservative9] {
        for workers in [1, 8] {
            let mut composer = Composer::with_threads(workers).unwrap();
            composer.set_primary_sampling(policy);
            for (snapshot, matched) in [
                (&untracked, 0),
                (&untracked, 1),
                (&tracked, 0),
                (&tracked, 1),
                (&untracked, 0),
            ] {
                assert_eq!(
                    composer
                        .compose(
                            snapshot,
                            document.camera(Mode::First),
                            0.,
                            Stage::Core,
                            1,
                            1,
                            1.
                        )
                        .1
                        .matched,
                    matched
                );
            }
        }
    }
}

#[test]
fn disjoint_commit_does_not_change_survivor_float_bits_order_or_motion() {
    for version in [1, 2] {
        let mut source = audit::probe_source();
        source["version"] = json!(version);
        if version == 1 {
            source.as_object_mut().unwrap().remove("cameras");
            source.as_object_mut().unwrap().remove("environment");
            source["light"].as_object_mut().unwrap().remove("color");
            for material in source["materials"].as_array_mut().unwrap() {
                material.as_object_mut().unwrap().remove("surface");
            }
        }
        let mut survivors = Vec::new();
        // Rounding-sensitive input values on both sides of f32 midpoints.
        for (index, low) in [0.1_f32, 0.3, 1.000001, 99.00001].into_iter().enumerate() {
            let high = f32::from_bits(low.to_bits() + 1);
            let midpoint = (low as f64 + high as f64) / 2.;
            for (side, value) in [
                f64::from_bits(midpoint.to_bits() - 1),
                f64::from_bits(midpoint.to_bits() + 1),
            ]
            .into_iter()
            .enumerate()
            {
                let id = 1 + index as u32 * 2 + side as u32;
                let mut survivor = object(id, Vec3::ZERO);
                survivor["position"] = json!([value, index as f64 * 2.0, side as f64 * 2.0]);
                survivor["rotation_y"] = json!(if side == 0 { -0.0 } else { value });
                survivor["motion"] = json!({"translation":{"amplitude":[0.17,-0.0,0.09],"speed":0.7,"phase":0.3},"yaw":{"amplitude":0.13,"speed":0.9,"phase":0.2}});
                survivors.push(survivor);
            }
        }
        source["objects"] = json!(survivors);
        let original = document(&source);
        let mut scene = RuntimeScene::new(original.clone()).unwrap();
        let before = scene.snapshot(0.);
        let handles: Vec<_> = scene.handles().collect();
        scene
            .apply_transaction(
                scene.revision(),
                &[Operation::Insert {
                    object: object(130, Vec3::splat(9000.)),
                }],
            )
            .unwrap();
        for handle in handles {
            assert_eq!(scene.handle(handle.id), Some(handle));
        }
        let after = scene.snapshot(0.);
        let mut hits = 0;
        for time in [0., 0.37, 2.] {
            for object in source["objects"].as_array().unwrap() {
                let position = &object["position"];
                let center = Vec3::new(
                    position[0].as_f64().unwrap() as f32,
                    position[1].as_f64().unwrap() as f32,
                    position[2].as_f64().unwrap() as f32,
                );
                for direction in [
                    Vec3::X,
                    Vec3::Y,
                    Vec3::Z,
                    Vec3::new(1., 0.3, 1.).normalize(),
                ] {
                    let ray = Ray {
                        origin: center + direction * 5.,
                        dir: -direction,
                    };
                    let reference = original.snapshot(time).trace(ray, time, Stage::Light, true);
                    hits += usize::from(reference.is_some());
                    assert_eq!(
                        audit::hit_bits(before.trace(ray, time, Stage::Light, true)),
                        audit::hit_bits(reference)
                    );
                    assert_eq!(audit::hit_bits(after.trace(ray, time, Stage::Light, true)), audit::hit_bits(reference),
                        "Disjoint edit changed survivor float arithmetic: version{version}, time{time}, ray{ray:?}");
                }
            }
        }
        assert!(
            hits >= 24,
            "Survivor comparison must include real intersections"
        );
    }
}
