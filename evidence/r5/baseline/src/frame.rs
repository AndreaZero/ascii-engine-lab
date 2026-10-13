//! Direct material/visibility queries -> semantic glyphs. No RGB image input.
use crate::{
    camera::{Camera, PreparedCamera},
    core::*,
};
use glam::Vec3;
use rayon::prelude::*;

type HistoryCell = (u32, u32, f32, [f32; 4]);

/// Each worker owns its output slot; history is read-only until all slots finish.
/// Statistics are intentionally not reduced by Rayon: floating-point addition
/// follows the original row-major order in `compose`.
#[derive(Clone, Copy)]
struct CellResult {
    cell: GlyphCell,
    history: HistoryCell,
    reflected: bool,
    matched: bool,
    glyph_changed: bool,
    color_delta: f32,
}

impl Default for CellResult {
    fn default() -> Self {
        Self {
            cell: GlyphCell::default(),
            history: (0, 32, 0., [0., 0., 0., 1.]),
            reflected: false,
            matched: false,
            glyph_changed: false,
            color_delta: 0.,
        }
    }
}

#[derive(Default, Debug, Clone)]
pub struct FrameStats {
    pub visible: usize,
    pub reflected: usize,
    pub matched: usize,
    pub glyph_changes: usize,
    pub mean_color_delta: f32,
}
#[derive(Default)]
pub struct Composer {
    previous_cells: Vec<HistoryCell>,
    scratch: Vec<CellResult>,
    // None means truly serial execution on the caller, without a Rayon pool.
    pool: Option<rayon::ThreadPool>,
}
impl Composer {
    /// Creates one private persistent pool for >=2 workers. Zero, like one,
    /// selects the serial path rather than Rayon's automatic/global pool.
    pub fn with_threads(threads: usize) -> Result<Self, rayon::ThreadPoolBuildError> {
        let pool = if threads >= 2 {
            Some(
                rayon::ThreadPoolBuilder::new()
                    .num_threads(threads)
                    .thread_name(|index| format!("ascii-compose-{index}"))
                    .build()?,
            )
        } else {
            None
        };
        Ok(Self {
            pool,
            ..Self::default()
        })
    }

    pub fn threads(&self) -> usize {
        self.pool
            .as_ref()
            .map_or(1, rayon::ThreadPool::current_num_threads)
    }

    pub fn reset(&mut self) {
        self.previous_cells.clear();
    }
    // Explicit prototype inputs; no global window/fixture coupling in this API.
    #[allow(clippy::too_many_arguments)]
    pub fn compose(
        &mut self,
        scene: &(impl Scene + Sync),
        camera: Camera,
        time: f32,
        stage: Stage,
        cols: u32,
        rows: u32,
        aspect: f32,
    ) -> (Vec<GlyphCell>, FrameStats) {
        let count = (cols * rows) as usize;
        let prepared = PreparedFrame {
            scene,
            camera: camera.prepare(cols, rows, aspect),
            lighting: PreparedLighting {
                moon: Vec3::new(-0.5, 0.85, 0.3).normalize(),
                light: if stage == Stage::Core {
                    Vec3::ZERO
                } else {
                    scene.light_position(time)
                },
            },
            time,
            stage,
            cols,
        };
        self.scratch.resize(count, CellResult::default());
        let previous = &self.previous_cells;
        let scratch = &mut self.scratch;
        if let Some(pool) = &self.pool {
            pool.install(|| {
                // Contiguous jobs amortize scheduling; the last odd-sized block
                // is handled by enumerate just like every other output cell.
                scratch
                    .par_iter_mut()
                    .enumerate()
                    .with_min_len(64)
                    .for_each(|(index, result)| {
                        *result = prepared.cell(index, previous.get(index));
                    });
            });
        } else {
            for (index, result) in scratch.iter_mut().enumerate() {
                *result = prepared.cell(index, previous.get(index));
            }
        }
        let mut cells = Vec::with_capacity(count);
        let mut stats = FrameStats::default();
        self.previous_cells.clear();
        self.previous_cells.reserve(count);
        for result in &self.scratch {
            if result.reflected {
                stats.reflected += 1;
            }
            if result.matched {
                stats.matched += 1;
                if result.glyph_changed {
                    stats.glyph_changes += 1;
                }
                stats.mean_color_delta += result.color_delta;
            }
            if result.cell.glyph != 32 {
                stats.visible += 1;
            }
            self.previous_cells.push(result.history);
            cells.push(result.cell);
        }
        if stats.matched > 0 {
            stats.mean_color_delta /= stats.matched as f32;
        }
        (cells, stats)
    }
}

struct PreparedLighting {
    moon: Vec3,
    light: Vec3,
}

struct PreparedFrame<'a, S: Scene + Sync> {
    scene: &'a S,
    camera: PreparedCamera,
    lighting: PreparedLighting,
    time: f32,
    stage: Stage,
    cols: u32,
}

