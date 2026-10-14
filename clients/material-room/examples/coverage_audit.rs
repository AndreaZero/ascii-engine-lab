//! Untimed R12 geometry audit. The dense reference queries analytic geometry;
//! it is never a rendered RGB image and never supplies the displayed glyphs.
//! Counts and reference construction are deliberately excluded from benchmarks.
use ascii_material_room::{coverage_study::coverage_camera, SceneDocument, DEFAULT_SCENE_V2_JSON};
use ascii_render::{
    camera::{Camera, Mode},
    core::{
        EnvironmentQuery, EnvironmentSample, GlyphCell, Hit, MaterialSample, MetallicSample, Ray,
        Scene, Stage,
    },
    frame::{Composer, PrimarySampling},
};
use glam::Vec3;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    error::Error,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const ASPECT: f32 = 16.0 / 9.0;
const MODES: [Mode; 3] = [Mode::First, Mode::Third, Mode::Overhead];
const SAMPLING: [PrimarySampling; 2] = [PrimarySampling::Center, PrimarySampling::Conservative9];

struct Options {
    scene: PathBuf,
    output: PathBuf,
    frames: usize,
    cols: u32,
    rows: u32,
    dense: u32,
    convergence: u32,
    duration: f32,
}
impl Options {
    fn parse() -> Result<Self> {
        let mut options = Self {
            scene: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/coverage-room-v2.json"),
            output: PathBuf::from("evidence/r12/coverage-audit"),
            frames: 13,
            cols: 80,
            rows: 45,
            dense: 16,
            convergence: 32,
            duration: 6.0,
        };
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            if arg == "--help" {
                println!("coverage_audit --scene FILE --output DIR --frames N --cols N --rows N --dense N --convergence N --duration SECONDS\nDefault: 13 inclusive samples over 6 s, 80x45 cells, dense16; convergence32 at first/middle/last. Three views, center/conservative9. All work is untimed. Scene must have unique fixed nonblank glyphs per object. Query-count compositions also cover packaged R11 v2 at t=0/3/6.");
                std::process::exit(0);
            }
            let value = args
                .next()
                .ok_or_else(|| format!("missing value for {arg}"))?;
            match arg.as_str() {
                "--scene" => options.scene = value.into(),
                "--output" => options.output = value.into(),
                "--frames" => options.frames = value.parse()?,
                "--cols" => options.cols = value.parse()?,
                "--rows" => options.rows = value.parse()?,
                "--dense" => options.dense = value.parse()?,
                "--convergence" => options.convergence = value.parse()?,
                "--duration" => options.duration = value.parse()?,
                _ => return Err(format!("unknown option {arg}").into()),
            }
        }
        if !(1..=121).contains(&options.frames)
            || !(1..=240).contains(&options.cols)
            || !(1..=135).contains(&options.rows)
            || !(1..=32).contains(&options.dense)
            || !(options.dense..=64).contains(&options.convergence)
            || !options.duration.is_finite()
            || !(0.0..=60.0).contains(&options.duration)
        {
            return Err("bounds: frames1..121, cols1..240, rows1..135, dense1..32, convergence dense..64, duration0..60".into());
        }
        let maximum_queries = 3
            * options.cols as u64
            * options.rows as u64
            * (options.frames as u64 * options.dense.pow(2) as u64
                + 3 * options.convergence.pow(2) as u64);
        if maximum_queries > 250_000_000 {
            return Err("dense-reference budget exceeds 250 million rays; reduce explicit audit dimensions/frame count".into());
        }
        Ok(options)
    }
}

