//! Bounded authoring over the existing scene and update formats.
//!
//! A project stores the authored starting scene, never a playback snapshot.
//! Every candidate validates both its scene and all future update events before
//! publication. File writes use a same-directory temporary file and cooperative
//! lock, with an exact-byte conflict check against the last open/save. An editor
//! that ignores the lock can still race the final check and replacement; this is
//! not a filesystem compare-and-swap or a power-loss durability guarantee.
use crate::{
    authoring_camera::CameraBank,
    content::{reject_duplicate_keys, SceneDocument},
    updates::{Playback, UpdateSequence},
    DEFAULT_SCENE_V2_JSON,
};
use glam::Vec3;
use serde_json::{json, Map, Value};
use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

const MAX_PROJECT_BYTES: usize = 2 * 1024 * 1024;
const MAX_PATCH_BYTES: usize = 1024 * 1024;
const MAX_PATCH_OPERATIONS: usize = 128;
static NEXT_TEMP: AtomicU64 = AtomicU64::new(1);

/// Validated source plus reusable preview inputs. Clones retain authored data,
/// not a running playback owner or its transient lifetime handles.
#[derive(Clone)]
pub struct Project {
    source: Value,
    document: SceneDocument,
    sequence: UpdateSequence,
    cameras: CameraBank,
}

impl Project {
    pub fn from_json(text: &str) -> Result<Self, String> {
        if text.len() > MAX_PROJECT_BYTES {
            return Err("project: JSON exceeds 2097152 bytes".into());
        }
        let source: Value = serde_json::from_str(text)
            .map_err(|error| format!("project: invalid JSON: {error}"))?;
        // Check the original whole text before extracting nested Values: serde
        // alone would already have discarded duplicate (including escaped) keys.
        reject_duplicate_keys(text)?;
        let root = fields(
            &source,
            "$",
            &["version", "scene", "updates", "camera_controls"],
        )?;
        if root.get("version").and_then(Value::as_u64) != Some(1) {
            return Err("$.version: expected project version 1".into());
        }
        let scene = required(root, "scene", "$")?;
        let document = SceneDocument::from_json(&encode(scene)?)
            .map_err(|error| nested_error("$.scene", &error))?;
        if document.version() != 2 {
            return Err("$.scene.version: authoring projects require scene version 2".into());
        }
        let sequence = UpdateSequence::from_json(&encode(required(root, "updates", "$")?)?)
            .map_err(|error| nested_error("$.updates", &error))?;
        let cameras = CameraBank::from_json(&document, required(root, "camera_controls", "$")?)?;
        Playback::new(document.clone(), sequence.clone())
            .map_err(|error| nested_error("$.updates", &error))?;
        let result = Self {
            source,
            document,
            sequence,
            cameras,
        };
        // Accepted projects can also be written in the documented pretty form.
        result.to_json()?;
        Ok(result)
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        let bytes = read_bounded(path, MAX_PROJECT_BYTES)?;
        Self::from_json(utf8(&bytes, path)?)
    }

    pub fn starter() -> Result<Self, String> {
        let document = SceneDocument::from_json(DEFAULT_SCENE_V2_JSON)?;
        Self::from_value(json!({
            "version": 1,
            "scene": document.source(),
            "updates": {"version": 1, "events": []},
            "camera_controls": CameraBank::defaults(&document).to_json(),
        }))
    }

    pub fn source(&self) -> &Value {
        &self.source
    }

    pub fn to_json(&self) -> Result<String, String> {
        let mut text = serde_json::to_string_pretty(&self.source)
            .map_err(|error| format!("project: cannot encode JSON: {error}"))?;
        text.push('\n');
        if text.len() > MAX_PROJECT_BYTES {
            return Err("project: formatted JSON exceeds 2097152 bytes".into());
        }
        Ok(text)
    }

    pub fn document(&self) -> &SceneDocument {
        &self.document
    }

    pub fn sequence(&self) -> &UpdateSequence {
        &self.sequence
    }

    pub fn cameras(&self) -> &CameraBank {
        &self.cameras
    }

    pub fn with_scene(&self, scene: Value) -> Result<Self, String> {
        let mut candidate = self.source.clone();
        candidate["scene"] = scene;
        // Controls are rebound to the candidate's authored CameraView values.
        Self::from_value(candidate)
    }

    pub fn with_cameras(&self, cameras: CameraBank) -> Result<Self, String> {
        let mut candidate = self.source.clone();
        candidate["camera_controls"] = cameras.to_json();
        Self::from_value(candidate)
    }

