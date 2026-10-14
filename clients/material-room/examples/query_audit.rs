//! Untimed R13 bounded-experiment audit, not ordinary-runtime operation counts.
//! Every experiment composition is compared bit-for-bit with the restored R12
//! runtime. Instrumentation never supplies timing data or enables runtime use.
use ascii_material_room::{
    content::QueryStats, coverage_study::coverage_camera, SceneDocument, Snapshot,
    DEFAULT_SCENE_JSON, DEFAULT_SCENE_V2_JSON,
};
use ascii_render::{
    camera::Mode,
    core::{
        EnvironmentQuery, EnvironmentSample, GlyphCell, Hit, MaterialSample, MetallicSample, Ray,
        Scene, Stage,
    },
    frame::{Composer, FrameStats, PrimarySampling},
};
use glam::Vec3;
use serde_json::{json, Value};
use std::{
    error::Error,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const ASPECT: f32 = 16.0 / 9.0;
const TIMES: [f32; 3] = [0.0, 3.0, 6.0];
const MODES: [Mode; 3] = [Mode::First, Mode::Third, Mode::Overhead];
const SAMPLING: [PrimarySampling; 2] = [PrimarySampling::Center, PrimarySampling::Conservative9];
const COVERAGE_SCENE_JSON: &str = include_str!("../assets/coverage-room-v2.json");

struct Options {
    output: PathBuf,
    cols: u32,
    rows: u32,
    workers: usize,
    stress128: bool,
}
impl Options {
    fn parse() -> Result<Self> {
        let mut options = Self {
            output: "evidence/r13/query-audit".into(),
            cols: 240,
            rows: 135,
            workers: 1,
            stress128: false,
        };
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--help" => {
                    println!("query_audit [--output DIR] [--cols N] [--rows N] [--workers N] [--stress128]\nUntimed bounded-experiment query work, compared with the ordinary R12 linear runtime: three packaged scenes, three views, t=0/3/6, center/conservative9, fixed coverage camera path. Default: 240x135, one worker. Optional generated 128-object stress is written separately. These are not ordinary-runtime counts or CPU/GPU timings.");
                    std::process::exit(0);
                }
                "--stress128" => options.stress128 = true,
                "--output" | "--cols" | "--rows" | "--workers" => {
                    let value = args
                        .next()
                        .ok_or_else(|| format!("missing value for {arg}"))?;
                    match arg.as_str() {
                        "--output" => options.output = value.into(),
                        "--cols" => options.cols = value.parse()?,
                        "--rows" => options.rows = value.parse()?,
                        "--workers" => options.workers = value.parse()?,
                        _ => unreachable!(),
                    }
                }
                _ => return Err(format!("unknown option {arg}").into()),
            }
        }
        if !(1..=240).contains(&options.cols)
            || !(1..=135).contains(&options.rows)
            || !(1..=64).contains(&options.workers)
        {
            return Err("bounds: cols1..240, rows1..135, workers1..64".into());
        }
        Ok(options)
    }
}

// One counter set per externally requested query category. The experiment's
// internal traversal is reported by QueryStats, never counted as another call.
#[derive(Default)]
struct Counters {
    calls: AtomicU64,
    successful_calls: AtomicU64,
    objects_considered: AtomicU64,
    primitive_tests: AtomicU64,
    direction_transforms: AtomicU64,
    box_axes_examined: AtomicU64,
    box_axes_unbounded_visited: AtomicU64,
    primitive_hits: AtomicU64,
    full_hit_builds: AtomicU64,
    valid_hits: AtomicU64,
    shadow_early_exits: AtomicU64,
}
impl Counters {
    fn record(&self, stats: QueryStats, success: bool) {
        self.calls.fetch_add(1, Ordering::Relaxed);
        self.successful_calls
            .fetch_add(u64::from(success), Ordering::Relaxed);
        self.objects_considered
            .fetch_add(stats.objects_considered as u64, Ordering::Relaxed);
        self.primitive_tests
            .fetch_add(stats.primitive_tests as u64, Ordering::Relaxed);
        self.direction_transforms
            .fetch_add(stats.direction_transforms as u64, Ordering::Relaxed);
        self.box_axes_examined
            .fetch_add(stats.box_axes_examined as u64, Ordering::Relaxed);
        self.box_axes_unbounded_visited
            .fetch_add(stats.box_axes_unbounded_visited as u64, Ordering::Relaxed);
        self.primitive_hits
            .fetch_add(stats.primitive_hits as u64, Ordering::Relaxed);
        self.full_hit_builds
            .fetch_add(stats.full_hit_builds as u64, Ordering::Relaxed);
        self.valid_hits
            .fetch_add(stats.valid_hits as u64, Ordering::Relaxed);
        self.shadow_early_exits
            .fetch_add(stats.shadow_early_exits as u64, Ordering::Relaxed);
    }

