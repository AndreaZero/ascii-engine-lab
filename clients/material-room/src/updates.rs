//! A bounded, versioned replay client of the runtime transaction API.
//! Events use absolute 30 Hz ticks; simulation ownership stays outside queries.
use crate::{
    content::{reject_duplicate_keys, SceneDocument},
    runtime::{Operation, RuntimeScene, Transform},
};
use glam::Vec3;
use serde_json::{Map, Value};
use std::{fs::File, io::Read, path::Path};

const MAX_BYTES: usize = 1024 * 1024;
pub const TICKS_PER_SECOND: u32 = 30;
pub const MAX_TICK: u32 = 86400 * TICKS_PER_SECOND;

#[derive(Clone)]
enum Edit {
    Insert(Value),
    Remove(u32),
    Transform(u32, Transform),
}
impl Edit {
    fn resolve(&self, state: &RuntimeScene) -> Result<Operation, String> {
        let handle = |id| {
            state
                .handle(id)
                .ok_or_else(|| format!("object {id} is not alive"))
        };
        Ok(match self {
            Self::Insert(object) => Operation::Insert {
                object: object.clone(),
            },
            Self::Remove(id) => Operation::Remove {
                handle: handle(*id)?,
            },
            Self::Transform(id, transform) => Operation::SetTransform {
                handle: handle(*id)?,
                transform: *transform,
            },
        })
    }
}

#[derive(Clone)]
struct Event {
    tick: u32,
    edits: Vec<Edit>,
}

/// File commands resolve IDs to the currently live instance at their event.
/// Applications holding references across commits should use ObjectHandle directly.
#[derive(Clone)]
pub struct UpdateSequence {
    source: Value,
    events: Vec<Event>,
}
impl UpdateSequence {
    pub fn from_json(text: &str) -> Result<Self, String> {
        if text.len() > MAX_BYTES {
            return Err("updates: JSON exceeds 1048576 bytes".into());
        }
        let source: Value = serde_json::from_str(text).map_err(|e| format!("updates: {e}"))?;
        reject_duplicate_keys(text)?;
        let root = fields(&source, "$", &["version", "events"])?;
        if root.get("version").and_then(Value::as_u64) != Some(1) {
            return Err("$.version: expected updates version 1".into());
        }
        let values = root
            .get("events")
            .and_then(Value::as_array)
            .ok_or("$.events: expected array")?;
        if values.len() > 256 {
            return Err("$.events: maximum 256 events".into());
        }
        let mut events = Vec::new();
        let mut previous_tick = None;
        for (index, value) in values.iter().enumerate() {
            let path = format!("$.events[{index}]");
            let event = fields(value, &path, &["tick", "operations"])?;
            let tick = uint(event.get("tick"), &format!("{path}.tick"), 0, MAX_TICK)?;
            if previous_tick.is_some_and(|old| tick <= old) {
                return Err(format!("{path}.tick: ticks must be strictly increasing"));
            }
            previous_tick = Some(tick);
            let operations = event
                .get("operations")
                .and_then(Value::as_array)
                .ok_or_else(|| format!("{path}.operations: expected array"))?;
            if operations.len() > 128 {
                return Err(format!("{path}.operations: maximum 128 operations"));
            }
            let mut edits = Vec::new();
            for (index, operation) in operations.iter().enumerate() {
                let path = format!("{path}.operations[{index}]");
                let op = operation
                    .get("op")
                    .and_then(Value::as_str)
                    .ok_or_else(|| format!("{path}.op: expected operation"))?;
                let edit = match op {
                    "insert" => {
                        let map = fields(operation, &path, &["op", "object"])?;
                        Edit::Insert(
                            map.get("object")
                                .ok_or_else(|| format!("{path}.object: required"))?
                                .clone(),
                        )
                    }
                    "remove" => {
                        let map = fields(operation, &path, &["op", "id"])?;
                        Edit::Remove(uint(map.get("id"), &format!("{path}.id"), 1, u32::MAX)?)
                    }
                    "transform" => {
                        let map = fields(
                            operation,
                            &path,
                            &["op", "id", "position", "half_extents", "rotation_y"],
                        )?;
                        Edit::Transform(
                            uint(map.get("id"), &format!("{path}.id"), 1, u32::MAX)?,
                            Transform {
                                position: vector(map.get("position"), &format!("{path}.position"))?,
                                half_extents: vector(
                                    map.get("half_extents"),
                                    &format!("{path}.half_extents"),
                                )?,
                                rotation_y: number(
                                    map.get("rotation_y"),
                                    &format!("{path}.rotation_y"),
                                )?,
                            },
                        )
                    }
                    _ => return Err(format!("{path}.op: expected insert|remove|transform")),
                };
                edits.push(edit);
            }
            events.push(Event { tick, edits });
        }
        Ok(Self { source, events })
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        let file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut bytes = Vec::new();
        file.take(MAX_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > MAX_BYTES {
            return Err("updates: JSON exceeds 1048576 bytes".into());
        }
        Self::from_json(
            &String::from_utf8(bytes).map_err(|e| format!("updates: invalid UTF-8: {e}"))?,
        )
    }

    pub fn source(&self) -> &Value {
        &self.source
    }
    pub fn event_count(&self) -> usize {
        self.events.len()
    }
}

/// Validates the complete event sequence against a disposable state before
/// exposing playback. Real events still execute transactions at runtime.
pub struct Playback {
    state: RuntimeScene,
    sequence: UpdateSequence,
    next_event: usize,
    tick: Option<u32>,
    commits: usize,
}
impl Playback {
    pub fn new(document: SceneDocument, sequence: UpdateSequence) -> Result<Self, String> {
        let mut preview = RuntimeScene::new(document.clone())?;
        for (index, event) in sequence.events.iter().enumerate() {
            apply_event(&mut preview, event).map_err(|e| format!("$.events[{index}]: {e}"))?;
        }
        Ok(Self {
            state: RuntimeScene::new(document)?,
            sequence,
            next_event: 0,
            tick: None,
            commits: 0,
        })
    }

