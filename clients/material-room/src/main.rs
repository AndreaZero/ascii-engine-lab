//! A separate host: content files and application state belong to this client.
use ascii_material_room::{
    authoring::{Project, ProjectFile},
    authoring_camera::CameraBank,
    content::SceneDocument,
    coverage_study::coverage_camera,
    runtime::RuntimeSnapshot,
    updates::{Playback, UpdateSequence, TICKS_PER_SECOND},
};
use ascii_render::{
    budget::Profile,
    camera::{Camera, Mode},
    core::{Scene, Stage},
    frame::{overlay, Composer, PrimarySampling},
    gpu::{BackendChoice, Gpu, GpuOptions},
};
use glam::Vec3;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashSet},
    error::Error,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use winit::{
    application::ApplicationHandler,
    event::{ElementState, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};
type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Clone)]
struct Options {
    scene: PathBuf,
    output: PathBuf,
    profile: Profile,
    workers: usize,
    gpu: GpuOptions,
    mode: Mode,
    all: bool,
    headless: bool,
    validate: bool,
    sequence: bool,
    frames: usize,
    time: f32,
    seconds: f32,
    orbit_speed: f32,
    sampling: PrimarySampling,
    coverage_path: bool,
    cpu_only: bool,
    updates: Option<PathBuf>,
    project: Option<PathBuf>,
    project_data: Option<Project>,
}
impl Options {
    fn read() -> Result<Self> {
        Self::parse(
            std::env::args().skip(1),
            std::thread::available_parallelism().map_or(1, |v| v.get()),
        )
    }
    fn parse(mut args: impl Iterator<Item = String>, capacity: usize) -> Result<Self> {
        let mut o = Self {
            // Embedded compatible sample: moved executables never read the build checkout.
            scene: PathBuf::new(),
            output: "evidence/material-room".into(),
            profile: Profile::Balanced,
            workers: 1,
            gpu: GpuOptions::default(),
            mode: Mode::Third,
            all: false,
            headless: false,
            validate: false,
            sequence: false,
            frames: 1,
            time: 0.,
            seconds: 0.,
            orbit_speed: 0.,
            sampling: PrimarySampling::Center,
            coverage_path: false,
            cpu_only: false,
            updates: None,
            project: None,
            project_data: None,
        };
        let mut requested = None;
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--headless" => o.headless = true,
                "--validate" => o.validate = true,
                "--cpu-only" => {
                    o.cpu_only = true;
                    o.headless = true;
                }
                "--help" => {
                    println!("R15: --project FILE opens a self-contained authoring project (exclusive with --scene/--updates). Create with author new FILE. Window: Tab select object, arrows/PgUp/PgDown move 0.1, Ctrl+D duplicate, Delete remove, Ctrl+S save, F5 restart preview. Project cameras persist per view; R rejects reload while unsaved. Close never auto-saves. Materials/forms/timeline: author apply FILE PATCH.json. No scripted camera path in project window.");
                    println!("R14: --updates FILE replays validated occupancy transactions at absolute 30 Hz ticks. R reloads scene and updates atomically, restarting their clock.");
                    println!("R12: --sampling center|conservative9 (default center)\n--camera-path static|coverage (6-second deterministic path)\n--cpu-only: CPU measurement without GPU/captures, implies --headless\nWindow: C toggles sampling, P toggles scripted path; metrics saved to --output/window-run.json");
                    println!("ASCII MATERIAL ROOM\n--scene FILE --validate\n--headless --camera first|third|overhead|all --frames N --time SECONDS --output DIR\n--sequence N: capture every frame at 30 simulation ticks/s\n--orbit-speed RADIANS_PER_SECOND: headless camera motion (default 0)\n--profile economy|balanced|detail --workers auto|N --backend auto|vulkan|dx12|metal|gl\n--seconds N: window smoke duration\nWindow: 1/2/3 view, Q/E rotate, WASD move, Z/X height, Space pause, R reload scene and camera, H labels, Esc exit.\nThe default scene is the compatible v1 sample; pass --scene clients/material-room/assets/material-room-v2.json for local materials/environment and authored cameras.");
                    std::process::exit(0);
                }
                _ => {
                    let value = args
                        .next()
                        .ok_or_else(|| format!("{arg} requires a value"))?;
                    match arg.as_str() {
                        "--scene" => o.scene = value.into(),
                        "--project" => o.project = Some(value.into()),
                        "--updates" => o.updates = Some(value.into()),
                        "--output" => o.output = value.into(),
                        "--profile" => {
                            o.profile = Profile::parse(&value).ok_or("invalid profile")?
                        }
                        "--workers" => {
                            requested = if value == "auto" {
                                None
                            } else {
                                Some(value.parse()?)
                            }
                        }
                        "--backend" => {
                            o.gpu.backend = BackendChoice::parse(&value).ok_or("invalid backend")?
                        }
                        "--camera" => {
                            o.all = value == "all";
                            if !o.all {
                                o.mode = Mode::parse(&value).ok_or("invalid camera")?;
                            }
                        }
                        "--frames" => o.frames = value.parse()?,
                        "--sequence" => {
                            o.frames = value.parse()?;
                            o.headless = true;
                            o.sequence = true;
                        }
                        "--time" => o.time = value.parse()?,
                        "--seconds" => o.seconds = value.parse()?,
                        "--orbit-speed" => o.orbit_speed = value.parse()?,
                        "--sampling" => {
                            o.sampling = match value.as_str() {
                                "center" => PrimarySampling::Center,
                                "conservative9" => PrimarySampling::Conservative9,
                                _ => return Err("sampling must be center|conservative9".into()),
                            };
                        }
                        "--camera-path" => {
                            o.coverage_path = match value.as_str() {
                                "static" => false,
                                "coverage" => true,
                                _ => return Err("camera-path must be static|coverage".into()),
                            };
                        }
                        _ => return Err(format!("unknown option {arg}").into()),
                    }
                }
            }
        }
        o.workers = o.profile.resolve_workers(capacity, requested)?;
        if !(1..=900).contains(&o.frames)
            || !o.time.is_finite()
            || !(0. ..=86400.).contains(&o.time)
            || !o.seconds.is_finite()
            || !(0. ..=3600.).contains(&o.seconds)
            || !o.orbit_speed.is_finite()
            || !(-2. ..=2.).contains(&o.orbit_speed)
        {
            return Err(
                "frames 1..900; finite time 0..86400; finite seconds 0..3600; orbit-speed -2..2 required".into(),
            );
        }
        if o.all && !o.headless && !o.validate {
            return Err("--camera all requires headless mode".into());
        }
        if o.orbit_speed != 0. && !o.headless {
            return Err("--orbit-speed requires headless mode".into());
        }
        if o.cpu_only && o.sequence {
            return Err("--cpu-only cannot capture --sequence".into());
        }
        if o.project.is_some() && (!o.scene.as_os_str().is_empty() || o.updates.is_some()) {
            return Err("--project is exclusive with --scene and --updates".into());
        }
        if o.project.is_some() && o.coverage_path && !o.headless {
            return Err(
                "project window saves authored cameras; --camera-path coverage is headless only"
                    .into(),
            );
        }
        Ok(o)
    }
    fn camera_at(&self, base: Camera, elapsed: f32) -> Camera {
        let mut cam = if self.coverage_path {
            coverage_camera(base, elapsed)
        } else {
            base
        };
        cam.orbit += self.orbit_speed * elapsed;
        cam
    }
    fn sampling_name(&self) -> &'static str {
        match self.sampling {
            PrimarySampling::Center => "center",
            PrimarySampling::Conservative9 => "conservative9",
        }
    }
}
fn load_document(o: &mut Options) -> Result<SceneDocument> {
    if let Some(path) = &o.project {
        let project = Project::load(path)?;
        let doc = project.document().clone();
        o.project_data = Some(project);
        Ok(doc)
    } else if o.scene.as_os_str().is_empty() {
        Ok(SceneDocument::from_json(
            ascii_material_room::DEFAULT_SCENE_JSON,
        )?)
    } else {
        Ok(SceneDocument::load(&o.scene)?)
    }
}
fn camera(o: &Options, doc: &SceneDocument, mode: Mode) -> Camera {
    o.project_data
        .as_ref()
        .map_or_else(|| doc.camera(mode), |p| p.cameras().get(mode))
}
fn distribution(values: &[f64]) -> Value {
    if values.is_empty() {
        return Value::Null;
    }
    let mut values = values.to_vec();
    values.sort_by(f64::total_cmp);
    let q = |p: f64| values[((values.len() - 1) as f64 * p).round() as usize];
    json!({"p50":q(0.5),"p95":q(0.95),"p99":q(0.99)})
}

