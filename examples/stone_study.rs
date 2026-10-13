//! R7: one carved stone, direct semantic glyphs, matched dry/wet scenes.
#[path = "support/stone_geometry.rs"]
mod geometry;
#[path = "support/stone_surface.rs"]
mod surface;
use ascii_engine_lab::{
    budget::Profile,
    camera::{Camera, Mode},
    core::*,
    frame::{overlay, Composer},
    gpu::{BackendChoice, Gpu, GpuOptions},
};
use glam::Vec3;
use serde_json::{json, Value};
use std::{
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
const WIDTH: u32 = 1920;
const HEIGHT: u32 = 1080;
const FLOOR: u32 = 40;
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Variant {
    Baseline,
    Shape,
    Dry,
    Wet,
}
impl Variant {
    fn name(self) -> &'static str {
        match self {
            Self::Baseline => "baseline",
            Self::Shape => "shape",
            Self::Dry => "dry",
            Self::Wet => "wet",
        }
    }
    fn parse(s: &str) -> Option<Self> {
        [Self::Baseline, Self::Shape, Self::Dry, Self::Wet]
            .into_iter()
            .find(|v| v.name() == s)
    }
    fn rich(self) -> bool {
        matches!(self, Self::Dry | Self::Wet)
    }
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Path {
    Static,
    Orbit,
    Light,
    Approach,
    Showcase,
}
impl Path {
    fn name(self) -> &'static str {
        match self {
            Self::Static => "static",
            Self::Orbit => "orbit",
            Self::Light => "light",
            Self::Approach => "approach",
            Self::Showcase => "showcase",
        }
    }
    fn parse(s: &str) -> Option<Self> {
        [
            Self::Static,
            Self::Orbit,
            Self::Light,
            Self::Approach,
            Self::Showcase,
        ]
        .into_iter()
        .find(|v| v.name() == s)
    }
}
#[derive(Clone)]
struct Options {
    variants: Vec<Variant>,
    modes: Vec<Mode>,
    path: Path,
    frames: usize,
    fps: u32,
    cols: u32,
    rows: u32,
    workers: usize,
    out: PathBuf,
    backend: BackendChoice,
    benchmark: bool,
    window: bool,
    seconds: f32,
    capture_every: usize,
}
impl Options {
    fn parse() -> Result<Self> {
        let mut o = Self {
            variants: vec![Variant::Dry, Variant::Wet],
            modes: vec![Mode::First, Mode::Third, Mode::Overhead],
            path: Path::Showcase,
            frames: 180,
            fps: 30,
            cols: 240,
            rows: 135,
            workers: 8,
            out: "evidence/r7/study".into(),
            backend: BackendChoice::Auto,
            benchmark: false,
            window: false,
            seconds: 0.,
            capture_every: 1,
        };
        let capacity = std::thread::available_parallelism().map_or(1, |n| n.get());
        o.workers = o.workers.min(capacity);
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--benchmark" => {
                    o.benchmark = true;
                    continue;
                }
                "--window" => {
                    o.window = true;
                    continue;
                }
                "--help" => {
                    println!("stone_study --variant baseline|shape|dry|wet|pair|all --camera first|third|overhead|all --path static|orbit|light|approach|showcase --frames N --fps N --grid economy|balanced|detail --workers auto|N --backend auto|vulkan|dx12 --out DIR --capture-every N --benchmark --window --seconds N\nDefault: dry/wet, three views, 180 frames at simulated30Hz, 240x135 cells, 1920x1080 output. --benchmark skips GPU; --capture-every 0 saves first/last only. Window: 1/2/3 view, V dry/wet, B shape baseline, O orbit/light path, Space pause, R restart, H labels, Esc close.");
                    std::process::exit(0);
                }
                _ => {}
            }
            let value = args
                .next()
                .ok_or_else(|| format!("{arg} requires a value"))?;
            match arg.as_str() {
                "--variant" => {
                    o.variants = match value.as_str() {
                        "all" => vec![
                            Variant::Baseline,
                            Variant::Shape,
                            Variant::Dry,
                            Variant::Wet,
                        ],
                        "pair" => vec![Variant::Dry, Variant::Wet],
                        _ => vec![Variant::parse(&value).ok_or("invalid variant")?],
                    }
                }
                "--camera" => {
                    o.modes = if value == "all" {
                        vec![Mode::First, Mode::Third, Mode::Overhead]
                    } else {
                        vec![Mode::parse(&value).ok_or("invalid camera")?]
                    }
                }
                "--path" => o.path = Path::parse(&value).ok_or("invalid path")?,
                "--frames" => o.frames = value.parse()?,
                "--fps" => o.fps = value.parse()?,
                "--grid" => {
                    let (_, _, c, r) = Profile::parse(&value).ok_or("invalid grid")?.dimensions();
                    o.cols = c;
                    o.rows = r;
                }
                "--workers" => {
                    o.workers = Profile::Detail.resolve_workers(
                        capacity,
                        if value == "auto" {
                            None
                        } else {
                            Some(value.parse()?)
                        },
                    )?
                }
                "--backend" => o.backend = BackendChoice::parse(&value).ok_or("invalid backend")?,
                "--out" => o.out = value.into(),
                "--seconds" => o.seconds = value.parse()?,
                "--capture-every" => o.capture_every = value.parse()?,
                _ => return Err(format!("unknown option {arg}").into()),
            }
        }
        if !(1..=3600).contains(&o.frames)
            || !(1..=120).contains(&o.fps)
            || !o.seconds.is_finite()
            || !(0. ..=3600.).contains(&o.seconds)
        {
            return Err("frames1..3600, fps1..120 and finite seconds0..3600 required".into());
        }
        if o.window && o.benchmark {
            return Err("--window and --benchmark are exclusive".into());
        }
        Ok(o)
    }
}

