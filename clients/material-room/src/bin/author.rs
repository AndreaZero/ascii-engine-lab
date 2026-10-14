//! Small command-line authoring companion; it does not initialize a GPU.
use ascii_material_room::{
    authoring::{Project, ProjectFile},
    authoring_camera::CameraBank,
    updates::UpdateSequence,
    SceneDocument,
};
use glam::Vec3;
use serde_json::json;
use std::{fs::File, io::Read, path::Path};

const HELP: &str = r#"ASCII Material Room authoring

author new FILE                         Create a starter project; never overwrite.
author import FILE SCENE UPDATES        Bundle a v2 scene and v1 update plan.
author inspect FILE                     Show authored IDs, materials and events.
author apply FILE PATCHFILE              Validate all edits, then save atomically.
author insert FILE OBJECTFILE           Add an object in the existing v2 schema.
author duplicate FILE ID                 Copy a base object using an unused ID.
author remove FILE ID                    Remove a base object if future events remain valid.
author translate FILE ID DX DY DZ        Move its authored base; preserve motion.
author material FILE ID MATERIAL_ID      Assign an existing material.
author save-as FILE NEWFILE              Copy the whole project without overwriting.

apply accepts bounded JSON pointers, for example:
{"version":1,"operations":[
  {"op":"replace","path":"/scene/objects/1/position","value":[1,1,0]},
  {"op":"replace","path":"/scene/objects/1/material","value":32}
]}
Operations: add, remove, replace; at most 128, applied in order to one candidate.
Paths stay inside /scene, /updates and /camera_controls. Array positions are
zero-based; add /scene/objects/- appends. Invalid candidates do not change disk.
Each save checks the exact bytes read at open, uses a cooperative .lock and
same-directory temporary file. Non-cooperating writers can still race the final
check; close other editors while saving. Existing/stale locks are never stolen.
Preview: ascii-material-room --project FILE; validate: add --validate.
All commands store authored starting data, never the state after playback.
"#;

fn main() {
    if let Err(error) = run() {
        eprintln!("Authoring failed: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || matches!(args[0].as_str(), "help" | "--help" | "-h") {
        println!("{HELP}");
        return Ok(());
    }
    let command = args[0].as_str();
    let expected = match command {
        "new" | "inspect" => 2,
        "apply" | "insert" | "duplicate" | "remove" | "save-as" => 3,
        "import" | "material" => 4,
        "translate" => 6,
        _ => return Err(format!("unknown command {command}; run author --help")),
    };
    if args.len() != expected {
        return Err(format!(
            "{command}: expected {} arguments; run author --help",
            expected - 1
        ));
    }
    let path = Path::new(&args[1]);
    match command {
        "new" => {
            ProjectFile::create(path, Project::starter()?)?;
            println!("Created {}", path.display());
        }
        "import" => {
            let scene = SceneDocument::load(Path::new(&args[2]))?;
            let updates = UpdateSequence::load(Path::new(&args[3]))?;
            let source = json!({
                "version": 1,
                "scene": scene.source(),
                "updates": updates.source(),
                "camera_controls": CameraBank::defaults(&scene).to_json(),
            });
            let project = Project::from_json(&source.to_string())?;
            ProjectFile::create(path, project)?;
            println!("Created self-contained project {}", path.display());
        }
        "inspect" => {
            let file = ProjectFile::open(path)?;
            let project = file.project();
            let scene = project.document().source();
            let objects: Vec<_> = scene["objects"]
                .as_array()
                .expect("validated objects")
                .iter()
                .enumerate()
                .map(|(index, object)| {
                    json!({
                        "array_index": index, "id": object["id"], "name": object["name"],
                        "shape": object["shape"], "material": object["material"],
                        "position": object["position"], "half_extents": object["half_extents"],
                    })
                })
                .collect();
            let materials: Vec<_> = scene["materials"]
                .as_array()
                .expect("validated materials")
                .iter()
                .enumerate()
                .map(|(index, material)| {
                    json!({
                        "array_index": index, "id": material["id"], "name": material["name"],
                    })
                })
                .collect();
            let summary = json!({
                "path": file.path(), "project_version": 1,
                "scene_version": project.document().version(),
                "name": project.document().name(), "objects": objects, "materials": materials,
                "update_events": project.sequence().source()["events"],
                "camera_controls": project.cameras().to_json(),
            });
            println!(
                "{}",
                serde_json::to_string_pretty(&summary).map_err(|e| e.to_string())?
            );
        }
        "save-as" => {
            let file = ProjectFile::open(path)?;
            ProjectFile::create(Path::new(&args[2]), file.project().clone())?;
            println!("Created {}", args[2]);
        }
        _ => {
            let mut file = ProjectFile::open(path)?;
            let candidate = match command {
                "apply" => file
                    .project()
                    .apply_patch(&read_edit(Path::new(&args[2]))?)?,
                "insert" => {
                    let object = read_edit(Path::new(&args[2]))?;
                    // Keep original object text inside the patch so duplicate
                    // keys are checked before serde can discard them.
                    let patch = format!(
                        "{{\"version\":1,\"operations\":[{{\"op\":\"add\",\"path\":\"/scene/objects/-\",\"value\":{object}}}]}}"
                    );
                    file.project().apply_patch(&patch)?
                }
                "duplicate" => {
                    let (project, id) = file.project().duplicate(object_id(&args[2])?)?;
                    println!("New object ID: {id}");
                    project
                }
                "remove" => file.project().remove(object_id(&args[2])?)?,
                "translate" => {
                    let mut delta = [0_f32; 3];
                    for (index, value) in args[3..].iter().enumerate() {
                        delta[index] = value
                            .parse()
                            .map_err(|_| format!("invalid translation {value}"))?;
                    }
                    file.project()
                        .translate(object_id(&args[2])?, Vec3::from_array(delta))?
                }
                "material" => {
                    let id = object_id(&args[2])?;
                    let material = object_id(&args[3])?;
                    let mut scene = file.project().document().source().clone();
                    let object = scene["objects"]
                        .as_array_mut()
                        .expect("validated objects")
                        .iter_mut()
                        .find(|object| object["id"].as_u64() == Some(id as u64))
                        .ok_or_else(|| {
                            format!("object {id} is not in the authored starting scene")
                        })?;
                    object["material"] = json!(material);
                    file.project().with_scene(scene)?
                }
                _ => unreachable!("validated command"),
            };
            file.replace(candidate);
            file.save()?;
            println!("Saved {}", file.path().display());
        }
    }
    Ok(())
}

fn object_id(value: &str) -> Result<u32, String> {
    value
        .parse::<u32>()
        .ok()
        .filter(|id| *id != 0)
        .ok_or_else(|| format!("{value}: expected integer ID in 1..={}", u32::MAX))
}

fn read_edit(path: &Path) -> Result<String, String> {
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|error| format!("{}: {error}", path.display()))?
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    if bytes.len() > 1024 * 1024 {
        return Err(format!(
            "{}: edit file exceeds 1048576 bytes",
            path.display()
        ));
    }
    String::from_utf8(bytes).map_err(|error| format!("{}: invalid UTF-8: {error}", path.display()))
}