struct ObjectInfo {
    id: u32,
    name: String,
    glyph: u32,
}
struct Identity {
    objects: Vec<ObjectInfo>,
    by_id: BTreeMap<u32, usize>,
    by_glyph: BTreeMap<u32, usize>,
}
impl Identity {
    fn from_document(document: &SceneDocument) -> Result<Self> {
        let source = document.source();
        let mut material_glyphs = BTreeMap::new();
        for material in source["materials"].as_array().ok_or("missing materials")? {
            let palette = material["pattern"]["palette"]
                .as_str()
                .ok_or("missing palette")?;
            if material["pattern"]["kind"] != "plain" || palette.len() != 1 || palette == " " {
                return Err(
                    "coverage audit requires plain single nonblank ASCII glyph per material".into(),
                );
            }
            material_glyphs.insert(
                material["id"].as_u64().ok_or("missing material id")? as u32,
                palette.as_bytes()[0] as u32,
            );
        }
        let mut objects = Vec::new();
        for object in source["objects"].as_array().ok_or("missing objects")? {
            let material = object["material"]
                .as_u64()
                .ok_or("missing material reference")? as u32;
            objects.push(ObjectInfo {
                id: object["id"].as_u64().ok_or("missing object id")? as u32,
                name: object["name"]
                    .as_str()
                    .ok_or("missing object name")?
                    .to_owned(),
                glyph: *material_glyphs.get(&material).ok_or("unknown material")?,
            });
        }
        objects.sort_by_key(|object| object.id);
        let mut by_id = BTreeMap::new();
        let mut by_glyph = BTreeMap::new();
        for (index, object) in objects.iter().enumerate() {
            by_id.insert(object.id, index);
            if by_glyph.insert(object.glyph, index).is_some() {
                return Err("each object needs a distinct glyph/material for independent output attribution".into());
            }
        }
        Ok(Self {
            objects,
            by_id,
            by_glyph,
        })
    }

    fn owner(&self, cell: &GlyphCell) -> Result<Option<usize>> {
        if cell.glyph == 32 {
            return Ok(None);
        }
        self.by_glyph
            .get(&cell.glyph)
            .copied()
            .map(Some)
            .ok_or_else(|| format!("unexpected output glyph {}", cell.glyph).into())
    }
}

struct DenseReference {
    counts: Vec<u16>,
    objects: usize,
    samples: u32,
}
impl DenseReference {
    fn fraction(&self, cell: usize, object: usize) -> f64 {
        self.counts[cell * self.objects + object] as f64 / self.samples as f64
    }
}

fn dense_reference(
    scene: &impl Scene,
    camera: Camera,
    time: f32,
    options: &Options,
    identity: &Identity,
    subdivisions: u32,
) -> DenseReference {
    let object_count = identity.objects.len();
    let mut counts = vec![0u16; options.cols as usize * options.rows as usize * object_count];
    // Independent midpoint lattice across each cell, NOT PRIMARY_OFFSETS or the
    // production hit winner. A higher-resolution camera grid supplies rays.
    for y in 0..options.rows {
        for x in 0..options.cols {
            let start = (y * options.cols + x) as usize * object_count;
            for dy in 0..subdivisions {
                for dx in 0..subdivisions {
                    let ray = camera.ray(
                        x * subdivisions + dx,
                        y * subdivisions + dy,
                        options.cols * subdivisions,
                        options.rows * subdivisions,
                        ASPECT,
                    );
                    if let Some(hit) = scene.trace(ray, time, Stage::Light, true) {
                        if let Some(index) = identity.by_id.get(&hit.object_id) {
                            counts[start + index] += 1;
                        }
                    }
                }
            }
        }
    }
    DenseReference {
        counts,
        objects: object_count,
        samples: subdivisions * subdivisions,
    }
}

#[derive(Clone, Default)]
struct Metric {
    reference_area: f64,
    output_cells: usize,
    missed_area: f64,
    excess_area: f64,
    selected_without_dense_support: usize,
    owner_history_mismatches: usize,
    full_cell_history_mismatches: usize,
    border_cells: usize,
}
impl Metric {
    fn json(&self, object: &ObjectInfo) -> Value {
        json!({"object_id":object.id,"name":object.name,"glyph":char::from_u32(object.glyph),
            "reference_area_cells":self.reference_area,"output_cells":self.output_cells,
            "missed_area_cells":self.missed_area,"excess_area_cells":self.excess_area,
            "selected_without_dense_support":self.selected_without_dense_support,
            "owner_history_mismatches":self.owner_history_mismatches,"border_cells":self.border_cells,
            "full_cell_history_mismatches":self.full_cell_history_mismatches,
            "missing_while_reference_positive":self.reference_area > 0.0 && self.output_cells == 0})
    }
}