    fn read(&self) -> Totals {
        Totals {
            calls: self.calls.load(Ordering::Relaxed),
            successful_calls: self.successful_calls.load(Ordering::Relaxed),
            objects_considered: self.objects_considered.load(Ordering::Relaxed),
            primitive_tests: self.primitive_tests.load(Ordering::Relaxed),
            direction_transforms: self.direction_transforms.load(Ordering::Relaxed),
            box_axes_examined: self.box_axes_examined.load(Ordering::Relaxed),
            box_axes_unbounded_visited: self.box_axes_unbounded_visited.load(Ordering::Relaxed),
            primitive_hits: self.primitive_hits.load(Ordering::Relaxed),
            full_hit_builds: self.full_hit_builds.load(Ordering::Relaxed),
            valid_hits: self.valid_hits.load(Ordering::Relaxed),
            shadow_early_exits: self.shadow_early_exits.load(Ordering::Relaxed),
        }
    }
}

#[derive(Clone, Copy, Default)]
struct Totals {
    calls: u64,
    successful_calls: u64,
    objects_considered: u64,
    primitive_tests: u64,
    direction_transforms: u64,
    box_axes_examined: u64,
    box_axes_unbounded_visited: u64,
    primitive_hits: u64,
    full_hit_builds: u64,
    valid_hits: u64,
    shadow_early_exits: u64,
}
impl Totals {
    fn add(&mut self, other: Self) {
        self.calls += other.calls;
        self.successful_calls += other.successful_calls;
        self.objects_considered += other.objects_considered;
        self.primitive_tests += other.primitive_tests;
        self.direction_transforms += other.direction_transforms;
        self.box_axes_examined += other.box_axes_examined;
        self.box_axes_unbounded_visited += other.box_axes_unbounded_visited;
        self.primitive_hits += other.primitive_hits;
        self.full_hit_builds += other.full_hit_builds;
        self.valid_hits += other.valid_hits;
        self.shadow_early_exits += other.shadow_early_exits;
    }

    fn validate(self, objects: usize, shadow: bool) -> Result<()> {
        let full_scan = self.calls * objects as u64;
        if self.successful_calls > self.calls
            || self.objects_considered != self.primitive_tests
            || self.primitive_tests > full_scan
            || (!shadow && self.primitive_tests != full_scan)
            || self.direction_transforms > self.primitive_tests
            || (self.primitive_tests > 0 && self.direction_transforms == 0)
            || self.box_axes_examined > self.box_axes_unbounded_visited
            || self.box_axes_unbounded_visited > self.primitive_tests * 3
            || self.primitive_hits > self.primitive_tests
            || self.full_hit_builds > self.primitive_hits
            || self.valid_hits > self.full_hit_builds
            || (shadow && self.shadow_early_exits != self.successful_calls)
            || (!shadow && self.shadow_early_exits != 0)
        {
            return Err("inconsistent query counters".into());
        }
        Ok(())
    }

    fn json(self, objects: usize) -> Value {
        json!({
            "calls": self.calls,
            "successful_calls": self.successful_calls,
            "objects_considered": self.objects_considered,
            "primitive_tests": self.primitive_tests,
            "direction_transforms": self.direction_transforms,
            "box_axes_examined": self.box_axes_examined,
            "box_axes_unbounded_visited": self.box_axes_unbounded_visited,
            "box_axes_saved_with_limit_on_visited_boxes": self.box_axes_unbounded_visited - self.box_axes_examined,
            "primitive_hits": self.primitive_hits,
            "full_hit_builds": self.full_hit_builds,
            "valid_hits": self.valid_hits,
            "shadow_early_exits": self.shadow_early_exits,
            "kernel_candidates_not_finalized": self.primitive_hits - self.full_hit_builds,
            "derived_r12_full_scan_primitive_tests": self.calls * objects as u64,
            "primitive_tests_below_derived_r12_full_scan": self.calls * objects as u64 - self.primitive_tests,
            "direction_transforms_below_visited_primitive_count": self.primitive_tests - self.direction_transforms,
            "derived_r12_full_scan_direction_transforms": self.calls * objects as u64,
            "direction_transforms_below_derived_r12_full_scan": self.calls * objects as u64 - self.direction_transforms,
        })
    }
}

