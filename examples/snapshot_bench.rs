//! Controlled CPU-only comparison: frozen R2 query code vs prepared R3 state.
//! No GPU synchronization; do not interpret these numbers as application FPS.
pub use ascii_engine_lab::core;
#[allow(dead_code)]
#[path = "../evidence/r3/baseline/src/actor.rs"]
mod actor;
#[allow(dead_code)]
#[path = "../evidence/r3/baseline/src/world.rs"]
mod world;

use ascii_engine_lab::{
    camera::{Camera, Mode},
    core::{Hit, Ray, Scene, Stage},
    frame::Composer,
    LabScene,
};
use glam::Vec3;
use serde_json::json;
use std::{
    hint::black_box,
    sync::atomic::{AtomicUsize, Ordering},
    time::Instant,
};
struct Reference;
impl Scene for Reference {
    fn trace(&self, r: Ray, t: f32, s: Stage, w: bool) -> Option<Hit> {
        world::trace(r, t, s, w)
    }
    fn light_position(&self, t: f32) -> Vec3 {
        world::light_position(t)
    }
    fn pigment(&self, m: u32) -> Vec3 {
        world::pigment(m)
    }
    fn glyph(&self, m: u32, l: Vec3, n: Vec3, i: f32) -> u8 {
        world::glyph(m, l, n, i)
    }
}
struct Counted {
    primary: AtomicUsize,
    secondary: AtomicUsize,
}
impl Scene for Counted {
    fn trace(&self, r: Ray, t: f32, s: Stage, w: bool) -> Option<Hit> {
        if w { &self.primary } else { &self.secondary }.fetch_add(1, Ordering::Relaxed);
        world::trace(r, t, s, w)
    }
    fn light_position(&self, t: f32) -> Vec3 {
        world::light_position(t)
    }
    fn pigment(&self, m: u32) -> Vec3 {
        world::pigment(m)
    }
    fn glyph(&self, m: u32, l: Vec3, n: Vec3, i: f32) -> u8 {
        world::glyph(m, l, n, i)
    }
}
fn quantile(values: &[f64], q: f64) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    sorted[((sorted.len() - 1) as f64 * q).round() as usize]
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let paired = std::env::args().any(|arg| arg == "--paired");
    let output = std::path::Path::new(if paired {
        "evidence/r3/cpu-paired"
    } else {
        "evidence/r3/cpu"
    });
    std::fs::create_dir_all(output)?;
    let available = std::thread::available_parallelism().map_or(1, |n| n.get());
    let mut budgets = vec![1, available.min(8)];
    budgets.dedup();
    let mut runs = Vec::new();
    let mut queries = Vec::new();
    for mode in [Mode::First, Mode::Third, Mode::Overhead] {
        let counted = Counted {
            primary: AtomicUsize::new(0),
            secondary: AtomicUsize::new(0),
        };
        black_box(Composer::default().compose(
            &counted,
            Camera::new(mode),
            1.,
            Stage::Cloth,
            240,
            135,
            16. / 9.,
        ));
        queries.push(json!({"camera":mode.name(),"time":1.,"primary_queries":counted.primary.load(Ordering::Relaxed),"secondary_queries":counted.secondary.load(Ordering::Relaxed),"scope":"separate untimed R2 traversal; secondary combines shadow and reflected rays"}));
    }
    for repetition in 0..3 {
        for workers in &budgets {
            for mode in [Mode::First, Mode::Third, Mode::Overhead] {
                if paired {
                    // Two private histories/pools, only one active at a time. Alternating
                    // within a frame limits drift from desktop load between long blocks.
                    let mut old = Composer::with_threads(*workers)?;
                    let mut new = Composer::with_threads(*workers)?;
                    let mut samples: [Vec<f64>; 2] = [Vec::new(), Vec::new()];
                    let mut csv = [
                        String::from("frame,time_s,cpu_ms\n"),
                        String::from("frame,time_s,cpu_ms\n"),
                    ];
                    for i in 0..132 {
                        let time = i as f32 / 60.;
                        let order = if (i + repetition) % 2 == 0 {
                            [0, 1]
                        } else {
                            [1, 0]
                        };
                        for variant in order {
                            let begin = Instant::now();
                            let result = if variant == 1 {
                                new.compose(
                                    &LabScene::snapshot(time, Stage::Cloth),
                                    Camera::new(mode),
                                    time,
                                    Stage::Cloth,
                                    240,
                                    135,
                                    16. / 9.,
                                )
                            } else {
                                old.compose(
                                    &Reference,
                                    Camera::new(mode),
                                    time,
                                    Stage::Cloth,
                                    240,
                                    135,
                                    16. / 9.,
                                )
                            };
                            black_box(result);
                            let ms = begin.elapsed().as_secs_f64() * 1000.;
                            if i >= 12 {
                                samples[variant].push(ms);
                                csv[variant].push_str(&format!("{},{time:.6},{ms:.6}\n", i - 12));
                            }
                        }
                    }
                    for (variant, label) in ["r2_reference", "snapshot"].iter().enumerate() {
                        let values = &samples[variant];
                        std::fs::write(
                            output.join(format!(
                                "{label}-w{workers}-{}-r{}.csv",
                                mode.name(),
                                repetition + 1
                            )),
                            &csv[variant],
                        )?;
                        let p50 = quantile(values, 0.5);
                        let p95 = quantile(values, 0.95);
                        println!("paired {label} {} w{workers} repeat {}: CPU p50 {p50:.2} ms p95 {p95:.2} ms", mode.name(), repetition + 1);
                        runs.push(json!({"variant":label,"camera":mode.name(),"workers":workers,"repetition":repetition+1,"p50_ms":p50,"p95_ms":p95,"p99_ms":quantile(values,0.99)}));
                    }
                    continue;
                }
                let order = if repetition % 2 == 0 {
                    [false, true]
                } else {
                    [true, false]
                };
                for prepared in order {
                    let label = if prepared { "snapshot" } else { "r2_reference" };
                    let mut composer = Composer::with_threads(*workers)?;
                    let mut samples = Vec::new();
                    let mut csv = String::from("frame,time_s,cpu_ms\n");
                    for i in 0..132 {
                        let time = i as f32 / 60.;
                        let begin = Instant::now();
                        let result = if prepared {
                            composer.compose(
                                &LabScene::snapshot(time, Stage::Cloth),
                                Camera::new(mode),
                                time,
                                Stage::Cloth,
                                240,
                                135,
                                16. / 9.,
                            )
                        } else {
                            composer.compose(
                                &Reference,
                                Camera::new(mode),
                                time,
                                Stage::Cloth,
                                240,
                                135,
                                16. / 9.,
                            )
                        };
                        black_box(result);
                        let ms = begin.elapsed().as_secs_f64() * 1000.;
                        if i >= 12 {
                            samples.push(ms);
                            csv.push_str(&format!("{},{time:.6},{ms:.6}\n", i - 12));
                        }
                    }
                    std::fs::write(
                        output.join(format!(
                            "{label}-w{workers}-{}-r{}.csv",
                            mode.name(),
                            repetition + 1
                        )),
                        csv,
                    )?;
                    let p50 = quantile(&samples, 0.5);
                    let p95 = quantile(&samples, 0.95);
                    println!(
                        "{label} {} w{workers} repeat {}: CPU p50 {p50:.2} ms p95 {p95:.2} ms",
                        mode.name(),
                        repetition + 1
                    );
                    runs.push(json!({"variant":label,"camera":mode.name(),"workers":workers,"repetition":repetition+1,"p50_ms":p50,"p95_ms":p95,"p99_ms":quantile(&samples,0.99)}));
                }
            }
        }
    }
    let result = json!({"order":if paired {"per-frame paired, variant order alternates every frame and repetition; two private pools, only one composing at a time"} else {"blocked variants, order alternates per repetition"},"build_profile":if cfg!(debug_assertions){"debug"}else{"release"},"os":std::env::consts::OS,"arch":std::env::consts::ARCH,"available_parallelism_estimate":available,"grid":[240,135],"aspect":16./9.,"stage":"cloth","frames_per_run":120,"warmup":12,"simulation_dt":1./60.,"repetitions":3,"queries":queries,"runs":runs,"limits":["CPU only, no GPU or present/FPS timing","Includes snapshot preparation, composition and returned buffer disposal; both paths use unchanged R2 compositor","Frozen R2 world/actor source recompiled in same executable/toolchain, not old executable timing","Local desktop, not isolated load; no other CPU/GPU validated","Worker limits are not emulated low-end hardware"]});
    std::fs::write(
        output.join("benchmark.json"),
        serde_json::to_string_pretty(&result)?,
    )?;
    Ok(())
}
