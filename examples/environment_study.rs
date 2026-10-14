//! R10: common semantic scene and opt-in environment transport, direct glyphs.
#[path = "support/common_scene.rs"]
mod common_scene;
use common_scene::{CommonScene, TARGET};
// Historic stone surface module resolves its geometry through the host.
use ascii_engine_lab::{
    budget::Profile,
    camera::{Camera, Mode},
    core::*,
    frame::{overlay, Composer},
    gpu::{BackendChoice, Gpu, GpuOptions},
};
use common_scene::stone_geometry as geometry;
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
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Variant {
    Off,
    On,
}
impl Variant {
    fn name(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::On => "on",
        }
    }
    fn parse(s: &str) -> Option<Self> {
        match s {
            "off" => Some(Self::Off),
            "on" => Some(Self::On),
            _ => None,
        }
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
            variants: vec![Variant::Off, Variant::On],
            modes: vec![Mode::First, Mode::Third, Mode::Overhead],
            path: Path::Showcase,
            frames: 180,
            fps: 30,
            cols: 240,
            rows: 135,
            workers: 8,
            out: "evidence/r10/study".into(),
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
                    println!("environment_study --variant off|on|pair|all --camera first|third|overhead|all --path static|orbit|light|approach|showcase --frames N --fps N --grid economy|balanced|detail --workers auto|N --backend auto|vulkan|dx12 --out DIR --capture-every N --benchmark --window --seconds N\nDefault: environment off/on, 3 views,180 frames at simulated30Hz,240x135 glyphs,1920x1080 output. Window: 1/2/3 view,V environment,O orbit/light,Space pause,R restart,H labels,Esc close.");
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
                        "all" => vec![Variant::Off, Variant::On],
                        "pair" => vec![Variant::Off, Variant::On],
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

fn scene(variant: Variant, path: Path, time: f32) -> CommonScene {
    let t = if path == Path::Static { 0. } else { time };
    CommonScene::new(
        t,
        variant == Variant::On,
        matches!(path, Path::Light | Path::Showcase),
    )
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
        Mode::First => 1.10,
        Mode::Third => 0.84,
        Mode::Overhead => 0.74,
    } * factor;
    c.orbit = match mode {
        Mode::First => -0.194,
        Mode::Third => 0.05,
        Mode::Overhead => 0.05,
    } + orbit;
    let (_, target) = c.pose();
    c.offset = TARGET - target;
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
// Untimed audit fingerprint of exact cell RGB/alpha and surface identity. Glyphs
// are intentionally excluded so the readability A/B can verify unchanged light.
fn color_identity_hash(cells: &[GlyphCell]) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for cell in cells {
        for word in [cell.id, cell.flags]
            .into_iter()
            .chain(cell.color.map(f32::to_bits))
        {
            for byte in word.to_le_bytes() {
                hash ^= byte as u64;
                hash = hash.wrapping_mul(0x100000001b3);
            }
        }
    }
    format!("{hash:016x}")
}
fn coverage(scene: &CommonScene, c: Camera, o: &Options, cells: &[GlyphCell]) -> Value {
    let mut counts = [0_u32; 3];
    let mut borders = [0_u32; 3];
    let mut clipped = [0_u32; 3];
    let mut blanks = [0_u32; 3];
    for y in 0..o.rows {
        for x in 0..o.cols {
            if let Some(hit) = scene.trace(
                c.ray(x, y, o.cols, o.rows, 16. / 9.),
                0.,
                Stage::Cloth,
                true,
            ) {
                let index = match hit.object_id {
                    700 => Some(0),
                    800 => Some(1),
                    100..=142 => Some(2),
                    _ => None,
                };
                if let Some(i) = index {
                    counts[i] += 1;
                    borders[i] += u32::from(x == 0 || y == 0 || x + 1 == o.cols || y + 1 == o.rows);
                    let cell = &cells[(y * o.cols + x) as usize];
                    clipped[i] += u32::from(cell.color[..3].iter().any(|v| *v >= 1.));
                    blanks[i] += u32::from(cell.glyph == 32);
                }
            }
        }
    }
    json!({"subjects":["stone","metal","actor"],"counts":counts,"border_hits":borders,"channel_limit_cells":clipped,"blank_cells":blanks})
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
            let frozen = scene(*variant, o.path, 0.);
            let cam = camera(*mode, o.path, 0.);
            let (a, _) = composer.compose(&frozen, cam, 0., Stage::Cloth, o.cols, o.rows, 16. / 9.);
            let (b, _) = composer.compose(&frozen, cam, 0., Stage::Cloth, o.cols, o.rows, 16. / 9.);
            composer.reset();
            let (c, _) = composer.compose(&frozen, cam, 0., Stage::Cloth, o.cols, o.rows, 16. / 9.);
            if a != b || a != c {
                return Err("frozen state/history/reset not deterministic".into());
            }
            for _ in 0..12 {
                let (cells, _) =
                    composer.compose(&frozen, cam, 0., Stage::Cloth, o.cols, o.rows, 16. / 9.);
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
                let scene = scene(*variant, o.path, time);
                let cam = camera(*mode, o.path, time);
                let start = Instant::now();
                let (cells, stats) =
                    composer.compose(&scene, cam, time, Stage::Cloth, o.cols, o.rows, 16. / 9.);
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
                    coverage(&scene, cam, o, &cells)
                } else {
                    Value::Null
                };
                if let Some(subjects) = counts["counts"].as_array() {
                    if subjects.iter().any(|n| n.as_u64() == Some(0)) {
                        return Err(format!(
                            "subject absent: {} {} frame{frame}",
                            variant.name(),
                            mode.name()
                        )
                        .into());
                    }
                    if counts["border_hits"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|n| n.as_u64() != Some(0))
                    {
                        return Err(format!(
                            "subject clipped: {} {} frame{frame}",
                            variant.name(),
                            mode.name()
                        )
                        .into());
                    }
                }
                let (eye, target) = cam.pose();
                frames.push(json!({"frame":frame,"time":time,"eye":eye.to_array(),"target":target.to_array(),"light":scene.light_position(time).to_array(),"compose_cpu_ms":ms,"submit_wait_ms":wait,"matched":stats.matched,"glyph_changes":stats.glyph_changes,"mean_color_delta":stats.mean_color_delta,"changed_cells":changed,"rgb_id_hash":color_identity_hash(&cells),"coverage":counts}));
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
            let report = json!({"variant":variant.name(),"camera":mode.name(),"path":o.path.name(),"frame_count":o.frames,"fps_simulated":o.fps,"grid":[o.cols,o.rows],"output_pixels":[WIDTH,HEIGHT],"workers":o.workers,"stage":"cloth","light_rgb_linear":frozen.light_color().to_array(),"target":TARGET.to_array(),"subject_bounds":common_scene::subject_bounds().map(|(id,min,max)|json!({"id":id,"min":min.to_array(),"max":max.to_array()})),"frozen_changed_cells":0,"warmup":12,"compose_cpu_ms":dist(&cpu),"submit_wait_ms":dist(&waits),"gpu":gpu.as_ref().map(Gpu::capabilities),"frames":frames,"scope":"Matched common-scene geometry/time for environment off/on. Off is legacy ambient, not unlit. No mesh framebuffer. One-bounce environment approximation, not GI. CPU excludes coverage/capture/IO. Synchronized submit/wait is not GPU time or FPS. Capture cadence is not measured performance. Footprint material filtering is not geometric antialiasing. Frozen equality and screen-local ID counters do not certify perceptual stability."});
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
        let scene = scene(self.variant, self.path, t);
        let cam = camera(self.mode, self.path, t);
        let (mut cells, _) = self.composer.compose(
            &scene,
            cam,
            t,
            Stage::Cloth,
            self.o.cols,
            self.o.rows,
            self.size.0 as f32 / self.size.1 as f32,
        );
        if self.hud {
            overlay(
                &mut cells,
                self.o.cols,
                self.o.rows,
                &format!(
                    "COMMON SCENE R10 | {} {} | 1/2/3 view V environment O motion Space pause",
                    self.variant.name(),
                    self.mode.name()
                ),
            );
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
                        .with_title("ASCII common scene | environment off / on")
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
                            self.variant = if self.variant == Variant::On {
                                Variant::Off
                            } else {
                                Variant::On
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
                println!("Environment window smoke: {} frames", self.frames);
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
    fn common_scene_parallel_and_frozen_frames_are_identical() {
        for mode in [Mode::First, Mode::Third, Mode::Overhead] {
            for variant in [Variant::Off, Variant::On] {
                let mut a = Composer::with_threads(1).unwrap();
                let mut b = Composer::with_threads(4).unwrap();
                for t in [0., 1.7, 3.4] {
                    let world = scene(variant, Path::Showcase, t);
                    let c = camera(mode, Path::Showcase, t);
                    let (x, _) = a.compose(&world, c, t, Stage::Cloth, 60, 34, 16. / 9.);
                    let (y, _) = b.compose(&world, c, t, Stage::Cloth, 60, 34, 16. / 9.);
                    assert_eq!(x, y);
                    assert!(x.iter().all(|cell| (32..=126).contains(&cell.glyph)
                        && cell.color.iter().all(|v| v.is_finite())));
                }
                a.reset();
                let world = scene(variant, Path::Static, 0.);
                let c = camera(mode, Path::Static, 0.);
                let (x, _) = a.compose(&world, c, 0., Stage::Cloth, 60, 34, 16. / 9.);
                let (y, _) = a.compose(&world, c, 0., Stage::Cloth, 60, 34, 16. / 9.);
                assert_eq!(x, y);
            }
        }
    }
}