struct BoundedExperiment<'a> {
    scene: &'a Snapshot,
    primary: Counters,
    secondary: Counters,
    shadow: Counters,
}
impl<'a> BoundedExperiment<'a> {
    fn new(scene: &'a Snapshot) -> Self {
        Self {
            scene,
            primary: Counters::default(),
            secondary: Counters::default(),
            shadow: Counters::default(),
        }
    }
    fn totals(&self) -> [Totals; 3] {
        [
            self.primary.read(),
            self.secondary.read(),
            self.shadow.read(),
        ]
    }
}

// Forward every Scene hook: defaults would silently disable local materials,
// environment/conductors or structural history and profile a different scene.
impl Scene for BoundedExperiment<'_> {
    fn trace(&self, ray: Ray, time: f32, stage: Stage, include_water: bool) -> Option<Hit> {
        let (hit, stats) = self
            .scene
            .trace_bounded_counted(ray, time, stage, include_water);
        // This is the current Composer contract: true = primary visibility,
        // false = local environment/reflection visibility. Shadows use occluded.
        let category = if include_water {
            &self.primary
        } else {
            &self.secondary
        };
        category.record(stats, hit.is_some());
        hit
    }
    fn occluded(&self, ray: Ray, time: f32, stage: Stage, max_distance: f32) -> bool {
        let (blocked, stats) = self
            .scene
            .occluded_bounded_counted(ray, time, stage, max_distance);
        self.shadow.record(stats, blocked);
        blocked
    }
    fn light_position(&self, time: f32) -> Vec3 {
        self.scene.light_position(time)
    }
    fn pigment(&self, material: u32) -> Vec3 {
        self.scene.pigment(material)
    }
    fn glyph(&self, material: u32, local: Vec3, normal: Vec3, illumination: f32) -> u8 {
        self.scene.glyph(material, local, normal, illumination)
    }
    fn local_materials(&self) -> bool {
        self.scene.local_materials()
    }
    fn material_sample(&self, hit: Hit, footprint: f32) -> Option<MaterialSample> {
        self.scene.material_sample(hit, footprint)
    }
    fn metallic_sample(&self, hit: Hit, footprint: f32) -> Option<MetallicSample> {
        self.scene.metallic_sample(hit, footprint)
    }
    fn environment_sample(&self, query: EnvironmentQuery) -> Option<EnvironmentSample> {
        self.scene.environment_sample(query)
    }
    fn glyph_sample(&self, hit: Hit, illumination: f32, footprint: f32) -> u8 {
        self.scene.glyph_sample(hit, illumination, footprint)
    }
    fn glyph_sample_key(&self, hit: Hit, footprint: f32) -> u32 {
        self.scene.glyph_sample_key(hit, footprint)
    }
    fn light_color(&self) -> Vec3 {
        self.scene.light_color()
    }
    fn glyph_history_key(&self, material: u32, local: Vec3, normal: Vec3) -> u32 {
        self.scene.glyph_history_key(material, local, normal)
    }
}

fn stats_equal(a: &FrameStats, b: &FrameStats) -> bool {
    a.visible == b.visible
        && a.reflected == b.reflected
        && a.matched == b.matched
        && a.glyph_changes == b.glyph_changes
        && a.mean_color_delta.to_bits() == b.mean_color_delta.to_bits()
        && a.primary_rays == b.primary_rays
        && a.alternate_primary == b.alternate_primary
}

fn category_json(totals: [Totals; 3], objects: usize) -> Value {
    json!({
        "primary": totals[0].json(objects),
        "secondary": totals[1].json(objects),
        "shadow": totals[2].json(objects),
    })
}

