//! Actor-only R4/R5 visual study: same compositor, glyph GPU path and fixed light.
//! R4 actor source is preserved verbatim except its test module in support/.
pub use ascii_engine_lab::core;
#[allow(dead_code)]
#[path = "support/r4_actor.rs"]
mod actor;
#[allow(dead_code)]
#[path = "../evidence/r5/baseline/src/world.rs"]
mod old_world;

use ascii_engine_lab::{
    actor as current_actor,
    budget::Profile,
    camera::{Camera, Mode},
    core::{GlyphCell, Hit, Ray, Scene, Stage},
    frame::Composer,
    gpu::{BackendChoice, Gpu, GpuOptions},
    world,
};
use glam::Vec3;
use serde_json::{json, Value};
use std::{collections::BTreeMap, error::Error, path::PathBuf, time::Instant};

const COLS: u32 = 240;
const ROWS: u32 = 135;
const WIDTH: u32 = 1920;
const HEIGHT: u32 = 1080;
const LIGHT: Vec3 = Vec3::new(-2.3, 3.2, -1.3);
const WARMUP: usize = 12;

#[derive(Clone, Copy)]
enum Variant {
    Baseline,
    Current,
}
impl Variant {
    fn name(self) -> &'static str {
        match self {
            Self::Baseline => "baseline",
            Self::Current => "current",
        }
    }
}
struct Options {
    modes: Vec<Mode>,
    variant: Variant,
    frames: u32,
    fps: u32,
    out: PathBuf,
    workers: usize,
    backend: BackendChoice,
    orbit_speed: f32,
    time_start: f32,
    benchmark: bool,
}
impl Options {
    fn parse() -> Result<Option<Self>, Box<dyn Error>> {
        let mut result = Self {
            modes: vec![Mode::First, Mode::Third, Mode::Overhead],
            variant: Variant::Current,
            frames: 90,
            fps: 30,
            out: "evidence/r5/character-study".into(),
            workers: 1,
            backend: BackendChoice::Auto,
            orbit_speed: 0.,
            time_start: 0.,
            benchmark: false,
        };
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            if arg == "--help" {
                println!("character_study --camera first|third|overhead|all --variant baseline|current --frames N --fps N --out PATH --workers N --backend auto|vulkan|dx12 --orbit-speed RADIANS_PER_SECOND --time-start SECONDS --benchmark\nDefault: all/current, 90 frames at 30 simulation ticks/s from time zero, fixed camera, one worker. --benchmark skips GPU/captures; CPU composition timings are not FPS. PNGs: OUT/VARIANT/CAMERA/frame-NNNN.png");
                return Ok(None);
            }
            if arg == "--benchmark" {
                result.benchmark = true;
                continue;
            }
            let value = args
                .next()
                .ok_or_else(|| format!("{arg} requires a value"))?;
            match arg.as_str() {
                "--camera" => {
                    result.modes = if value == "all" {
                        vec![Mode::First, Mode::Third, Mode::Overhead]
                    } else {
                        vec![Mode::parse(&value).ok_or("invalid camera")?]
                    }
                }
                "--variant" => {
                    result.variant = match value.as_str() {
                        "baseline" => Variant::Baseline,
                        "current" => Variant::Current,
                        _ => return Err("variant must be baseline or current".into()),
                    }
                }
                "--frames" => result.frames = value.parse()?,
                "--fps" => result.fps = value.parse()?,
                "--out" => result.out = value.into(),
                "--workers" => result.workers = value.parse()?,
                "--backend" => {
                    result.backend = match value.to_ascii_lowercase().as_str() {
                        "auto" => BackendChoice::Auto,
                        "vulkan" => BackendChoice::Vulkan,
                        "dx12" => BackendChoice::Dx12,
                        _ => return Err("backend must be auto, vulkan or dx12".into()),
                    }
                }
                "--orbit-speed" => result.orbit_speed = value.parse()?,
                "--time-start" => result.time_start = value.parse()?,
                _ => return Err(format!("unknown argument: {arg}").into()),
            }
        }
        if !(1..=3600).contains(&result.frames) || !(1..=240).contains(&result.fps) {
            return Err("frames must be 1..3600 and fps 1..240".into());
        }
        if !result.orbit_speed.is_finite() {
            return Err("orbit-speed must be finite".into());
        }
        if !result.time_start.is_finite() || !(0. ..=86400.).contains(&result.time_start) {
            return Err("time-start must be finite and 0..86400 seconds".into());
        }
        let available = std::thread::available_parallelism().map_or(1, |n| n.get());
        result.workers = Profile::Detail.resolve_workers(available, Some(result.workers))?;
        Ok(Some(result))
    }
}

