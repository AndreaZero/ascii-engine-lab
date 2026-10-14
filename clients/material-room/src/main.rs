//! A separate host: content files and application state belong to this client.
use ascii_material_room::content::SceneDocument;
use ascii_render::{
    budget::Profile,
    camera::{Camera, Mode},
    core::{Scene, Stage},
    frame::{overlay, Composer},
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
            scene: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/material-room.json"),
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
        };
        let mut requested = None;
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--headless" => o.headless = true,
                "--validate" => o.validate = true,
                "--help" => {
                    println!("ASCII MATERIAL ROOM\n--scene FILE --validate\n--headless --camera first|third|overhead|all --frames N --time SECONDS --output DIR\n--sequence N: capture every frame at 30 simulation ticks/s\n--orbit-speed RADIANS_PER_SECOND: headless camera motion (default 0)\n--profile economy|balanced|detail --workers auto|N --backend auto|vulkan|dx12|metal|gl\n--seconds N: window smoke duration\nWindow: 1/2/3 view, Q/E rotate, WASD move, Z/X height, Space pause, R reload scene and camera, H labels, Esc exit.\nThe default scene is the compatible v1 sample; pass --scene clients/material-room/assets/material-room-v2.json for local materials/environment and authored cameras.");
                    std::process::exit(0);
                }
                _ => {
                    let value = args
                        .next()
                        .ok_or_else(|| format!("{arg} requires a value"))?;
                    match arg.as_str() {
                        "--scene" => o.scene = value.into(),
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
        Ok(o)
    }
}
fn distribution(values: &[f64]) -> Value {
    let mut values = values.to_vec();
    values.sort_by(f64::total_cmp);
    let q = |p: f64| values[((values.len() - 1) as f64 * p).round() as usize];
    json!({"p50":q(0.5),"p95":q(0.95),"p99":q(0.99)})
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
    let (w, h, cols, rows) = o.profile.dimensions();
    let mut gpu = pollster::block_on(Gpu::new_with_options(None, w, h, cols, rows, o.gpu))?;
    std::fs::create_dir_all(&o.output)?;
    let modes = if o.all {
        vec![Mode::First, Mode::Third, Mode::Overhead]
    } else {
        vec![o.mode]
    };
    let mut runs = Vec::new();
    for mode in modes {
        let folder = o.output.join(mode.name());
        std::fs::create_dir_all(&folder)?;
        let cam = doc.camera(mode);
        let aspect = w as f32 / h as f32;
        let mut composer = Composer::with_threads(o.workers)?;
        let frozen = doc.snapshot(o.time);
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
            gpu.render(&cells)?;
            gpu.render_wait()?;
        }
        composer.reset();
        let mut cpu = Vec::new();
        let mut gpu_wait = Vec::new();
        let mut frame_rows = Vec::new();
        let mut coverage_audits = Vec::new();
        let mut csv = String::from("frame,time_s,cpu_snapshot_and_compose_ms,submit_wait_ms,visible,matched,glyph_changes\n");
        for i in 0..o.frames {
            let t = o.time + i as f32 / 30.;
            let start = Instant::now();
            let snapshot = doc.snapshot(t);
            let mut moving_cam = cam;
            moving_cam.orbit += o.orbit_speed * (i as f32 / 30.);
            let (cells, stats) =
                composer.compose(&snapshot, moving_cam, t, Stage::Light, cols, rows, aspect);
            let cpu_ms = start.elapsed().as_secs_f64() * 1000.;
            cpu.push(cpu_ms);
            let start = Instant::now();
            gpu.render(&cells)?;
            gpu.render_wait()?;
            let wait_ms = start.elapsed().as_secs_f64() * 1000.;
            gpu_wait.push(wait_ms);
            if o.sequence || i == 0 || i + 1 == o.frames {
                gpu.capture(&cells, &folder.join(format!("frame-{i:04}.png")))?;
            }
            if i == 0 || i + 1 == o.frames || (o.sequence && i % 30 == 0) {
                coverage_audits.push(
                    json!({"frame":i,"objects":coverage(&snapshot,moving_cam,t,cols,rows,aspect)}),
                );
            }
            csv.push_str(&format!(
                "{i},{t:.8},{cpu_ms:.6},{wait_ms:.6},{},{},{}\n",
                stats.visible, stats.matched, stats.glyph_changes
            ));
            frame_rows.push(
                json!({"frame":i,"time":t,"visible":stats.visible,"matched":stats.matched,
                "glyph_changes":stats.glyph_changes,"mean_color_delta":stats.mean_color_delta}),
            );
        }
        std::fs::write(folder.join("frames.csv"), csv)?;
        let (eye, target) = cam.pose();
        runs.push(json!({"camera":mode.name(),"eye":eye.to_array(),"target":target.to_array(),"orbit":cam.orbit,"zoom":cam.zoom,
            "frame_count":o.frames,"static_changed_cells":0,"cpu_snapshot_and_compose_ms":distribution(&cpu),
            "submit_wait_ms":distribution(&gpu_wait),"frames":frame_rows,"coverage_audits":coverage_audits}));
        println!(
            "{}: {} GPU frames from {}",
            mode.name(),
            o.frames,
            doc.name()
        );
    }
    let manifest = json!({"client":"ascii-material-room","scene_path":o.scene,"scene_name":doc.name(),"scene_version":doc.version(),"objects":doc.object_count(),
        "input_scene":doc.source(),
        "grid":[cols,rows],"output":[w,h],"profile":o.profile.name(),"workers":o.workers,"stage":"light",
        "simulation_ticks_per_second":30,"time_start":o.time,"camera_orbit_speed":o.orbit_speed,"gpu":gpu.capabilities(),"runs":runs,
        "scope":"Independent client with JSON-owned immutable state, custom materials and analytic forms; no lab fixture imported.",
        "limits":["CPU includes snapshot and composition; GPU submit/wait is not a GPU timestamp or display FPS",
            "Offscreen synchronized run, 12 warmup frames excluded; per-frame capture and disk pauses affect scheduling",
            "Coverage audit is outside timers; counts visibility/borders, not visual quality or perceptual flicker",
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
    window: Option<Arc<Window>>,
    gpu: Option<Gpu>,
    composer: Composer,
    cam: Camera,
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
}
impl App {
    fn new(o: Options, doc: SceneDocument) -> Result<Self> {
        let (w, h, _, _) = o.profile.dimensions();
        Ok(Self {
            cam: doc.camera(o.mode),
            composer: Composer::with_threads(o.workers)?,
            time: o.time,
            o,
            doc,
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
        })
    }
    fn reload(&mut self) {
        match SceneDocument::load(&self.o.scene) {
            Ok(next) => {
                // Construct the complete replacement before publishing it. A
                // failed load leaves document, camera, time and history intact.
                let cam = next.camera(self.cam.mode);
                self.doc = next;
                self.cam = cam;
                self.composer.reset();
                self.reload_message = "Scene reloaded".into();
            }
            Err(error) => {
                eprintln!("Scene reload rejected; previous scene kept: {error}");
                self.reload_message = "Invalid scene - kept previous version; see console".into();
            }
        }
    }
    fn draw(&mut self) -> Result<()> {
        let now = Instant::now();
        let dt = (now - self.last).as_secs_f32().min(0.1);
        self.last = now;
        if !self.paused {
            self.time += dt;
        }
        let axis = |a, b| self.keys.contains(&a) as u8 as f32 - self.keys.contains(&b) as u8 as f32;
        self.cam.orbit += axis(KeyCode::KeyE, KeyCode::KeyQ) * dt * 0.65;
        let (eye, target) = self.cam.pose();
        let forward = Vec3::new(target.x - eye.x, 0., target.z - eye.z).normalize_or_zero();
        self.cam.offset += (forward * axis(KeyCode::KeyW, KeyCode::KeyS)
            + forward.cross(Vec3::Y) * axis(KeyCode::KeyD, KeyCode::KeyA)
            + Vec3::Y * axis(KeyCode::KeyX, KeyCode::KeyZ))
            * dt
            * 2.;
        let (_, _, cols, rows) = self.o.profile.dimensions();
        let snapshot = self.doc.snapshot(self.time);
        let (mut cells, _) = self.composer.compose(
            &snapshot,
            self.cam,
            self.time,
            Stage::Light,
            cols,
            rows,
            self.size.0 as f32 / self.size.1 as f32,
        );
        if self.hud {
            overlay(
                &mut cells,
                cols,
                rows,
                &format!(
                    "MATERIAL ROOM | {} | 1/2/3 view  R reload  Space pause | {}",
                    self.cam.mode.name(),
                    self.reload_message
                ),
            );
        }
        self.gpu
            .as_mut()
            .ok_or("GPU not initialized")?
            .render(&cells)?;
        self.frames += 1;
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
            WindowEvent::CloseRequested => e.exit(),
            WindowEvent::Resized(size) => {
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
                        if !event.repeat {
                            match key {
                                KeyCode::Escape => e.exit(),
                                KeyCode::Space => self.paused = !self.paused,
                                KeyCode::KeyH => self.hud = !self.hud,
                                KeyCode::KeyR => self.reload(),
                                KeyCode::Digit1 | KeyCode::Digit2 | KeyCode::Digit3 => {
                                    self.cam = self.doc.camera(match key {
                                        KeyCode::Digit1 => Mode::First,
                                        KeyCode::Digit2 => Mode::Third,
                                        _ => Mode::Overhead,
                                    });
                                    self.composer.reset();
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
            e.exit();
            return;
        }
        if !self.hidden && !self.zero_size {
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }
    }
}
fn main() -> Result<()> {
    let options = Options::read()?;
    let doc = SceneDocument::load(&options.scene)?;
    if options.validate {
        println!(
            "Valid v{} scene: {} ({} objects)",
            doc.version(),
            doc.name(),
            doc.object_count()
        );
        return Ok(());
    }
    if options.headless {
        return headless(&options, &doc);
    }
    let loop_ = EventLoop::new()?;
    loop_.set_control_flow(ControlFlow::Wait);
    let mut app = App::new(options, doc)?;
    loop_.run_app(&mut app)?;
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
}