fn audit(document: &SceneDocument, source_label: &str, options: &Options) -> Result<Value> {
    let objects = document.object_count();
    let mut records = Vec::new();
    let mut all_records = [Totals::default(); 3];
    for mode in MODES {
        for sampling in SAMPLING {
            let mut counted_composer = Composer::with_threads(options.workers)?;
            let mut plain_composer = Composer::with_threads(options.workers)?;
            counted_composer.set_primary_sampling(sampling);
            plain_composer.set_primary_sampling(sampling);
            // Both composers retain their own history across these three ticks.
            for time in TIMES {
                let snapshot = document.snapshot(time);
                let camera = coverage_camera(document.camera(mode), time);
                let counted = BoundedExperiment::new(&snapshot);
                let (cells, stats) = counted_composer.compose(
                    &counted,
                    camera,
                    time,
                    Stage::Light,
                    options.cols,
                    options.rows,
                    ASPECT,
                );
                let (plain_cells, plain_stats) = plain_composer.compose(
                    &snapshot,
                    camera,
                    time,
                    Stage::Light,
                    options.cols,
                    options.rows,
                    ASPECT,
                );
                if bytemuck::cast_slice::<GlyphCell, u8>(&cells)
                    != bytemuck::cast_slice::<GlyphCell, u8>(&plain_cells)
                    || !stats_equal(&stats, &plain_stats)
                {
                    return Err(format!(
                        "bounded experiment differs from runtime cells or frame stats: {source_label}, {}, {}, t={time}",
                        mode.name(),
                        sampling.name()
                    )
                    .into());
                }
                let totals = counted.totals();
                for (index, total) in totals.into_iter().enumerate() {
                    total.validate(objects, index == 2)?;
                    all_records[index].add(total);
                }
                let expected_primary = u64::from(options.cols)
                    * u64::from(options.rows)
                    * if sampling == PrimarySampling::Center {
                        1
                    } else {
                        9
                    };
                if totals[0].calls != expected_primary
                    || totals[0].calls != stats.primary_rays as u64
                {
                    return Err("primary query category does not match Composer sampling".into());
                }
                let (eye, target) = camera.pose();
                records.push(json!({
                    "camera": mode.name(), "sampling": sampling.name(), "time_s": time,
                    "eye": eye.to_array(), "target": target.to_array(),
                    "orbit": camera.orbit, "zoom": camera.zoom,
                    "counts": category_json(totals, objects),
                    "composer_primary_rays": stats.primary_rays,
                    "alternate_primary_cells": stats.alternate_primary,
                    "visible_cells": stats.visible,
                    "bounded_vs_runtime_cells_bitwise_equal": true,
                    "bounded_vs_runtime_frame_stats_bitwise_equal": true,
                }));
            }
            println!(
                "{source_label}: {} / {} audited",
                mode.name(),
                sampling.name()
            );
        }
    }
    Ok(json!({
        "source_label": source_label, "scene_name": document.name(),
        "input_scene": document.source(), "objects": objects,
        "records": records,
        "totals_across_all_18_records": category_json(all_records, objects),
    }))
}