// One stack snapshot per frame, shared by all rays. Keep both variants free of
// heap allocation so the comparison does not add allocation to just one path.
#[allow(clippy::large_enum_variant)]
enum StudyScene {
    Baseline(actor::ActorSnapshot),
    Current(current_actor::ActorSnapshot),
}
impl StudyScene {
    fn snapshot(variant: Variant, time: f32) -> Self {
        match variant {
            Variant::Baseline => Self::Baseline(actor::ActorSnapshot::new(time, Stage::Cloth)),
            Variant::Current => {
                Self::Current(current_actor::ActorSnapshot::new(time, Stage::Cloth))
            }
        }
    }
}
impl Scene for StudyScene {
    fn trace(&self, ray: Ray, _: f32, _: Stage, _: bool) -> Option<Hit> {
        match self {
            Self::Baseline(s) => s.trace(ray),
            Self::Current(s) => s.trace(ray),
        }
    }
    fn light_position(&self, _: f32) -> Vec3 {
        LIGHT
    }
    fn pigment(&self, material: u32) -> Vec3 {
        match self {
            Self::Baseline(_) => old_world::pigment(material),
            Self::Current(_) => world::pigment(material),
        }
    }
    fn glyph(&self, material: u32, local: Vec3, normal: Vec3, illumination: f32) -> u8 {
        match self {
            Self::Baseline(_) => old_world::glyph(material, local, normal, illumination),
            Self::Current(_) => world::glyph(material, local, normal, illumination),
        }
    }
    fn glyph_history_key(&self, material: u32, local: Vec3, normal: Vec3) -> u32 {
        match self {
            Self::Baseline(_) => 0,
            Self::Current(_) => {
                ascii_engine_lab::LabScene.glyph_history_key(material, local, normal)
            }
        }
    }
}

fn camera(mode: Mode, orbit_delta: f32) -> Camera {
    let mut c = Camera::new(mode);
    match mode {
        Mode::First => {
            // Eye-height observer from front/left; Camera's original FOV/pitch.
            c.offset = Vec3::new(-1.2, -0.12, 0.);
            c.orbit = -0.562;
            c.zoom = 0.64; // Aim point on the figure; first-person FOV stays 66°.
        }
        Mode::Third => {
            c.offset = Vec3::new(1., 0., 0.5);
            c.zoom = 0.34;
            c.orbit = 2.89;
        }
        Mode::Overhead => {
            c.offset = Vec3::new(1., 0.75, 0.8);
            c.zoom = 0.29;
            c.orbit = 2.65;
        }
    }
    if mode == Mode::First {
        // First-person Camera::orbit rotates its view at a fixed eye. For this
        // optional *study* orbit also move the eye around the same aim point,
        // keeping the figure framed; this is not a gameplay controller.
        let (eye, target) = c.pose();
        let delta = eye - target;
        let (s, cos) = orbit_delta.sin_cos();
        let rotated = Vec3::new(
            cos * delta.x + s * delta.z,
            delta.y,
            -s * delta.x + cos * delta.z,
        );
        c.offset += target + rotated - eye;
    }
    c.orbit += orbit_delta;
    c
}

fn differences(a: &[GlyphCell], b: &[GlyphCell]) -> usize {
    a.iter()
        .zip(b)
        .filter(|(a, b)| bytemuck::bytes_of(*a) != bytemuck::bytes_of(*b))
        .count()
}
fn quantile(values: &[f64], q: f64) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    sorted[((sorted.len() - 1) as f64 * q).round() as usize]
}

