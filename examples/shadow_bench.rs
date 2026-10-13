//! Paired CPU comparison: default nearest-hit shadows vs specialized occlusion.
use ascii_engine_lab::{
    camera::{Camera, Mode},
    core::{Hit, Ray, Scene, Stage},
    frame::Composer,
    world::WorldSnapshot,
    LabScene,
};
use glam::Vec3;
use serde_json::json;
use std::{hint::black_box, time::Instant};

// Deliberately retains Scene's compatible nearest-hit default for occluded.
struct Nearest(WorldSnapshot);
impl Scene for Nearest {
    fn trace(&self, r: Ray, t: f32, s: Stage, w: bool) -> Option<Hit> {
        Scene::trace(&self.0, r, t, s, w)
    }
    fn light_position(&self, t: f32) -> Vec3 {
        Scene::light_position(&self.0, t)
    }
    fn pigment(&self, m: u32) -> Vec3 {
        Scene::pigment(&self.0, m)
    }
    fn glyph(&self, m: u32, l: Vec3, n: Vec3, i: f32) -> u8 {
        Scene::glyph(&self.0, m, l, n, i)
    }
    fn glyph_history_key(&self, m: u32, l: Vec3, n: Vec3) -> u32 {
        Scene::glyph_history_key(&self.0, m, l, n)
    }
}
fn quantile(values: &[f64], q: f64) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    sorted[((sorted.len() - 1) as f64 * q).round() as usize]
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::path::Path::new("evidence/r4/cpu-paired");
    std::fs::create_dir_all(output)?;
    let available = std::thread::available_parallelism().map_or(1, |n| n.get());
    let mut workers_list = vec![1, available.min(8)];
    workers_list.dedup();
    let mut runs = Vec::new();
    for repetition in 0..3 {
        for workers in &workers_list {
            for mode in [Mode::First, Mode::Third, Mode::Overhead] {
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
                        let snapshot = LabScene::snapshot(time, Stage::Cloth);
                        let result = if variant == 0 {
                            old.compose(
                                &Nearest(snapshot),
                                Camera::new(mode),
                                time,
                                Stage::Cloth,
                                240,
                                135,
                                16. / 9.,
                            )
                        } else {
                            new.compose(
                                &snapshot,
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
                for (variant, label) in ["nearest_shadow", "occlusion"].iter().enumerate() {
                    let v = &samples[variant];
                    std::fs::write(
                        output.join(format!(
                            "{label}-w{workers}-{}-r{}.csv",
                            mode.name(),
                            repetition + 1
                        )),
                        &csv[variant],
                    )?;
                    let p50 = quantile(v, 0.5);
                    let p95 = quantile(v, 0.95);
                    println!(
                        "{label} {} w{workers} repeat {}: p50 {p50:.2} ms p95 {p95:.2} ms",
                        mode.name(),
                        repetition + 1
                    );
                    runs.push(json!({"variant":label,"camera":mode.name(),"workers":workers,"repetition":repetition+1,"p50_ms":p50,"p95_ms":p95,"p99_ms":quantile(v,0.99)}));
                }
            }
        }
    }
    let report = json!({"os":std::env::consts::OS,"arch":std::env::consts::ARCH,"available_parallelism_estimate":available,"build_profile":if cfg!(debug_assertions){"debug"}else{"release"},"grid":[240,135],"aspect":16./9.,"stage":"cloth","repetitions":3,"warmup":12,"frames_per_variant":120,"dt":1./60.,"method":"Paired variants per frame; order alternates every frame/repetition. Independent private histories and pools; only one composing at a time. Both variants use the same current primary/reflection traversal and compositor, only Scene::occluded differs.","limits":["CPU only, not GPU or window FPS","Includes snapshot preparation, composition and result disposal; pool startup excluded","Local desktop is not isolated; thread count is not another processor","Default nearest-hit query preserves R3 shadow behavior; neither variant lowers quality"],"runs":runs});
    std::fs::write(
        output.join("benchmark.json"),
        serde_json::to_string_pretty(&report)?,
    )?;
    Ok(())
}
