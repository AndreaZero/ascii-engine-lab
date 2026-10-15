//! R16 controlled data-driven figure study; direct semantic glyph rendering.
#[path = "support/authored_figure.rs"]
mod authored_figure;
use ascii_engine_lab::{
    actor::ActorSnapshot,
    budget::Profile,
    camera::{Camera, CameraView, Mode},
    core::*,
    frame::{Composer, PrimarySampling},
    gpu::{BackendChoice, Gpu, GpuOptions},
};
use authored_figure::{FigureDefinition, FigureSnapshot};
use glam::{Quat, Vec3};
use serde_json::{json, Value};
use std::{collections::BTreeMap, error::Error, path::PathBuf, sync::Arc, time::Instant};
use winit::{
    application::ApplicationHandler,
    event::{ElementState, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};
type Result<T> = std::result::Result<T, Box<dyn Error>>;
const COLS: u32 = 240;
const ROWS: u32 = 135;
const WIDTH: u32 = 1920;
const HEIGHT: u32 = 1080;
const LIGHT: Vec3 = Vec3::new(-2.3, 3.2, 2.5);
const FLOOR: u32 = 31;
const WARMUP: usize = 12;

struct Options {
    figure: String,
    file: Option<PathBuf>,
    pass: String,
    path: String,
    distance: String,
    modes: Vec<Mode>,
    frames: usize,
    workers: usize,
    backend: BackendChoice,
    sampling: PrimarySampling,
    capture_every: usize,
    benchmark: bool,
    window: bool,
    seconds: f32,
    out: PathBuf,
}
impl Options {
    fn parse() -> Result<Self> {
        let mut o = Self {
            figure: "warden".into(),
            file: None,
            pass: "structured".into(),
            path: "static".into(),
            distance: "middle".into(),
            modes: vec![Mode::First, Mode::Third, Mode::Overhead],
            frames: 60,
            workers: 8,
            backend: BackendChoice::Auto,
            sampling: PrimarySampling::Center,
            capture_every: 1,
            benchmark: false,
            window: false,
            seconds: 4.,
            out: "evidence/r16/study".into(),
        };
        let mut args = std::env::args().skip(1);
        while let Some(a) = args.next() {
            match a.as_str() {
                "--benchmark" => {
                    o.benchmark = true;
                    continue;
                }
                "--window" => {
                    o.window = true;
                    continue;
                }
                "--help" => {
                    println!("visual_capacity --figure baseline|warden|courier [--file JSON] --pass clay|structured --camera first|third|overhead|all --distance near|middle|far --path static|rotate|camera|deform --frames N --capture-every N --workers N --sampling center|conservative9 --backend auto|vulkan|dx12 --out DIR --benchmark --window --seconds N\nFixed 240x135 cells / 1920x1080 output, 30 simulated ticks/s. Baseline always clay. Window requires one camera; keys 1/2/3 change view, ESC exits. --surface aliases --pass; --asset aliases --file. Paths separate pose, figure rotation and camera clocks.");
                    std::process::exit(0);
                }
                _ => {}
            }
            let v = args.next().ok_or_else(|| format!("{a} needs a value"))?;
            match a.as_str() {
                "--figure" => o.figure = v,
                "--file" | "--asset" => o.file = Some(v.into()),
                "--pass" | "--surface" => o.pass = v,
                "--path" => o.path = v,
                "--distance" => o.distance = v,
                "--camera" => {
                    o.modes = if v == "all" {
                        vec![Mode::First, Mode::Third, Mode::Overhead]
                    } else {
                        vec![Mode::parse(&v).ok_or("invalid camera")?]
                    }
                }
                "--capture-every" => o.capture_every = v.parse()?,
                "--frames" => o.frames = v.parse()?,
                "--workers" => o.workers = v.parse()?,
                "--backend" => o.backend = BackendChoice::parse(&v).ok_or("invalid backend")?,
                "--sampling" => {
                    o.sampling = PrimarySampling::parse(&v).ok_or("invalid sampling")?
                }
                "--seconds" => o.seconds = v.parse()?,
                "--out" => o.out = v.into(),
                _ => return Err(format!("unknown option {a}").into()),
            }
        }
        if !["baseline", "warden", "courier"].contains(&o.figure.as_str())
            || !["clay", "structured"].contains(&o.pass.as_str())
            || !["static", "rotate", "camera", "deform"].contains(&o.path.as_str())
            || !["near", "middle", "far"].contains(&o.distance.as_str())
        {
            return Err("invalid figure/pass/path/distance".into());
        }
        if o.figure == "baseline" {
            o.pass = "clay".into();
        }
        if o.capture_every == 0
            || !(1..=3600).contains(&o.frames)
            || !o.seconds.is_finite()
            || o.seconds <= 0.
            || o.seconds > 300.
        {
            return Err("frames 1..3600; seconds >0 and <=300 required".into());
        }
        if o.window && (o.modes.len() != 1 || o.benchmark) {
            return Err("window requires one camera and excludes benchmark".into());
        }
        let capacity = std::thread::available_parallelism().map_or(1, |n| n.get());
        o.workers = Profile::Detail.resolve_workers(capacity, Some(o.workers))?;
        Ok(o)
    }
    fn target_cells(&self) -> f32 {
        match self.distance.as_str() {
            "near" => 80.,
            "far" => 24.,
            _ => 48.,
        }
    }
}
enum Subject {
    Baseline(Box<ActorSnapshot>),
    Authored(FigureSnapshot),
}
struct StudyScene {
    subject: Subject,
    rotation: Quat,
    structured: bool,
}
fn clay(_i: f32) -> u8 {
    // Constant ink separates shape/lighting from a luminance-to-density ramp.
    // Composer still applies its ordinary deep-dark cutoff and RGB shading.
    b'o'
}
impl StudyScene {
    fn new(o: &Options, definition: Option<&FigureDefinition>, time: f32) -> Self {
        let pose = if o.path == "deform" { time } else { 0. };
        let yaw = if o.path == "rotate" {
            time * std::f32::consts::PI
        } else {
            0.
        };
        let subject = if let Some(d) = definition {
            Subject::Authored(d.snapshot(pose))
        } else {
            Subject::Baseline(Box::new(ActorSnapshot::new(pose, Stage::Cloth)))
        };
        Self {
            subject,
            rotation: Quat::from_rotation_y(yaw),
            structured: o.pass == "structured",
        }
    }
    fn local_hit(&self, mut h: Hit) -> Hit {
        h.position = self.rotation.conjugate() * h.position;
        h.normal = self.rotation.conjugate() * h.normal;
        h
    }
    fn subject_trace(&self, ray: Ray) -> Option<Hit> {
        let inv = self.rotation.conjugate();
        let local = Ray {
            origin: inv * ray.origin,
            dir: inv * ray.dir,
        };
        let mut hit = match &self.subject {
            Subject::Authored(s) => s.trace(local),
            Subject::Baseline(s) => s
                .trace(Ray {
                    origin: local.origin + Vec3::X,
                    dir: local.dir,
                })
                .map(|mut h| {
                    h.position -= Vec3::X;
                    h.material = 32;
                    h
                }),
        }?;
        hit.position = self.rotation * hit.position;
        hit.normal = self.rotation * hit.normal;
        Some(hit)
    }
    fn source_bounds(&self) -> ([f32; 3], [f32; 3]) {
        match &self.subject {
            Subject::Authored(s) => {
                let (a, b) = s.bounds();
                (a.to_array(), b.to_array())
            }
            Subject::Baseline(_) => ([-0.75, -0.01, -0.70], [0.75, 2.07, 0.55]),
        }
    }
}
impl Scene for StudyScene {
    fn trace(&self, ray: Ray, _: f32, _: Stage, _: bool) -> Option<Hit> {
        let subject = self.subject_trace(ray);
        if ray.dir.y.abs() < 1e-6 {
            return subject;
        }
        let d = (-0.025 - ray.origin.y) / ray.dir.y;
        if d <= 0.0001 || subject.is_some_and(|h| h.distance <= d) {
            return subject;
        }
        let p = ray.origin + ray.dir * d;
        if p.x.abs() > 8. || p.z.abs() > 8. {
            return subject;
        }
        Some(Hit {
            distance: d,
            position: p,
            normal: Vec3::Y,
            material: FLOOR,
            object_id: 1,
            local: p,
        })
    }
    fn light_position(&self, _: f32) -> Vec3 {
        LIGHT
    }
    fn light_color(&self) -> Vec3 {
        Vec3::new(1., 0.90, 0.78)
    }
    fn pigment(&self, m: u32) -> Vec3 {
        if m == FLOOR {
            Vec3::splat(0.12)
        } else if self.structured {
            match &self.subject {
                Subject::Authored(s) => s.pigment(m),
                _ => Vec3::splat(0.55),
            }
        } else {
            Vec3::splat(0.55)
        }
    }
    fn glyph(&self, _: u32, _: Vec3, _: Vec3, i: f32) -> u8 {
        clay(i)
    }
    fn local_materials(&self) -> bool {
        true
    }
    fn material_sample(&self, h: Hit, f: f32) -> Option<MaterialSample> {
        if h.material != FLOOR && self.structured {
            if let Subject::Authored(s) = &self.subject {
                let mut m = s.material_sample(self.local_hit(h), f)?;
                m.normal = self.rotation * m.normal;
                return Some(m);
            }
        }
        Some(MaterialSample {
            albedo: self.pigment(h.material),
            normal: h.normal,
            roughness: 0.85,
            f0: 0.04,
            occlusion: 1.,
        })
    }
    fn glyph_sample(&self, h: Hit, i: f32, f: f32) -> u8 {
        if h.material == FLOOR {
            return b'.';
        }
        if h.material != FLOOR && self.structured {
            if let Subject::Authored(s) = &self.subject {
                return s.glyph_sample(self.local_hit(h), i, f);
            }
        }
        clay(i)
    }
    fn glyph_sample_key(&self, h: Hit, f: f32) -> u32 {
        if h.material != FLOOR && self.structured {
            if let Subject::Authored(s) = &self.subject {
                return s.glyph_sample_key(self.local_hit(h), f);
            }
        }
        0
    }
    fn environment_sample(&self, _: EnvironmentQuery) -> Option<EnvironmentSample> {
        Some(EnvironmentSample {
            diffuse_irradiance: Vec3::splat(0.16 * std::f32::consts::PI),
            specular_radiance: Vec3::ZERO,
            max_distance: 0.,
        })
    }
}
fn camera(o: &Options, mode: Mode, time: f32) -> Result<Camera> {
    let target = Vec3::new(0., 1.05, 0.);
    let nominal_height = 2.1;
    let half_height = nominal_height * ROWS as f32 / (2. * o.target_cells());
    let radius = half_height / (33_f32.to_radians().tan());
    let direction = match mode {
        Mode::First => Vec3::new(-0.32, 0.12, 1.),
        Mode::Third => Vec3::new(0.55, 0.30, -1.),
        Mode::Overhead => Vec3::new(0.25, 1., 0.6),
    }
    .normalize();
    let offset = if o.path == "camera" {
        Vec3::new((time * std::f32::consts::PI).sin() * 0.25, 0., 0.)
    } else {
        Vec3::ZERO
    };
    Ok(Camera::with_view(
        mode,
        CameraView::new(
            target + direction * radius + offset,
            target + offset,
            66.,
            half_height,
        )?,
    ))
}
fn distribution(v: &[f64]) -> Value {
    if v.is_empty() {
        return Value::Null;
    }
    let mut s = v.to_vec();
    s.sort_by(f64::total_cmp);
    let q = |f: f64| s[((s.len() - 1) as f64 * f).round() as usize];
    json!({"p50":q(0.5),"p95":q(0.95),"p99":q(0.99),"max":s[s.len()-1],"samples":s.len()})
}
fn metrics(scene: &StudyScene, cam: Camera, cells: &[GlyphCell], aspect: f32) -> Value {
    let mut parts = BTreeMap::<u32, usize>::new();
    let mut bounds: Option<[u32; 4]> = None;
    let (mut occupied, mut blanks, mut border) = (0, 0, 0);
    for y in 0..ROWS {
        for x in 0..COLS {
            if let Some(h) = scene.subject_trace(cam.ray(x, y, COLS, ROWS, aspect)) {
                occupied += 1;
                *parts.entry(h.object_id).or_default() += 1;
                blanks += usize::from(cells[(y * COLS + x) as usize].glyph == 32);
                border += usize::from(x == 0 || y == 0 || x + 1 == COLS || y + 1 == ROWS);
                bounds = Some(match bounds {
                    None => [x, y, x, y],
                    Some(b) => [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)],
                });
            }
        }
    }
    json!({"scope":"untimed subject-only center-ray geometry; not Conservative9 selected coverage; blank counts sample final cells at center hits","occupied_cells":occupied,"bounds_inclusive":bounds,"height_cells":bounds.map(|b|b[3]-b[1]+1),"width_cells":bounds.map(|b|b[2]-b[0]+1),"blank_at_center_hits":blanks,"border_hits":border,"part_cells":parts,"actual_output_nonspace_cells":cells.iter().filter(|c|c.glyph!=32).count(),"actual_output_scope":"all scene cells including floor; actual selected glyph buffer"})
}
fn metadata(o: &Options, source: &Option<String>) -> Value {
    json!({"figure":o.figure,"input_json":source,"pass":o.pass,"path":o.path,"distance":o.distance,"nominal_subject_height_cells":o.target_cells(),"framing_reference_height_world":2.1,"grid":[COLS,ROWS],"output":[WIDTH,HEIGHT],"workers":o.workers,"primary_sampling":o.sampling.name(),"simulation_hz":30,"capture_every":o.capture_every,"light":LIGHT.to_array(),"ambient_irradiance":0.16*std::f32::consts::PI,"warmup":WARMUP,"scope":"procedural pose, no cloth physics; R5 workload differs; no GPU timestamps or FPS claim","clock_contract":"static all zero; rotate yaw=PI*t only; camera x=0.25*sin(PI*t) only; deform pose=t only","framing_scope":"fixed common camera, approximate target only; measured per-view center geometry is authoritative; eye-height first is an observer with radius-dependent eye height, not gameplay controls"})
}
fn composer(o: &Options) -> Result<Composer> {
    let mut c = Composer::with_threads(o.workers)?;
    c.set_primary_sampling(o.sampling);
    Ok(c)
}
fn write_json(path: PathBuf, value: &Value) -> Result<()> {
    std::fs::write(path, serde_json::to_string_pretty(value)?)?;
    Ok(())
}
fn headless(
    o: &Options,
    definition: Option<&FigureDefinition>,
    source: &Option<String>,
) -> Result<()> {
    std::fs::create_dir_all(&o.out)?;
    let mut gpu = if o.benchmark {
        None
    } else {
        Some(pollster::block_on(Gpu::new_with_options(
            None,
            WIDTH,
            HEIGHT,
            COLS,
            ROWS,
            GpuOptions {
                backend: o.backend,
                ..Default::default()
            },
        ))?)
    };
    let mut runs = Vec::new();
    for &mode in &o.modes {
        let out = o.out.join(mode.name());
        std::fs::create_dir_all(&out)?;
        let mut comp = composer(o)?;
        let initial = StudyScene::new(o, definition, 0.);
        let cam = camera(o, mode, 0.)?;
        let (a, _) = comp.compose(&initial, cam, 0., Stage::Cloth, COLS, ROWS, 16. / 9.);
        let (b, _) = comp.compose(&initial, cam, 0., Stage::Cloth, COLS, ROWS, 16. / 9.);
        comp.reset();
        let (c, _) = comp.compose(&initial, cam, 0., Stage::Cloth, COLS, ROWS, 16. / 9.);
        if bytemuck::cast_slice::<GlyphCell, u8>(&a) != bytemuck::cast_slice::<GlyphCell, u8>(&b)
            || bytemuck::cast_slice::<GlyphCell, u8>(&a)
                != bytemuck::cast_slice::<GlyphCell, u8>(&c)
        {
            return Err("frozen composition is not bitwise stable".into());
        }
        for _ in 0..WARMUP {
            std::hint::black_box(comp.compose(
                &initial,
                cam,
                0.,
                Stage::Cloth,
                COLS,
                ROWS,
                16. / 9.,
            ));
        }
        comp.reset();
        let (mut snapshots, mut compositions, mut totals, mut waits) =
            (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        let mut frames = Vec::new();
        let mut previous: Option<Vec<GlyphCell>> = None;
        for i in 0..o.frames {
            let t = i as f32 / 30.;
            let cam = camera(o, mode, t)?;
            let start = Instant::now();
            let scene = StudyScene::new(o, definition, t);
            let snapshot_ms = start.elapsed().as_secs_f64() * 1000.;
            let start = Instant::now();
            let (cells, stats) = comp.compose(&scene, cam, t, Stage::Cloth, COLS, ROWS, 16. / 9.);
            let compose_ms = start.elapsed().as_secs_f64() * 1000.;
            snapshots.push(snapshot_ms);
            compositions.push(compose_ms);
            totals.push(snapshot_ms + compose_ms);
            let changed = previous
                .as_ref()
                .map(|p| p.iter().zip(&cells).filter(|(a, b)| a != b).count());
            let coverage = if !o.benchmark || i == 0 || i + 1 == o.frames {
                metrics(&scene, cam, &cells, 16. / 9.)
            } else {
                Value::Null
            };
            let wait = if let Some(g) = &mut gpu {
                if i % o.capture_every == 0 || i + 1 == o.frames {
                    g.capture(&cells, &out.join(format!("frame-{i:04}.png")))?;
                }
                let begin = Instant::now();
                g.render(&cells)?;
                g.render_wait()?;
                let ms = begin.elapsed().as_secs_f64() * 1000.;
                waits.push(ms);
                Some(ms)
            } else {
                None
            };
            let (eye, target) = cam.pose();
            frames.push(json!({"frame":i,"time":t,"eye":eye.to_array(),"target":target.to_array(),"source_bounds":scene.source_bounds(),"snapshot_ms":snapshot_ms,"compose_ms":compose_ms,"submit_wait_ms":wait,"visible":stats.visible,"matched":stats.matched,"glyph_changes":stats.glyph_changes,"mean_color_delta":stats.mean_color_delta,"primary_rays":stats.primary_rays,"alternate_primary":stats.alternate_primary,"changed_cells":changed,"geometry_and_output":coverage}));
            previous = Some(cells);
        }
        runs.push(json!({"camera":mode.name(),"frozen_stable":true,"frames":frames,"snapshot_ms":distribution(&snapshots),"compose_ms":distribution(&compositions),"cpu_total_ms":distribution(&totals),"submit_wait_ms":distribution(&waits)}));
    }
    let mut report = metadata(o, source);
    report["runs"] = json!(runs);
    report["gpu"] = gpu.as_ref().map_or(Value::Null, Gpu::capabilities);
    report["cpu_only"] = json!(o.benchmark);
    report["timing_scope"]=json!("snapshot and compose only; metrics/PNG/I/O excluded; submit_wait is separate redraw plus wait, not GPU timestamp. CPU-only benchmark recommended.");
    write_json(o.out.join("run.json"), &report)
}

struct App {
    o: Options,
    definition: Option<FigureDefinition>,
    source: Option<String>,
    window: Option<Arc<Window>>,
    gpu: Option<Gpu>,
    composer: Composer,
    start: Instant,
    frames: usize,
    size: (u32, u32),
    error: Option<String>,
    snapshots: Vec<f64>,
    compositions: Vec<f64>,
    present: Vec<f64>,
    intervals: Vec<f64>,
    last: Instant,
    resizes: usize,
    first_metrics: Value,
    last_metrics: Value,
}
impl App {
    fn draw(&mut self) -> Result<()> {
        let now = Instant::now();
        let elapsed = now.duration_since(self.start).as_secs_f32();
        let dt = now.duration_since(self.last).as_secs_f64() * 1000.;
        self.last = now;
        if self.frames == WARMUP {
            self.composer.reset();
            self.start = now;
        }
        let t = if self.frames <= WARMUP { 0. } else { elapsed };
        let cam = camera(&self.o, self.o.modes[0], t)?;
        let start = Instant::now();
        let scene = StudyScene::new(&self.o, self.definition.as_ref(), t);
        let snapshot = start.elapsed().as_secs_f64() * 1000.;
        let start = Instant::now();
        let (cells, _) = self.composer.compose(
            &scene,
            cam,
            t,
            Stage::Cloth,
            COLS,
            ROWS,
            self.size.0 as f32 / self.size.1 as f32,
        );
        let composition = start.elapsed().as_secs_f64() * 1000.;
        let start = Instant::now();
        self.gpu.as_mut().ok_or("missing GPU")?.render(&cells)?;
        let present = start.elapsed().as_secs_f64() * 1000.;
        if self.frames > WARMUP {
            self.snapshots.push(snapshot);
            self.compositions.push(composition);
            self.present.push(present);
            self.intervals.push(dt);
        }
        if self.frames == 0 {
            self.first_metrics =
                metrics(&scene, cam, &cells, self.size.0 as f32 / self.size.1 as f32);
        }
        if elapsed >= self.o.seconds {
            self.last_metrics =
                metrics(&scene, cam, &cells, self.size.0 as f32 / self.size.1 as f32);
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
            let w = Arc::new(
                e.create_window(
                    Window::default_attributes()
                        .with_title("ASCII visual capacity R16")
                        .with_resizable(false)
                        .with_inner_size(winit::dpi::PhysicalSize::new(WIDTH, HEIGHT)),
                )?,
            );
            let actual = w.inner_size();
            self.size = (actual.width, actual.height);
            self.gpu = Some(pollster::block_on(Gpu::new_with_options(
                Some(w.clone()),
                actual.width,
                actual.height,
                COLS,
                ROWS,
                GpuOptions {
                    backend: self.o.backend,
                    ..Default::default()
                },
            ))?);
            self.window = Some(w);
            self.start = Instant::now();
            self.last = self.start;
            Ok(())
        })();
        if let Err(err) = result {
            self.error = Some(err.to_string());
            e.exit();
        }
    }
    fn window_event(&mut self, e: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => e.exit(),
            WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed && !event.repeat =>
            {
                if let PhysicalKey::Code(key) = event.physical_key {
                    match key {
                        KeyCode::Escape => e.exit(),
                        KeyCode::Digit1 | KeyCode::Digit2 | KeyCode::Digit3 => {
                            self.o.modes[0] = match key {
                                KeyCode::Digit1 => Mode::First,
                                KeyCode::Digit2 => Mode::Third,
                                _ => Mode::Overhead,
                            };
                            self.composer.reset();
                            self.resizes += 1;
                        }
                        _ => {}
                    }
                }
            }
            WindowEvent::Resized(s) => {
                if self.size != (s.width, s.height) {
                    self.resizes += 1;
                }
                self.size = (s.width, s.height);
                if let Some(g) = &mut self.gpu {
                    g.resize(s.width, s.height);
                }
            }
            WindowEvent::RedrawRequested if self.size.0 > 0 && self.size.1 > 0 => {
                if let Err(err) = self.draw() {
                    self.error = Some(err.to_string());
                    e.exit();
                }
                if self.start.elapsed().as_secs_f32() >= self.o.seconds {
                    e.exit();
                }
            }
            _ => {}
        }
    }
    fn about_to_wait(&mut self, e: &ActiveEventLoop) {
        if self.start.elapsed().as_secs_f32() >= self.o.seconds && self.frames > 0 {
            e.exit();
            return;
        }
        e.set_control_flow(ControlFlow::Poll);
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }
}
fn main() -> Result<()> {
    let o = Options::parse()?;
    let source = if o.figure == "baseline" {
        None
    } else {
        Some(std::fs::read_to_string(o.file.clone().unwrap_or_else(
            || {
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("examples/assets/figures")
                    .join(format!("{}.json", o.figure))
            },
        ))?)
    };
    let definition = source
        .as_ref()
        .map(|s| FigureDefinition::from_json(s))
        .transpose()?;
    if !o.window {
        return headless(&o, definition.as_ref(), &source);
    }
    std::fs::create_dir_all(&o.out)?;
    let comp = composer(&o)?;
    let now = Instant::now();
    let mut app = App {
        o,
        definition,
        source,
        window: None,
        gpu: None,
        composer: comp,
        start: now,
        last: now,
        frames: 0,
        size: (WIDTH, HEIGHT),
        error: None,
        snapshots: Vec::new(),
        compositions: Vec::new(),
        present: Vec::new(),
        intervals: Vec::new(),
        resizes: 0,
        first_metrics: Value::Null,
        last_metrics: Value::Null,
    };
    EventLoop::new()?.run_app(&mut app)?;
    if let Some(error) = app.error {
        return Err(error.into());
    }
    if app.last_metrics.is_null() {
        let t = app.start.elapsed().as_secs_f32();
        let scene = StudyScene::new(&app.o, app.definition.as_ref(), t);
        let cam = camera(&app.o, app.o.modes[0], t)?;
        let (cells, _) = app.composer.compose(
            &scene,
            cam,
            t,
            Stage::Cloth,
            COLS,
            ROWS,
            app.size.0 as f32 / app.size.1 as f32,
        );
        app.last_metrics = metrics(&scene, cam, &cells, app.size.0 as f32 / app.size.1 as f32);
    }
    let mut report = metadata(&app.o, &app.source);
    report["camera"] = json!(app.o.modes[0].name());
    report["actual_output"] = json!(app.size);
    report["frames"] = json!(app.frames);
    report["configuration_changes"] = json!(app.resizes);
    report["snapshot_ms"] = distribution(&app.snapshots);
    report["compose_ms"] = distribution(&app.compositions);
    report["acquire_submit_present_ms"] = distribution(&app.present);
    report["frame_interval_ms"] = distribution(&app.intervals);
    report["first_metrics"] = app.first_metrics;
    report["last_metrics"] = app.last_metrics;
    report["gpu"] = app.gpu.as_ref().map_or(Value::Null, Gpu::capabilities);
    write_json(app.o.out.join("window-run.json"), &report)
}