#[derive(Clone, Copy)]
struct StoneScene {
    variant: Variant,
    light: Vec3,
}
impl StoneScene {
    fn new(variant: Variant, path: Path, time: f32) -> Self {
        let light_time = match path {
            Path::Light => time,
            Path::Showcase => (time - 3.).max(0.),
            _ => 0.,
        };
        Self {
            variant,
            light: Vec3::new(-2.4 + 4.8 * (light_time * 0.65).sin(), 3.4, 4.2),
        }
    }
}
impl Scene for StoneScene {
    fn trace(&self, ray: Ray, _time: f32, _stage: Stage, _water: bool) -> Option<Hit> {
        let stone = geometry::trace(ray, self.variant != Variant::Baseline);
        let floor = if ray.dir.y.abs() > 1e-7 {
            let floor_height = geometry::CENTER.y - geometry::HALF_EXTENTS.y;
            let d = (floor_height - ray.origin.y) / ray.dir.y;
            let p = ray.origin + ray.dir * d;
            if d > 0.0001 && p.x.abs() < 4.8 && p.z.abs() < 4.8 {
                Some(Hit {
                    distance: d,
                    position: p,
                    normal: Vec3::Y,
                    local: p,
                    material: FLOOR,
                    object_id: 701,
                })
            } else {
                None
            }
        } else {
            None
        };
        match (stone, floor) {
            (Some(a), Some(b)) => Some(if a.distance < b.distance { a } else { b }),
            (a, b) => a.or(b),
        }
    }
    fn light_position(&self, _time: f32) -> Vec3 {
        self.light
    }
    fn pigment(&self, material: u32) -> Vec3 {
        if material == FLOOR {
            Vec3::new(0.065, 0.073, 0.084)
        } else {
            Vec3::new(0.58, 0.55, 0.48)
        }
    }
    fn glyph(&self, material: u32, _p: Vec3, _n: Vec3, i: f32) -> u8 {
        if material == FLOOR {
            if i > 0.065 {
                b':'
            } else {
                b'.'
            }
        } else {
            let ramp = b" .,:;ox%#@";
            ramp[((i * 1.65).clamp(0., 1.) * (ramp.len() - 1) as f32) as usize]
        }
    }
    fn local_materials(&self) -> bool {
        self.variant.rich()
    }
    fn light_color(&self) -> Vec3 {
        Vec3::new(1., 0.94, 0.84) * 5.
    }
    fn material_sample(&self, hit: Hit, footprint: f32) -> Option<MaterialSample> {
        if !self.variant.rich() {
            return None;
        }
        Some(if hit.material == FLOOR {
            MaterialSample {
                albedo: self.pigment(FLOOR),
                normal: hit.normal,
                roughness: 0.92,
                f0: 0.025,
                occlusion: 1.,
            }
        } else {
            surface::sample(
                hit,
                if self.variant == Variant::Wet { 1. } else { 0. },
                footprint,
            )
        })
    }
    fn glyph_sample(&self, hit: Hit, i: f32, footprint: f32) -> u8 {
        if hit.material == FLOOR {
            self.glyph(hit.material, hit.local, hit.normal, i)
        } else {
            surface::glyph(hit, i, footprint)
        }
    }
    fn glyph_sample_key(&self, hit: Hit, footprint: f32) -> u32 {
        if hit.material == FLOOR {
            0
        } else {
            surface::history_key(hit, footprint)
        }
    }
}

