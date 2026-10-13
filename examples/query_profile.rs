//! Untimed serial collection, followed by isolated query-batch CPU replay.
//! No per-ray clock or synchronization is included in the reported timings.
use ascii_engine_lab::{
    camera::{Camera, Mode},
    core::{Hit, Ray, Scene, Stage},
    frame::Composer,
    LabScene,
};
use glam::Vec3;
use serde_json::json;
use std::{error::Error, hint::black_box, path::PathBuf, sync::Mutex, time::Instant};

const TIME: f32 = 1.;
const STAGE: Stage = Stage::Cloth;
const COLS: u32 = 240;
const ROWS: u32 = 135;
const WARMUP: usize = 3;
const SAMPLES: usize = 21;
const NAMES: [&str; 3] = ["primary", "reflection", "shadow"];

#[derive(Clone, Copy)]
struct Query {
    ray: Ray,
    time: f32,
    stage: Stage,
    max_distance: f32,
}

#[derive(Default)]
struct Collected {
    groups: [Vec<Query>; 3],
    // Primary/reflection: trace returned Some. Shadow: occluded returned true.
    positive_results: [usize; 3],
}

struct Recorder<S> {
    scene: S,
    collected: Mutex<Collected>,
}

impl<S: Scene> Recorder<S> {
    fn record(&self, group: usize, query: Query, positive: bool) {
        // Composer requires Sync. This mutex exists only in the deliberately
        // untimed collection pass, never in the timed replay below.
        let mut collected = self.collected.lock().unwrap();
        collected.groups[group].push(query);
        collected.positive_results[group] += usize::from(positive);
    }
}

impl<S: Scene> Scene for Recorder<S> {
    fn trace(&self, ray: Ray, time: f32, stage: Stage, water: bool) -> Option<Hit> {
        let result = self.scene.trace(ray, time, stage, water);
        self.record(
            if water { 0 } else { 1 },
            Query {
                ray,
                time,
                stage,
                max_distance: 0.,
            },
            result.is_some(),
        );
        result
    }