fn playback(o: &Options, doc: &SceneDocument) -> Result<Option<Playback>> {
    if let Some(project) = &o.project_data {
        return Ok(Some(Playback::new(
            doc.clone(),
            project.sequence().clone(),
        )?));
    }
    o.updates
        .as_ref()
        .map(|path| Ok(Playback::new(doc.clone(), UpdateSequence::load(path)?)?))
        .transpose()
}

fn snapshot(doc: &SceneDocument, playback: &Option<Playback>, time: f32) -> RuntimeSnapshot {
    playback.as_ref().map_or_else(
        || RuntimeSnapshot::untracked(doc.snapshot(time)),
        |playback| playback.state().snapshot(time),
    )
}

fn update_state(playback: &Option<Playback>) -> Value {
    playback.as_ref().map_or(Value::Null, |playback| json!({
        "tick":playback.tick(),"completed_events":playback.completed_events(),
        "committed_updates":playback.committed_updates(),
        "revision":playback.state().revision().revision,
        "objects":playback.state().object_count(),
        "identities":playback.state().handles().map(|h| json!({"id":h.id,"generation":h.generation})).collect::<Vec<_>>()
    }))
}
// Evidence only, outside composition/submission timers. Counts query coverage,
// not the readability or perceptual stability of the resulting glyphs.
fn coverage(scene: &impl Scene, cam: Camera, t: f32, cols: u32, rows: u32, aspect: f32) -> Value {
    let mut objects: BTreeMap<u32, (usize, usize)> = BTreeMap::new();
    for y in 0..rows {
        for x in 0..cols {
            if let Some(hit) =
                scene.trace(cam.ray(x, y, cols, rows, aspect), t, Stage::Light, false)
            {
                let count = objects.entry(hit.object_id).or_default();
                count.0 += 1;
                count.1 += usize::from(x == 0 || y == 0 || x + 1 == cols || y + 1 == rows);
            }
        }
    }
    json!(objects
        .into_iter()
        .map(|(id, (cells, border))| {
            json!({"object_id":id,"visible_cells":cells,"border_cells":border})
        })
        .collect::<Vec<_>>())
}
fn headless(o: &Options, doc: &SceneDocument) -> Result<()> {
    // Reject the complete update plan before creating GPU resources.
    // Freeze the file once for every view and for the recorded provenance.
    let sequence = if let Some(project) = &o.project_data {
        Some(project.sequence().clone())
    } else {
        o.updates
            .as_ref()
            .map(|path| UpdateSequence::load(path))
            .transpose()?
    };
    let make_playback = || -> Result<Option<Playback>> {
        sequence
            .as_ref()
            .map(|sequence| Ok(Playback::new(doc.clone(), sequence.clone())?))
            .transpose()
    };
    let mut validated_playback = make_playback()?;
    let input_updates = sequence.as_ref().map(|sequence| sequence.source().clone());
    let (w, h, cols, rows) = o.profile.dimensions();
    let mut gpu = if o.cpu_only {
        None
    } else {
        Some(pollster::block_on(Gpu::new_with_options(
            None, w, h, cols, rows, o.gpu,
        ))?)
    };
    std::fs::create_dir_all(&o.output)?;
    let modes = if o.all {
        vec![Mode::First, Mode::Third, Mode::Overhead]
    } else {
        vec![o.mode]
    };
    let mut runs = Vec::new();
    for mode in modes {
        let mut playback = if let Some(first) = validated_playback.take() {
            Some(first)
        } else {
            make_playback()?
        };
        let start_tick = (o.time * TICKS_PER_SECOND as f32).floor() as u32;
        if let Some(playback) = &mut playback {
            playback.advance_to(start_tick)?;
        }
        let folder = o.output.join(mode.name());
        std::fs::create_dir_all(&folder)?;
        let cam = camera(o, doc, mode);
        let aspect = w as f32 / h as f32;
        let mut composer = Composer::with_threads(o.workers)?;
        composer.set_primary_sampling(o.sampling);
        let frozen = snapshot(doc, &playback, o.time);
        let (a, _) = composer.compose(&frozen, cam, o.time, Stage::Light, cols, rows, aspect);
        let (b, _) = composer.compose(&frozen, cam, o.time, Stage::Light, cols, rows, aspect);
        composer.reset();
        let (c, _) = composer.compose(&frozen, cam, o.time, Stage::Light, cols, rows, aspect);
        if a != b || a != c {
            return Err("static composition/history/reset differs".into());
        }
        for _ in 0..12 {
            let (cells, _) =
                composer.compose(&frozen, cam, o.time, Stage::Light, cols, rows, aspect);
            if let Some(gpu) = &mut gpu {
                gpu.render(&cells)?;
                gpu.render_wait()?;
            }
        }
        composer.reset();
        let mut cpu = Vec::new();
        let mut snapshot_times = Vec::new();
        let mut composition_times = Vec::new();
        let mut update_times = Vec::new();
        let mut commit_times = Vec::new();
        let mut total_times = Vec::new();
        let mut gpu_wait = Vec::new();
        let mut frame_rows = Vec::new();
        let mut coverage_audits = Vec::new();
        let mut csv = String::from("frame,time_s,cpu_snapshot_and_compose_ms,submit_wait_ms,visible,matched,glyph_changes,snapshot_ms,compose_ms,primary_rays,alternate_primary,update_ms,cpu_total_ms,commits\n");
        for i in 0..o.frames {
            let t = o.time + i as f32 / 30.;
            let update_start = Instant::now();
            let commits = if let Some(playback) = &mut playback {
                playback.advance_to(start_tick + i as u32)?
            } else {
                0
            };
            let update_ms = update_start.elapsed().as_secs_f64() * 1000.;
            let start = Instant::now();
            let snapshot = snapshot(doc, &playback, t);
            let snapshot_ms = start.elapsed().as_secs_f64() * 1000.;
            let moving_cam = o.camera_at(cam, i as f32 / 30.);
            let compose_start = Instant::now();
            let (cells, stats) =
                composer.compose(&snapshot, moving_cam, t, Stage::Light, cols, rows, aspect);
            let cpu_ms = start.elapsed().as_secs_f64() * 1000.;
            let compose_ms = compose_start.elapsed().as_secs_f64() * 1000.;
            let total_ms = update_ms + cpu_ms;
            update_times.push(update_ms);
            total_times.push(total_ms);
            if commits > 0 {
                commit_times.push(update_ms);
            }
            cpu.push(cpu_ms);
            snapshot_times.push(snapshot_ms);
            composition_times.push(compose_ms);
            let mut wait_ms = 0.;
            if let Some(gpu) = &mut gpu {
                let start = Instant::now();
                gpu.render(&cells)?;
                gpu.render_wait()?;
                wait_ms = start.elapsed().as_secs_f64() * 1000.;
                gpu_wait.push(wait_ms);
                if o.sequence || i == 0 || i + 1 == o.frames {
                    gpu.capture(&cells, &folder.join(format!("frame-{i:04}.png")))?;
                }
            }
            if !o.cpu_only && (i == 0 || i + 1 == o.frames || (o.sequence && i % 30 == 0)) {
                coverage_audits.push(
                    json!({"frame":i,"objects":coverage(&snapshot,moving_cam,t,cols,rows,aspect)}),
                );
            }
            csv.push_str(&format!(
                "{i},{t:.8},{cpu_ms:.6},{wait_ms:.6},{},{},{},{snapshot_ms:.6},{compose_ms:.6},{},{},{update_ms:.6},{total_ms:.6},{commits}\n",
                stats.visible, stats.matched, stats.glyph_changes, stats.primary_rays, stats.alternate_primary
            ));
            frame_rows.push(
                json!({"frame":i,"time":t,"visible":stats.visible,"matched":stats.matched,
                "glyph_changes":stats.glyph_changes,"mean_color_delta":stats.mean_color_delta,
                "primary_rays":stats.primary_rays,"alternate_primary":stats.alternate_primary,
                "commits":commits,"update_ms":update_ms,"dynamic_state":update_state(&playback)}),
            );
        }
        std::fs::write(folder.join("frames.csv"), csv)?;
        let (eye, target) = cam.pose();
        runs.push(json!({"camera":mode.name(),"eye":eye.to_array(),"target":target.to_array(),"orbit":cam.orbit,"zoom":cam.zoom,
            "frame_count":o.frames,"static_changed_cells":0,"cpu_snapshot_and_compose_ms":distribution(&cpu),
            "snapshot_ms":distribution(&snapshot_times),"compose_ms":distribution(&composition_times),
            "update_ms":distribution(&update_times),"commit_frame_update_ms":distribution(&commit_times),
            "cpu_update_snapshot_and_compose_ms":distribution(&total_times),"final_dynamic_state":update_state(&playback),
            "submit_wait_ms":distribution(&gpu_wait),"frames":frame_rows,"coverage_audits":coverage_audits}));
        println!(
            "{}: {} {} frames from {}",
            mode.name(),
            o.frames,
            if o.cpu_only { "CPU-only" } else { "GPU" },
            doc.name()
        );
    }
    let manifest = json!({"client":"ascii-material-room","scene_path":o.scene,"scene_name":doc.name(),"scene_version":doc.version(),"objects":doc.object_count(),
        "input_scene":doc.source(),"project_path":o.project,"input_project":o.project_data.as_ref().map(Project::source),
        "input_updates":input_updates,"updates_path":o.updates,
        "grid":[cols,rows],"output":[w,h],"profile":o.profile.name(),"workers":o.workers,"stage":"light",
        "simulation_ticks_per_second":30,"time_start":o.time,"camera_orbit_speed":o.orbit_speed,
        "primary_sampling":o.sampling_name(),"camera_path":if o.coverage_path {"coverage"} else {"static"},
        "cpu_only":o.cpu_only,"gpu":gpu.as_ref().map(Gpu::capabilities),"runs":runs,
        "scope":"Independent client with JSON-owned immutable state, custom materials and analytic forms; no lab fixture imported.",
        "limits":["CPU includes snapshot and composition; GPU submit/wait is not a GPU timestamp or display FPS",
            "R14 updates are timed separately; CPU total adds update, snapshot and composition. Initial preflight/catch-up is outside frame timers",
            "Every effective transaction conservatively invalidates all glyph history; continuous updates reduce hysteresis reuse",
            "Offscreen synchronized run, 12 warmup frames excluded; per-frame capture and disk pauses affect scheduling",
            "Coverage audit uses central rays only outside timers; use coverage_audit example for independent dense-reference comparisons",
            "CPU-only skips GPU, image capture and central-ray coverage audits; submit_wait_ms is null in JSON (zero CSV placeholder)",
            "Conservative9 makes nine primary queries and shades one semantic hit; no exact area coverage or RGB averaging",
            "Static determinism is not a complete perceptual motion/flicker test; shapes/materials remain technical samples"]});
    std::fs::write(
        o.output.join("run.json"),
        serde_json::to_string_pretty(&manifest)?,
    )?;
    Ok(())
}