fn camera(mode: Mode, path: Path, time: f32) -> Camera {
    let mut c = Camera::new(mode);
    let (orbit, factor) = match path {
        Path::Orbit => (-0.30 + 0.60 * (time / 6.).min(1.), 1.),
        Path::Light | Path::Static => (0., 1.),
        Path::Approach => (
            0.,
            1.32 - 0.32 * (0.5 - 0.5 * (std::f32::consts::PI * (time / 6.).min(1.)).cos()),
        ),
        Path::Showcase => (-0.24 + 0.48 * (time / 3.).min(1.), 1.),
    };
    c.zoom = match mode {
        Mode::First => 0.66,
        Mode::Third => 0.44,
        Mode::Overhead => 0.38,
    } * factor;
    c.orbit = match mode {
        Mode::First => -0.194,
        Mode::Third => 0.05,
        Mode::Overhead => 0.05,
    } + orbit;
    let (_, target) = c.pose();
    c.offset = geometry::CENTER - target;
    c
}
fn quantile(v: &[f64], q: f64) -> f64 {
    let mut s = v.to_vec();
    s.sort_by(f64::total_cmp);
    s[((s.len() - 1) as f64 * q).round() as usize]
}
fn differences(a: &[GlyphCell], b: &[GlyphCell]) -> usize {
    a.iter().zip(b).filter(|(a, b)| a != b).count()
}
fn coverage(scene: &StoneScene, c: Camera, o: &Options) -> Value {
    let mut count = 0;
    let mut border = 0;
    let mut bounds: Option<[u32; 4]> = None;
    for y in 0..o.rows {
        for x in 0..o.cols {
            if scene
                .trace(
                    c.ray(x, y, o.cols, o.rows, 16. / 9.),
                    0.,
                    Stage::Light,
                    true,
                )
                .is_some_and(|h| h.object_id == 700)
            {
                count += 1;
                border += usize::from(x == 0 || y == 0 || x + 1 == o.cols || y + 1 == o.rows);
                bounds = Some(match bounds {
                    None => [x, y, x, y],
                    Some(b) => [x.min(b[0]), y.min(b[1]), x.max(b[2]), y.max(b[3])],
                });
            }
        }
    }
    json!({"stone_cells":count,"stone_border_hits":border,"stone_bounds":bounds})
}
fn headless(o: &Options) -> Result<()> {
    let mut gpu = if o.benchmark {
        None
    } else {
        Some(pollster::block_on(Gpu::new_with_options(
            None,
            WIDTH,
            HEIGHT,
            o.cols,
            o.rows,
            GpuOptions {
                backend: o.backend,
                ..Default::default()
            },
        ))?)
    };
    for variant in &o.variants {
        for mode in &o.modes {
            let folder = o.out.join(variant.name()).join(mode.name());
            std::fs::create_dir_all(&folder)?;
            let mut composer = Composer::with_threads(o.workers)?;
            let frozen = StoneScene::new(*variant, o.path, 0.);
            let cam = camera(*mode, o.path, 0.);
            let (a, _) = composer.compose(&frozen, cam, 0., Stage::Light, o.cols, o.rows, 16. / 9.);
            let (b, _) = composer.compose(&frozen, cam, 0., Stage::Light, o.cols, o.rows, 16. / 9.);
            composer.reset();
            let (c, _) = composer.compose(&frozen, cam, 0., Stage::Light, o.cols, o.rows, 16. / 9.);
            if a != b || a != c {
                return Err("frozen state/history/reset not deterministic".into());
            }
            for _ in 0..12 {
                let (cells, _) =
                    composer.compose(&frozen, cam, 0., Stage::Light, o.cols, o.rows, 16. / 9.);
                if let Some(g) = &mut gpu {
                    g.render(&cells)?;
                    g.render_wait()?;
                }
            }
            composer.reset();
            let mut previous: Option<Vec<GlyphCell>> = None;
            let mut cpu = Vec::new();
            let mut waits = Vec::new();
            let mut frames = Vec::new();
            let mut csv=String::from("frame,time_s,compose_cpu_ms,submit_wait_ms,matched,glyph_changes,mean_color_delta,changed_cells\n");
            for frame in 0..o.frames {
                let time = frame as f32 / o.fps as f32;
                let scene = StoneScene::new(*variant, o.path, time);
                let cam = camera(*mode, o.path, time);
                let start = Instant::now();
                let (cells, stats) =
                    composer.compose(&scene, cam, time, Stage::Light, o.cols, o.rows, 16. / 9.);
                let ms = start.elapsed().as_secs_f64() * 1000.;
                cpu.push(ms);
                let mut wait = None;
                if let Some(g) = &mut gpu {
                    let start = Instant::now();
                    g.render(&cells)?;
                    g.render_wait()?;
                    let ms = start.elapsed().as_secs_f64() * 1000.;
                    waits.push(ms);
                    wait = Some(ms);
                    if frame == 0
                        || frame + 1 == o.frames
                        || (o.capture_every > 0 && frame % o.capture_every == 0)
                    {
                        g.capture(&cells, &folder.join(format!("frame-{frame:04}.png")))?;
                    }
                }
                let changed = previous.as_ref().map_or(0, |old| differences(old, &cells));
                let counts = if !o.benchmark || frame == 0 || frame + 1 == o.frames {
                    coverage(&scene, cam, o)
                } else {
                    Value::Null
                };
                if counts["stone_border_hits"].as_u64().is_some_and(|n| n > 0) {
                    return Err(format!(
                        "stone clipped in {} {} frame{frame}",
                        variant.name(),
                        mode.name()
                    )
                    .into());
                }
                let (eye, target) = cam.pose();
                frames.push(json!({"frame":frame,"time":time,"eye":eye.to_array(),"target":target.to_array(),"light":scene.light.to_array(),"compose_cpu_ms":ms,"submit_wait_ms":wait,"matched":stats.matched,"glyph_changes":stats.glyph_changes,"mean_color_delta":stats.mean_color_delta,"changed_cells":changed,"coverage":counts}));
                csv.push_str(&format!(
                    "{frame},{time:.8},{ms:.6},{},{},{},{:.8},{changed}\n",
                    wait.map_or(String::new(), |v| format!("{v:.6}")),
                    stats.matched,
                    stats.glyph_changes,
                    stats.mean_color_delta
                ));
                previous = Some(cells);
            }
            let dist = |v: &[f64]| {
                if v.is_empty() {
                    Value::Null
                } else {
                    json!({"p50":quantile(v,0.5),"p95":quantile(v,0.95),"p99":quantile(v,0.99)})
                }
            };
            let report = json!({"variant":variant.name(),"camera":mode.name(),"path":o.path.name(),"frame_count":o.frames,"fps_simulated":o.fps,"grid":[o.cols,o.rows],"output_pixels":[WIDTH,HEIGHT],"workers":o.workers,"stage":"light","light_rgb_linear":if variant.rich(){frozen.light_color().to_array()}else{[1.,0.58,0.23]},"stone_center":geometry::CENTER.to_array(),"stone_half_extents":geometry::HALF_EXTENTS.to_array(),"floor_height":geometry::CENTER.y-geometry::HALF_EXTENTS.y,"frozen_changed_cells":0,"warmup":12,"compose_cpu_ms":dist(&cpu),"submit_wait_ms":dist(&waits),"gpu":gpu.as_ref().map(Gpu::capabilities),"frames":frames,"scope":"Matched geometry and paths for dry/wet, no mesh framebuffer. CPU excludes coverage/capture/IO. Synchronized submit/wait is not GPU time or FPS. Capture cadence is not measured performance. Footprint material filtering is not geometric antialiasing. Frozen equality and screen-local ID counters do not certify perceptual stability."});
            std::fs::write(
                folder.join("study.json"),
                serde_json::to_string_pretty(&report)?,
            )?;
            std::fs::write(folder.join("frames.csv"), csv)?;
            println!(
                "{} {} {}: CPU p50 {:.3}, p95 {:.3} ms",
                variant.name(),
                mode.name(),
                o.path.name(),
                quantile(&cpu, 0.5),
                quantile(&cpu, 0.95)
            );
        }
    }
    Ok(())
}