    /// Catch-up applies every due batch in order, even if a display frame skips
    /// several ticks. Each batch is atomic, not the entire catch-up operation.
    /// Events are bounded to one day; playback may continue past that point,
    /// retaining final occupancy while ordinary authored motion still advances.
    pub fn advance_to(&mut self, tick: u32) -> Result<usize, String> {
        if self.tick.is_some_and(|old| tick < old) {
            return Err("playback tick must be monotone".into());
        }
        let before = self.commits;
        while let Some(event) = self.sequence.events.get(self.next_event) {
            if event.tick > tick {
                break;
            }
            let changed = apply_event(&mut self.state, event)
                .map_err(|e| format!("$.events[{}]: {e}", self.next_event))?;
            self.commits += usize::from(changed);
            self.next_event += 1;
        }
        self.tick = Some(tick);
        Ok(self.commits - before)
    }
    pub fn state(&self) -> &RuntimeScene {
        &self.state
    }
    pub fn completed_events(&self) -> usize {
        self.next_event
    }
    pub fn committed_updates(&self) -> usize {
        self.commits
    }
    pub fn tick(&self) -> Option<u32> {
        self.tick
    }
    pub fn source(&self) -> &Value {
        self.sequence.source()
    }
}

fn apply_event(state: &mut RuntimeScene, event: &Event) -> Result<bool, String> {
    let operations = event
        .edits
        .iter()
        .map(|edit| edit.resolve(state))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(state
        .apply_transaction(state.revision(), &operations)?
        .changed)
}

fn fields<'a>(
    value: &'a Value,
    path: &str,
    allowed: &[&str],
) -> Result<&'a Map<String, Value>, String> {
    let map = value
        .as_object()
        .ok_or_else(|| format!("{path}: expected object"))?;
    if let Some(key) = map.keys().find(|key| !allowed.contains(&key.as_str())) {
        return Err(format!("{path}.{key}: unknown field"));
    }
    Ok(map)
}
fn uint(value: Option<&Value>, path: &str, min: u32, max: u32) -> Result<u32, String> {
    value
        .and_then(Value::as_u64)
        .filter(|n| *n >= min as u64 && *n <= max as u64)
        .map(|n| n as u32)
        .ok_or_else(|| format!("{path}: expected integer {min}..{max}"))
}
fn number(value: Option<&Value>, path: &str) -> Result<f32, String> {
    value
        .and_then(Value::as_f64)
        .map(|n| n as f32)
        .filter(|n| n.is_finite())
        .ok_or_else(|| format!("{path}: expected finite f32 number"))
}
fn vector(value: Option<&Value>, path: &str) -> Result<Vec3, String> {
    let v = value
        .and_then(Value::as_array)
        .filter(|v| v.len() == 3)
        .ok_or_else(|| format!("{path}: expected three numbers"))?;
    Ok(Vec3::new(
        number(v.first(), path)?,
        number(v.get(1), path)?,
        number(v.get(2), path)?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DEFAULT_SCENE_V2_JSON;
    use serde_json::json;

    #[test]
    fn rejects_schema_duplicates_order_and_bounds() {
        for text in [
            "",
            "{",
            "{\"events\": [",
            r#"{"version":1,"version":1,"events":[]}"#,
            r#"{"version":1,"events":[{"tick":1,"operations":[]},{"tick":1,"operations":[]}]}"#,
            r#"{"version":1,"events":[{"tick":-1,"operations":[]}]}"#,
            r#"{"version":1,"events":[{"tick":0,"operations":[{"op":"remove","id":0}]}]}"#,
            r#"{"version":1,"events":[],"extra":0}"#,
        ] {
            assert!(UpdateSequence::from_json(text).is_err(), "{text}");
        }
    }
    #[test]
    fn full_preflight_rejects_later_invalid_event_without_publishing_playback() {
        let doc = SceneDocument::from_json(DEFAULT_SCENE_V2_JSON).unwrap();
        let original = doc.source().clone();
        let events = json!({"version":1,"events":[
            {"tick":1,"operations":[{"op":"remove","id":2}]},
            {"tick":99,"operations":[{"op":"remove","id":2}]}]});
        let plan = UpdateSequence::from_json(&events.to_string()).unwrap();
        assert!(Playback::new(doc.clone(), plan).is_err());
        assert_eq!(doc.source(), &original);
    }
    #[test]
    fn catchup_and_stepwise_playback_publish_same_data_and_generations() {
        let doc = SceneDocument::from_json(DEFAULT_SCENE_V2_JSON).unwrap();
        let object = doc.source()["objects"][1].clone();
        let plan = UpdateSequence::from_json(
            &json!({"version":1,"events":[
                {"tick":30,"operations":[{"op":"remove","id":2}]},
                {"tick":60,"operations":[{"op":"insert","object":object}]},
                {"tick":90,"operations":[]}
            ]})
            .to_string(),
        )
        .unwrap();
        let mut stepped = Playback::new(doc.clone(), plan.clone()).unwrap();
        let mut jumped = Playback::new(doc, plan).unwrap();
        let old = stepped.state().handle(2).unwrap();
        for tick in 0..=90 {
            stepped.advance_to(tick).unwrap();
        }
        assert_eq!(jumped.advance_to(90).unwrap(), 2);
        assert_eq!(stepped.state().source(), jumped.state().source());
        assert_eq!(
            stepped.state().handle(2).unwrap().generation,
            jumped.state().handle(2).unwrap().generation
        );
        assert_ne!(old, stepped.state().handle(2).unwrap());
        assert_eq!(stepped.completed_events(), 3);
        let epoch = stepped.state().snapshot(3.).history_epoch();
        assert_eq!(stepped.advance_to(90).unwrap(), 0);
        assert_eq!(stepped.state().snapshot(3.).history_epoch(), epoch);
        assert!(stepped.advance_to(89).is_err());
        assert_eq!(stepped.advance_to(MAX_TICK + 1).unwrap(), 0);
        assert_eq!(stepped.state().snapshot(86401.).history_epoch(), epoch);
    }
}