fn counts(scene: &StudyScene, camera: Camera, time: f32, cells: &[GlyphCell]) -> Value {
    let mut objects = BTreeMap::<u32, usize>::new();
    let mut materials = BTreeMap::<u32, usize>::new();
    let mut glyphs = BTreeMap::<u32, usize>::new();
    let mut occupied = 0usize;
    let mut border_hits = 0usize;
    let mut dark_hits = 0usize;
    let mut bounds: Option<[u32; 4]> = None;
    // Separate untimed pass: report geometric coverage, even for blank glyphs.
    for y in 0..ROWS {
        for x in 0..COLS {
            let cell = cells[(y * COLS + x) as usize];
            if cell.glyph != 32 {
                *glyphs.entry(cell.glyph).or_default() += 1;
            }
            if let Some(hit) = scene.trace(
                camera.ray(x, y, COLS, ROWS, 16. / 9.),
                time,
                Stage::Cloth,
                true,
            ) {
                occupied += 1;
                dark_hits += usize::from(cell.glyph == 32);
                border_hits += usize::from(x == 0 || y == 0 || x == COLS - 1 || y == ROWS - 1);
                *objects.entry(hit.object_id).or_default() += 1;
                *materials.entry(hit.material).or_default() += 1;
                bounds = Some(match bounds {
                    None => [x, y, x, y],
                    Some(b) => [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)],
                });
            }
        }
    }
    json!({"occupied_cells":occupied,"blank_glyph_hits":dark_hits,"border_hits":border_hits,
        "occupied_bounds_inclusive":bounds,"object_hits":objects,"material_hits":materials,"nonspace_glyph_histogram":glyphs})
}