    /// Translate the authored base position; an existing authored motion is
    /// preserved. This is an authoring edit, not RuntimeScene::SetTransform.
    pub fn translate(&self, id: u32, delta: Vec3) -> Result<Self, String> {
        if !delta.is_finite() {
            return Err("translation delta must be finite".into());
        }
        let mut scene = self.document.source().clone();
        let object = find_object_mut(&mut scene, id)?;
        let values = object["position"].as_array().expect("validated position");
        let position = Vec3::new(
            values[0].as_f64().expect("validated x") as f32,
            values[1].as_f64().expect("validated y") as f32,
            values[2].as_f64().expect("validated z") as f32,
        ) + delta;
        if !position.is_finite() {
            return Err("translated position must be finite".into());
        }
        object["position"] = json!(position.to_array());
        self.with_scene(scene)
    }

    /// Duplicate with a deterministic fresh authored ID. Reserve IDs mentioned
    /// by future events too, so the convenience command cannot steal a later
    /// insertion's identity. The complete timeline is still revalidated.
    pub fn duplicate(&self, id: u32) -> Result<(Self, u32), String> {
        let mut scene = self.document.source().clone();
        let mut duplicate = find_object_mut(&mut scene, id)?.clone();
        let mut used = BTreeSet::new();
        for object in scene["objects"].as_array().expect("validated objects") {
            used.insert(object["id"].as_u64().expect("validated id") as u32);
        }
        for event in self.sequence.source()["events"]
            .as_array()
            .expect("validated events")
        {
            for operation in event["operations"]
                .as_array()
                .expect("validated operations")
            {
                let value = if operation["op"] == "insert" {
                    &operation["object"]["id"]
                } else {
                    &operation["id"]
                };
                used.insert(value.as_u64().expect("validated operation id") as u32);
            }
        }
        let mut next = 1_u32;
        while used.contains(&next) {
            next = next.checked_add(1).ok_or("no unused authored object ID")?;
        }
        duplicate["id"] = json!(next);
        let old_name = duplicate["name"].as_str().expect("validated name");
        let name: String = old_name.chars().take(75).collect();
        duplicate["name"] = json!(format!("{name} copy"));
        scene["objects"]
            .as_array_mut()
            .expect("validated objects")
            .push(duplicate);
        Ok((self.with_scene(scene)?, next))
    }

    pub fn remove(&self, id: u32) -> Result<Self, String> {
        let mut scene = self.document.source().clone();
        find_object_mut(&mut scene, id)?;
        scene["objects"]
            .as_array_mut()
            .expect("validated objects")
            .retain(|object| object["id"].as_u64() != Some(id as u64));
        self.with_scene(scene)
    }

    /// Apply a bounded subset of JSON Patch to this project only. Paths must
    /// begin with /scene, /updates or /camera_controls. Operations are ordered;
    /// only the complete candidate is validated, allowing related edits in one
    /// atomic batch. No filesystem paths or external references are followed.
    pub fn apply_patch(&self, text: &str) -> Result<Self, String> {
        if text.len() > MAX_PATCH_BYTES {
            return Err("patch: JSON exceeds 1048576 bytes".into());
        }
        let patch: Value =
            serde_json::from_str(text).map_err(|error| format!("patch: invalid JSON: {error}"))?;
        reject_duplicate_keys(text)?;
        let root = fields(&patch, "$", &["version", "operations"])?;
        if root.get("version").and_then(Value::as_u64) != Some(1) {
            return Err("patch $.version: expected version 1".into());
        }
        let operations = required(root, "operations", "$")?
            .as_array()
            .ok_or("patch $.operations: expected array")?;
        if operations.len() > MAX_PATCH_OPERATIONS {
            return Err("patch $.operations: maximum 128 operations".into());
        }
        let mut candidate = self.source.clone();
        for (index, value) in operations.iter().enumerate() {
            let path = format!("patch $.operations[{index}]");
            let operation = value
                .get("op")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("{path}.op: expected add|remove|replace"))?;
            let allowed = match operation {
                "add" | "replace" => &["op", "path", "value"][..],
                "remove" => &["op", "path"][..],
                _ => return Err(format!("{path}.op: expected add|remove|replace")),
            };
            let fields = fields(value, &path, allowed)?;
            let pointer = required(fields, "path", &path)?
                .as_str()
                .ok_or_else(|| format!("{path}.path: expected JSON pointer string"))?;
            let tokens =
                pointer_tokens(pointer).map_err(|error| format!("{path}.path: {error}"))?;
            let replacement = if operation == "remove" {
                None
            } else {
                Some(required(fields, "value", &path)?.clone())
            };
            patch_value(&mut candidate, &tokens, operation, replacement)
                .map_err(|error| format!("{path} ({pointer}): {error}"))?;
        }
        Self::from_value(candidate).map_err(|error| format!("patch candidate rejected: {error}"))
    }

    fn from_value(source: Value) -> Result<Self, String> {
        Self::from_json(&encode(&source)?)
    }
}

