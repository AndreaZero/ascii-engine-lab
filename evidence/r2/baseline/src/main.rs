use ascii_engine_lab::{
    camera::{Camera, Mode},
    core::Stage,
    frame::{overlay, Composer},
    gpu::Gpu,
    LabScene,
};
use glam::Vec3;
use serde_json::{json, Value};
use std::{collections::HashSet, error::Error, path::PathBuf, sync::Arc, time::Instant};
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
    width: u32,
    height: u32,
    cols: u32,
    rows: u32,
    frames: usize,
    sequence: usize,
    headless: bool,
    benchmark: bool,
    seconds: f32,
    orbit: f32,
    stage: Stage,
    mode: Mode,
    output: PathBuf,
}
impl Options {
    fn read() -> Result<Self> {
        let mut o = Self {
            width: 1280,
            height: 720,
            cols: 160,
            rows: 90,
            frames: 120,
            sequence: 0,
            headless: false,
            benchmark: false,
            seconds: 0.,
            orbit: 0.,
            stage: Stage::Cloth,
            mode: Mode::Third,
            output: "evidence".into(),
        };
        let mut args = std::env::args().skip(1);
        while let Some(a) = args.next() {
            match a.as_str() {
                "--headless" => o.headless = true,
                "--benchmark" => {
                    o.headless = true;
                    o.benchmark = true;
                }
                "--width" => o.width = args.next().ok_or("width missing")?.parse()?,
                "--height" => o.height = args.next().ok_or("height missing")?.parse()?,
                "--cols" => o.cols = args.next().ok_or("cols missing")?.parse()?,
                "--rows" => o.rows = args.next().ok_or("rows missing")?.parse()?,
                "--frames" => o.frames = args.next().ok_or("frames missing")?.parse()?,
                "--sequence" => {
                    o.headless = true;
                    o.sequence = args.next().ok_or("sequence missing")?.parse()?;
                }
                "--orbit" => o.orbit = args.next().ok_or("orbit missing")?.parse()?,
                "--seconds" => o.seconds = args.next().ok_or("seconds missing")?.parse()?,
                "--output" => o.output = args.next().ok_or("output missing")?.into(),
                "--stage" => {
                    o.stage = Stage::parse(&args.next().ok_or("stage missing")?)
                        .ok_or("stage: core|light|water|cloth")?
                }
                "--camera" => {
                    o.mode = Mode::parse(&args.next().ok_or("camera missing")?)
                        .ok_or("camera: first|third|overhead")?
                }
                "--help" => {
                    println!("ASCII ENGINE LAB\n--benchmark [--frames 120] [--width 1280 --height 720 --cols 160 --rows 90]\n--headless --camera first|third|overhead --stage core|light|water|cloth\n--orbit RADIANS: initial yaw/orbit\n--sequence N: GPU frames at 30Hz; --output PATH; --seconds N: window smoke test\nWindow: 1/2/3 cameras, F1..F4 stages, Q/E rotate, WASD move observer, Z/X height, Space pause, H HUD, P capture, R reset, Esc exit");
                    std::process::exit(0);
                }
                _ => return Err(format!("unknown option {a}").into()),
            }
        }
        if !(16..=4096).contains(&o.width)
            || !(16..=4096).contains(&o.height)
            || o.cols < 8
            || o.rows < 8
            || o.cols > 512
            || o.rows > 512
            || o.frames == 0
            || o.frames > 10000
            || o.sequence > 900
            || !o.orbit.is_finite()
            || !o.seconds.is_finite()
            || o.seconds < 0.
        {
            return Err("invalid dimensions/count; output 16..4096, grid 8..512, frames 1..10000, sequence <=900".into());
        }
        Ok(o)
    }
    fn camera(&self, mode: Mode) -> Camera {
        let mut camera = Camera::new(mode);
        camera.orbit = self.orbit;
        camera
    }
    fn aspect(&self) -> f32 {
        self.width as f32 / self.height as f32
    }
}
fn percentile(v: &[f64], q: f64) -> f64 {
    let mut x = v.to_vec();
    x.sort_by(f64::total_cmp);
    x[((x.len() - 1) as f64 * q).round() as usize]
}
fn distribution(v: &[f64]) -> Value {
    json!({"p50":percentile(v,0.5),"p95":percentile(v,0.95),"p99":percentile(v,0.99)})
}
fn headless(o: &Options) -> Result<()> {
    std::fs::create_dir_all(&o.output)?;
    let mut gpu = pollster::block_on(Gpu::new(None, o.width, o.height, o.cols, o.rows))?;
    let adapter = gpu.adapter_description();
    println!("Adapter: {adapter}");
    let mut results = Vec::new();
    let modes = if o.benchmark {
        vec![Mode::First, Mode::Third, Mode::Overhead]
    } else {
        vec![o.mode]
    };
    let stages = if o.benchmark {
        vec![Stage::Core, Stage::Light, Stage::Water, Stage::Cloth]
    } else {
        vec![o.stage]
    };
    for mode in modes {
        for stage in &stages {
            let camera = o.camera(mode);
            let mut composer = Composer::default();
            let (frozen, _) =
                composer.compose(&LabScene, camera, 1., *stage, o.cols, o.rows, o.aspect());
            composer.reset();
            let (repeat, _) =
                composer.compose(&LabScene, camera, 1., *stage, o.cols, o.rows, o.aspect());
            let frozen_changed = frozen.iter().zip(&repeat).filter(|(a, b)| a != b).count();
            let (continued, _) =
                composer.compose(&LabScene, camera, 1., *stage, o.cols, o.rows, o.aspect());
            let history_changed = repeat
                .iter()
                .zip(&continued)
                .filter(|(a, b)| a != b)
                .count();
            if frozen_changed != 0 || history_changed != 0 {
                return Err("frozen frame stability regression".into());
            }
            gpu.capture(
                &frozen,
                &o.output
                    .join(format!("{}-{}.png", mode.name(), stage.name())),
            )?;
            if o.benchmark {
                let mut cpu = Vec::new();
                let mut submit_wait = Vec::new();
                let mut total = Vec::new();
                let mut matched = 0usize;
                let mut changes = 0usize;
                let mut visible = 0usize;
                let mut reflected = 0usize;
                let mut frame_csv=String::from("frame,time_s,cpu_ms,submit_wait_ms,total_ms,visible,reflected,matched,glyph_changes\n");
                for i in 0..o.frames + 12 {
                    let t = i as f32 / 60.;
                    let start = Instant::now();
                    let (cells, stats) =
                        composer.compose(&LabScene, camera, t, *stage, o.cols, o.rows, o.aspect());
                    let a = start.elapsed().as_secs_f64() * 1000.;
                    let g = Instant::now();
                    gpu.render(&cells)?;
                    gpu.render_wait()?;
                    let b = g.elapsed().as_secs_f64() * 1000.;
                    let c = start.elapsed().as_secs_f64() * 1000.;
                    if i >= 12 {
                        cpu.push(a);
                        submit_wait.push(b);
                        total.push(c);
                        matched += stats.matched;
                        changes += stats.glyph_changes;
                        visible += stats.visible;
                        reflected += stats.reflected;
                        frame_csv.push_str(&format!(
                            "{},{:.5},{:.5},{:.5},{:.5},{},{},{},{}\n",
                            i - 12,
                            t,
                            a,
                            b,
                            c,
                            stats.visible,
                            stats.reflected,
                            stats.matched,
                            stats.glyph_changes
                        ));
                    }
                }
                std::fs::write(
                    o.output
                        .join(format!("{}-{}.csv", mode.name(), stage.name())),
                    frame_csv,
                )?;
                // Controlled pan with frozen simulation: changes are a stability diagnostic, not all artifacts.
                composer.reset();
                let mut pan_matches = 0;
                let mut pan_changes = 0;
                for i in 0..30 {
                    let mut c = camera;
                    c.orbit = o.orbit + i as f32 * 0.004;
                    let (_, s) =
                        composer.compose(&LabScene, c, 1., *stage, o.cols, o.rows, o.aspect());
                    pan_matches += s.matched;
                    pan_changes += s.glyph_changes;
                }
                let entry = json!({"camera":mode.name(),"stage":stage.name(),"frames":o.frames,"warmup":12,"cpu_generation_ms":distribution(&cpu),"submit_and_wait_ms":distribution(&submit_wait),"synchronized_offscreen_frame_ms":distribution(&total),"gpu_timestamp_ms":null,"static_repeat_different_cells":frozen_changed,"static_history_different_cells":history_changed,"mean_visible_glyphs":visible as f64/o.frames as f64,"mean_reflected_cells":reflected as f64/o.frames as f64,"animated_matched_cells":matched,"animated_glyph_change_ratio":changes as f64/matched.max(1)as f64,"frozen_world_pan_matched_cells":pan_matches,"frozen_world_pan_change_ratio":pan_changes as f64/pan_matches.max(1)as f64});
                println!("{} / {}: CPU p50 {:.2}ms p95 {:.2}ms; sync frame p95 {:.2}ms; repeat changes {}",mode.name(),stage.name(),percentile(&cpu,0.5),percentile(&cpu,0.95),percentile(&total,0.95),frozen_changed);
                results.push(entry);
            }
        }
    }
    if o.sequence > 0 {
        let mut c = Composer::default();
        for i in 0..o.sequence {
            let mut camera = o.camera(o.mode);
            camera.orbit = o.orbit + i as f32 * 0.004;
            let (cells, _) = c.compose(
                &LabScene,
                camera,
                i as f32 / 30.,
                o.stage,
                o.cols,
                o.rows,
                o.aspect(),
            );
            gpu.capture(&cells, &o.output.join(format!("sequence-{:04}.png", i)))?;
        }
    }
    if o.benchmark {
        let report = json!({"version":env!("CARGO_PKG_VERSION"),"adapter":adapter,"output":[o.width,o.height],"grid":[o.cols,o.rows],"fixed_seed":0,"initial_orbit_radians":o.orbit,"simulation_dt":1./60.,"profile":if cfg!(debug_assertions){"debug"}else{"release"},"pipeline":"CPU semantic ray query -> glyph IDs/colors -> wgpu font atlas; no mesh world framebuffer","limits":["Offscreen synchronized timing, not window present latency","No GPU timestamp queries; submit/wait includes driver and synchronization","Correspondence requires the same screen cell and quantized local surface ID; unmatched cells excluded, no history reprojection","Animated light and cloth changes may be intentional","No equivalent Godot benchmark, no superiority claim"],"runs":results});
        std::fs::write(
            o.output.join("benchmark.json"),
            serde_json::to_string_pretty(&report)?,
        )?;
    }
    Ok(())
}

