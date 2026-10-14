//! Atomic client-owned occupancy updates over the existing v1/v2 documents.
//!
//! Transactions clone and revalidate content only when an event is applied.
//! Queries still use the ordinary immutable `Snapshot` and its original math.
//! Object handles belong to one runtime; removing and reinserting an ID creates
//! a new lifetime. History epochs also change on effective transforms because
//! occupancy affects unchanged receivers through shadows and reflections.
use crate::{SceneDocument, Snapshot};
use ascii_render::{
    camera::{Camera, Mode},
    core::{
        EnvironmentQuery, EnvironmentSample, Hit, MaterialSample, MetallicSample, Ray, Scene, Stage,
    },
};
use glam::Vec3;
use serde_json::{json, Map, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

const MAX_OPERATIONS: usize = 128;
// Zero is reserved for ordinary, untracked snapshots. This allocator is touched
// only when creating a runtime or publishing a successful, effective commit.
static NEXT_TOKEN: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObjectHandle {
    pub namespace: u64,
    pub id: u32,
    pub generation: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneRevision {
    pub namespace: u64,
    pub revision: u64,
}

/// Absolute authored transform. Applying it removes any existing `motion`.
/// Coordinates are in [-10000,10000], extents in [0.0001,10000], and yaw in
/// [-1024,1024] radians. All components must be finite.
#[derive(Clone, Copy, Debug)]
pub struct Transform {
    pub position: Vec3,
    pub half_extents: Vec3,
    pub rotation_y: f32,
}

#[derive(Clone, Debug)]
pub enum Operation {
    /// The same object schema accepted by SceneDocument, including optional
    /// authored motion. References and all fields are validated at commit.
    Insert {
        object: Value,
    },
    Remove {
        handle: ObjectHandle,
    },
    SetTransform {
        handle: ObjectHandle,
        transform: Transform,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommitReceipt {
    pub changed: bool,
    pub revision: SceneRevision,
    /// New handles, in insertion-operation order. Existing handles stay valid
    /// across transforms and changes to other objects.
    pub inserted: Vec<ObjectHandle>,
}

/// Mutable owner used between frames. It is deliberately not Clone: another
/// runtime gets its own namespace instead of branching the same handle domain.
pub struct RuntimeScene {
    document: SceneDocument,
    revision: SceneRevision,
    epoch: u64,
    generations: Arc<BTreeMap<u32, u64>>,
    last_generation: u64,
}

impl RuntimeScene {
    pub fn new(document: SceneDocument) -> Result<Self, String> {
        let mut generations = BTreeMap::new();
        for object in document.source()["objects"]
            .as_array()
            .expect("validated document objects")
        {
            generations.insert(validated_id(object), 0);
        }
        // Assignment is independent from v1/v2 query order and JSON reordering.
        let mut last_generation = 0_u64;
        for generation in generations.values_mut() {
            last_generation = last_generation
                .checked_add(1)
                .ok_or("runtime generation exhausted")?;
            *generation = last_generation;
        }
        let generations = Arc::new(generations);
        let namespace = allocate_token(&NEXT_TOKEN)?;
        Ok(Self {
            document,
            revision: SceneRevision {
                namespace,
                revision: 0,
            },
            epoch: namespace,
            generations,
            last_generation,
        })
    }

    pub fn revision(&self) -> SceneRevision {
        self.revision
    }

    pub fn history_epoch(&self) -> u64 {
        self.epoch
    }

    pub fn handle(&self, id: u32) -> Option<ObjectHandle> {
        self.generations.get(&id).map(|&generation| ObjectHandle {
            namespace: self.revision.namespace,
            id,
            generation,
        })
    }

    /// Live identities in ascending ID order, without allocating a list.
    pub fn handles(&self) -> impl Iterator<Item = ObjectHandle> + '_ {
        self.generations
            .iter()
            .map(|(&id, &generation)| ObjectHandle {
                namespace: self.revision.namespace,
                id,
                generation,
            })
    }

    pub fn source(&self) -> &Value {
        self.document.source()
    }

    pub fn version(&self) -> u64 {
        self.document.version()
    }

    pub fn object_count(&self) -> usize {
        self.document.object_count()
    }

    pub fn camera(&self, mode: Mode) -> Camera {
        self.document.camera(mode)
    }

    /// Time follows SceneDocument::snapshot's finite-time contract. Subsequent
    /// commits cannot alter this snapshot, even its different-time fallback.
    pub fn snapshot(&self, time: f32) -> RuntimeSnapshot {
        RuntimeSnapshot {
            inner: self.document.snapshot(time),
            revision: self.revision,
            epoch: self.epoch,
            generations: Some(self.generations.clone()),
        }
    }

    /// Apply at most 128 operations with distinct target IDs. The expected
    /// namespace/revision and every handle must match the current state.
    ///
    /// V1 survivors retain array order and inserts append in operation order;
    /// V2 retains SceneDocument's ID-based query order. The final document must
    /// satisfy all original limits, including at most 128 live objects.
    ///
    /// Empty batches and bit-identical transforms without motion are no-ops.
    /// Removing motion, even zero-amplitude motion, is an effective edit.
    /// Failed/no-op commits consume neither generations nor history tokens.
    pub fn apply_transaction(
        &mut self,
        expected: SceneRevision,
        operations: &[Operation],
    ) -> Result<CommitReceipt, String> {
        if expected.namespace != self.revision.namespace {
            return Err("$.expected.namespace: belongs to a different runtime".into());
        }
        if expected.revision != self.revision.revision {
            return Err(format!(
                "$.expected.revision: stale revision {}, current is {}",
                expected.revision, self.revision.revision
            ));
        }
        if operations.len() > MAX_OPERATIONS {
            return Err("$.operations: exceeds 128 entries".into());
        }
        if operations.is_empty() {
            return Ok(self.unchanged());
        }

        let mut targets = BTreeSet::new();
        for (index, operation) in operations.iter().enumerate() {
            let path = format!("$.operations[{index}]");
            let id = match operation {
                Operation::Insert { object } => {
                    let id = insert_id(object, &path)?;
                    if self.generations.contains_key(&id) {
                        return Err(format!("{path}.object.id: object {id} already exists"));
                    }
                    id
                }
                Operation::Remove { handle } => {
                    self.validate_handle(*handle, &path)?;
                    handle.id
                }
                Operation::SetTransform { handle, transform } => {
                    self.validate_handle(*handle, &path)?;
                    validate_transform(*transform, &format!("{path}.transform"))?;
                    handle.id
                }
            };
            if !targets.insert(id) {
                return Err(format!("{path}: duplicate operation for object {id}"));
            }
        }

        let mut candidate = self.document.source().clone();
        let objects = candidate["objects"]
            .as_array_mut()
            .expect("validated document objects");
        let mut generations = self.generations.as_ref().clone();
        let mut last_generation = self.last_generation;
        let mut inserted = Vec::new();
        let mut changed = false;
        for (index, operation) in operations.iter().enumerate() {
            match operation {
                Operation::Insert { object } => {
                    let id = insert_id(object, &format!("$.operations[{index}]"))?;
                    last_generation = last_generation
                        .checked_add(1)
                        .ok_or("runtime generation exhausted")?;
                    generations.insert(id, last_generation);
                    inserted.push(ObjectHandle {
                        namespace: self.revision.namespace,
                        id,
                        generation: last_generation,
                    });
                    objects.push(object.clone());
                    changed = true;
                }
                Operation::Remove { handle } => {
                    objects.retain(|object| validated_id(object) != handle.id);
                    generations.remove(&handle.id);
                    changed = true;
                }
                Operation::SetTransform { handle, transform } => {
                    let object = objects
                        .iter_mut()
                        .find(|object| validated_id(object) == handle.id)
                        .expect("validated live handle")
                        .as_object_mut()
                        .expect("validated object");
                    if !same_transform(object, *transform) {
                        object.insert("position".into(), json!(transform.position.to_array()));
                        object.insert(
                            "half_extents".into(),
                            json!(transform.half_extents.to_array()),
                        );
                        object.insert("rotation_y".into(), json!(transform.rotation_y));
                        object.remove("motion");
                        changed = true;
                    }
                }
            }
        }
        if !changed {
            return Ok(self.unchanged());
        }

        let json = serde_json::to_string(&candidate)
            .map_err(|error| format!("candidate scene: cannot encode JSON: {error}"))?;
        let document = SceneDocument::from_json(&json)
            .map_err(|error| format!("candidate scene rejected: {error}"))?;
        let revision = SceneRevision {
            namespace: self.revision.namespace,
            revision: self
                .revision
                .revision
                .checked_add(1)
                .ok_or("runtime revision exhausted")?,
        };
        let generations = Arc::new(generations);
        // This is the last fallible operation. None of the mutable owner state
        // has changed, and all returned storage has already been prepared.
        let epoch = allocate_token(&NEXT_TOKEN)?;
        self.document = document;
        self.generations = generations;
        self.last_generation = last_generation;
        self.revision = revision;
        self.epoch = epoch;
        Ok(CommitReceipt {
            changed: true,
            revision,
            inserted,
        })
    }

    fn unchanged(&self) -> CommitReceipt {
        CommitReceipt {
            changed: false,
            revision: self.revision,
            inserted: Vec::new(),
        }
    }

    fn validate_handle(&self, handle: ObjectHandle, path: &str) -> Result<(), String> {
        if handle.namespace != self.revision.namespace {
            return Err(format!(
                "{path}.handle.namespace: belongs to a different runtime"
            ));
        }
        match self.generations.get(&handle.id) {
            None => Err(format!(
                "{path}.handle.id: object {} is not live",
                handle.id
            )),
            Some(&generation) if generation != handle.generation => Err(format!(
                "{path}.handle.generation: stale lifetime for object {}",
                handle.id
            )),
            Some(_) => Ok(()),
        }
    }
}

fn allocate_token(counter: &AtomicU64) -> Result<u64, String> {
    counter
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
            next.checked_add(1)
        })
        .map_err(|_| "runtime history token space exhausted".into())
}

fn insert_id(object: &Value, path: &str) -> Result<u32, String> {
    object
        .as_object()
        .and_then(|object| object.get("id"))
        .and_then(Value::as_u64)
        .filter(|id| (1..=u32::MAX as u64).contains(id))
        .map(|id| id as u32)
        .ok_or_else(|| format!("{path}.object.id: expected integer id in 1..={}", u32::MAX))
}

fn validated_id(object: &Value) -> u32 {
    object["id"].as_u64().expect("validated object ID") as u32
}

fn validate_transform(transform: Transform, path: &str) -> Result<(), String> {
    for (name, vector, min, max) in [
        ("position", transform.position, -10000.0, 10000.0),
        ("half_extents", transform.half_extents, 0.0001, 10000.0),
    ] {
        for (axis, value) in vector.to_array().into_iter().enumerate() {
            if !value.is_finite() || value < min || value > max {
                return Err(format!(
                    "{path}.{name}[{axis}]: expected finite f32 in [{min}, {max}]"
                ));
            }
        }
    }
    if !transform.rotation_y.is_finite() || !(-1024.0..=1024.0).contains(&transform.rotation_y) {
        return Err(format!(
            "{path}.rotation_y: expected finite f32 in [-1024, 1024]"
        ));
    }
    Ok(())
}

fn same_transform(object: &Map<String, Value>, transform: Transform) -> bool {
    let rotation_y = object.get("rotation_y").map_or(0.0, |value| {
        value.as_f64().expect("validated rotation") as f32
    });
    !object.contains_key("motion")
        && same_vector(&object["position"], transform.position)
        && same_vector(&object["half_extents"], transform.half_extents)
        && rotation_y.to_bits() == transform.rotation_y.to_bits()
}

fn same_vector(value: &Value, vector: Vec3) -> bool {
    value
        .as_array()
        .expect("validated vector")
        .iter()
        .zip(vector.to_array())
        .all(|(source, component)| {
            (source.as_f64().expect("validated component") as f32).to_bits() == component.to_bits()
        })
}

/// Frozen occupancy and lifetime metadata. Every query delegates to one inner
/// Snapshot; metadata cannot change during parallel composition.
#[derive(Clone)]
pub struct RuntimeSnapshot {
    inner: Snapshot,
    revision: SceneRevision,
    epoch: u64,
    generations: Option<Arc<BTreeMap<u32, u64>>>,
}

impl RuntimeSnapshot {
    /// Uniform host wrapper for an ordinary document, with no handle allocation
    /// or dynamic lifetime tracking. All query methods still delegate directly.
    pub fn untracked(inner: Snapshot) -> Self {
        Self {
            inner,
            revision: SceneRevision {
                namespace: 0,
                revision: 0,
            },
            epoch: 0,
            generations: None,
        }
    }

    pub fn revision(&self) -> SceneRevision {
        self.revision
    }

    pub fn history_epoch(&self) -> u64 {
        self.epoch
    }

    pub fn handle(&self, id: u32) -> Option<ObjectHandle> {
        self.generations
            .as_ref()?
            .get(&id)
            .map(|&generation| ObjectHandle {
                namespace: self.revision.namespace,
                id,
                generation,
            })
    }

    pub fn handles(&self) -> impl Iterator<Item = ObjectHandle> + '_ {
        self.generations
            .iter()
            .flat_map(|generations| generations.iter())
            .map(|(&id, &generation)| ObjectHandle {
                namespace: self.revision.namespace,
                id,
                generation,
            })
    }
}