fn find_object_mut(scene: &mut Value, id: u32) -> Result<&mut Value, String> {
    scene["objects"]
        .as_array_mut()
        .expect("validated objects")
        .iter_mut()
        .find(|object| object["id"].as_u64() == Some(id as u64))
        .ok_or_else(|| {
            format!("$.scene.objects: object {id} is not in the authored starting scene")
        })
}

fn fields<'a>(
    value: &'a Value,
    path: &str,
    allowed: &[&str],
) -> Result<&'a Map<String, Value>, String> {
    let fields = value
        .as_object()
        .ok_or_else(|| format!("{path}: expected object"))?;
    if let Some(key) = fields.keys().find(|key| !allowed.contains(&key.as_str())) {
        return Err(format!("{path}.{key}: unknown field"));
    }
    Ok(fields)
}

fn required<'a>(
    fields: &'a Map<String, Value>,
    key: &str,
    path: &str,
) -> Result<&'a Value, String> {
    fields
        .get(key)
        .ok_or_else(|| format!("{path}.{key}: required field missing"))
}

fn encode(value: &Value) -> Result<String, String> {
    serde_json::to_string(value).map_err(|error| format!("cannot encode JSON: {error}"))
}

fn nested_error(prefix: &str, error: &str) -> String {
    if let Some(rest) = error.strip_prefix('$') {
        format!("{prefix}{rest}")
    } else {
        format!("{prefix}: {error}")
    }
}

fn pointer_tokens(pointer: &str) -> Result<Vec<String>, String> {
    if pointer.len() > 512 || !pointer.starts_with('/') {
        return Err("expected absolute JSON pointer of at most 512 bytes".into());
    }
    let mut tokens = Vec::new();
    for component in pointer[1..].split('/') {
        let mut chars = component.chars();
        let mut token = String::new();
        while let Some(character) = chars.next() {
            match character {
                '~' => match chars.next() {
                    Some('0') => token.push('~'),
                    Some('1') => token.push('/'),
                    _ => return Err("only ~0 and ~1 escapes are supported".into()),
                },
                value => token.push(value),
            }
        }
        tokens.push(token);
    }
    if tokens.len() > 32 {
        return Err("JSON pointer exceeds 32 components".into());
    }
    if !matches!(
        tokens.first().map(String::as_str),
        Some("scene" | "updates" | "camera_controls")
    ) {
        return Err("only /scene, /updates and /camera_controls may be edited".into());
    }
    Ok(tokens)
}

fn array_index(token: &str, len: usize, allow_end: bool) -> Result<usize, String> {
    if allow_end && token == "-" {
        return Ok(len);
    }
    if token.is_empty()
        || !token.bytes().all(|byte| byte.is_ascii_digit())
        || (token.len() > 1 && token.starts_with('0'))
    {
        return Err("expected canonical nonnegative array index".into());
    }
    let index: usize = token.parse().map_err(|_| "array index is too large")?;
    if index > len || (!allow_end && index == len) {
        return Err(format!("array index {index} outside length {len}"));
    }
    Ok(index)
}

fn patch_value(
    candidate: &mut Value,
    tokens: &[String],
    operation: &str,
    replacement: Option<Value>,
) -> Result<(), String> {
    let (last, parents) = tokens.split_last().ok_or("empty JSON pointer")?;
    let mut parent = candidate;
    for token in parents {
        parent = match parent {
            Value::Object(fields) => fields
                .get_mut(token)
                .ok_or_else(|| format!("missing parent field {token}"))?,
            Value::Array(values) => {
                let index = array_index(token, values.len(), false)?;
                &mut values[index]
            }
            _ => return Err("parent is not an object or array".into()),
        };
    }
    match parent {
        Value::Object(fields) => {
            if operation != "add" && !fields.contains_key(last) {
                return Err(format!("field {last} does not exist"));
            }
            if operation == "remove" {
                fields.remove(last);
            } else {
                fields.insert(last.clone(), replacement.expect("required patch value"));
            }
        }
        Value::Array(values) => {
            let index = array_index(last, values.len(), operation == "add")?;
            match operation {
                "add" => values.insert(index, replacement.expect("required patch value")),
                "remove" => {
                    values.remove(index);
                }
                "replace" => values[index] = replacement.expect("required patch value"),
                _ => unreachable!("validated operation"),
            }
        }
        _ => return Err("parent is not an object or array".into()),
    }
    Ok(())
}

