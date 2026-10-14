//! Authoring acceptance: failed edits and disk conflicts must preserve useful
//! data; saved/reopened projects replay the same scene in all three views.
use ascii_material_room::{
    authoring::{Project, ProjectFile},
    authoring_camera::CameraBank,
    updates::{Playback, UpdateSequence},
    SceneDocument,
};
use ascii_render::{camera::Mode, core::Stage, frame::Composer};
use glam::Vec3;
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(1);
struct TestDirectory(PathBuf);
impl TestDirectory {
    fn new() -> Self {
        for _ in 0..128 {
            let id = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("ascii-authoring-test-{}-{id}", std::process::id()));
            match fs::create_dir(&path) {
                Ok(()) => return Self(path.canonicalize().unwrap()),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("cannot create test directory: {error}"),
            }
        }
        panic!("cannot reserve test directory")
    }
    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}
impl Drop for TestDirectory {
    fn drop(&mut self) {
        // This absolute, unique directory was created by this guard alone.
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn dynamic_project() -> Project {
    let document =
        SceneDocument::from_json(include_str!("../assets/dynamic-room-v2.json")).unwrap();
    let sequence =
        UpdateSequence::from_json(include_str!("../assets/dynamic-updates-v1.json")).unwrap();
    Project::from_json(
        &json!({
            "version": 1, "scene": document.source(), "updates": sequence.source(),
            "camera_controls": CameraBank::defaults(&document).to_json(),
        })
        .to_string(),
    )
    .unwrap()
}

fn parse(value: Value) -> Result<Project, String> {
    Project::from_json(&value.to_string())
}
fn patch(operations: Value) -> String {
    json!({"version":1,"operations":operations}).to_string()
}

#[test]
fn project_checks_original_duplicate_keys_and_complete_nested_contracts() {
    let project = dynamic_project();
    let source = project.to_json().unwrap();
    for (field, value, expected) in [
        ("version", json!(2), "$.version"),
        ("extra", json!(true), "$.extra"),
        ("camera_controls", json!({}), "$.camera_controls.first"),
    ] {
        let mut candidate = project.source().clone();
        candidate[field] = value;
        assert!(parse(candidate).err().unwrap().contains(expected));
    }
    let mut legacy = project.source().clone();
    legacy["scene"] = serde_json::from_str(ascii_material_room::DEFAULT_SCENE_JSON).unwrap();
    assert!(parse(legacy).err().unwrap().contains("$.scene.version"));
    // Both spellings decode to the same key; checking only extracted Values
    // would have silently discarded the first version and accepted the second.
    let duplicated = source.replacen("\"version\": 1", "\"version\": 1, \"vers\\u0069on\": 1", 1);
    assert!(Project::from_json(&duplicated).is_err());
    assert!(Project::from_json(&" ".repeat(2 * 1024 * 1024 + 1)).is_err());
}

#[test]
fn base_edits_revalidate_future_events_and_leave_original_project_immutable() {
    let project = dynamic_project();
    let original = project.to_json().unwrap();
    let failure = project.remove(3).err().unwrap();
    assert!(failure.contains("$.updates.events"), "{failure}");
    assert_eq!(project.to_json().unwrap(), original);
    let modified = project.translate(4, Vec3::new(0.5, 0., 0.)).unwrap();
    assert_ne!(modified.document().source(), project.document().source());
    assert_eq!(project.to_json().unwrap(), original);
    // A repair spanning scene and timeline must validate the final candidate,
    // not reject the intermediate removal before the obsolete plan is removed.
    let repaired = project
        .apply_patch(&patch(json!([
            {"op":"remove","path":"/scene/objects/2"},
            {"op":"replace","path":"/updates/events","value":[]}
        ])))
        .unwrap();
    assert_eq!(
        repaired.document().object_count(),
        project.document().object_count() - 1
    );
    assert_eq!(repaired.sequence().event_count(), 0);
}

#[test]
fn invalid_last_event_is_rejected_before_a_project_is_exposed() {
    let project = dynamic_project();
    let mut candidate = project.source().clone();
    candidate["updates"]["events"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "tick": 1000, "operations":[{"op":"remove","id":999999}]
        }));
    let error = parse(candidate).err().unwrap();
    assert!(error.contains("$.updates.events[11]"), "{error}");
}

#[test]
fn duplicate_reserves_future_ids_and_material_patch_changes_actual_cells() {
    let project = dynamic_project();
    let mut source = project.source().clone();
    let mut future = source["scene"]["objects"][3].clone();
    future["id"] = json!(8);
    source["updates"] = json!({"version":1,"events":[
        {"tick":1,"operations":[{"op":"insert","object":future}]}
    ]});
    let project = parse(source).unwrap();
    let (duplicate, id) = project.duplicate(4).unwrap();
    assert_eq!(id, 9);
    assert_eq!(duplicate.document().object_count(), 8);
    let recolored = project
        .apply_patch(&patch(json!([
            {"op":"replace","path":"/scene/objects/0/material","value":34}
        ])))
        .unwrap();
    let camera = project.cameras().get(Mode::Third);
    let before = Composer::default()
        .compose(
            &project.document().snapshot(0.),
            camera,
            0.,
            Stage::Light,
            23,
            15,
            16. / 9.,
        )
        .0;
    let after = Composer::default()
        .compose(
            &recolored.document().snapshot(0.),
            camera,
            0.,
            Stage::Light,
            23,
            15,
            16. / 9.,
        )
        .0;
    assert_ne!(
        bytemuck::cast_slice::<_, u8>(&before),
        bytemuck::cast_slice::<_, u8>(&after)
    );
}