struct App {
    o: Options,
    window: Option<Arc<Window>>,
    gpu: Option<Gpu>,
    composer: Composer,
    mode: Mode,
    variant: Variant,
    path: Path,
    time: f32,
    paused: bool,
    hud: bool,
    last: Instant,
    start: Instant,
    hidden: bool,
    size: (u32, u32),
    frames: usize,
    error: Option<String>,
}
impl App {
    fn new(o: Options) -> Result<Self> {
        Ok(Self {
            composer: Composer::with_threads(o.workers)?,
            mode: o.modes[0],
            variant: o.variants[0],
            path: o.path,
            o,
            window: None,
            gpu: None,
            time: 0.,
            paused: false,
            hud: true,
            last: Instant::now(),
            start: Instant::now(),
            hidden: false,
            size: (WIDTH, HEIGHT),
            frames: 0,
            error: None,
        })
    }
    fn draw(&mut self) -> Result<()> {
        let now = Instant::now();
        let dt = (now - self.last).as_secs_f32().min(0.1);
        self.last = now;
        if !self.paused {
            self.time += dt;
        }
        // Ease forward/back across the study path; no hard cut at loop boundary.
        let t = 3. * (1. - (self.time * std::f32::consts::PI / 6.).cos());
        let scene = StoneScene::new(self.variant, self.path, t);
        let cam = camera(self.mode, self.path, t);
        let (mut cells, _) = self.composer.compose(
            &scene,
            cam,
            t,
            Stage::Light,
            self.o.cols,
            self.o.rows,
            self.size.0 as f32 / self.size.1 as f32,
        );
        if self.hud {
            overlay(&mut cells,self.o.cols,self.o.rows,&format!("STONE R7 | {} {} | 1/2/3 view V dry/wet B shape O motion Space pause R restart",self.variant.name(),self.mode.name()));
        }
        self.gpu.as_mut().ok_or("GPU missing")?.render(&cells)?;
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
                        .with_title("ASCII stone study | dry / wet")
                        .with_inner_size(winit::dpi::PhysicalSize::new(1280, 720)),
                )?,
            );
            self.size = (1280, 720);
            self.gpu = Some(pollster::block_on(Gpu::new_with_options(
                Some(window.clone()),
                1280,
                720,
                self.o.cols,
                self.o.rows,
                GpuOptions {
                    backend: self.o.backend,
                    ..Default::default()
                },
            ))?);
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
            WindowEvent::Occluded(v) => {
                self.hidden = v;
                self.last = Instant::now();
            }
            WindowEvent::Resized(s) => {
                self.size = (s.width, s.height);
                if let Some(g) = &mut self.gpu {
                    g.resize(s.width, s.height);
                }
                self.last = Instant::now();
            }
            WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed && !event.repeat =>
            {
                if let PhysicalKey::Code(key) = event.physical_key {
                    match key {
                        KeyCode::Escape => e.exit(),
                        KeyCode::Space => self.paused = !self.paused,
                        KeyCode::KeyH => self.hud = !self.hud,
                        KeyCode::KeyR => {
                            self.time = 0.;
                            self.composer.reset();
                        }
                        KeyCode::Digit1 | KeyCode::Digit2 | KeyCode::Digit3 => {
                            self.mode = match key {
                                KeyCode::Digit1 => Mode::First,
                                KeyCode::Digit2 => Mode::Third,
                                _ => Mode::Overhead,
                            };
                            self.composer.reset();
                        }
                        KeyCode::KeyV => {
                            self.variant = if self.variant == Variant::Wet {
                                Variant::Dry
                            } else {
                                Variant::Wet
                            };
                            self.composer.reset();
                        }
                        KeyCode::KeyB => {
                            self.variant = if self.variant == Variant::Baseline {
                                Variant::Shape
                            } else {
                                Variant::Baseline
                            };
                            self.composer.reset();
                        }
                        KeyCode::KeyO => {
                            self.path = if self.path == Path::Light {
                                Path::Orbit
                            } else {
                                Path::Light
                            };
                            self.time = 0.;
                            self.composer.reset();
                        }
                        _ => {}
                    }
                }
            }
            WindowEvent::RedrawRequested if !self.hidden && self.size.0 > 0 && self.size.1 > 0 => {
                if let Err(err) = self.draw() {
                    self.error = Some(err.to_string());
                    e.exit();
                }
            }
            _ => {}
        }
    }
    fn about_to_wait(&mut self, e: &ActiveEventLoop) {
        if self.o.seconds > 0. {
            let end = self.start + Duration::from_secs_f32(self.o.seconds);
            if Instant::now() >= end {
                println!("Stone window smoke: {} frames", self.frames);
                e.exit();
                return;
            }
            e.set_control_flow(ControlFlow::WaitUntil(end));
        }
        if !self.hidden && self.size.0 > 0 && self.size.1 > 0 {
            if let Some(w) = &self.window {
                w.request_redraw();
            }
        }
    }
}
fn main() -> Result<()> {
    let o = Options::parse()?;
    if o.window {
        let events = EventLoop::new()?;
        events.set_control_flow(ControlFlow::Wait);
        let mut app = App::new(o)?;
        events.run_app(&mut app)?;
        if let Some(e) = app.error {
            return Err(e.into());
        }
        Ok(())
    } else {
        headless(&o)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dry_wet_share_visibility_and_camera_paths() {
        for mode in [Mode::First, Mode::Third, Mode::Overhead] {
            for time in [0., 1.5, 3., 5.9] {
                let c = camera(mode, Path::Showcase, time);
                let dry = StoneScene::new(Variant::Dry, Path::Showcase, time);
                let wet = StoneScene::new(Variant::Wet, Path::Showcase, time);
                assert_eq!(dry.light, wet.light);
                for y in 0..17 {
                    for x in 0..31 {
                        let r = c.ray(x, y, 31, 17, 16. / 9.);
                        let a = dry.trace(r, time, Stage::Light, true);
                        let b = wet.trace(r, time, Stage::Light, true);
                        assert_eq!(
                            a.map(|h| (h.distance, h.object_id)),
                            b.map(|h| (h.distance, h.object_id))
                        );
                    }
                }
            }
        }
    }
    #[test]
    fn dry_wet_composition_is_distinct_but_static_and_worker_deterministic() {
        for variant in [Variant::Dry, Variant::Wet] {
            let scene = StoneScene::new(variant, Path::Static, 0.);
            for mode in [Mode::First, Mode::Third, Mode::Overhead] {
                let c = camera(mode, Path::Static, 0.);
                let mut a = Composer::with_threads(1).unwrap();
                let mut b = Composer::with_threads(8).unwrap();
                let (x, _) = a.compose(&scene, c, 0., Stage::Light, 80, 45, 16. / 9.);
                let (y, _) = b.compose(&scene, c, 0., Stage::Light, 80, 45, 16. / 9.);
                assert_eq!(x, y);
                let (z, _) = a.compose(&scene, c, 0., Stage::Light, 80, 45, 16. / 9.);
                assert_eq!(x, z);
                let opposite = StoneScene::new(
                    if variant == Variant::Wet {
                        Variant::Dry
                    } else {
                        Variant::Wet
                    },
                    Path::Static,
                    0.,
                );
                b.reset();
                let (w, _) = b.compose(&opposite, c, 0., Stage::Light, 80, 45, 16. / 9.);
                assert!(differences(&x, &w) > 100);
            }
        }
    }
}