    fn occluded(&self, ray: Ray, time: f32, stage: Stage, max_distance: f32) -> bool {
        // Invoke the wrapped scene directly: its internal trace must not also
        // be misclassified as a reflected ray by this recorder.
        let result = self.scene.occluded(ray, time, stage, max_distance);
        self.record(
            2,
            Query {
                ray,
                time,
                stage,
                max_distance,
            },
            result,
        );
        result
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
    fn glyph_history_key(&self, material: u32, local: Vec3, normal: Vec3) -> u32 {
        self.scene.glyph_history_key(material, local, normal)
    }
}

fn replay(scene: &impl Scene, group: usize, queries: &[Query]) -> usize {
    let mut positive_results = 0;
    // Dispatch is outside the ray loop. Returned Hit values are black-boxed
    // whole, so the compiler cannot specialize trace into an any-hit query.
    match group {
        0 | 1 => {
            let include_water = group == 0;
            for query in queries {
                let q = black_box(*query);
                let hit = black_box(scene.trace(q.ray, q.time, q.stage, include_water));
                positive_results += usize::from(hit.is_some());
            }
        }
        2 => {
            for query in queries {
                let q = black_box(*query);
                positive_results += usize::from(black_box(scene.occluded(
                    q.ray,
                    q.time,
                    q.stage,
                    q.max_distance,
                )));
            }
        }
        _ => unreachable!(),
    }
    black_box(positive_results)
}

fn quantile(samples: &[f64], q: f64) -> f64 {
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    sorted[((sorted.len() - 1) as f64 * q).round() as usize]
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut output = PathBuf::from("evidence/r4/query-profile");
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--output" => output = args.next().ok_or("--output requires a directory")?.into(),
            "--help" => {
                println!("query_profile [--output DIRECTORY]: serial CPU query batches, no GPU or frame/FPS timing");
                return Ok(());
            }
            _ => return Err(format!("unknown argument: {arg}").into()),
        }
    }
    std::fs::create_dir_all(&output)?;
    let mut summaries = Vec::new();
    let mut csv =
        String::from("camera,group,sample,queries,positive_results,batch_ms,mean_ns_per_query\n");
    for mode in [Mode::First, Mode::Third, Mode::Overhead] {
        let recorder = Recorder {
            scene: LabScene::snapshot(TIME, STAGE),
            collected: Mutex::new(Collected::default()),
        };
        let mut composer = Composer::with_threads(1)?;
        assert_eq!(composer.threads(), 1);
        // No clock around instrumentation, snapshot creation, or composition.
        black_box(composer.compose(
            &recorder,
            Camera::new(mode),
            TIME,
            STAGE,
            COLS,
            ROWS,
            16. / 9.,
        ));
        let Recorder { scene, collected } = recorder;
        let collected = collected.into_inner().unwrap();
        assert_eq!(collected.groups[0].len(), (COLS * ROWS) as usize);
        let mut samples: [Vec<f64>; 3] = std::array::from_fn(|_| Vec::with_capacity(SAMPLES));
        for sample in 0..(WARMUP + SAMPLES) {
            // Rotate group order to avoid assigning all warm/cold positions to
            // the same category. All queries within a group retain call order.
            for offset in 0..3 {
                let group = (sample + offset) % 3;
                let queries = &collected.groups[group];
                if sample < WARMUP {
                    assert_eq!(
                        replay(&scene, group, queries),
                        collected.positive_results[group]
                    );
                    continue;
                }
                let start = Instant::now();
                let positives = replay(&scene, group, queries);
                let elapsed = start.elapsed().as_secs_f64();
                // Assertions, aggregation, formatting and I/O are untimed.
                assert_eq!(positives, collected.positive_results[group]);
                samples[group].push(elapsed);
                let per_query = if queries.is_empty() {
                    0.
                } else {
                    elapsed * 1e9 / queries.len() as f64
                };
                csv.push_str(&format!(
                    "{},{},{},{},{},{:.9},{:.3}\n",
                    mode.name(),
                    NAMES[group],
                    sample - WARMUP,
                    queries.len(),
                    positives,
                    elapsed * 1000.,
                    per_query
                ));
            }
        }
        for group in 0..3 {
            let count = collected.groups[group].len();
            let p50 = quantile(&samples[group], 0.5);
            let p95 = quantile(&samples[group], 0.95);
            let ns_per_query = (count > 0).then(|| p50 * 1e9 / count as f64);
            summaries.push(json!({
                "camera": mode.name(), "group": NAMES[group], "queries": count,
                "positive_results": collected.positive_results[group],
                "positive_result_means": if group == 2 {"occluded=true"} else {"trace=Some(Hit)"},
                "batch_p50_ms": p50 * 1000., "batch_p95_ms": p95 * 1000.,
                "p50_batch_mean_ns_per_query": ns_per_query,
            }));
            println!(
                "{} {}: {count} queries, {} positive; batch p50 {:.3} ms, p95 {:.3} ms",
                mode.name(),
                NAMES[group],
                collected.positive_results[group],
                p50 * 1000.,
                p95 * 1000.
            );
        }
    }
    std::fs::write(output.join("query-batches.csv"), csv)?;
    std::fs::write(
        output.join("profile.json"),
        serde_json::to_string_pretty(&json!({
            "build_profile": if cfg!(debug_assertions) {"debug"} else {"release"},
            "os": std::env::consts::OS, "arch": std::env::consts::ARCH,
            "time": TIME, "stage": STAGE.name(), "grid": [COLS, ROWS], "aspect": 16. / 9.,
            "workers": 1, "warmup_batches_per_group": WARMUP, "samples_per_group": SAMPLES,
            "collection": "Untimed serial composition of a fresh LabScene snapshot. Primary trace includes water; reflection trace excludes water; shadow calls occluded directly. Shadow traces are never double-counted as reflections.",
            "timing": "Separate serial replay of recorded rays against the same immutable snapshot; one Instant pair per complete group batch; group order rotates each round. No locks, allocation, I/O or per-ray clocks in timed batches. Timings include loop, black_box and positive-result counting overhead.",
            "limits": ["Isolated query CPU cost, not composition time, GPU time or FPS", "Grouping changes cache behavior versus interleaved composition; batch costs cannot reconstruct frame time", "One frozen tick and default camera pose each; no claim about all scenes or motion", "Per-query figure is a batch average, not individual-ray latency", "Local desktop scheduling and frequency variation are not controlled"],
            "groups": summaries,
        }))?,
    )?;
    Ok(())
}