impl<S: Scene + Sync> PreparedFrame<'_, S> {
    fn cell(&self, index: usize, previous: Option<&HistoryCell>) -> CellResult {
        let x = (index % self.cols as usize) as u32;
        let y = (index / self.cols as usize) as u32;
        let ray = self.camera.ray(x, y);
        let Some(hit) = self.scene.trace(ray, self.time, self.stage, true) else {
            return CellResult::default();
        };
        let mut result = CellResult::default();
        let mut id = surface_id(hit);
        let mut color = shade(self.scene, hit, self.time, self.stage, &self.lighting);
        let mut source = hit;
        if hit.material == WATER {
            let nx = (hit.position.x * 3.2 + self.time * 1.4).cos() * 0.055;
            let nz = (hit.position.z * 4.1 - self.time * 1.1).sin() * 0.045;
            let n = Vec3::new(nx, 1., nz).normalize();
            let reflected = Ray {
                origin: hit.position + Vec3::Y * 0.012,
                dir: ray.dir - 2. * ray.dir.dot(n) * n,
            };
            if let Some(r) = self.scene.trace(reflected, self.time, self.stage, false) {
                let fresnel = 0.28 + 0.52 * (1. - (-ray.dir.dot(n)).clamp(0., 1.)).powi(3);
                color = shade(self.scene, r, self.time, self.stage, &self.lighting)
                    * Vec3::new(0.58, 0.76, 0.85)
                    * fresnel
                    + color * 0.15;
                source = r;
                id = surface_id(r) ^ 0x80000000;
                result.reflected = true;
            } else {
                color *= 0.5;
            }
        }
        let illumination = color.max_element();
        let mut glyph = self
            .scene
            .glyph(source.material, source.local, source.normal, illumination)
            as u32;
        if illumination < 0.012 {
            glyph = 32;
        }
        let fog = (-hit.distance * 0.026).exp();
        color = (color * fog).min(Vec3::ONE);
        // Same-cell history/hold rules as R1, with no shared mutable worker state.
        let mut anchor = illumination;
        if let Some(&(previous_id, old, old_i, old_color)) = previous {
            if previous_id == id && old != 32 && glyph != 32 && (illumination - old_i).abs() < 0.022
            {
                glyph = old;
                anchor = old_i;
            }
            if previous_id == id {
                result.matched = true;
                result.glyph_changed = old != glyph;
                result.color_delta =
                    (0..3).map(|c| (color[c] - old_color[c]).abs()).sum::<f32>() / 3.;
            }
        }
        let color = [color.x.min(1.), color.y.min(1.), color.z.min(1.), 1.];
        result.cell = GlyphCell {
            glyph,
            flags: if hit.material == WATER { 1 } else { 0 },
            id,
            pad: 0,
            color,
        };
        result.history = (id, glyph, anchor, color);
        result
    }
}