struct App {
    o: Options,
    doc: SceneDocument,
    playback: Option<Playback>,
    window: Option<Arc<Window>>,
    gpu: Option<Gpu>,
    composer: Composer,
    cam: Camera,
    project_file: Option<ProjectFile>,
    cameras: CameraBank,
    camera_dirty: bool,
    selected: Option<u32>,
    keys: HashSet<KeyCode>,
    last: Instant,
    start: Instant,
    time: f32,
    paused: bool,
    hud: bool,
    hidden: bool,
    zero_size: bool,
    size: (u32, u32),
    frames: u64,
    error: Option<String>,
    reload_message: String,
    window_intervals: Vec<f64>,
    window_cpu: Vec<f64>,
    window_updates: Vec<f64>,
    window_commit_updates: Vec<f64>,
    window_total_cpu: Vec<f64>,
    window_submit: Vec<f64>,
    configuration_changes: u32,
}
impl App {
    fn new(mut o: Options, mut doc: SceneDocument) -> Result<Self> {
        let project_file = o
            .project
            .as_ref()
            .map(|p| ProjectFile::open(p))
            .transpose()?;
        if let Some(file) = &project_file {
            doc = file.project().document().clone();
            o.project_data = Some(file.project().clone());
        }
        let cameras = o
            .project_data
            .as_ref()
            .map_or_else(|| CameraBank::defaults(&doc), |p| p.cameras().clone());
        let mut playback = playback(&o, &doc)?;
        if let Some(playback) = &mut playback {
            playback.advance_to((o.time * TICKS_PER_SECOND as f32).floor() as u32)?;
        }
        let (w, h, _, _) = o.profile.dimensions();
        let mut composer = Composer::with_threads(o.workers)?;
        composer.set_primary_sampling(o.sampling);
        Ok(Self {
            cam: camera(&o, &doc, o.mode),
            selected: doc.source()["objects"]
                .as_array()
                .and_then(|v| v.first())
                .and_then(|v| v["id"].as_u64())
                .map(|v| v as u32),
            project_file,
            cameras,
            camera_dirty: false,
            composer,
            time: o.time,
            o,
            doc,
            playback,
            window: None,
            gpu: None,
            keys: HashSet::new(),
            last: Instant::now(),
            start: Instant::now(),
            paused: false,
            hud: true,
            hidden: false,
            zero_size: false,
            size: (w, h),
            frames: 0,
            error: None,
            reload_message: String::new(),
            window_intervals: Vec::new(),
            window_cpu: Vec::new(),
            window_updates: Vec::new(),
            window_commit_updates: Vec::new(),
            window_total_cpu: Vec::new(),
            window_submit: Vec::new(),
            configuration_changes: 0,
        })
    }
    fn reload(&mut self) {
        if self.project_file.is_some() {
            let result = (|| -> Result<()> {
                if self.camera_dirty {
                    return Err("unsaved cameras: save before reload".into());
                }
                let file = self.project_file.as_mut().unwrap();
                file.reload()?;
                let project = file.project().clone();
                self.install_project(project)?;
                Ok(())
            })();
            self.authoring_message(result, "Project reloaded; preview at tick 0");
            return;
        }
        let replacement = (|| -> Result<_> {
            let next = if self.o.scene.as_os_str().is_empty() {
                SceneDocument::from_json(ascii_material_room::DEFAULT_SCENE_JSON)?
            } else {
                SceneDocument::load(&self.o.scene)?
            };
            let mut next_playback = playback(&self.o, &next)?;
            let time = if next_playback.is_some() {
                self.o.time
            } else {
                self.time
            };
            if let Some(playback) = &mut next_playback {
                playback.advance_to((time * TICKS_PER_SECOND as f32).floor() as u32)?;
            }
            Ok((next, next_playback, time))
        })();
        match replacement {
            Ok((next, next_playback, time)) => {
                // Construct the complete replacement before publishing it. A
                // failed load leaves document, camera, time and history intact.
                let cam = next.camera(self.cam.mode);
                self.doc = next;
                self.playback = next_playback;
                self.time = time;
                self.cam = cam;
                self.cameras = CameraBank::defaults(&self.doc);
                self.composer.reset();
                self.configuration_changes += 1;
                self.reload_message = "Scene reloaded".into();
            }
            Err(error) => {
                eprintln!("Scene reload rejected; previous scene kept: {error}");
                self.reload_message = "Invalid scene - kept previous version; see console".into();
            }
        }
    }
    fn authoring_message(&mut self, result: Result<()>, success: &str) {
        self.reload_message = match result {
            Ok(()) => success.into(),
            Err(error) => {
                eprintln!("Authoring rejected: {error}");
                format!("Rejected: {error}")
            }
        };
    }
    fn dirty(&self) -> bool {
        self.camera_dirty || self.project_file.as_ref().is_some_and(ProjectFile::dirty)
    }
    fn request_close(&mut self, discard: bool) -> bool {
        if self.dirty() && !discard {
            self.reload_message =
                "Unsaved project: Ctrl+S saves; Shift+Esc discards and closes".into();
            false
        } else {
            true
        }
    }
    fn install_project(&mut self, project: Project) -> Result<()> {
        let next = Playback::new(project.document().clone(), project.sequence().clone())?;
        self.doc = project.document().clone();
        self.cameras = project.cameras().clone();
        self.cam = self.cameras.get(self.cam.mode);
        self.o.project_data = Some(project);
        self.playback = Some(next);
        self.time = 0.;
        self.paused = true;
        self.camera_dirty = false;
        self.composer.reset();
        self.configuration_changes += 1;
        if !self.doc.source()["objects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["id"].as_u64().map(|v| v as u32) == self.selected)
        {
            self.selected = self.doc.source()["objects"]
                .as_array()
                .unwrap()
                .first()
                .and_then(|v| v["id"].as_u64())
                .map(|v| v as u32);
        }
        Ok(())
    }
    fn save_project(&mut self) {
        let result = (|| -> Result<()> {
            self.cameras.set(self.cam)?;
            let file = self
                .project_file
                .as_mut()
                .ok_or("use --project to save authoring")?;
            file.replace(file.project().with_cameras(self.cameras.clone())?);
            file.save()?;
            self.o.project_data = Some(file.project().clone());
            self.camera_dirty = false;
            Ok(())
        })();
        self.authoring_message(result, "Project saved");
    }
    fn select_next(&mut self) {
        let objects = self.doc.source()["objects"].as_array().unwrap();
        let index = objects
            .iter()
            .position(|v| v["id"].as_u64().map(|v| v as u32) == self.selected);
        self.selected = if objects.is_empty() {
            None
        } else {
            objects[(index.map_or(0, |i| i + 1)) % objects.len()]["id"]
                .as_u64()
                .map(|v| v as u32)
        };
    }
    fn edit_object(&mut self, key: KeyCode) {
        let result = (|| -> Result<()> {
            let id = self
                .selected
                .ok_or("no selected object; use author apply to insert")?;
            self.cameras.set(self.cam)?;
            let file = self
                .project_file
                .as_ref()
                .ok_or("use --project for editing")?;
            let project = file.project().with_cameras(self.cameras.clone())?;
            let (next, selected) = match key {
                KeyCode::Delete => (project.remove(id)?, None),
                KeyCode::KeyD => {
                    let (p, id) = project.duplicate(id)?;
                    (p, Some(id))
                }
                _ => {
                    let delta = match key {
                        KeyCode::ArrowLeft => Vec3::new(-0.1, 0., 0.),
                        KeyCode::ArrowRight => Vec3::new(0.1, 0., 0.),
                        KeyCode::ArrowUp => Vec3::new(0., 0., -0.1),
                        KeyCode::ArrowDown => Vec3::new(0., 0., 0.1),
                        KeyCode::PageUp => Vec3::new(0., 0.1, 0.),
                        KeyCode::PageDown => Vec3::new(0., -0.1, 0.),
                        _ => return Err("unsupported editor action".into()),
                    };
                    (project.translate(id, delta)?, Some(id))
                }
            };
            self.install_project(next.clone())?;
            self.project_file.as_mut().unwrap().replace(next);
            if selected.is_some() {
                self.selected = selected;
            }
            Ok(())
        })();
        self.authoring_message(result, "Edited base scene; paused at tick 0; Ctrl+S saves");
    }
    fn switch_camera(&mut self, mode: Mode) {
        if self.project_file.is_some() {
            if let Err(e) = self.cameras.set(self.cam) {
                self.reload_message = e;
                return;
            }
            self.cam = self.cameras.get(mode);
        } else {
            self.cam = self.doc.camera(mode);
        }
        self.composer.reset();
        self.configuration_changes += 1;
    }
    fn draw(&mut self) -> Result<()> {
        let now = Instant::now();
        let elapsed = now - self.last;
        let dt = elapsed.as_secs_f32().min(0.1);
        self.last = now;
        if !self.paused {
            self.time += dt;
        }
        let modifier = self.keys.contains(&KeyCode::ControlLeft)
            || self.keys.contains(&KeyCode::ControlRight)
            || self.keys.contains(&KeyCode::AltLeft)
            || self.keys.contains(&KeyCode::AltRight);
        let axis = |a, b| {
            if modifier {
                0.
            } else {
                self.keys.contains(&a) as u8 as f32 - self.keys.contains(&b) as u8 as f32
            }
        };
        let moving = axis(KeyCode::KeyE, KeyCode::KeyQ) != 0.
            || axis(KeyCode::KeyW, KeyCode::KeyS) != 0.
            || axis(KeyCode::KeyD, KeyCode::KeyA) != 0.
            || axis(KeyCode::KeyX, KeyCode::KeyZ) != 0.;
        if moving {
            self.configuration_changes += 1;
            self.camera_dirty |= self.project_file.is_some();
        }
        let before = self.cam;
        self.cam.orbit += axis(KeyCode::KeyE, KeyCode::KeyQ) * dt * 0.65;
        let (eye, target) = self.cam.pose();
        let forward = if self.project_file.is_some() {
            let direction = self.cam.ray(0, 0, 1, 1, 1.).dir;
            Vec3::new(direction.x, 0., direction.z).normalize_or_zero()
        } else {
            Vec3::new(target.x - eye.x, 0., target.z - eye.z).normalize_or_zero()
        };
        self.cam.offset += (forward * axis(KeyCode::KeyW, KeyCode::KeyS)
            + forward.cross(Vec3::Y) * axis(KeyCode::KeyD, KeyCode::KeyA)
            + Vec3::Y * axis(KeyCode::KeyX, KeyCode::KeyZ))
            * dt
            * 2.;
        if moving && self.project_file.is_some() && self.cameras.set(self.cam).is_err() {
            self.cam = before;
            self.reload_message = "Camera control limit reached".into();
        }
        let (_, _, cols, rows) = self.o.profile.dimensions();
        let update_start = Instant::now();
        let commits = if let Some(playback) = &mut self.playback {
            playback.advance_to((self.time * TICKS_PER_SECOND as f32).floor() as u32)?
        } else {
            0
        };
        let update_ms = update_start.elapsed().as_secs_f64() * 1000.;
        let cpu_start = Instant::now();
        let snapshot = snapshot(&self.doc, &self.playback, self.time);
        let cam = self.o.camera_at(self.cam, self.time - self.o.time);
        let (mut cells, _) = self.composer.compose(
            &snapshot,
            cam,
            self.time,
            Stage::Light,
            cols,
            rows,
            self.size.0 as f32 / self.size.1 as f32,
        );
        let cpu_ms = cpu_start.elapsed().as_secs_f64() * 1000.;
        if self.hud {
            overlay(
                &mut cells,
                cols,
                rows,
                &format!(
                    "MATERIAL ROOM | {} | {} | rev {} | {} | {}",
                    self.cam.mode.name(),
                    self.o.sampling_name(),
                    snapshot.revision().revision,
                    if self.project_file.is_some() {
                        format!(
                            "ID {} {} Tab select arrows move CtrlS save",
                            self.selected.unwrap_or(0),
                            if self.dirty() { "*" } else { "" }
                        )
                    } else {
                        "C sampling P path 1/2/3 view".into()
                    },
                    self.reload_message
                ),
            );
        }
        let submit_start = Instant::now();
        self.gpu
            .as_mut()
            .ok_or("GPU not initialized")?
            .render(&cells)?;
        let submit_ms = submit_start.elapsed().as_secs_f64() * 1000.;
        if self.frames >= 12 {
            self.window_intervals.push(elapsed.as_secs_f64() * 1000.);
            self.window_cpu.push(cpu_ms);
            self.window_updates.push(update_ms);
            self.window_total_cpu.push(update_ms + cpu_ms);
            if commits > 0 {
                self.window_commit_updates.push(update_ms);
            }
            self.window_submit.push(submit_ms);
        }
        self.frames += 1;
        Ok(())
    }
    fn write_window_report(&self) -> Result<()> {
        std::fs::create_dir_all(&self.o.output)?;
        let (w, h, cols, rows) = self.o.profile.dimensions();
        let report = json!({
            "scene_path":self.o.scene,"input_scene":self.doc.source(),"camera":self.cam.mode.name(),
            "project_path":self.o.project,"unsaved_changes":self.dirty(),"camera_controls":self.cameras.to_json(),
            "updates_path":self.o.updates,"input_updates":self.playback.as_ref().map(Playback::source),
            "final_dynamic_state":update_state(&self.playback),
            "primary_sampling":self.o.sampling_name(),"camera_path":if self.o.coverage_path {"coverage"} else {"static"},
            "workers":self.o.workers,"grid":[cols,rows],"requested_output":[w,h],"actual_output":self.size,
            "gpu":self.gpu.as_ref().map(Gpu::capabilities),"frames":self.frames,"warmup_frames_excluded":12,
            "configuration_changes":self.configuration_changes,"wall_seconds":self.start.elapsed().as_secs_f64(),
            "frame_interval_ms":distribution(&self.window_intervals),"cpu_snapshot_and_compose_ms":distribution(&self.window_cpu),
            "update_ms":distribution(&self.window_updates),"commit_frame_update_ms":distribution(&self.window_commit_updates),
            "cpu_update_snapshot_and_compose_ms":distribution(&self.window_total_cpu),
            "acquire_submit_present_ms":distribution(&self.window_submit),
            "raw_frame_interval_ms":self.window_intervals,"raw_cpu_ms":self.window_cpu,"raw_submit_ms":self.window_submit,
            "raw_update_ms":self.window_updates,"raw_total_cpu_ms":self.window_total_cpu,
            "limits":["Real window callbacks and FIFO presentation on one host; intervals include scheduling and prior frame work",
                "R14 update events are applied before snapshot; effective commits invalidate all glyph history. Update and composition timers are separate",
                "Acquire/submit/present is CPU wall time, not GPU timestamps or scanout timing",
                "Simulation dt is clamped at 100ms; warmup excludes first 12 frames; interactive changes are counted"]
        });
        std::fs::write(
            self.o.output.join("window-run.json"),
            serde_json::to_string_pretty(&report)?,
        )?;
        Ok(())
    }
}
impl ApplicationHandler for App {
    fn resumed(&mut self, e: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let result = (|| -> Result<()> {
            let window = Arc::new(
                e.create_window(
                    Window::default_attributes()
                        .with_title("ASCII Material Room | independent JSON client")
                        .with_inner_size(winit::dpi::PhysicalSize::new(self.size.0, self.size.1)),
                )?,
            );
            let (_, _, cols, rows) = self.o.profile.dimensions();
            self.gpu = Some(pollster::block_on(Gpu::new_with_options(
                Some(window.clone()),
                self.size.0,
                self.size.1,
                cols,
                rows,
                self.o.gpu,
            ))?);
            self.window = Some(window);
            self.last = Instant::now();
            self.start = self.last;
            Ok(())
        })();
        if let Err(error) = result {
            self.error = Some(error.to_string());
            e.exit();
        }
    }
    fn window_event(&mut self, e: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                if self.request_close(false) {
                    e.exit();
                }
            }
            WindowEvent::Resized(size) => {
                if self.frames > 0 && self.size != (size.width, size.height) {
                    self.configuration_changes += 1;
                }
                self.zero_size = size.width == 0 || size.height == 0;
                self.last = Instant::now();
                if let Some(g) = &mut self.gpu {
                    g.resize(size.width, size.height);
                }
                if !self.zero_size {
                    self.size = (size.width, size.height);
                }
            }
            WindowEvent::Focused(false) => self.keys.clear(),
            WindowEvent::Occluded(value) => {
                self.hidden = value;
                self.keys.clear();
                self.last = Instant::now();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(key) = event.physical_key {
                    if event.state == ElementState::Pressed {
                        self.keys.insert(key);
                        let ctrl = self.keys.contains(&KeyCode::ControlLeft)
                            || self.keys.contains(&KeyCode::ControlRight);
                        if ctrl
                            && matches!(key, KeyCode::KeyS | KeyCode::KeyD)
                            && self.project_file.is_some()
                        {
                            self.keys.remove(&key);
                            if !event.repeat {
                                if key == KeyCode::KeyS {
                                    self.save_project();
                                } else {
                                    self.edit_object(key);
                                }
                            }
                            return;
                        }
                        if !event.repeat {
                            match key {
                                KeyCode::Tab if self.project_file.is_some() => self.select_next(),
                                KeyCode::ArrowLeft
                                | KeyCode::ArrowRight
                                | KeyCode::ArrowUp
                                | KeyCode::ArrowDown
                                | KeyCode::PageUp
                                | KeyCode::PageDown
                                | KeyCode::Delete
                                    if self.project_file.is_some() =>
                                {
                                    self.edit_object(key)
                                }
                                KeyCode::F5 if self.project_file.is_some() => {
                                    let result = (|| -> Result<()> {
                                        self.cameras.set(self.cam)?;
                                        let project = self
                                            .project_file
                                            .as_ref()
                                            .unwrap()
                                            .project()
                                            .with_cameras(self.cameras.clone())?;
                                        let was_camera_dirty = self.camera_dirty;
                                        self.install_project(project)?;
                                        self.camera_dirty = was_camera_dirty;
                                        self.paused = false;
                                        Ok(())
                                    })();
                                    self.authoring_message(result, "Preview restarted");
                                }
                                KeyCode::Escape => {
                                    if self.request_close(
                                        self.keys.contains(&KeyCode::ShiftLeft)
                                            || self.keys.contains(&KeyCode::ShiftRight),
                                    ) {
                                        e.exit();
                                    }
                                }
                                KeyCode::Space => {
                                    self.paused = !self.paused;
                                    self.configuration_changes += 1;
                                }
                                KeyCode::KeyH => {
                                    self.hud = !self.hud;
                                    self.configuration_changes += 1;
                                }
                                KeyCode::KeyR => self.reload(),
                                KeyCode::KeyC => {
                                    self.o.sampling = match self.o.sampling {
                                        PrimarySampling::Center => PrimarySampling::Conservative9,
                                        PrimarySampling::Conservative9 => PrimarySampling::Center,
                                    };
                                    self.composer.set_primary_sampling(self.o.sampling);
                                    self.configuration_changes += 1;
                                }
                                KeyCode::KeyP if self.project_file.is_none() => {
                                    self.o.coverage_path = !self.o.coverage_path;
                                    self.composer.reset();
                                    self.configuration_changes += 1;
                                }
                                KeyCode::Digit1 | KeyCode::Digit2 | KeyCode::Digit3 => {
                                    self.switch_camera(match key {
                                        KeyCode::Digit1 => Mode::First,
                                        KeyCode::Digit2 => Mode::Third,
                                        _ => Mode::Overhead,
                                    });
                                }
                                _ => {}
                            }
                        }
                    } else {
                        self.keys.remove(&key);
                    }
                }
            }
            WindowEvent::RedrawRequested if !self.hidden && !self.zero_size => {
                if let Err(error) = self.draw() {
                    self.error = Some(error.to_string());
                    e.exit();
                }
            }
            _ => {}
        }
    }
    fn about_to_wait(&mut self, e: &ActiveEventLoop) {
        if self.o.seconds > 0. {
            e.set_control_flow(ControlFlow::WaitUntil(
                self.start + Duration::from_secs_f32(self.o.seconds),
            ));
        }
        if self.o.seconds > 0. && self.start.elapsed().as_secs_f32() >= self.o.seconds {
            println!("Material room window smoke: {} frames", self.frames);
            self.o.seconds = 0.;
            e.set_control_flow(ControlFlow::Wait);
            if self.request_close(false) {
                e.exit();
                return;
            }
        }
        if !self.hidden && !self.zero_size {
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }
    }
}
fn main() -> Result<()> {
    let mut options = Options::read()?;
    let doc = load_document(&mut options)?;
    if options.validate {
        let updates = playback(&options, &doc)?;
        println!(
            "Valid v{} scene: {} ({} objects)",
            doc.version(),
            doc.name(),
            doc.object_count()
        );
        if let Some(updates) = updates {
            println!(
                "Complete update sequence preflight passed: {} events",
                updates.source()["events"].as_array().map_or(0, Vec::len)
            );
        }
        return Ok(());
    }
    if options.headless {
        return headless(&options, &doc);
    }
    let loop_ = EventLoop::new()?;
    loop_.set_control_flow(ControlFlow::Wait);
    let mut app = App::new(options, doc)?;
    loop_.run_app(&mut app)?;
    app.write_window_report()?;
    if let Some(error) = app.error {
        return Err(error.into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ascii_material_room::DEFAULT_SCENE_JSON;
    #[test]
    fn project_editor_persists_three_views_and_rejects_conflicting_reload_save() {
        let directory =
            std::env::temp_dir().join(format!("ascii-r15-window-test-{}", std::process::id()));
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("project.json");
        ProjectFile::create(&path, Project::starter().unwrap()).unwrap();
        let mut options = Options::parse(
            ["--project".into(), path.to_string_lossy().into_owned()].into_iter(),
            1,
        )
        .unwrap();
        let doc = load_document(&mut options).unwrap();
        let mut app = App::new(options, doc).unwrap();
        let mut poses = Vec::new();
        for (i, mode) in [Mode::First, Mode::Third, Mode::Overhead]
            .into_iter()
            .enumerate()
        {
            app.switch_camera(mode);
            app.cam.orbit = 0.12 * (i + 1) as f32;
            app.cam.offset = Vec3::new(0.2, 0.1, -0.05);
            app.camera_dirty = true;
            poses.push(app.cam);
        }
        app.selected = Some(7);
        app.edit_object(KeyCode::ArrowRight);
        assert!(app.dirty());
        assert!(app.paused);
        assert_eq!(app.time, 0.);
        let edited = app.doc.source().clone();
        app.reload();
        assert_eq!(app.doc.source(), &edited);
        assert!(app.reload_message.contains("Rejected"));
        assert!(!app.request_close(false));
        assert!(app.request_close(true));
        app.save_project();
        assert!(!app.dirty(), "{}", app.reload_message);
        assert!(app.request_close(false));
        let reopened = Project::load(&path).unwrap();
        assert_eq!(reopened.document().source(), &edited);
        for cam in poses {
            let mut before = Composer::with_threads(1).unwrap();
            let mut after = Composer::with_threads(2).unwrap();
            let old = snapshot(&app.doc, &app.playback, 0.75);
            let new = reopened.document().snapshot(0.75);
            let (a, _) = before.compose(&old, cam, 0.75, Stage::Light, 80, 45, 16. / 9.);
            let (b, _) = after.compose(
                &new,
                reopened.cameras().get(cam.mode),
                0.75,
                Stage::Light,
                80,
                45,
                16. / 9.,
            );
            assert_eq!(a, b, "saved view {}", cam.mode.name());
        }
        // Saving after an ordinary external write must never replace it.
        let external = Project::starter().unwrap().to_json().unwrap();
        std::fs::write(&path, &external).unwrap();
        app.edit_object(KeyCode::ArrowLeft);
        app.save_project();
        assert!(app.dirty());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), external);
        app.reload();
        assert!(app.reload_message.contains("Rejected"));
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn portable_default_is_embedded_and_project_flags_do_not_mix_inputs() {
        let mut options = Options::parse(std::iter::empty(), 1).unwrap();
        assert!(options.scene.as_os_str().is_empty());
        assert_eq!(
            load_document(&mut options).unwrap().source(),
            SceneDocument::from_json(DEFAULT_SCENE_JSON)
                .unwrap()
                .source()
        );
        for args in [
            vec!["--project", "p.json", "--scene", "s.json"],
            vec!["--project", "p.json", "--updates", "u.json"],
            vec!["--project", "p.json", "--camera-path", "coverage"],
        ] {
            assert!(Options::parse(args.into_iter().map(str::to_string), 1).is_err());
        }
    }
    #[test]
    fn dynamic_reload_preserves_everything_on_invalid_future_event_and_restarts_on_success() {
        let prefix =
            std::env::temp_dir().join(format!("ascii-room-r14-reload-{}", std::process::id()));
        let scene_path = prefix.with_extension("scene.json");
        let updates_path = prefix.with_extension("updates.json");
        let source: Value =
            serde_json::from_str(ascii_material_room::DEFAULT_SCENE_V2_JSON).unwrap();
        std::fs::write(&scene_path, source.to_string()).unwrap();
        std::fs::write(
            &updates_path,
            json!({"version":1,"events":[{"tick":30,"operations":[{"op":"remove","id":2}]}]})
                .to_string(),
        )
        .unwrap();
        let mut options = Options::parse(std::iter::empty(), 1).unwrap();
        options.scene = scene_path.clone();
        options.updates = Some(updates_path.clone());
        let doc = SceneDocument::load(&scene_path).unwrap();
        let mut app = App::new(options, doc).unwrap();
        app.time = 2.;
        app.cam.offset = Vec3::new(0.1, 0.2, 0.3);
        app.playback.as_mut().unwrap().advance_to(60).unwrap();
        let before_state = update_state(&app.playback);
        let before_pose = app.cam.pose();
        let before = snapshot(&app.doc, &app.playback, app.time);
        let epoch = before.history_epoch();
        app.composer
            .compose(&before, app.cam, app.time, Stage::Light, 40, 23, 16. / 9.);
        let mut changed = source.clone();
        changed["name"] = json!("Valid new scene with invalid update plan");
        std::fs::write(&scene_path, changed.to_string()).unwrap();
        std::fs::write(
            &updates_path,
            json!({"version":1,"events":[{"tick":99,"operations":[{"op":"remove","id":999}]}]})
                .to_string(),
        )
        .unwrap();
        app.reload();
        assert_eq!(app.doc.source(), &source);
        assert_eq!(app.time, 2.);
        assert_eq!(app.cam.pose(), before_pose);
        assert_eq!(update_state(&app.playback), before_state);
        let unchanged = snapshot(&app.doc, &app.playback, app.time);
        assert_eq!(unchanged.history_epoch(), epoch);
        let (_, stats) = app.composer.compose(
            &unchanged,
            app.cam,
            app.time,
            Stage::Light,
            40,
            23,
            16. / 9.,
        );
        assert!(stats.matched > 0, "failed reload must retain history");
        for invalid in ["", "  ", "{", "{\"events\":["] {
            std::fs::write(&updates_path, invalid).unwrap();
            app.reload();
            assert_eq!(update_state(&app.playback), before_state);
            assert_eq!(app.time, 2.);
            assert_eq!(app.doc.source(), &source);
        }
        std::fs::write(&updates_path, r#"{"version":1,"events":[]}"#).unwrap();
        app.reload();
        assert_eq!(app.doc.source(), &changed);
        assert_eq!(app.time, app.o.time);
        let replaced = snapshot(&app.doc, &app.playback, app.time);
        assert_ne!(replaced.history_epoch(), epoch);
        assert!(app.playback.as_ref().unwrap().state().handle(2).is_some());
        let (_, stats) =
            app.composer
                .compose(&replaced, app.cam, app.time, Stage::Light, 40, 23, 16. / 9.);
        assert_eq!(stats.matched, 0);
        std::fs::remove_file(scene_path).unwrap();
        std::fs::remove_file(updates_path).unwrap();
    }
    #[test]
    fn independent_scene_workers_history_and_snapshot_state_agree() {
        let doc = SceneDocument::from_json(DEFAULT_SCENE_JSON).unwrap();
        for mode in [Mode::First, Mode::Third, Mode::Overhead] {
            let mut serial = Composer::with_threads(1).unwrap();
            let mut parallel = Composer::with_threads(8).unwrap();
            let cam = doc.camera(mode);
            let frozen = doc.snapshot(0.);
            let mut first = None;
            let mut changed = false;
            for t in [0., 0.25, 1., 2.] {
                let scene = doc.snapshot(t);
                let (a, sa) = serial.compose(&scene, cam, t, Stage::Light, 80, 45, 16. / 9.);
                let (b, sb) = parallel.compose(&scene, cam, t, Stage::Light, 80, 45, 16. / 9.);
                assert_eq!(a, b);
                assert_eq!(
                    (sa.matched, sa.glyph_changes, sa.mean_color_delta.to_bits()),
                    (sb.matched, sb.glyph_changes, sb.mean_color_delta.to_bits())
                );
                assert!(sa.visible > 200);
                if let Some(initial) = &first {
                    changed |= initial != &a;
                } else {
                    first = Some(a);
                }
            }
            assert!(changed, "moving state must affect {}", mode.name());
            serial.reset();
            let (reset, _) = serial.compose(&frozen, cam, 0., Stage::Light, 80, 45, 16. / 9.);
            assert_eq!(
                first.unwrap(),
                reset,
                "old snapshot remains frozen after newer frames"
            );
        }
    }
    #[test]
    fn reload_rejects_invalid_file_and_preserves_loaded_source() {
        let path =
            std::env::temp_dir().join(format!("ascii-room-r6-reload-{}.json", std::process::id()));
        std::fs::write(&path, DEFAULT_SCENE_JSON).unwrap();
        let mut options = Options::parse(std::iter::empty(), 2).unwrap();
        options.scene = path.clone();
        let doc = SceneDocument::load(&path).unwrap();
        let original = doc.source().clone();
        let mut app = App::new(options, doc).unwrap();
        std::fs::write(&path, "{invalid").unwrap();
        app.reload();
        assert_eq!(app.doc.source(), &original);
        let mut changed = original.clone();
        changed["name"] = json!("Reloaded independent room");
        std::fs::write(&path, changed.to_string()).unwrap();
        app.reload();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(
            app.doc.source(),
            &changed,
            "provenance survives removal of source file"
        );
        assert_eq!(app.doc.name(), "Reloaded independent room");
    }
    #[test]
    fn v2_reload_commits_camera_and_scene_together_and_resets_history_only_on_success() {
        let path =
            std::env::temp_dir().join(format!("ascii-room-r11-reload-{}.json", std::process::id()));
        let source: Value =
            serde_json::from_str(ascii_material_room::DEFAULT_SCENE_V2_JSON).unwrap();
        std::fs::write(&path, source.to_string()).unwrap();
        let mut options = Options::parse(std::iter::empty(), 2).unwrap();
        options.scene = path.clone();
        let doc = SceneDocument::load(&path).unwrap();
        let mut app = App::new(options, doc).unwrap();
        app.time = 0.75;
        app.cam.offset = Vec3::new(0.2, 0.0, 0.1);
        let original_pose = app.cam.pose();
        let old_snapshot = app.doc.snapshot(app.time);
        let (before, _) = app.composer.compose(
            &old_snapshot,
            app.cam,
            app.time,
            Stage::Light,
            80,
            45,
            16. / 9.,
        );
        let (held, _) = app.composer.compose(
            &old_snapshot,
            app.cam,
            app.time,
            Stage::Light,
            80,
            45,
            16. / 9.,
        );
        assert_eq!(before, held);
        let mut invalid = source.clone();
        invalid["cameras"]["third"]["eye"] = invalid["cameras"]["third"]["target"].clone();
        std::fs::write(&path, invalid.to_string()).unwrap();
        app.reload();
        assert_eq!(app.doc.source(), &source);
        assert_eq!(app.cam.pose(), original_pose);
        assert_eq!(app.time, 0.75);
        let (after_rejection, stats) = app.composer.compose(
            &app.doc.snapshot(app.time),
            app.cam,
            app.time,
            Stage::Light,
            80,
            45,
            16. / 9.,
        );
        assert_eq!(before, after_rejection);
        assert!(
            stats.matched > 0,
            "failed reload must keep temporal history"
        );

        let mut changed = source;
        changed["materials"][0]["pattern"]["palette"] = json!("@%");
        changed["materials"][0]["pigment"] = json!([0.7, 0.15, 0.1]);
        changed["cameras"]["third"]["eye"][0] = json!(5.5);
        std::fs::write(&path, changed.to_string()).unwrap();
        app.reload();
        assert_eq!(app.doc.source(), &changed);
        assert_ne!(app.cam.pose(), original_pose);
        assert_eq!(app.cam.offset, Vec3::ZERO);
        assert_eq!(
            app.time, 0.75,
            "reload does not advance or restart simulation"
        );
        let (reloaded, stats) = app.composer.compose(
            &app.doc.snapshot(app.time),
            app.cam,
            app.time,
            Stage::Light,
            80,
            45,
            16. / 9.,
        );
        assert_eq!(
            stats.matched, 0,
            "successful reload must invalidate history"
        );
        let mut fresh = Composer::default();
        let (expected, _) = fresh.compose(
            &app.doc.snapshot(app.time),
            app.cam,
            app.time,
            Stage::Light,
            80,
            45,
            16. / 9.,
        );
        assert_eq!(reloaded, expected);
        assert_ne!(reloaded, before);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn cli_rejects_unbounded_or_invalid_work_and_respects_profiles() {
        for args in [
            vec!["--time", "NaN"],
            vec!["--frames", "0"],
            vec!["--seconds", "-1"],
            vec!["--workers", "65"],
            vec!["--camera", "all"],
            vec!["--headless", "--orbit-speed", "NaN"],
            vec!["--headless", "--orbit-speed", "3"],
            vec!["--orbit-speed", "0.1"],
            vec!["--sampling", "adaptive"],
            vec!["--camera-path", "random"],
            vec!["--cpu-only", "--sequence", "2"],
        ] {
            assert!(Options::parse(args.into_iter().map(String::from), 16).is_err());
        }
        let options = Options::parse(
            [
                "--profile",
                "economy",
                "--workers",
                "auto",
                "--sequence",
                "60",
                "--camera",
                "all",
            ]
            .into_iter()
            .map(String::from),
            4,
        )
        .unwrap();
        assert_eq!(options.workers, 2);
        assert_eq!(options.frames, 60);
        assert!(options.sequence && options.headless && options.all);
    }
    #[test]
    fn cpu_only_and_sampling_are_explicit_and_camera_path_is_repeatable() {
        let options = Options::parse(
            [
                "--cpu-only",
                "--sampling",
                "conservative9",
                "--camera-path",
                "coverage",
                "--camera",
                "all",
            ]
            .into_iter()
            .map(String::from),
            8,
        )
        .unwrap();
        assert!(options.headless && options.cpu_only && options.all);
        assert_eq!(options.sampling, PrimarySampling::Conservative9);
        let doc = SceneDocument::from_json(ascii_material_room::DEFAULT_SCENE_V2_JSON).unwrap();
        for mode in [Mode::First, Mode::Third, Mode::Overhead] {
            let cam = doc.camera(mode);
            assert_eq!(options.camera_at(cam, 0.).pose(), cam.pose());
            assert_ne!(options.camera_at(cam, 3.).pose(), cam.pose());
            assert_eq!(
                options.camera_at(cam, 3.).pose(),
                options.camera_at(cam, 3.).pose()
            );
        }
        assert_eq!(distribution(&[]), Value::Null);
    }
}