struct App {
    options: Options,
    window: Option<Arc<Window>>,
    gpu: Option<Gpu>,
    composer: Composer,
    camera: Camera,
    keys: HashSet<KeyCode>,
    last: Instant,
    start: Instant,
    time: f32,
    paused: bool,
    hud: bool,
    frames: u64,
    error: Option<String>,
    capture: bool,
    zero_size: bool,
    occluded: bool,
}
impl App {
    fn new(o: Options) -> Self {
        Self {
            camera: o.camera(o.mode),
            options: o,
            window: None,
            gpu: None,
            composer: Composer::default(),
            keys: HashSet::new(),
            last: Instant::now(),
            start: Instant::now(),
            time: 0.,
            paused: false,
            hud: true,
            frames: 0,
            error: None,
            capture: false,
            zero_size: false,
            occluded: false,
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
        self.camera.orbit += axis(KeyCode::KeyE, KeyCode::KeyQ) * dt * 0.65;
        let (eye, target) = self.camera.pose();
        let mut forward = target - eye;
        forward.y = 0.;
        forward = forward.normalize();
        let right = forward.cross(Vec3::Y);
        self.camera.offset += (forward * axis(KeyCode::KeyW, KeyCode::KeyS)
            + right * axis(KeyCode::KeyD, KeyCode::KeyA)
            + Vec3::Y * axis(KeyCode::KeyX, KeyCode::KeyZ))
            * dt
            * 2.;
        let begin = Instant::now();
        let (mut cells, s) = self.composer.compose(
            &LabScene,
            self.camera,
            self.time,
            self.options.stage,
            self.options.cols,
            self.options.rows,
            self.options.aspect(),
        );
        let cpu = begin.elapsed().as_secs_f64() * 1000.;
        if self.hud {
            overlay(
                &mut cells,
                self.options.cols,
                self.options.rows,
                &format!(
                    "ASCII LAB | {} | {} | CPU {:.1}ms | {} glyphs | 1/2/3 view F1-F4 stage",
                    self.camera.mode.name(),
                    self.options.stage.name(),
                    cpu,
                    s.visible
                ),
            );
        }
        let gpu = self.gpu.as_mut().unwrap();
        gpu.render(&cells)?;
        if self.capture {
            std::fs::create_dir_all(&self.options.output)?;
            gpu.capture(
                &cells,
                &self
                    .options
                    .output
                    .join(format!("window-{:06}.png", self.frames)),
            )?;
            self.capture = false;
        }
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
                        .with_title("ASCII Engine Lab | 1/2/3 cameras - F1..F4 stages")
                        .with_inner_size(winit::dpi::PhysicalSize::new(
                            self.options.width,
                            self.options.height,
                        )),
                )?,
            );
            let gpu = pollster::block_on(Gpu::new(
                Some(window.clone()),
                self.options.width,
                self.options.height,
                self.options.cols,
                self.options.rows,
            ))?;
            println!("{}", gpu.adapter_description());
            self.gpu = Some(gpu);
            self.window = Some(window);
            self.last = Instant::now();
            self.start = self.last;
            Ok(())
        })();
        if let Err(err) = result {
            self.error = Some(err.to_string());
            e.exit();
        }
    }
    fn window_event(&mut self, e: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => e.exit(),
            WindowEvent::Resized(size) => {
                self.zero_size = size.width == 0 || size.height == 0;
                self.last = Instant::now();
                if let Some(g) = &mut self.gpu {
                    g.resize(size.width, size.height);
                }
                if size.width > 0 && size.height > 0 {
                    self.options.width = size.width;
                    self.options.height = size.height;
                }
            }
            WindowEvent::Focused(false) => self.keys.clear(),
            WindowEvent::Occluded(value) => {
                self.occluded = value;
                self.last = Instant::now();
                if value {
                    self.keys.clear();
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(k) = event.physical_key {
                    if event.state == ElementState::Pressed {
                        self.keys.insert(k);
                        if !event.repeat {
                            match k {
                                KeyCode::Escape => e.exit(),
                                KeyCode::Digit1 => {
                                    self.camera = Camera::new(Mode::First);
                                    self.composer.reset();
                                }
                                KeyCode::Digit2 => {
                                    self.camera = Camera::new(Mode::Third);
                                    self.composer.reset();
                                }
                                KeyCode::Digit3 => {
                                    self.camera = Camera::new(Mode::Overhead);
                                    self.composer.reset();
                                }
                                KeyCode::F1 => {
                                    self.options.stage = Stage::Core;
                                    self.composer.reset();
                                }
                                KeyCode::F2 => {
                                    self.options.stage = Stage::Light;
                                    self.composer.reset();
                                }
                                KeyCode::F3 => {
                                    self.options.stage = Stage::Water;
                                    self.composer.reset();
                                }
                                KeyCode::F4 => {
                                    self.options.stage = Stage::Cloth;
                                    self.composer.reset();
                                }
                                KeyCode::Space => self.paused = !self.paused,
                                KeyCode::KeyH => self.hud = !self.hud,
                                KeyCode::KeyP => self.capture = true,
                                KeyCode::KeyR => {
                                    self.camera = Camera::new(self.camera.mode);
                                    self.time = 0.;
                                    self.composer.reset();
                                }
                                _ => {}
                            }
                        }
                    } else {
                        self.keys.remove(&k);
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                if self.zero_size || self.occluded {
                    return;
                }
                if let Err(err) = self.draw() {
                    self.error = Some(err.to_string());
                    e.exit();
                }
                if self.options.seconds > 0.
                    && self.start.elapsed().as_secs_f32() >= self.options.seconds
                {
                    println!("Window smoke completed: {} frames", self.frames);
                    e.exit();
                }
            }
            _ => {}
        }
    }
    fn about_to_wait(&mut self, _: &ActiveEventLoop) {
        if self.zero_size || self.occluded {
            return;
        }
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }
}
fn main() -> Result<()> {
    let options = Options::read()?;
    if options.headless {
        return headless(&options);
    }
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = App::new(options);
    event_loop.run_app(&mut app)?;
    if let Some(error) = app.error {
        return Err(error.into());
    }
    Ok(())
}