fn shade(
    scene: &impl Scene,
    hit: Hit,
    time: f32,
    stage: Stage,
    lighting: &PreparedLighting,
) -> Vec3 {
    if hit.material == EMISSIVE {
        return Vec3::new(1., 0.55, 0.12);
    }
    let base = scene.pigment(hit.material);
    let ambient =
        Vec3::new(0.16, 0.23, 0.31) * (0.13 + 0.36 * hit.normal.dot(lighting.moon).max(0.));
    if stage == Stage::Core {
        return base * (ambient + Vec3::splat(0.1));
    }
    let to = lighting.light - hit.position;
    let dist = to.length();
    let dir = to / dist;
    let diffuse = hit.normal.dot(dir).max(0.);
    let blocked = diffuse > 0.
        && scene.occluded(
            Ray {
                origin: hit.position + hit.normal * 0.012,
                dir,
            },
            time,
            stage,
            dist - 0.09,
        );
    let response = if blocked {
        0.
    } else {
        diffuse * 4.5 / (1. + dist * dist * 0.45)
    };
    base * (ambient + Vec3::new(1., 0.58, 0.23) * response)
}
pub fn surface_id(hit: Hit) -> u32 {
    let q = (hit.local * 12.).floor().as_ivec3();
    let mut id = hit.object_id.wrapping_mul(16777619);
    for v in [q.x, q.y, q.z] {
        id = (id ^ (v as u32)).wrapping_mul(16777619);
    }
    id.max(1)
}
pub fn overlay(cells: &mut [GlyphCell], cols: u32, rows: u32, text: &str) {
    if rows < 2 {
        return;
    }
    for (i, ch) in text
        .bytes()
        .take(cols.saturating_sub(2) as usize)
        .enumerate()
    {
        cells[(cols + 1) as usize + i] = GlyphCell {
            glyph: ch as u32,
            flags: 2,
            id: 0,
            pad: 0,
            color: [0.5, 0.61, 0.64, 1.],
        };
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{camera::Mode, LabScene};

    fn assert_stats_bits(actual: &FrameStats, expected: &FrameStats) {
        assert_eq!(actual.visible, expected.visible);
        assert_eq!(actual.reflected, expected.reflected);
        assert_eq!(actual.matched, expected.matched);
        assert_eq!(actual.glyph_changes, expected.glyph_changes);
        assert_eq!(
            actual.mean_color_delta.to_bits(),
            expected.mean_color_delta.to_bits()
        );
    }

    fn assert_history_bits(actual: &Composer, expected: &Composer) {
        assert_eq!(actual.previous_cells.len(), expected.previous_cells.len());
        for (a, b) in actual.previous_cells.iter().zip(&expected.previous_cells) {
            assert_eq!((a.0, a.1, a.2.to_bits()), (b.0, b.1, b.2.to_bits()));
            assert_eq!(a.3.map(f32::to_bits), b.3.map(f32::to_bits));
        }
    }

    #[test]
    fn worker_configuration_and_reset_keep_private_pool() {
        for threads in [0, 1] {
            let c = Composer::with_threads(threads).unwrap();
            assert_eq!(c.threads(), 1);
            assert!(c.pool.is_none());
        }
        let c = Composer::default();
        assert_eq!(c.threads(), 1);
        assert!(c.pool.is_none());
        let mut parallel = Composer::with_threads(2).unwrap();
        let pool_address = parallel.pool.as_ref().unwrap() as *const rayon::ThreadPool;
        parallel.compose(
            &LabScene,
            Camera::new(Mode::Third),
            1.,
            Stage::Cloth,
            17,
            11,
            16. / 9.,
        );
        let capacity = parallel.scratch.capacity();
        parallel.reset();
        assert!(parallel.previous_cells.is_empty());
        assert_eq!(parallel.threads(), 2);
        assert_eq!(
            parallel.pool.as_ref().unwrap() as *const rayon::ThreadPool,
            pool_address
        );
        assert_eq!(parallel.scratch.capacity(), capacity);
    }

    #[test]
    fn serial_and_2_4_8_workers_match_cells_stats_and_history_bitwise() {
        let mut serial = Composer::with_threads(1).unwrap();
        let mut parallel: Vec<_> = [2, 4, 8]
            .into_iter()
            .map(|threads| Composer::with_threads(threads).unwrap())
            .collect();
        // Repeated frozen input, moving time, pan, grid shrink/grow and explicit
        // reset all exercise history and non-divisible parallel work lengths.
        let sequence = [
            (31, 19, 1., 0., false),
            (31, 19, 1., 0., false),
            (31, 19, 1. + 1. / 60., 0.004, false),
            (17, 11, 2.75, -0.17, false),
            (31, 19, 2.75, -0.17, false),
            (31, 19, 2.75, -0.17, false),
            (17, 11, 0., 0., true),
            (17, 11, 0., 0., false),
        ];
        for mode in [Mode::First, Mode::Third, Mode::Overhead] {
            for stage in [Stage::Core, Stage::Light, Stage::Water, Stage::Cloth] {
                serial.reset();
                for p in &mut parallel {
                    p.reset();
                }
                for (cols, rows, time, orbit, reset) in sequence {
                    if reset {
                        serial.reset();
                        for p in &mut parallel {
                            p.reset();
                        }
                    }
                    let mut camera = Camera::new(mode);
                    camera.orbit = orbit;
                    camera.offset = Vec3::new(orbit * 0.3, 0., orbit * -0.2);
                    let (expected, expected_stats) =
                        serial.compose(&LabScene, camera, time, stage, cols, rows, 16. / 9.);
                    for p in &mut parallel {
                        let (actual, actual_stats) =
                            p.compose(&LabScene, camera, time, stage, cols, rows, 16. / 9.);
                        assert_eq!(
                            bytemuck::cast_slice::<GlyphCell, u8>(&actual),
                            bytemuck::cast_slice::<GlyphCell, u8>(&expected),
                            "mode={mode:?}, stage={stage:?}, time={time}, workers={}",
                            p.threads()
                        );
                        assert_stats_bits(&actual_stats, &expected_stats);
                        assert_history_bits(p, &serial);
                    }
                }
            }
        }
    }
    #[test]
    fn frozen_scene_deterministic_and_printable() {
        for m in [Mode::First, Mode::Third, Mode::Overhead] {
            let mut a = Composer::default();
            let mut b = Composer::default();
            let c = Camera::new(m);
            let (x, _) = a.compose(&LabScene, c, 1., Stage::Cloth, 40, 24, 16. / 9.);
            let (y, _) = b.compose(&LabScene, c, 1., Stage::Cloth, 40, 24, 16. / 9.);
            assert_eq!(x, y);
            assert!(x.iter().all(|c| (32..=126).contains(&c.glyph)));
            assert!(x.iter().any(|c| c.glyph != 32));
            let (z, _) = a.compose(&LabScene, c, 1., Stage::Cloth, 40, 24, 16. / 9.);
            assert!(x == z, "history must not change a frozen image");
        }
    }
}