fn report(scenes: Vec<Value>, options: &Options, synthetic: bool) -> Value {
    json!({
        "audit": "r13_bounded_experiment_query_work", "synthetic_stress": synthetic,
        "measured_query_path": "bounded_experiment",
        "comparison_query_path": "runtime_r12_linear",
        "ordinary_runtime_counters": false,
        "grid": [options.cols, options.rows], "aspect": ASPECT,
        "workers": options.workers, "stage": "light", "times_s": TIMES,
        "path": "coverage_camera: fixed six-second combined motion",
        "history": "Independent bounded-experiment/runtime histories retained across t=0,3,6; reset for each view/sampling/scene.",
        "scenes": scenes,
        "counter_definitions": {
            "primary": "Experiment wrapper trace(include_water=true).",
            "secondary": "Experiment wrapper trace(include_water=false): combined environment visibility and reflection queries.",
            "shadow": "Experiment wrapper occluded calls; internal experiment work is not counted again as secondary trace.",
            "successful_calls": "Returned Some for trace or true for occluded.",
            "direction_transforms": "Experiment inverse quaternion times world-ray direction operations, performed separately for every visited object.",
            "direction_transforms_below_visited_primitive_count": "Experiment primitive_tests minus direction_transforms, excluding unvisited objects. Zero for this experiment, which does not reuse directions.",
            "box_axes_examined": "Actual slab axes examined by the bounded box solver, including parallel axes; not divisions or GPU work. Ellipsoids do not contribute.",
            "box_axes_unbounded_visited": "Untimed counterfactual: axes examined by the same box solver with a positive-infinite limit, for the same visited boxes and local rays only. Objects skipped after a shadow early exit are excluded.",
            "box_axes_saved_with_limit_on_visited_boxes": "box_axes_unbounded_visited minus box_axes_examined; isolates limit-driven slab savings on visited boxes. It is not the difference against all R12 shadow work.",
            "primitive_hits": "Experiment primitive solver returned Some. Box results already obey the experiment's strict nearest/shadow limit; these are not unbounded R12 candidate counts.",
            "full_hit_builds": "Actual complete Hit reconstruction attempts, including rejected nonfinite complete hits.",
            "valid_hits": "Complete reconstruction returned a finite accepted Hit, possibly more than one per nearest trace.",
            "kernel_candidates_not_finalized": "Experiment primitive_hits minus full_hit_builds. Bookkeeping of returned candidates only, not avoided reconstructions relative to R12.",
            "derived_r12_full_scan_primitive_tests": "Counterfactual calls times object_count, derived from the R12 full-scan algorithm; not an instrumented measurement of the frozen R12 binary.",
            "derived_r12_full_scan_direction_transforms": "Counterfactual calls times object_count: R12 transformed each ray direction once per object. Derived, not measured from the frozen binary.",
            "direction_transforms_below_derived_r12_full_scan": "Derived full-scan direction transforms minus experiment direction transforms. The experiment has no direction reuse, so only shadow early exits avoid these operations.",
        },
        "limits": [
            "Untimed diagnostic compositions with atomic aggregation. These are operation counts, not CPU/GPU times, FPS or a performance benchmark.",
            "Counts describe only the explicit bounded diagnostic experiment. The ordinary Scene runtime retains R12 linear nearest traversal and default nearest-hit occlusion; these counts do not describe that runtime.",
            "The experiment has no broadphase: primary/secondary traces still test every object. Its shadow queries can terminate at the first fully validated blocker.",
            "Bounded box queries preserve the original slab arithmetic and strict filtered result, but can skip remaining axes once an entry cannot beat the limit. Ellipsoid kernels are unchanged.",
            "Experiment objects retain their original query order and independent world-to-local transforms, with no direction reuse or deferred Hit reconstruction. Operation savings do not establish wall-clock gains or justify enabling this path in the ordinary runtime.",
            "Bounded box filtering and shadow early exits hide unbounded candidates; no R12 full-hit count can be inferred from these counters. The axes counterfactual covers only boxes actually visited, never an extrapolated full shadow scan.",
            "Bounded-experiment cells and FrameStats are compared bitwise against the restored ordinary R12 runtime, including history. The separate frozen-source tests also compare both query paths with R12.",
            "Three discrete ticks do not measure every intervening frame. Full scene JSON is retained so the inputs are reviewable.",
            "Optional stress128 is generated synthetic content, reported separately; it is not a representative scene, a timing scaling result or a visual quality claim."
        ],
    })
}

fn stress_document() -> Result<SceneDocument> {
    let mut source: Value = serde_json::from_str(DEFAULT_SCENE_V2_JSON)?;
    source["name"] = "R13 generated 128-object query stress (synthetic)".into();
    source["objects"] = Value::Array(
        (0..128)
            .map(|index| {
                json!({
                    "id": index + 1, "name": format!("Synthetic sample {index}"),
                    "shape": if index % 4 == 0 { "ellipsoid" } else { "box" },
                    "material": 32 + index % 5,
                    "position": [(index % 16) as f32 * 0.42 - 3.15,
                        0.3 + (index % 3) as f32 * 0.27, (index / 16) as f32 * 0.55 - 2.0],
                    "half_extents": [0.17, 0.27, 0.19],
                    "rotation_y": (index % 7) as f32 * 0.13,
                })
            })
            .collect(),
    );
    Ok(SceneDocument::from_json(&serde_json::to_string(&source)?)?)
}

fn main() -> Result<()> {
    let options = Options::parse()?;
    let mut scenes = Vec::new();
    for (label, source) in [
        ("packaged-v1", DEFAULT_SCENE_JSON),
        ("packaged-v2", DEFAULT_SCENE_V2_JSON),
        ("coverage-v2", COVERAGE_SCENE_JSON),
    ] {
        scenes.push(audit(&SceneDocument::from_json(source)?, label, &options)?);
    }
    std::fs::create_dir_all(&options.output)?;
    let output = options.output.join("query-audit.json");
    std::fs::write(
        &output,
        serde_json::to_string_pretty(&report(scenes, &options, false))?,
    )?;
    println!("{}", output.display());
    if options.stress128 {
        let stress = audit(&stress_document()?, "generated-stress128", &options)?;
        let output = options.output.join("stress128-query-audit.json");
        std::fs::write(
            &output,
            serde_json::to_string_pretty(&report(vec![stress], &options, true))?,
        )?;
        println!("{}", output.display());
    }
    Ok(())
}