/// Disk baseline and editable in-memory project. `replace` does not write.
/// Saving/reloading never silently discards a dirty project or external edits.
pub struct ProjectFile {
    path: PathBuf,
    project: Project,
    expected_bytes: Vec<u8>,
    saved_source: String,
    dirty: bool,
}

impl ProjectFile {
    pub fn open(path: &Path) -> Result<Self, String> {
        let path = anchored_path(path)?;
        ensure_regular_file(&path)?;
        let expected_bytes = read_bounded(&path, MAX_PROJECT_BYTES)?;
        let project = Project::from_json(utf8(&expected_bytes, &path)?)?;
        let saved_source = encode(project.source())?;
        Ok(Self {
            path,
            project,
            expected_bytes,
            saved_source,
            dirty: false,
        })
    }

    /// Create a new file; an existing destination (including a symlink) is an
    /// error. Parent directories must already exist.
    pub fn create(path: &Path, project: Project) -> Result<Self, String> {
        let path = anchored_path(path)?;
        let bytes = project.to_json()?.into_bytes();
        save_bytes(&path, None, &bytes)?;
        let saved_source = encode(project.source())?;
        Ok(Self {
            path,
            project,
            expected_bytes: bytes,
            saved_source,
            dirty: false,
        })
    }

    pub fn project(&self) -> &Project {
        &self.project
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn replace(&mut self, project: Project) {
        // Compact serialization preserves number/signed-zero distinctions that
        // Value equality may erase. Formatting alone does not make a load dirty.
        self.dirty = encode(project.source()).expect("validated JSON source") != self.saved_source;
        self.project = project;
    }

    pub fn dirty(&self) -> bool {
        self.dirty
    }

    pub fn save(&mut self) -> Result<(), String> {
        let bytes = self.project.to_json()?.into_bytes();
        // Even a clean save checks for external changes; it is never a force
        // overwrite merely because the in-memory document was not edited.
        save_bytes(&self.path, Some(&self.expected_bytes), &bytes)?;
        self.expected_bytes = bytes;
        self.saved_source = encode(self.project.source())?;
        self.dirty = false;
        Ok(())
    }

    pub fn reload(&mut self) -> Result<(), String> {
        if self.dirty {
            return Err(format!(
                "{}: unsaved edits; save or explicitly reopen before reloading",
                self.path.display()
            ));
        }
        let replacement = Self::open(&self.path)?;
        *self = replacement;
        Ok(())
    }
}

fn anchored_path(path: &Path) -> Result<PathBuf, String> {
    let name = path
        .file_name()
        .ok_or_else(|| format!("{}: expected a project file name", path.display()))?;
    let parent = path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let parent = fs::canonicalize(parent).map_err(|error| {
        format!(
            "{}: cannot resolve parent directory: {error}",
            path.display()
        )
    })?;
    if !parent.is_dir() {
        return Err(format!("{}: parent is not a directory", path.display()));
    }
    Ok(parent.join(name))
}

fn ensure_regular_file(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("{}: cannot inspect file: {error}", path.display()))?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(format!(
            "{}: expected a regular file, not a link/directory",
            path.display()
        ));
    }
    Ok(())
}

fn read_bounded(path: &Path, max_bytes: usize) -> Result<Vec<u8>, String> {
    let file =
        File::open(path).map_err(|error| format!("{}: cannot open: {error}", path.display()))?;
    let mut bytes = Vec::new();
    file.take(max_bytes as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("{}: cannot read: {error}", path.display()))?;
    if bytes.len() > max_bytes {
        return Err(format!(
            "{}: file exceeds {max_bytes} bytes",
            path.display()
        ));
    }
    Ok(bytes)
}

fn utf8<'a>(bytes: &'a [u8], path: &Path) -> Result<&'a str, String> {
    std::str::from_utf8(bytes)
        .map_err(|error| format!("{}: invalid UTF-8: {error}", path.display()))
}

struct OwnedFile {
    path: PathBuf,
    handle: Option<File>,
}