fn measure(
    cells: &[GlyphCell],
    fresh: &[GlyphCell],
    reference: &DenseReference,
    identity: &Identity,
    cols: u32,
    rows: u32,
) -> Result<Vec<Metric>> {
    let mut metrics = vec![Metric::default(); identity.objects.len()];
    for (cell_index, cell) in cells.iter().enumerate() {
        let owner = identity.owner(cell)?;
        let fresh_owner = identity.owner(&fresh[cell_index])?;
        for (index, metric) in metrics.iter_mut().enumerate() {
            let area = reference.fraction(cell_index, index);
            metric.reference_area += area;
            if owner == Some(index) {
                metric.output_cells += 1;
                metric.excess_area += 1.0 - area;
                metric.selected_without_dense_support += usize::from(area == 0.0);
                let x = cell_index as u32 % cols;
                let y = cell_index as u32 / cols;
                metric.border_cells +=
                    usize::from(x == 0 || y == 0 || x + 1 == cols || y + 1 == rows);
            } else {
                metric.missed_area += area;
            }
            if owner != fresh_owner && (owner == Some(index) || fresh_owner == Some(index)) {
                metric.owner_history_mismatches += 1;
            }
            if cell != &fresh[cell_index] && (owner == Some(index) || fresh_owner == Some(index)) {
                metric.full_cell_history_mismatches += 1;
            }
        }
    }
    Ok(metrics)
}

fn temporal_summary(sequence: &[Vec<Metric>], identity: &Identity) -> Value {
    Value::Array(identity.objects.iter().enumerate().map(|(index, object)| {
        let mut missing = 0;
        let mut longest = 0;
        let mut run = 0;
        let mut disappearances = 0;
        let mut reappearances = 0;
        let mut prior: Option<&Metric> = None;
        let mut missed = 0.0;
        let mut excess = 0.0;
        let mut history = 0;
        let mut full_history = 0;
        for frame in sequence {
            let now = &frame[index];
            let absent = now.reference_area > 0.0 && now.output_cells == 0;
            if absent { missing += 1; run += 1; longest = longest.max(run); } else { run = 0; }
            if let Some(previous) = prior {
                if previous.output_cells > 0 && absent { disappearances += 1; }
                if previous.reference_area > 0.0 && previous.output_cells == 0 && now.output_cells > 0 { reappearances += 1; }
            }
            missed += now.missed_area;
            excess += now.excess_area;
            history += now.owner_history_mismatches;
            full_history += now.full_cell_history_mismatches;
            prior = Some(now);
        }
        json!({"object_id":object.id,"name":object.name,"missing_samples_while_reference_positive":missing,
            "longest_missing_sample_run":longest,"sampled_disappearances":disappearances,
            "sampled_reappearances":reappearances,"missed_area_cell_samples":missed,
            "excess_area_cell_samples":excess,"owner_history_mismatches":history,
            "full_cell_history_mismatches":full_history})
    }).collect())
}

