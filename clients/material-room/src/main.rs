//! A separate host: content files and application state belong to this client.
use ascii_material_room::{content::SceneDocument, coverage_study::coverage_camera};
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
            sampling: PrimarySampling::Center,
            coverage_path: false,
            cpu_only: false,
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
fn distribution(values: &[f64]) -> Value {
    if values.is_empty() {
        return Value::Null;
    }
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
        let folder = o.output.join(mode.name());
        std::fs::create_dir_all(&folder)?;
        let cam = doc.camera(mode);
        let aspect = w as f32 / h as f32;
        let mut composer = Composer::with_threads(o.workers)?;
        composer.set_primary_sampling(o.sampling);
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
            if let Some(gpu) = &mut gpu {
                gpu.render(&cells)?;
                gpu.render_wait()?;
            }
        }
        composer.reset();
        let mut cpu = Vec::new();
        let mut snapshot_times = Vec::new();
        let mut composition_times = Vec::new();
        let mut gpu_wait = Vec::new();
        let mut frame_rows = Vec::new();
        let mut coverage_audits = Vec::new();
        let mut csv = String::from("frame,time_s,cpu_snapshot_and_compose_ms,submit_wait_ms,visible,matched,glyph_changes,snapshot_ms,compose_ms,primary_rays,alternate_primary\n");
        for i in 0..o.frames {
            let t = o.time + i as f32 / 30.;
            let start = Instant::now();
            let snapshot = doc.snapshot(t);
            let snapshot_ms = start.elapsed().as_secs_f64() * 1000.;
            let moving_cam = o.camera_at(cam, i as f32 / 30.);
            let compose_start = Instant::now();
            let (cells, stats) =
                composer.compose(&snapshot, moving_cam, t, Stage::Light, cols, rows, aspect);
            let cpu_ms = start.elapsed().as_secs_f64() * 1000.;
            let compose_ms = compose_start.elapsed().as_secs_f64() * 1000.;
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
                "{i},{t:.8},{cpu_ms:.6},{wait_ms:.6},{},{},{},{snapshot_ms:.6},{compose_ms:.6},{},{}\n",
                stats.visible, stats.matched, stats.glyph_changes, stats.primary_rays, stats.alternate_primary
            ));
            frame_rows.push(
                json!({"frame":i,"time":t,"visible":stats.visible,"matched":stats.matched,
                "glyph_changes":stats.glyph_changes,"mean_color_delta":stats.mean_color_delta,
                "primary_rays":stats.primary_rays,"alternate_primary":stats.alternate_primary}),
            );
        }
        std::fs::write(folder.join("frames.csv"), csv)?;
        let (eye, target) = cam.pose();
        runs.push(json!({"camera":mode.name(),"eye":eye.to_array(),"target":target.to_array(),"orbit":cam.orbit,"zoom":cam.zoom,
            "frame_count":o.frames,"static_changed_cells":0,"cpu_snapshot_and_compose_ms":distribution(&cpu),
            "snapshot_ms":distribution(&snapshot_times),"compose_ms":distribution(&composition_times),
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
        "input_scene":doc.source(),
        "grid":[cols,rows],"output":[w,h],"profile":o.profile.name(),"workers":o.workers,"stage":"light",
        "simulation_ticks_per_second":30,"time_start":o.time,"camera_orbit_speed":o.orbit_speed,
        "primary_sampling":o.sampling_name(),"camera_path":if o.coverage_path {"coverage"} else {"static"},
        "cpu_only":o.cpu_only,"gpu":gpu.as_ref().map(Gpu::capabilities),"runs":runs,
        "scope":"Independent client with JSON-owned immutable state, custom materials and analytic forms; no lab fixture imported.",
        "limits":["CPU includes snapshot and composition; GPU submit/wait is not a GPU timestamp or display FPS",
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
    window_intervals: Vec<f64>,
    window_cpu: Vec<f64>,
    window_submit: Vec<f64>,
    configuration_changes: u32,
}
impl App {
    fn new(o: Options, doc: SceneDocument) -> Result<Self> {
        let (w, h, _, _) = o.profile.dimensions();
        let mut composer = Composer::with_threads(o.workers)?;
        composer.set_primary_sampling(o.sampling);
        Ok(Self {
            cam: doc.camera(o.mode),
            composer,
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
            window_intervals: Vec::new(),
            window_cpu: Vec::new(),
            window_submit: Vec::new(),
            configuration_changes: 0,
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
                self.configuration_changes += 1;
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
        let elapsed = now - self.last;
        let dt = elapsed.as_secs_f32().min(0.1);
        self.last = now;
        if !self.paused {
            self.time += dt;
        }
        let axis = |a, b| self.keys.contains(&a) as u8 as f32 - self.keys.contains(&b) as u8 as f32;
        if axis(KeyCode::KeyE, KeyCode::KeyQ) != 0.
            || axis(KeyCode::KeyW, KeyCode::KeyS) != 0.
            || axis(KeyCode::KeyD, KeyCode::KeyA) != 0.
            || axis(KeyCode::KeyX, KeyCode::KeyZ) != 0.
        {
            self.configuration_changes += 1;
        }
        self.cam.orbit += axis(KeyCode::KeyE, KeyCode::KeyQ) * dt * 0.65;
        let (eye, target) = self.cam.pose();
        let forward = Vec3::new(target.x - eye.x, 0., target.z - eye.z).normalize_or_zero();
        self.cam.offset += (forward * axis(KeyCode::KeyW, KeyCode::KeyS)
            + forward.cross(Vec3::Y) * axis(KeyCode::KeyD, KeyCode::KeyA)
            + Vec3::Y * axis(KeyCode::KeyX, KeyCode::KeyZ))
            * dt
            * 2.;
        let (_, _, cols, rows) = self.o.profile.dimensions();
        let cpu_start = Instant::now();
        let snapshot = self.doc.snapshot(self.time);
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
                    "MATERIAL ROOM | {} | {} | C sampling P path 1/2/3 view | {}",
                    self.cam.mode.name(),
                    self.o.sampling_name(),
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
            "primary_sampling":self.o.sampling_name(),"camera_path":if self.o.coverage_path {"coverage"} else {"static"},
            "workers":self.o.workers,"grid":[cols,rows],"requested_output":[w,h],"actual_output":self.size,
            "gpu":self.gpu.as_ref().map(Gpu::capabilities),"frames":self.frames,"warmup_frames_excluded":12,
            "configuration_changes":self.configuration_changes,"wall_seconds":self.start.elapsed().as_secs_f64(),
            "frame_interval_ms":distribution(&self.window_intervals),"cpu_snapshot_and_compose_ms":distribution(&self.window_cpu),
            "acquire_submit_present_ms":distribution(&self.window_submit),
            "raw_frame_interval_ms":self.window_intervals,"raw_cpu_ms":self.window_cpu,"raw_submit_ms":self.window_submit,
            "limits":["Real window callbacks and FIFO presentation on one host; intervals include scheduling and prior frame work",
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
            WindowEvent::CloseRequested => e.exit(),
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
                        if !event.repeat {
                            match key {
                                KeyCode::Escape => e.exit(),
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
                                KeyCode::KeyP => {
                                    self.o.coverage_path = !self.o.coverage_path;
                                    self.composer.reset();
                                    self.configuration_changes += 1;
                                }
                                KeyCode::Digit1 | KeyCode::Digit2 | KeyCode::Digit3 => {
                                    self.cam = self.doc.camera(match key {
                                        KeyCode::Digit1 => Mode::First,
                                        KeyCode::Digit2 => Mode::Third,
                                        _ => Mode::Overhead,
                                    });
                                    self.composer.reset();
                                    self.configuration_changes += 1;
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