#[test]
fn bounded_patches_reject_bad_pointers_fields_and_partial_publication() {
    let project = Project::starter().unwrap();
    let before = project.to_json().unwrap();
    for operation in [
        json!({"op":"replace","path":"/version","value":2}),
        json!({"op":"replace","path":"/scene/objects/00/name","value":"bad"}),
        json!({"op":"replace","path":"/scene/~2name","value":"bad"}),
        json!({"op":"replace","path":"/scene/missing/child","value":1}),
        json!({"op":"remove","path":"/scene/objects/-"}),
        json!({"op":"replace","path":"/scene/name","value":"valid","unknown":1}),
        json!({"op":"remove","path":"/scene/name","value":"forbidden"}),
        json!({"op":"copy","path":"/scene/name","value":"bad"}),
    ] {
        let operations = json!([
            {"op":"replace","path":"/scene/name","value":"intermediate change"},
            operation
        ]);
        assert!(project.apply_patch(&patch(operations)).is_err());
        assert_eq!(project.to_json().unwrap(), before);
    }
    let repeated = vec![json!({"op":"replace","path":"/scene/name","value":"x"}); 129];
    assert!(project.apply_patch(&patch(json!(repeated))).is_err());
}

#[test]
fn save_reopen_preserves_all_authored_views_and_future_replay_cells() {
    let directory = TestDirectory::new();
    let path = directory.path("progetto-é.json");
    let project = dynamic_project()
        .translate(4, Vec3::new(0.4, 0., 0.2))
        .unwrap();
    let mut cameras = project.cameras().clone();
    for mode in [Mode::First, Mode::Third, Mode::Overhead] {
        let mut camera = cameras.get(mode);
        camera.orbit = 0.17;
        camera.offset = Vec3::new(0.2, 0.1, -0.1);
        camera.zoom = 1.2;
        cameras.set(camera).unwrap();
    }
    let project = project.with_cameras(cameras).unwrap();
    ProjectFile::create(&path, project.clone()).unwrap();
    let reopened = Project::load(&path).unwrap();
    assert_eq!(project.to_json().unwrap(), reopened.to_json().unwrap());
    // Deliberately preview late events before saving again: saved source must
    // still contain the original panel and complete timeline on reopening.
    let mut late = Playback::new(project.document().clone(), project.sequence().clone()).unwrap();
    late.advance_to(166).unwrap();
    assert_ne!(late.state().source(), project.document().source());
    for mode in [Mode::First, Mode::Third, Mode::Overhead] {
        for tick in [0, 31, 94, 166] {
            let mut a =
                Playback::new(project.document().clone(), project.sequence().clone()).unwrap();
            let mut b =
                Playback::new(reopened.document().clone(), reopened.sequence().clone()).unwrap();
            a.advance_to(tick).unwrap();
            b.advance_to(tick).unwrap();
            let time = tick as f32 / 30.;
            let a = Composer::default()
                .compose(
                    &a.state().snapshot(time),
                    project.cameras().get(mode),
                    time,
                    Stage::Light,
                    23,
                    15,
                    16. / 9.,
                )
                .0;
            let b = Composer::default()
                .compose(
                    &b.state().snapshot(time),
                    reopened.cameras().get(mode),
                    time,
                    Stage::Light,
                    23,
                    15,
                    16. / 9.,
                )
                .0;
            assert_eq!(
                bytemuck::cast_slice::<_, u8>(&a),
                bytemuck::cast_slice::<_, u8>(&b)
            );
        }
    }
}

#[test]
fn open_is_clean_despite_formatting_and_signed_zero_edits_are_not_discarded() {
    let directory = TestDirectory::new();
    let path = directory.path("format.json");
    let project = Project::starter().unwrap();
    fs::write(&path, project.source().to_string()).unwrap();
    let mut file = ProjectFile::open(&path).unwrap();
    assert!(!file.dirty());
    file.replace(file.project().clone());
    assert!(!file.dirty());
    let modified = file
        .project()
        .apply_patch(&patch(json!([
            {"op":"replace","path":"/camera_controls/first/orbit","value":-0.0}
        ])))
        .unwrap();
    file.replace(modified);
    assert!(file.dirty());
    file.save().unwrap();
    assert!(!file.dirty());
    assert_eq!(
        Project::load(&path)
            .unwrap()
            .cameras()
            .get(Mode::First)
            .orbit
            .to_bits(),
        (-0_f32).to_bits()
    );
}

