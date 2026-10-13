//! Direct material/visibility queries -> semantic glyphs. No RGB image input.
use crate::{
    camera::{Camera, PreparedCamera},
    core::*,
};
use glam::Vec3;
use rayon::prelude::*;

// Surface ID, glyph, illumination anchor, color, source material, structure key.
type HistoryCell = (u32, u32, f32, [f32; 4], u32, u32);

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
            history: (0, 32, 0., [0., 0., 0., 1.], 0, 0),
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
        let history_key =
            self.scene
                .glyph_history_key(source.material, source.local, source.normal);
        // Lighting hysteresis must not hide authored region/material changes.
        // Matching statistics remain ID-only for compatibility with prior runs.
        let mut anchor = illumination;
        if let Some(&(previous_id, old, old_i, old_color, old_material, old_key)) = previous {
            if previous_id == id
                && old_material == source.material
                && old_key == history_key
                && old != 32
                && glyph != 32
                && (illumination - old_i).abs() < 0.022
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
        result.history = (id, glyph, anchor, color, source.material, history_key);
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
    use crate::camera::Mode;

    const TEST_FOLD: u32 = 21;
    const TEST_SURFACE_A: u32 = 22;
    const TEST_SURFACE_B: u32 = 23;

    // Deliberately tiny independent vocabulary: tests exercise the producer
    // contract rather than importing a client's material implementation.
    fn synthetic_glyph(material: u32, local: Vec3, normal: Vec3, illumination: f32) -> u8 {
        match material {
            TEST_FOLD if local.x < 0.53 && local.y > 0.45 => b'|',
            TEST_FOLD if normal.x * normal.z > 0.18 => b')',
            TEST_FOLD if illumination > 0.12 => b':',
            TEST_FOLD => b',',
            TEST_SURFACE_A => b':',
            TEST_SURFACE_B => b'-',
            WATER => b'~',
            EMISSIVE => b'*',
            _ => b'#',
        }
    }

    struct SyntheticScene;
    impl Scene for SyntheticScene {
        fn trace(&self, ray: Ray, time: f32, stage: Stage, include_water: bool) -> Option<Hit> {
            let mut nearest: Option<Hit> = None;
            let mut consider = |distance: f32, normal: Vec3, material, object_id, local| {
                if nearest.is_none_or(|old| distance < old.distance) {
                    nearest = Some(Hit {
                        distance,
                        position: ray.origin + ray.dir * distance,
                        normal,
                        material,
                        object_id,
                        local,
                    });
                }
            };
            // Sphere + lamp + ground/pool exercise hits, misses, shadow tests,
            // reflections and moving local coordinates in every camera mode.
            let center = Vec3::new((time * 0.7).sin() * 0.15, 1., 0.);
            for (position, radii, material, id) in [
                (center, Vec3::splat(0.85), TEST_FOLD, 41),
                (Vec3::new(-1.2, 2., -0.8), Vec3::splat(0.2), EMISSIVE, 42),
            ] {
                if let Some((distance, normal)) = intersect_ellipsoid(ray, position, radii) {
                    consider(
                        distance,
                        normal,
                        material,
                        id,
                        (ray.origin + ray.dir * distance - position) / radii,
                    );
                }
            }
            if ray.dir.y.abs() > 1e-8 {
                let ground = -ray.origin.y / ray.dir.y;
                if ground > 0.0001 {
                    consider(
                        ground,
                        Vec3::Y,
                        TEST_SURFACE_A,
                        43,
                        ray.origin + ray.dir * ground,
                    );
                }
                if include_water && matches!(stage, Stage::Water | Stage::Cloth) {
                    let distance = (0.035 - ray.origin.y) / ray.dir.y;
                    let p = ray.origin + ray.dir * distance;
                    if distance > 0.0001 && p.x.abs() < 1.5 && p.z.abs() < 2. {
                        consider(distance, Vec3::Y, WATER, 44, p);
                    }
                }
            }
            nearest
        }
        fn light_position(&self, time: f32) -> Vec3 {
            Vec3::new(-1.2 + time.sin() * 0.1, 2., -0.8)
        }
        fn pigment(&self, material: u32) -> Vec3 {
            if material == TEST_SURFACE_A {
                Vec3::new(0.25, 0.4, 0.3)
            } else {
                Vec3::new(0.5, 0.35, 0.2)
            }
        }
        fn glyph(&self, material: u32, local: Vec3, normal: Vec3, illumination: f32) -> u8 {
            synthetic_glyph(material, local, normal, illumination)
        }
        fn glyph_history_key(&self, material: u32, local: Vec3, normal: Vec3) -> u32 {
            synthetic_glyph(material, local, normal, 1.) as u32
        }
    }

    #[derive(Clone, Copy)]
    struct HistoryProbe {
        local: Vec3,
        normal: Vec3,
        material: u32,
        brightness: f32,
        structural: bool,
    }
    impl Scene for HistoryProbe {
        fn trace(&self, _: Ray, _: f32, _: Stage, _: bool) -> Option<Hit> {
            Some(Hit {
                distance: 1.,
                position: Vec3::ZERO,
                normal: self.normal,
                material: self.material,
                object_id: 130,
                local: self.local,
            })
        }
        fn light_position(&self, _: f32) -> Vec3 {
            Vec3::Y
        }
        fn pigment(&self, _: u32) -> Vec3 {
            Vec3::splat(self.brightness)
        }
        fn glyph(&self, m: u32, l: Vec3, n: Vec3, i: f32) -> u8 {
            synthetic_glyph(m, l, n, i)
        }
        fn glyph_history_key(&self, m: u32, l: Vec3, n: Vec3) -> u32 {
            if self.structural {
                SyntheticScene.glyph_history_key(m, l, n)
            } else {
                0
            }
        }
    }

    fn probe_frame(composer: &mut Composer, scene: &HistoryProbe) -> (GlyphCell, FrameStats) {
        let (cells, stats) =
            composer.compose(scene, Camera::new(Mode::Third), 0., Stage::Core, 1, 1, 1.);
        (cells[0], stats)
    }

    #[test]
    fn history_follows_normal_seam_and_material_changes_inside_one_surface_bin() {
        let base = HistoryProbe {
            local: Vec3::new(0.54, 0.60, 0.),
            normal: Vec3::Z,
            material: TEST_FOLD,
            brightness: 0.2,
            structural: true,
        };
        let cases = [
            // Fold orientation changes while UV/surface ID stays fixed.
            (
                base,
                HistoryProbe {
                    normal: Vec3::new(0.7, 0., 0.714).normalize(),
                    ..base
                },
            ),
            // Both UVs floor to x-bin 6, but only .52 belongs to the seam.
            (
                base,
                HistoryProbe {
                    local: Vec3::new(0.52, 0.60, 0.),
                    ..base
                },
            ),
            // Material guard must work independently of the opt-in key.
            (
                HistoryProbe {
                    material: TEST_SURFACE_A,
                    structural: false,
                    ..base
                },
                HistoryProbe {
                    material: TEST_SURFACE_B,
                    structural: false,
                    ..base
                },
            ),
        ];
        for (before, after) in cases {
            let mut composer = Composer::default();
            let (old, _) = probe_frame(&mut composer, &before);
            let previous_anchor = composer.previous_cells[0].2;
            let (new, stats) = probe_frame(&mut composer, &after);
            let (fresh, _) = probe_frame(&mut Composer::default(), &after);
            assert_eq!(
                old.id, new.id,
                "test must remain inside the same surface bin"
            );
            assert!((composer.previous_cells[0].2 - previous_anchor).abs() < 0.022);
            assert_ne!(
                old.glyph, fresh.glyph,
                "test must require a structural glyph change"
            );
            assert_ne!(old.glyph, 32);
            assert_ne!(new.glyph, 32);
            assert_eq!(
                new, fresh,
                "history may not suppress structure/material changes"
            );
            assert_eq!(stats.matched, 1, "matching statistics remain ID-only");
            assert_eq!(stats.glyph_changes, 1);
        }
    }

    #[test]
    fn history_still_holds_small_lighting_changes_within_a_structural_region() {
        let before = HistoryProbe {
            local: Vec3::new(0.54, 0.60, 0.),
            normal: Vec3::Z,
            material: TEST_FOLD,
            brightness: 0.66,
            structural: true,
        };
        let after = HistoryProbe {
            brightness: 0.74,
            ..before
        };
        let mut composer = Composer::default();
        let (first, _) = probe_frame(&mut composer, &before);
        let (frozen, _) = probe_frame(&mut composer, &before);
        assert_eq!(first, frozen);
        let anchor = composer.previous_cells[0].2;
        let (held, stats) = probe_frame(&mut composer, &after);
        let (fresh, _) = probe_frame(&mut Composer::default(), &after);
        assert_eq!(first.glyph, b',' as u32);
        assert_eq!(fresh.glyph, b':' as u32);
        assert_eq!(held.glyph, first.glyph);
        assert_eq!(composer.previous_cells[0].2.to_bits(), anchor.to_bits());
        assert_eq!(stats.matched, 1);
        assert_eq!(stats.glyph_changes, 0);
        composer.reset();
        assert_eq!(probe_frame(&mut composer, &after).0, fresh);
    }

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
            assert_eq!((a.4, a.5), (b.4, b.5));
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
            &SyntheticScene,
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
                        serial.compose(&SyntheticScene, camera, time, stage, cols, rows, 16. / 9.);
                    for p in &mut parallel {
                        let (actual, actual_stats) =
                            p.compose(&SyntheticScene, camera, time, stage, cols, rows, 16. / 9.);
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
            let (x, _) = a.compose(&SyntheticScene, c, 1., Stage::Cloth, 40, 24, 16. / 9.);
            let (y, _) = b.compose(&SyntheticScene, c, 1., Stage::Cloth, 40, 24, 16. / 9.);
            assert_eq!(x, y);
            assert!(x.iter().all(|c| (32..=126).contains(&c.glyph)));
            assert!(x.iter().any(|c| c.glyph != 32));
            let (z, _) = a.compose(&SyntheticScene, c, 1., Stage::Cloth, 40, 24, 16. / 9.);
            assert!(x == z, "history must not change a frozen image");
        }
    }
}