fn main() -> Result<(), Box<dyn Error>> {
    let Some(options) = Options::parse()? else {
        return Ok(());
    };
    let mut gpu = if options.benchmark {
        None
    } else {
        Some(pollster::block_on(Gpu::new_with_options(
            None,
            WIDTH,
            HEIGHT,
            COLS,
            ROWS,
            GpuOptions {
                backend: options.backend,
                ..Default::default()
            },
        ))?)
    };
    for &mode in &options.modes {
        let out = options.out.join(options.variant.name()).join(mode.name());
        std::fs::create_dir_all(&out)?;
        let mut composer = Composer::with_threads(options.workers)?;
        let mut samples = Vec::new();
        let mut frames = Vec::new();
        let mut previous: Option<Vec<GlyphCell>> = None;
        let mut stability = Value::Null;
        let mut csv = String::from("frame,time_s,snapshot_cpu_ms,compose_cpu_ms,visible,matched,glyph_changes,mean_color_delta,changed_cells\n");
        for i in 0..options.frames {
            let time = options.time_start + i as f32 / options.fps as f32;
            let c = camera(mode, options.orbit_speed * time);
            let begin = Instant::now();
            let scene = StudyScene::snapshot(options.variant, time);
            let snapshot_ms = begin.elapsed().as_secs_f64() * 1000.;
            if i == 0 {
                let mut frozen = Composer::with_threads(options.workers)?;
                let (first, _) =
                    frozen.compose(&scene, c, time, Stage::Cloth, COLS, ROWS, 16. / 9.);
                let (second, _) =
                    frozen.compose(&scene, c, time, Stage::Cloth, COLS, ROWS, 16. / 9.);
                let (third, _) =
                    frozen.compose(&scene, c, time, Stage::Cloth, COLS, ROWS, 16. / 9.);
                let d12 = differences(&first, &second);
                let d23 = differences(&second, &third);
                frozen.reset();
                let (reset, _) =
                    frozen.compose(&scene, c, time, Stage::Cloth, COLS, ROWS, 16. / 9.);
                let reset_difference = differences(&first, &reset);
                stability = json!({"frozen_time":time,"first_to_second_changed_cells":d12,"second_to_third_changed_cells":d23,"after_reset_changed_cells":reset_difference,"bitwise_stable":d12==0 && d23==0 && reset_difference==0,"timed":false});
                if d12 != 0 || d23 != 0 || reset_difference != 0 {
                    return Err("frozen frame changed with history; refusing unstable study".into());
                }
                // Warm the actual timed composer/pool and scratch allocation.
                // Reset history afterwards so frame zero is a fresh composition.
                for _ in 0..WARMUP {
                    std::hint::black_box(composer.compose(
                        &scene,
                        c,
                        time,
                        Stage::Cloth,
                        COLS,
                        ROWS,
                        16. / 9.,
                    ));
                }
                composer.reset();
            }
            let begin = Instant::now();
            let (cells, stats) =
                composer.compose(&scene, c, time, Stage::Cloth, COLS, ROWS, 16. / 9.);
            let cpu_ms = begin.elapsed().as_secs_f64() * 1000.;
            samples.push(cpu_ms);
            let changed = previous.as_ref().map_or(0, |old| differences(old, &cells));
            let geometric_counts = counts(&scene, c, time, &cells);
            let filename = format!("frame-{i:04}.png");
            if let Some(gpu) = &mut gpu {
                gpu.capture(&cells, &out.join(&filename))?;
            }
            let (eye, target) = c.pose();
            frames.push(json!({"frame":i,"time":time,"snapshot_cpu_ms":snapshot_ms,"compose_cpu_ms":cpu_ms,
                "png": if options.benchmark { None } else {Some(&filename)},
                "camera_eye":eye.to_array(),"camera_target":target.to_array(),
                "camera_offset":c.offset.to_array(),"camera_orbit":c.orbit,"camera_zoom":c.zoom,
                "visible":stats.visible,"history_matched":stats.matched,"matched_glyph_changes":stats.glyph_changes,
                "matched_mean_color_delta":stats.mean_color_delta,"changed_cells_from_previous":changed,
                "first_frame_has_no_previous":i==0,"counts":geometric_counts}));
            csv.push_str(&format!(
                "{i},{time:.8},{snapshot_ms:.6},{cpu_ms:.6},{},{},{},{:.8},{changed}\n",
                stats.visible, stats.matched, stats.glyph_changes, stats.mean_color_delta
            ));
            previous = Some(cells);
        }
        let manifest = json!({"variant":options.variant.name(),"camera":mode.name(),"frame_count":options.frames,
            "simulation_ticks_per_second":options.fps,"orbit_speed_rad_per_s":options.orbit_speed,"time_start":options.time_start,
            "output_pixels":[WIDTH,HEIGHT],"grid":[COLS,ROWS],"workers":options.workers,
            "os":std::env::consts::OS,"arch":std::env::consts::ARCH,
            "build_profile":if cfg!(debug_assertions){"debug"}else{"release"},
            "light_position":LIGHT.to_array(),"scene":"Actor-only procedural idle pose/mantle study, no locomotion and no ground; black empty background; Stage::Cloth",
            "camera_scope":"Three observer projections of the same figure, not gameplay controls. First retains eye-height FOV/pitch; third views the cape from behind; overhead is orthographic. Optional first-camera orbit moves its eye around the initial aim point.",
            "baseline_sources":["examples/support/r4_actor.rs: derived from evidence/r5/baseline/src/actor.rs, only test module removed","evidence/r5/baseline/src/world.rs: original R4 pigment/glyph functions; world geometry unused"],
            "comparison_scope":"Baseline uses frozen R4 actor and materials, current uses current actor and materials. Both use the current unchanged compositor, camera and GPU glyph path, same light, tick schedule and grid. Intentional visual changes, not image parity. Timing comparisons are not equivalent-quality optimization benchmarks: geometry/material workload changes with variant.",
            "gpu":gpu.as_ref().map(Gpu::capabilities),"gpu_capture_enabled":!options.benchmark,
            "cpu_compose_p50_ms":quantile(&samples,0.5),"cpu_compose_p95_ms":quantile(&samples,0.95),
            "warmup_compositions_excluded":WARMUP,
            "timing_scope":"Only Composer::compose is timed in compose_cpu_ms. Snapshot preparation separately reported. Excludes geometry-count pass, GPU capture, PNG encoding, disposal and file I/O. Twelve untimed compositions use the first snapshot with the actual composer/pool, then reset history before measured frame zero; scratch capacity stays warm. No GPU timestamps and no application/display FPS. Capture pauses, CPU frequency and desktop load can affect samples; use --benchmark for CPU-only sequence.",
            "stability_scope":"Frozen bitwise check tests determinism, not perceptual motion quality. Dynamic counts/history metrics do not certify flicker-free animation.",
            "frozen_stability":stability,"frames":frames});
        std::fs::write(out.join("frames.csv"), csv)?;
        std::fs::write(
            out.join("study.json"),
            serde_json::to_string_pretty(&manifest)?,
        )?;
        println!(
            "{} {}: {} frames, compose CPU p50 {:.3} ms; {}",
            options.variant.name(),
            mode.name(),
            options.frames,
            quantile(&samples, 0.5),
            out.display()
        );
    }
    Ok(())
}