#[test]
fn two_sessions_detect_exact_byte_conflicts_without_losing_memory_or_disk() {
    let directory = TestDirectory::new();
    let path = directory.path("project.json");
    ProjectFile::create(&path, Project::starter().unwrap()).unwrap();
    let mut first = ProjectFile::open(&path).unwrap();
    let mut second = ProjectFile::open(&path).unwrap();
    first.replace(first.project().translate(2, Vec3::X).unwrap());
    first.save().unwrap();
    let saved = fs::read(&path).unwrap();
    second.replace(second.project().translate(2, Vec3::Y).unwrap());
    let pending = second.project().to_json().unwrap();
    assert!(second.save().err().unwrap().contains("changed on disk"));
    assert!(second.dirty());
    assert_eq!(second.project().to_json().unwrap(), pending);
    assert_eq!(fs::read(&path).unwrap(), saved);
    assert!(second.reload().err().unwrap().contains("unsaved edits"));
    assert_eq!(fs::read(&path).unwrap(), saved);
}

#[test]
fn new_files_never_overwrite_and_stale_locks_are_not_stolen() {
    let directory = TestDirectory::new();
    let path = directory.path("existing.json");
    fs::write(&path, b"important unrecognized content").unwrap();
    assert!(ProjectFile::create(&path, Project::starter().unwrap()).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"important unrecognized content");
    let locked = directory.path("locked.json");
    let lock = directory.path("locked.json.lock");
    fs::write(&lock, b"pid=unknown\n").unwrap();
    assert!(ProjectFile::create(&locked, Project::starter().unwrap()).is_err());
    assert!(!locked.exists());
    assert_eq!(fs::read(&lock).unwrap(), b"pid=unknown\n");
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 2);
}

#[test]
fn invalid_reload_keeps_previous_project_and_valid_reload_publishes_whole_candidate() {
    let directory = TestDirectory::new();
    let path = directory.path("reload.json");
    let mut file = ProjectFile::create(&path, Project::starter().unwrap()).unwrap();
    let original = file.project().to_json().unwrap();
    fs::write(&path, b"{ invalid").unwrap();
    assert!(file.reload().is_err());
    assert_eq!(file.project().to_json().unwrap(), original);
    assert!(!file.dirty());
    let candidate = dynamic_project();
    fs::write(&path, candidate.to_json().unwrap()).unwrap();
    file.reload().unwrap();
    assert_eq!(
        file.project().to_json().unwrap(),
        candidate.to_json().unwrap()
    );
    assert_eq!(file.project().sequence().event_count(), 11);
}

#[cfg(windows)]
#[test]
fn denied_atomic_replacement_keeps_old_file_and_dirty_working_copy() {
    use std::fs::OpenOptions;
    use std::os::windows::fs::OpenOptionsExt;
    let directory = TestDirectory::new();
    let path = directory.path("busy.json");
    let mut file = ProjectFile::create(&path, Project::starter().unwrap()).unwrap();
    let original = fs::read(&path).unwrap();
    // Allow reads/writes but deny delete/rename sharing. Baseline reads work;
    // publication must fail after the complete temporary file was prepared.
    let _held = OpenOptions::new()
        .read(true)
        .share_mode(3)
        .open(&path)
        .unwrap();
    file.replace(file.project().translate(2, Vec3::X).unwrap());
    assert!(file.save().is_err());
    assert!(file.dirty());
    assert_eq!(fs::read(&path).unwrap(), original);
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 1);
}

fn author(arguments: &[&str], directory: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_author"))
        .args(arguments)
        .current_dir(directory)
        .output()
        .unwrap()
}

#[test]
fn actual_cli_creates_inspects_edits_and_rejects_invalid_patch_without_overwrite() {
    let directory = TestDirectory::new();
    assert!(author(&["new", "room.json"], &directory.0).status.success());
    let inspected = author(&["inspect", "room.json"], &directory.0);
    assert!(inspected.status.success());
    let inspected: Value = serde_json::from_slice(&inspected.stdout).unwrap();
    assert!(inspected["objects"].as_array().unwrap().len() > 1);
    assert!(author(
        &["translate", "room.json", "2", "0.25", "0", "0"],
        &directory.0
    )
    .status
    .success());
    let path = directory.path("room.json");
    let valid = fs::read(&path).unwrap();
    fs::write(
        directory.path("bad.patch.json"),
        patch(json!([
            {"op":"replace","path":"/scene/name","value":"would change"},
            {"op":"replace","path":"/scene/objects/0/material","value":1}
        ])),
    )
    .unwrap();
    let rejected = author(&["apply", "room.json", "bad.patch.json"], &directory.0);
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("material"));
    assert_eq!(fs::read(&path).unwrap(), valid);
    assert!(!author(&["new", "room.json"], &directory.0).status.success());
    assert_eq!(fs::read(&path).unwrap(), valid);
    assert!(author(&["save-as", "room.json", "copy.json"], &directory.0)
        .status
        .success());
    assert_eq!(fs::read(directory.path("copy.json")).unwrap(), valid);
}