// Complete forwarding is essential: dropping local hooks would silently profile
// the legacy pipeline. Shadow queries call the inner producer directly, so its
// implementation's internal trace is not counted again as a secondary ray.
struct Counted<'a, S> {
    scene: &'a S,
    primary: AtomicU64,
    secondary: AtomicU64,
    shadow: AtomicU64,
    blocked: AtomicU64,
}
impl<'a, S> Counted<'a, S> {
    fn new(scene: &'a S) -> Self {
        Self {
            scene,
            primary: AtomicU64::new(0),
            secondary: AtomicU64::new(0),
            shadow: AtomicU64::new(0),
            blocked: AtomicU64::new(0),
        }
    }
    fn json(&self, objects: usize) -> Value {
        let primary = self.primary.load(Ordering::Relaxed);
        let secondary = self.secondary.load(Ordering::Relaxed);
        let shadow = self.shadow.load(Ordering::Relaxed);
        json!({"primary_trace_calls":primary,"secondary_trace_calls":secondary,"occluded_calls":shadow,
            "occluded_true":self.blocked.load(Ordering::Relaxed),
            "linear_producer_primitive_attempts":(primary+secondary+shadow)*objects as u64})
    }
}
impl<S: Scene> Scene for Counted<'_, S> {
    fn trace(&self, ray: Ray, time: f32, stage: Stage, water: bool) -> Option<Hit> {
        if water {
            self.primary.fetch_add(1, Ordering::Relaxed);
        } else {
            self.secondary.fetch_add(1, Ordering::Relaxed);
        }
        self.scene.trace(ray, time, stage, water)
    }
    fn occluded(&self, ray: Ray, time: f32, stage: Stage, distance: f32) -> bool {
        self.shadow.fetch_add(1, Ordering::Relaxed);
        let blocked = self.scene.occluded(ray, time, stage, distance);
        self.blocked
            .fetch_add(u64::from(blocked), Ordering::Relaxed);
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

fn count_queries(
    document: &SceneDocument,
    options: &Options,
    moving_camera: bool,
) -> Result<Value> {
    let mut records = Vec::new();
    for mode in MODES {
        for sampling in SAMPLING {
            let mut composer = Composer::default();
            composer.set_primary_sampling(sampling);
            let mut plain = Composer::default();
            plain.set_primary_sampling(sampling);
            for time in [0.0, 3.0, 6.0] {
                let snapshot = document.snapshot(time);
                let camera = if moving_camera {
                    coverage_camera(document.camera(mode), time)
                } else {
                    document.camera(mode)
                };
                let counted = Counted::new(&snapshot);
                let (cells, stats) = composer.compose(
                    &counted,
                    camera,
                    time,
                    Stage::Light,
                    options.cols,
                    options.rows,
                    ASPECT,
                );
                let (unwrapped, _) = plain.compose(
                    &snapshot,
                    camera,
                    time,
                    Stage::Light,
                    options.cols,
                    options.rows,
                    ASPECT,
                );
                if cells != unwrapped {
                    return Err("counting wrapper changed composed output".into());
                }
                records.push(json!({"camera":mode.name(),"sampling":sampling.name(),"time_s":time,
                    "counts":counted.json(document.object_count()),"composer_primary_rays":stats.primary_rays,
                    "alternate_primary_cells":stats.alternate_primary,"wrapper_output_equal":true}));
            }
        }
    }
    Ok(
        json!({"scene_name":document.name(),"input_scene":document.source(),"objects":document.object_count(),"grid":[options.cols,options.rows],
        "path":if moving_camera {"coverage6s"} else {"authored_static"},"records":records,
        "scope":"Untimed serial counting compositions. Secondary combines environment visibility and reflected traces. occluded calls bypass wrapper trace. linear_producer_primitive_attempts is the hypothetical R12 full-scan count (calls times objects), not the actual R13 traversal work; use the R13 query audit for actual attempts. Not CPU time or GPU work."}),
    )
}

fn main() -> Result<()> {
    let options = Options::parse()?;
    let document = SceneDocument::load(&options.scene)?;
    let identity = Identity::from_document(&document)?;
    std::fs::create_dir_all(&options.output)?;
    let mut all_views = Vec::new();
    for mode in MODES {
        let mut composers = [Composer::default(), Composer::default()];
        composers[1].set_primary_sampling(PrimarySampling::Conservative9);
        let mut sequences: [Vec<Vec<Metric>>; 2] = [Vec::new(), Vec::new()];
        let mut frames = Vec::new();
        for frame in 0..options.frames {
            let time = if options.frames == 1 {
                0.0
            } else {
                options.duration * frame as f32 / (options.frames - 1) as f32
            };
            let snapshot = document.snapshot(time);
            let camera = coverage_camera(document.camera(mode), time);
            let reference =
                dense_reference(&snapshot, camera, time, &options, &identity, options.dense);
            let convergence = if frame == 0
                || frame == options.frames / 2
                || frame + 1 == options.frames
            {
                let fine = dense_reference(
                    &snapshot,
                    camera,
                    time,
                    &options,
                    &identity,
                    options.convergence,
                );
                Some(Value::Array(identity.objects.iter().enumerate().map(|(object,info)| {
                    let mut coarse_area=0.0;
                    let mut fine_area=0.0;
                    let mut l1=0.0;
                    for cell in 0..(options.cols*options.rows) as usize {
                        let a=reference.fraction(cell,object);
                        let b=fine.fraction(cell,object);
                        coarse_area+=a; fine_area+=b; l1+=(a-b).abs();
                    }
                    json!({"object_id":info.id,"coarse_area_cells":coarse_area,"fine_area_cells":fine_area,"absolute_cell_area_difference":l1})
                }).collect()))
            } else {
                None
            };
            let mut variants = Vec::new();
            for (index, sampling) in SAMPLING.into_iter().enumerate() {
                let (cells, stats) = composers[index].compose(
                    &snapshot,
                    camera,
                    time,
                    Stage::Light,
                    options.cols,
                    options.rows,
                    ASPECT,
                );
                let mut fresh = Composer::default();
                fresh.set_primary_sampling(sampling);
                let (fresh_cells, _) = fresh.compose(
                    &snapshot,
                    camera,
                    time,
                    Stage::Light,
                    options.cols,
                    options.rows,
                    ASPECT,
                );
                let metrics = measure(
                    &cells,
                    &fresh_cells,
                    &reference,
                    &identity,
                    options.cols,
                    options.rows,
                )?;
                variants.push(json!({"sampling":sampling.name(),"primary_rays":stats.primary_rays,"alternate_primary_cells":stats.alternate_primary,
                    "fresh_history_full_cell_mismatches":cells.iter().zip(&fresh_cells).filter(|(a,b)|a!=b).count(),
                    "blank_cells":cells.iter().filter(|cell|cell.glyph==32).count(),
                    "objects":metrics.iter().zip(&identity.objects).map(|(metric,object)|metric.json(object)).collect::<Vec<_>>()}));
                sequences[index].push(metrics);
            }
            let (eye, target) = camera.pose();
            frames.push(json!({"sample":frame,"time_s":time,"eye":eye.to_array(),"target":target.to_array(),
                "orbit":camera.orbit,"zoom":camera.zoom,"variants":variants,"convergence":convergence}));
            println!(
                "{} audit sample {}/{}",
                mode.name(),
                frame + 1,
                options.frames
            );
        }
        all_views.push(json!({"camera":mode.name(),"frames":frames,
            "temporal_summary":SAMPLING.into_iter().enumerate().map(|(index,sampling)|json!({"sampling":sampling.name(),"objects":temporal_summary(&sequences[index],&identity)})).collect::<Vec<_>>()}));
    }
    let counts = json!({"diagnostic_scene":count_queries(&document,&options,true)?,
        "r11_packaged_v2":count_queries(&SceneDocument::from_json(DEFAULT_SCENE_V2_JSON)?,&options,false)?});
    let report = json!({"scene_path":options.scene,"input_scene":document.source(),"grid":[options.cols,options.rows],
        "aspect":ASPECT,"dense_midpoints_per_axis":options.dense,"convergence_midpoints_per_axis":options.convergence,
        "duration_s":options.duration,"sample_count":options.frames,"path":"coverage_camera: fixed six-second combined motion",
        "workers":1,"stage":"light","views":all_views,"query_counts":counts,
        "limits":["Finite analytic midpoint reference, not exact geometric area or perceptual ground truth. Convergence differences are retained.",
            "Reference rays use the same analytic scene producer; this audits cell sampling, not independent primitive-intersection correctness.",
            "The dense lattice never chooses displayed glyphs. Actual output owner is decoded from each object's unique authored fixed glyph.",
            "Area is measured in cells; missing/excess are sums of fractional reference occupancy versus binary output owner, not ink coverage.",
            "Selected without dense support is a finite-reference discrepancy, not proof of leakage: the dense lattice can itself miss thin geometry. Blank output can include lighting rejection.",
            "Samples may be sparse in time; missing run lengths and transitions count audit samples, not continuous frames or perceptual flicker.",
            "Floor and backdrop are reported separately by ID1/2; do not let their large areas hide thin-subject failures.",
            "No timing, GPU capture or visual PASS is asserted by this audit. Nine rays may enlarge silhouettes and still miss details between samples."]});
    std::fs::write(
        options.output.join("coverage-audit.json"),
        serde_json::to_string_pretty(&report)?,
    )?;
    println!("{}", options.output.join("coverage-audit.json").display());
    Ok(())
}