impl Drop for OwnedFile {
    fn drop(&mut self) {
        self.handle.take();
        if let Err(error) = fs::remove_file(&self.path) {
            if error.kind() != std::io::ErrorKind::NotFound {
                // Publication may already have succeeded. A cleanup warning
                // must not report that the save failed without changing disk.
                eprintln!(
                    "{}: temporary/lock cleanup warning: {error}",
                    self.path.display()
                );
            }
        }
    }
}

fn lock_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().expect("anchored file name").to_os_string();
    name.push(".lock");
    path.with_file_name(name)
}

fn acquire_lock(path: &Path) -> Result<OwnedFile, String> {
    let lock_path = lock_path(path);
    let mut lock = OwnedFile {
        handle: Some(
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&lock_path)
                .map_err(|error| {
                    format!(
                        "{}: cannot acquire project lock (another writer or stale lock): {error}",
                        lock_path.display()
                    )
                })?,
        ),
        path: lock_path,
    };
    writeln!(
        lock.handle.as_mut().expect("open lock"),
        "pid={}",
        std::process::id()
    )
    .map_err(|error| format!("{}: cannot write lock: {error}", lock.path.display()))?;
    Ok(lock)
}

fn temporary_file(path: &Path) -> Result<OwnedFile, String> {
    for _ in 0..128 {
        let sequence = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let mut name = path.file_name().expect("anchored file name").to_os_string();
        name.push(format!(".tmp-{}-{sequence}", std::process::id()));
        let temporary_path = path.with_file_name(name);
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary_path)
        {
            Ok(handle) => {
                return Ok(OwnedFile {
                    path: temporary_path,
                    handle: Some(handle),
                })
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!(
                    "{}: cannot create temporary file: {error}",
                    temporary_path.display()
                ))
            }
        }
    }
    Err(format!(
        "{}: cannot reserve a unique temporary file",
        path.display()
    ))
}

fn check_baseline(path: &Path, expected: Option<&[u8]>) -> Result<(), String> {
    if let Some(expected) = expected {
        ensure_regular_file(path)?;
        let current = read_bounded(path, MAX_PROJECT_BYTES)?;
        if current != expected {
            return Err(format!(
                "{}: changed on disk since open/save; reload or save to a new file",
                path.display()
            ));
        }
    } else {
        match fs::symlink_metadata(path) {
            Ok(_) => {
                return Err(format!(
                    "{}: already exists; new project will not overwrite it",
                    path.display()
                ))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "{}: cannot inspect destination: {error}",
                    path.display()
                ))
            }
        }
    }
    Ok(())
}

fn save_bytes(path: &Path, expected: Option<&[u8]>, bytes: &[u8]) -> Result<(), String> {
    let _lock = acquire_lock(path)?;
    check_baseline(path, expected)?;
    let mut temporary = temporary_file(path)?;
    let file = temporary.handle.as_mut().expect("open temporary file");
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| {
            format!(
                "{}: cannot write/sync temporary file: {error}",
                temporary.path.display()
            )
        })?;
    temporary.handle.take();
    // Detect non-cooperating changes made while serializing/writing too. There
    // is still an unavoidable check-to-replace race with writers ignoring lock.
    check_baseline(path, expected)?;
    publish_file(&temporary.path, path, expected.is_some()).map_err(|error| {
        format!(
            "{}: cannot publish project; previous destination retained: {error}",
            path.display()
        )
    })?;
    Ok(())
}

#[cfg(windows)]
fn publish_file(source: &Path, destination: &Path, replace: bool) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    extern "system" {
        fn MoveFileExW(existing: *const u16, new: *const u16, flags: u32) -> i32;
    }
    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    // MOVEFILE_WRITE_THROUGH, plus MOVEFILE_REPLACE_EXISTING only when the
    // caller has an exact-byte baseline. No copy-across-volume fallback.
    let flags = 0x8 | if replace { 0x1 } else { 0 };
    // SAFETY: both UTF-16 buffers remain live and NUL-terminated for the call.
    let result = unsafe { MoveFileExW(source.as_ptr(), destination.as_ptr(), flags) };
    if result == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn publish_file(source: &Path, destination: &Path, replace: bool) -> std::io::Result<()> {
    if replace {
        fs::rename(source, destination)
    } else {
        // A hard link is an atomic create-if-absent operation on the same
        // filesystem. The temporary name is removed by its guard afterwards.
        fs::hard_link(source, destination)
    }
}