impl Scene for RuntimeSnapshot {
    fn history_epoch(&self) -> u64 {
        self.epoch
    }
    fn trace(&self, ray: Ray, time: f32, stage: Stage, include_water: bool) -> Option<Hit> {
        self.inner.trace(ray, time, stage, include_water)
    }
    fn occluded(&self, ray: Ray, time: f32, stage: Stage, max_distance: f32) -> bool {
        self.inner.occluded(ray, time, stage, max_distance)
    }
    fn light_position(&self, time: f32) -> Vec3 {
        self.inner.light_position(time)
    }
    fn pigment(&self, material: u32) -> Vec3 {
        self.inner.pigment(material)
    }
    fn glyph(&self, material: u32, local: Vec3, normal: Vec3, illumination: f32) -> u8 {
        self.inner.glyph(material, local, normal, illumination)
    }
    fn local_materials(&self) -> bool {
        self.inner.local_materials()
    }
    fn material_sample(&self, hit: Hit, footprint: f32) -> Option<MaterialSample> {
        self.inner.material_sample(hit, footprint)
    }
    fn metallic_sample(&self, hit: Hit, footprint: f32) -> Option<MetallicSample> {
        self.inner.metallic_sample(hit, footprint)
    }
    fn environment_sample(&self, query: EnvironmentQuery) -> Option<EnvironmentSample> {
        self.inner.environment_sample(query)
    }
    fn glyph_sample(&self, hit: Hit, illumination: f32, footprint: f32) -> u8 {
        self.inner.glyph_sample(hit, illumination, footprint)
    }
    fn glyph_sample_key(&self, hit: Hit, footprint: f32) -> u32 {
        self.inner.glyph_sample_key(hit, footprint)
    }
    fn light_color(&self) -> Vec3 {
        self.inner.light_color()
    }
    fn glyph_history_key(&self, material: u32, local: Vec3, normal: Vec3) -> u32 {
        self.inner.glyph_history_key(material, local, normal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DEFAULT_SCENE_JSON, DEFAULT_SCENE_V2_JSON};

    fn object(id: u32) -> Value {
        json!({
            "id": id, "name": "Runtime block", "shape": "box", "material": 32,
            "position": [0, 1, 0], "half_extents": [0.5, 0.5, 0.5]
        })
    }

    fn fixture(version: u64) -> Value {
        let text = if version == 1 {
            DEFAULT_SCENE_JSON
        } else {
            DEFAULT_SCENE_V2_JSON
        };
        let mut value: Value = serde_json::from_str(text).unwrap();
        value["objects"] = json!([object(1)]);
        value
    }

    fn runtime(value: Value) -> RuntimeScene {
        RuntimeScene::new(SceneDocument::from_json(&value.to_string()).unwrap()).unwrap()
    }

    fn ray() -> Ray {
        Ray {
            origin: Vec3::new(0.0, 1.0, 3.0),
            dir: -Vec3::Z,
        }
    }

    fn transform() -> Transform {
        Transform {
            position: Vec3::new(0.0, 1.0, 0.0),
            half_extents: Vec3::splat(0.5),
            rotation_y: 0.0,
        }
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

    fn state(scene: &RuntimeScene) -> (Value, SceneRevision, u64, u64, Vec<ObjectHandle>) {
        (
            scene.source().clone(),
            scene.revision(),
            scene.history_epoch(),
            scene.last_generation,
            scene.handles().collect(),
        )
    }

    #[test]
    fn remove_reinsert_rejects_old_handles_and_preserves_old_snapshots() {
        let mut scene = runtime(fixture(2));
        let old_handle = scene.handle(1).unwrap();
        let old = scene.snapshot(0.0);
        let hit = old.trace(ray(), 0.0, Stage::Light, true);
        assert!(hit.is_some());
        let removed = scene
            .apply_transaction(
                scene.revision(),
                &[Operation::Remove { handle: old_handle }],
            )
            .unwrap();
        assert!(removed.changed);
        assert_eq!(removed.revision.revision, 1);
        assert!(scene.handle(1).is_none());
        assert!(scene
            .snapshot(0.0)
            .trace(ray(), 0.0, Stage::Light, true)
            .is_none());

        let mut replacement = object(1);
        replacement["position"][0] = json!(3);
        let inserted = scene
            .apply_transaction(
                scene.revision(),
                &[Operation::Insert {
                    object: replacement,
                }],
            )
            .unwrap();
        let current = scene.handle(1).unwrap();
        assert_eq!(inserted.inserted, vec![current]);
        assert_eq!(current.namespace, old_handle.namespace);
        assert!(current.generation > old_handle.generation);
        assert_ne!(old.history_epoch(), scene.history_epoch());
        assert_eq!(old.handle(1), Some(old_handle));
        assert_eq!(old.revision().revision, 0);
        for time in [0.0, 2.0, -3.0] {
            assert_eq!(
                hit_bits(old.trace(ray(), time, Stage::Light, true)),
                hit_bits(hit)
            );
            assert!(old.occluded(ray(), time, Stage::Light, 3.0));
            let current = scene.snapshot(time);
            assert!(current.trace(ray(), time, Stage::Light, true).is_none());
            assert!(!current.occluded(ray(), time, Stage::Light, 3.0));
        }
        let before = state(&scene);
        assert!(scene
            .apply_transaction(
                scene.revision(),
                &[Operation::Remove { handle: old_handle }]
            )
            .unwrap_err()
            .contains("generation"));
        assert_eq!(state(&scene), before);
    }

    #[test]
    fn invalid_batch_rolls_back_content_identity_and_generation_allocation() {
        let mut scene = runtime(fixture(2));
        let before = state(&scene);
        let mut invalid = object(2);
        invalid["material"] = json!(500);
        let error = scene
            .apply_transaction(
                scene.revision(),
                &[
                    Operation::Remove {
                        handle: scene.handle(1).unwrap(),
                    },
                    Operation::Insert { object: invalid },
                ],
            )
            .unwrap_err();
        assert!(error.contains("material"), "{error}");
        assert_eq!(state(&scene), before);
        let receipt = scene
            .apply_transaction(scene.revision(), &[Operation::Insert { object: object(2) }])
            .unwrap();
        assert_eq!(receipt.inserted[0].generation, before.3 + 1);
        assert_eq!(scene.handle(1), Some(before.4[0]));
    }

    #[test]
    fn revision_namespace_and_unique_targets_are_checked_before_publication() {
        let mut scene = runtime(fixture(1));
        let foreign = runtime(fixture(1));
        let before = state(&scene);
        assert_ne!(foreign.revision().namespace, scene.revision().namespace);
        assert!(scene.apply_transaction(foreign.revision(), &[]).is_err());
        assert!(scene
            .apply_transaction(
                scene.revision(),
                &[Operation::Remove {
                    handle: foreign.handle(1).unwrap()
                }],
            )
            .is_err());
        let stale = SceneRevision {
            revision: 1,
            ..scene.revision()
        };
        assert!(scene.apply_transaction(stale, &[]).is_err());
        let handle = scene.handle(1).unwrap();
        assert!(scene
            .apply_transaction(
                scene.revision(),
                &[
                    Operation::SetTransform {
                        handle,
                        transform: transform()
                    },
                    Operation::Remove { handle },
                ],
            )
            .unwrap_err()
            .contains("duplicate operation"));
        assert_eq!(state(&scene), before);
    }

    #[test]
    fn empty_and_effectively_identical_transforms_keep_source_and_epoch() {
        let mut scene = runtime(fixture(2));
        let before = state(&scene);
        let empty = scene.apply_transaction(scene.revision(), &[]).unwrap();
        assert!(!empty.changed);
        let same = scene
            .apply_transaction(
                scene.revision(),
                &[Operation::SetTransform {
                    handle: scene.handle(1).unwrap(),
                    transform: transform(),
                }],
            )
            .unwrap();
        assert_eq!(same, empty);
        // Authored integer coordinates and missing rotation_y stay untouched.
        assert_eq!(state(&scene), before);

        let negative_zero = Transform {
            rotation_y: -0.0,
            ..transform()
        };
        let change = scene
            .apply_transaction(
                scene.revision(),
                &[Operation::SetTransform {
                    handle: scene.handle(1).unwrap(),
                    transform: negative_zero,
                }],
            )
            .unwrap();
        assert!(change.changed);
        let stored = scene.source()["objects"][0]["rotation_y"].as_f64().unwrap() as f32;
        assert_eq!(stored.to_bits(), (-0.0_f32).to_bits());
        assert_eq!(scene.handle(1), Some(before.4[0]));
    }

    #[test]
    fn absolute_transform_cancels_motion_and_keeps_the_same_lifetime() {
        let mut value = fixture(2);
        value["objects"][0]["motion"] = json!({"translation":{"amplitude":[2,0,0],"speed":1}});
        let mut scene = runtime(value);
        let old = scene.snapshot(0.0);
        let handle = scene.handle(1).unwrap();
        assert!(old
            .trace(ray(), std::f32::consts::FRAC_PI_2, Stage::Light, true)
            .is_none());
        scene
            .apply_transaction(
                scene.revision(),
                &[Operation::SetTransform {
                    handle,
                    transform: transform(),
                }],
            )
            .unwrap();
        assert_eq!(scene.handle(1), Some(handle));
        assert!(scene.source()["objects"][0].get("motion").is_none());
        for time in [0.0, std::f32::consts::FRAC_PI_2] {
            assert!(scene
                .snapshot(time)
                .trace(ray(), time, Stage::Light, true)
                .is_some());
        }
        assert!(old
            .trace(ray(), std::f32::consts::FRAC_PI_2, Stage::Light, true)
            .is_none());

        let mut zero_motion = fixture(2);
        zero_motion["objects"][0]["motion"] = json!({"yaw":{"amplitude":0,"speed":0}});
        let mut zero = runtime(zero_motion);
        let operation = Operation::SetTransform {
            handle: zero.handle(1).unwrap(),
            transform: transform(),
        };
        assert!(
            zero.apply_transaction(zero.revision(), std::slice::from_ref(&operation))
                .unwrap()
                .changed
        );
        assert!(
            !zero
                .apply_transaction(zero.revision(), &[operation])
                .unwrap()
                .changed
        );
    }

    #[test]
    fn malformed_insert_and_nonfinite_or_out_of_range_transforms_are_atomic() {
        let mut scene = runtime(fixture(2));
        let before = state(&scene);
        for object in [Value::Null, json!({"id":0}), json!({"id":2,"unknown":true})] {
            assert!(scene
                .apply_transaction(scene.revision(), &[Operation::Insert { object }])
                .is_err());
            assert_eq!(state(&scene), before);
        }
        for transform in [
            Transform {
                position: Vec3::splat(f32::NAN),
                ..transform()
            },
            Transform {
                position: Vec3::new(10001.0, 1.0, 0.0),
                ..transform()
            },
            Transform {
                half_extents: Vec3::ZERO,
                ..transform()
            },
            Transform {
                rotation_y: f32::INFINITY,
                ..transform()
            },
            Transform {
                rotation_y: 1025.0,
                ..transform()
            },
        ] {
            assert!(scene
                .apply_transaction(
                    scene.revision(),
                    &[Operation::SetTransform {
                        handle: scene.handle(1).unwrap(),
                        transform
                    }],
                )
                .unwrap_err()
                .contains("$.operations[0].transform"));
            assert_eq!(state(&scene), before);
        }
    }

    #[test]
    fn limit_applies_to_final_occupancy_and_batch_size() {
        let mut value = fixture(2);
        value["objects"] = Value::Array((1..=128).map(object).collect());
        let mut scene = runtime(value);
        let before = state(&scene);
        let replacement = Operation::Insert {
            object: object(129),
        };
        assert!(scene
            .apply_transaction(scene.revision(), std::slice::from_ref(&replacement))
            .is_err());
        assert_eq!(state(&scene), before);
        let too_many = vec![
            Operation::Remove {
                handle: scene.handle(1).unwrap()
            };
            129
        ];
        assert!(scene
            .apply_transaction(scene.revision(), &too_many)
            .unwrap_err()
            .contains("128"));
        assert_eq!(state(&scene), before);
        // The insert occurs first in the batch, but the final state has 128.
        scene
            .apply_transaction(
                scene.revision(),
                &[
                    replacement,
                    Operation::Remove {
                        handle: scene.handle(1).unwrap(),
                    },
                ],
            )
            .unwrap();
        assert_eq!(scene.object_count(), 128);
        assert!(scene.handle(1).is_none());
        assert_eq!(scene.handle(129).unwrap().generation, 129);
    }

    #[test]
    fn inserts_preserve_v1_priority_and_v2_id_ties() {
        for version in [1, 2] {
            let mut value = fixture(version);
            value["objects"] = json!([object(7), object(9)]);
            let mut scene = runtime(value);
            let survivor = scene.handle(7).unwrap();
            scene
                .apply_transaction(scene.revision(), &[Operation::Insert { object: object(2) }])
                .unwrap();
            let hit = scene
                .snapshot(0.0)
                .trace(ray(), 0.0, Stage::Light, true)
                .unwrap();
            assert_eq!(hit.object_id, if version == 1 { 7 } else { 2 });
            assert_eq!(scene.handle(7), Some(survivor));
            let ids: Vec<_> = scene.source()["objects"]
                .as_array()
                .unwrap()
                .iter()
                .map(validated_id)
                .collect();
            assert_eq!(ids, vec![7, 9, 2]);
        }
    }

    #[test]
    fn overflow_is_rejected_without_mutating_the_owner_or_wrapping_tokens() {
        let counter = AtomicU64::new(u64::MAX - 1);
        assert_eq!(allocate_token(&counter).unwrap(), u64::MAX - 1);
        assert!(allocate_token(&counter).is_err());
        assert_eq!(counter.load(Ordering::Relaxed), u64::MAX);

        let mut generations = runtime(fixture(2));
        generations.last_generation = u64::MAX;
        let before = state(&generations);
        assert!(generations
            .apply_transaction(
                generations.revision(),
                &[Operation::Insert { object: object(2) }]
            )
            .unwrap_err()
            .contains("generation exhausted"));
        assert_eq!(state(&generations), before);

        let mut revisions = runtime(fixture(2));
        revisions.revision.revision = u64::MAX;
        let before = state(&revisions);
        assert!(revisions
            .apply_transaction(
                revisions.revision(),
                &[Operation::Remove {
                    handle: revisions.handle(1).unwrap()
                }],
            )
            .unwrap_err()
            .contains("revision exhausted"));
        assert_eq!(state(&revisions), before);
        assert!(
            !revisions
                .apply_transaction(revisions.revision(), &[])
                .unwrap()
                .changed
        );
    }

    #[test]
    fn untracked_wrapper_has_no_dynamic_identity_and_preserves_queries() {
        let document = SceneDocument::from_json(DEFAULT_SCENE_V2_JSON).unwrap();
        let plain = document.snapshot(0.0);
        let untracked = RuntimeSnapshot::untracked(plain.clone());
        assert_eq!(untracked.history_epoch(), 0);
        assert_eq!(Scene::history_epoch(&untracked), 0);
        assert_eq!(
            untracked.revision(),
            SceneRevision {
                namespace: 0,
                revision: 0
            }
        );
        assert!(untracked.handle(1).is_none());
        assert_eq!(untracked.handles().count(), 0);
        assert!(untracked.generations.is_none());
        for time in [0.0, 1.0] {
            for include_water in [false, true] {
                assert_eq!(
                    hit_bits(plain.trace(ray(), time, Stage::Light, include_water)),
                    hit_bits(untracked.trace(ray(), time, Stage::Light, include_water))
                );
            }
            assert_eq!(
                plain.occluded(ray(), time, Stage::Light, 20.0),
                untracked.occluded(ray(), time, Stage::Light, 20.0)
            );
        }
    }
}
