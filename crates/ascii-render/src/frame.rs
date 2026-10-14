//! Direct material/visibility queries -> semantic glyphs. No RGB image input.
use crate::{
    camera::{Camera, PreparedCamera},
    core::*,
};
use glam::Vec3;
use rayon::prelude::*;

/// Fixed primary visibility samples. Center preserves the original renderer.
/// Conservative9 traces a regular 3x3 pattern and shades one semantic surface:
/// nearest positive finite forward depth, then object/material/sample index for
/// exact ties. The center is preferred when it belongs to the winning group.
/// This can widen silhouettes by a cell and still miss geometry between samples;
/// it is neither exact area coverage nor an RGB supersampling/averaging pass.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PrimarySampling {
    #[default]
    Center,
    Conservative9,
}
impl PrimarySampling {
    pub fn name(self) -> &'static str {
        match self {
            Self::Center => "center",
            Self::Conservative9 => "conservative9",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "center" => Some(Self::Center),
            "conservative9" => Some(Self::Conservative9),
            _ => None,
        }
    }
    fn rays(self) -> usize {
        match self {
            Self::Center => 1,
            Self::Conservative9 => 9,
        }
    }
}

// Center first, then row-major with the repeated center omitted. Offsets are
// relative to the cell center; the resulting samples are uniformly spaced.
const PRIMARY_OFFSETS: [[f32; 2]; 9] = [
    [0., 0.],
    [-1. / 3., -1. / 3.],
    [0., -1. / 3.],
    [1. / 3., -1. / 3.],
    [-1. / 3., 0.],
    [1. / 3., 0.],
    [-1. / 3., 1. / 3.],
    [0., 1. / 3.],
    [1. / 3., 1. / 3.],
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct PrimaryHistory {
    object: u32,
    material: u32,
    sample: u8,
    coverage_mask: u16,
}

// Surface ID, glyph, illumination anchor, color, source material, structure key,
// and exact primary sampling provenance (all zero for legacy Center semantics).
type HistoryCell = (u32, u32, f32, [f32; 4], u32, u32, PrimaryHistory);

#[derive(Clone, Copy)]
struct PrimaryHit {
    ray: Ray,
    hit: Hit,
    history: PrimaryHistory,
}

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
    alternate_primary: bool,
}

impl Default for CellResult {
    fn default() -> Self {
        Self {
            cell: GlyphCell::default(),
            history: (0, 32, 0., [0., 0., 0., 1.], 0, 0, PrimaryHistory::default()),
            reflected: false,
            matched: false,
            glyph_changed: false,
            color_delta: 0.,
            alternate_primary: false,
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
    /// Attempted primary visibility traces, excluding shadows/reflections.
    pub primary_rays: usize,
    /// Cells whose selected semantic hit used a non-center primary sample.
    pub alternate_primary: usize,
}
#[derive(Default)]
pub struct Composer {
    previous_cells: Vec<HistoryCell>,
    previous_epoch: u64,
    scratch: Vec<CellResult>,
    // None means truly serial execution on the caller, without a Rayon pool.
    pool: Option<rayon::ThreadPool>,
    primary_sampling: PrimarySampling,
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

    pub fn primary_sampling(&self) -> PrimarySampling {
        self.primary_sampling
    }

    /// Changing the visibility policy invalidates history; setting the same
    /// value keeps it. The worker pool and allocated buffers are retained.
    pub fn set_primary_sampling(&mut self, sampling: PrimarySampling) {
        if self.primary_sampling != sampling {
            self.primary_sampling = sampling;
            self.reset();
        }
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
        let epoch = scene.history_epoch();
        if epoch != self.previous_epoch {
            self.reset();
            self.previous_epoch = epoch;
        }
        let count = (cols * rows) as usize;
        let local_materials = scene.local_materials();
        let prepared = PreparedFrame {
            scene,
            camera: camera.prepare(cols, rows, aspect),
            lighting: PreparedLighting {
                color: if local_materials {
                    scene.light_color()
                } else {
                    Vec3::new(1., 0.58, 0.23)
                },
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
            local_materials,
            primary_sampling: self.primary_sampling,
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
        // Every cell attempts the entire fixed pattern, including empty cells.
        // No per-ray atomics or extra per-cell counters are needed in workers.
        let mut stats = FrameStats {
            primary_rays: count * self.primary_sampling.rays(),
            ..FrameStats::default()
        };
        self.previous_cells.clear();
        self.previous_cells.reserve(count);
        for result in &self.scratch {
            stats.alternate_primary += usize::from(result.alternate_primary);
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
    color: Vec3,
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
    local_materials: bool,
    primary_sampling: PrimarySampling,
}

impl<S: Scene + Sync> PreparedFrame<'_, S> {
    fn primary_hit(&self, x: u32, y: u32) -> Option<PrimaryHit> {
        if self.primary_sampling == PrimarySampling::Center {
            let ray = self.camera.ray(x, y);
            return self
                .scene
                .trace(ray, self.time, self.stage, true)
                .map(|hit| PrimaryHit {
                    ray,
                    hit,
                    history: PrimaryHistory::default(),
                });
        }
        let samples: [Option<(Ray, Hit, f32)>; 9] = std::array::from_fn(|index| {
            let ray = self.camera.ray_sample(x, y, PRIMARY_OFFSETS[index]);
            let hit = self.scene.trace(ray, self.time, self.stage, true)?;
            let depth = self.camera.forward_depth(hit.position);
            (depth.is_finite() && depth > 0. && hit.distance.is_finite() && hit.distance > 0.)
                .then_some((ray, hit, depth))
        });
        let (winner_index, &(_, winner, _)) = samples
            .iter()
            .enumerate()
            .filter_map(|(index, sample)| sample.as_ref().map(|sample| (index, sample)))
            .min_by(|(a_index, a), (b_index, b)| {
                a.2.total_cmp(&b.2)
                    .then_with(|| a.1.object_id.cmp(&b.1.object_id))
                    .then_with(|| a.1.material.cmp(&b.1.material))
                    .then_with(|| a_index.cmp(b_index))
            })?;
        let same_group =
            |hit: Hit| hit.object_id == winner.object_id && hit.material == winner.material;
        let index = if samples[0].is_some_and(|(_, hit, _)| same_group(hit)) {
            0
        } else {
            winner_index
        };
        let (ray, hit, _) = samples[index]?;
        let coverage_mask = samples.iter().enumerate().fold(0, |mask, (index, sample)| {
            if sample.is_some_and(|(_, hit, _)| same_group(hit)) {
                mask | (1 << index)
            } else {
                mask
            }
        });
        Some(PrimaryHit {
            ray,
            hit,
            history: PrimaryHistory {
                object: hit.object_id,
                material: hit.material,
                sample: index as u8,
                coverage_mask,
            },
        })
    }

    fn cell(&self, index: usize, previous: Option<&HistoryCell>) -> CellResult {
        let x = (index % self.cols as usize) as u32;
        let y = (index / self.cols as usize) as u32;
        let mut result = CellResult::default();
        let Some(primary) = self.primary_hit(x, y) else {
            return result;
        };
        let PrimaryHit {
            ray,
            hit,
            history: primary_history,
        } = primary;
        result.alternate_primary = primary_history.sample != 0;
        let mut id = surface_id(hit);
        let adjacent = self.local_materials.then(|| {
            if primary_history.sample == 0 {
                [self.camera.ray(x + 1, y), self.camera.ray(x, y + 1)]
            } else {
                let offset = PRIMARY_OFFSETS[primary_history.sample as usize];
                [
                    self.camera.ray_sample(x + 1, y, offset),
                    self.camera.ray_sample(x, y + 1, offset),
                ]
            }
        });
        let mut footprint = adjacent.map_or(0., |rays| surface_footprint(ray, rays, hit));
        let mut color = self.shade_hit(hit, -ray.dir, footprint);
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
                // Approximate reflected footprint by the camera cone at total
                // path length. It ignores curvature/focusing of the water.
                footprint = adjacent.map_or(0., |rays| {
                    cone_footprint(ray, rays, hit.distance + r.distance)
                });
                color = self.shade_hit(r, -reflected.dir, footprint)
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
        let mut glyph = if self.local_materials {
            self.scene.glyph_sample(source, illumination, footprint)
        } else {
            self.scene
                .glyph(source.material, source.local, source.normal, illumination)
        } as u32;
        if illumination < 0.012 {
            glyph = 32;
        }
        let fog = (-hit.distance * 0.026).exp();
        color = (color * fog).min(Vec3::ONE);
        let history_key = if self.local_materials {
            self.scene.glyph_sample_key(source, footprint)
        } else {
            self.scene
                .glyph_history_key(source.material, source.local, source.normal)
        };
        // Lighting hysteresis must not hide authored region/material changes.
        // Matching statistics remain ID-only for compatibility with prior runs.
        let mut anchor = illumination;
        if let Some(&(previous_id, old, old_i, old_color, old_material, old_key, old_primary)) =
            previous
        {
            if previous_id == id
                && old_material == source.material
                && old_key == history_key
                && old_primary == primary_history
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
        result.history = (
            id,
            glyph,
            anchor,
            color,
            source.material,
            history_key,
            primary_history,
        );
        result
    }

    fn shade_hit(&self, hit: Hit, view: Vec3, footprint: f32) -> Vec3 {
        if self.local_materials && hit.material != EMISSIVE {
            if let Some(sample) = self.scene.material_sample(hit, footprint) {
                return shade_material(
                    self.scene,
                    hit,
                    sample,
                    view,
                    footprint,
                    self.time,
                    self.stage,
                    &self.lighting,
                );
            }
        }
        shade(self.scene, hit, self.time, self.stage, &self.lighting)
    }
}

fn cone_footprint(ray: Ray, adjacent: [Ray; 2], distance: f32) -> f32 {
    let center = ray.origin + ray.dir * distance;
    adjacent
        .into_iter()
        .map(|r| (r.origin + r.dir * distance - center).length())
        .fold(0.0001_f32, f32::max)
}

fn surface_footprint(ray: Ray, adjacent: [Ray; 2], hit: Hit) -> f32 {
    let mut width = cone_footprint(ray, adjacent, hit.distance);
    for r in adjacent {
        let denominator = r.dir.dot(hit.normal);
        if denominator.abs() > 1e-5 {
            let t = (hit.position - r.origin).dot(hit.normal) / denominator;
            if t > 0. && t.is_finite() {
                let size = (r.origin + r.dir * t - hit.position).length();
                if size.is_finite() {
                    width = width.max(size);
                }
            }
        }
    }
    width
}

fn unit_value(value: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0., 1.)
    } else {
        fallback
    }
}

// GGX + Smith height-correlated visibility + Schlick + Lambert, following the
// primary Filament equations: https://google.github.io/filament/main/filament.html
// (Standard model summary; metallic remapping in section 4.8.3).
// Default scalar F0, authored cavity, legacy artistic light
// attenuation and a world-unit footprint roughness floor are simplifications;
// this is neither calibrated physical transport nor a full specular-AA filter.
#[allow(clippy::too_many_arguments)]
fn shade_material(
    scene: &impl Scene,
    hit: Hit,
    sample: MaterialSample,
    view: Vec3,
    footprint: f32,
    time: f32,
    stage: Stage,
    lighting: &PreparedLighting,
) -> Vec3 {
    shade_material_depth(
        scene, hit, sample, view, footprint, time, stage, lighting, true,
    )
}

#[allow(clippy::too_many_arguments)]
fn shade_material_depth(
    scene: &impl Scene,
    hit: Hit,
    sample: MaterialSample,
    view: Vec3,
    footprint: f32,
    time: f32,
    stage: Stage,
    lighting: &PreparedLighting,
    primary: bool,
) -> Vec3 {
    // Keep the dielectric arithmetic below untouched when the extension is
    // absent/zero. RGB Schlick is an approximation to conductor Fresnel, not
    // PBRT's spectral eta/k response. Environment is a separate opt-in path.
    let metallic = scene.metallic_sample(hit, footprint).and_then(|sample| {
        let metalness = unit_value(sample.metalness, 0.);
        (metalness > 0.).then_some(MetallicSample {
            metalness,
            ..sample
        })
    });
    let albedo = Vec3::from_array(sample.albedo.to_array().map(|v| unit_value(v, 0.)));
    let occlusion = unit_value(sample.occlusion, 1.);
    let mut n = sample.normal.normalize_or_zero();
    if !n.is_finite() || n.length_squared() < 0.5 {
        n = hit.normal;
    }
    if n.dot(hit.normal) < 0. {
        n = -n;
    }
    let ambient = albedo * (0.07 + 0.10 * n.dot(lighting.moon).max(0.)) * occlusion;
    let ambient = if let Some(metal) = metallic {
        ambient * (1. - metal.metalness)
    } else {
        ambient
    };
    let ambient = environment_ambient(
        scene, hit, sample, metallic, n, view, footprint, time, stage, lighting, primary,
    )
    .unwrap_or(ambient);
    if stage == Stage::Core {
        return ambient;
    }
    let to = lighting.light - hit.position;
    let distance = to.length();
    if !distance.is_finite() || distance < 0.0001 {
        return ambient;
    }
    let l = to / distance;
    let v = view.normalize_or_zero();
    let no_l = n.dot(l).max(0.);
    let no_v = n.dot(v).max(0.);
    // Decorative normals cannot illuminate the geometric back face or move a
    // shadow origin into the surface. Occlusion uses the original hit normal.
    if hit.normal.dot(l) <= 0. || no_l <= 0. || no_v <= 0. {
        return ambient;
    }
    if scene.occluded(
        Ray {
            origin: hit.position + hit.normal * 0.012,
            dir: l,
        },
        time,
        stage,
        distance - 0.09,
    ) {
        return ambient;
    }
    let h = (v + l).normalize_or_zero();
    let no_h = n.dot(h).clamp(0., 1.);
    let vo_h = v.dot(h).clamp(0., 1.);
    // Conservative heuristic at the laboratory's world scale, not normal
    // variance integration. Producers should also band-limit their relief.
    let floor = (0.16 + footprint.max(0.) * 2.).clamp(0.16, 0.65);
    let roughness = unit_value(sample.roughness, 1.).max(floor);
    let alpha = roughness * roughness;
    let a2 = alpha * alpha;
    let d_den = no_h * no_h * (a2 - 1.) + 1.;
    let distribution = a2 / (std::f32::consts::PI * d_den * d_den).max(1e-8);
    let visibility_den =
        no_l * (no_v * no_v * (1. - a2) + a2).sqrt() + no_v * (no_l * no_l * (1. - a2) + a2).sqrt();
    let visibility = 0.5 / visibility_den.max(1e-5);
    let (diffuse, specular) = if let Some(metal) = metallic {
        let dielectric_f0 = unit_value(sample.f0, 0.04);
        let conductor_f0 =
            Vec3::from_array(metal.f0.to_array().map(|v| unit_value(v, dielectric_f0)));
        let f0 =
            Vec3::splat(dielectric_f0) * (1. - metal.metalness) + conductor_f0 * metal.metalness;
        let fresnel = f0 + (Vec3::ONE - f0) * (1. - vo_h).powi(5);
        let specular = (fresnel * (distribution * visibility)).min(Vec3::splat(64.));
        let diffuse =
            albedo * (Vec3::ONE - fresnel) * ((1. - metal.metalness) / std::f32::consts::PI);
        (diffuse, specular)
    } else {
        let f0 = unit_value(sample.f0, 0.04);
        let fresnel = f0 + (1. - f0) * (1. - vo_h).powi(5);
        let specular = (distribution * visibility * fresnel).min(64.);
        let diffuse = albedo * ((1. - fresnel) / std::f32::consts::PI);
        (diffuse, Vec3::splat(specular))
    };
    let light = Vec3::from_array(lighting.color.to_array().map(|v| {
        if v.is_finite() {
            v.clamp(0., 16.)
        } else {
            0.
        }
    }));
    let attenuation = 4.5 / (1. + distance * distance * 0.45);
    ambient + (diffuse + specular) * light * (no_l * attenuation * occlusion)
}

fn environment_color(color: Vec3) -> Vec3 {
    Vec3::from_array(
        color
            .to_array()
            .map(|v| if v.is_finite() { v.clamp(0., 16.) } else { 0. }),
    )
}

// The diffuse E/PI and Fresnel material split follow the indirect-lighting
// distinction in https://google.github.io/filament/main/filament.html.
// This deliberately bounded approximation is NOT Filament's split-sum IBL:
// no integrated DFG, GGX convolution, multi-scattering or hemispherical AO.
#[allow(clippy::too_many_arguments)]
fn environment_ambient(
    scene: &impl Scene,
    hit: Hit,
    sample: MaterialSample,
    metallic: Option<MetallicSample>,
    normal: Vec3,
    view: Vec3,
    footprint: f32,
    time: f32,
    stage: Stage,
    lighting: &PreparedLighting,
    primary: bool,
) -> Option<Vec3> {
    let view = view.normalize_or_zero();
    let reflected = (2. * view.dot(normal) * normal - view).normalize_or_zero();
    let floor = (0.16 + footprint.max(0.) * 2.).clamp(0.16, 0.65);
    let roughness = unit_value(sample.roughness, 1.).max(floor);
    let environment = scene.environment_sample(EnvironmentQuery {
        hit,
        normal,
        reflected_direction: reflected,
        roughness,
        time,
        stage,
    })?;
    let max_distance = if environment.max_distance.is_finite() {
        environment.max_distance.clamp(0., 100.)
    } else {
        0.
    };
    let dielectric = unit_value(sample.f0, 0.04);
    let metalness = metallic.map_or(0., |m| m.metalness);
    let f0 = metallic.map_or(Vec3::splat(dielectric), |m| {
        Vec3::splat(dielectric) * (1. - m.metalness)
            + Vec3::from_array(m.f0.to_array().map(|v| unit_value(v, dielectric))) * m.metalness
    });
    let fresnel = f0 + (Vec3::ONE - f0) * (1. - normal.dot(view).clamp(0., 1.)).powi(5);
    let albedo = Vec3::from_array(sample.albedo.to_array().map(|v| unit_value(v, 0.)));
    let mut diffuse = albedo
        * (Vec3::ONE - fresnel)
        * environment_color(environment.diffuse_irradiance)
        * ((1. - metalness) / std::f32::consts::PI);
    let origin = hit.position + hit.normal * 0.012;
    // A single normal visibility sample, explicitly not a hemisphere integral.
    // trace rather than legacy occluded: even emissive surfaces block distant E.
    if primary
        && max_distance > 0.
        && diffuse.max_element() > 0.
        && scene
            .trace(
                Ray {
                    origin,
                    dir: hit.normal,
                },
                time,
                stage,
                false,
            )
            .is_some_and(|h| h.distance > 0. && h.distance < max_distance)
    {
        diffuse = Vec3::ZERO;
    }
    let occlusion = unit_value(sample.occlusion, 1.);
    if !primary {
        return Some(diffuse * occlusion);
    }
    // A decorative normal may not send the reflected ray into the geometric
    // back face. The geometric offset remains unchanged by material relief.
    if reflected.dot(hit.normal) <= 0. || normal.dot(view) <= 0. {
        return Some(diffuse * occlusion);
    }
    let mut radiance = environment_color(environment.specular_radiance);
    if max_distance > 0. && fresnel.max_element() > 0. {
        if let Some(other) = scene
            .trace(
                Ray {
                    origin,
                    dir: reflected,
                },
                time,
                stage,
                false,
            )
            .filter(|h| h.distance > 0. && h.distance < max_distance)
        {
            // A footprint cone heuristic, not curved-surface ray differentials.
            let secondary_width =
                footprint.max(0.0001) * (1. + other.distance / hit.distance.max(0.0001));
            let bounced = if other.material != EMISSIVE {
                if let Some(material) = scene.material_sample(other, secondary_width) {
                    shade_material_depth(
                        scene,
                        other,
                        material,
                        -reflected,
                        secondary_width,
                        time,
                        stage,
                        lighting,
                        false,
                    )
                } else {
                    shade(scene, other, time, stage, lighting)
                }
            } else {
                shade(scene, other, time, stage, lighting)
            };
            // A local hit exclusively replaces the distant source. Attenuation
            // with roughness avoids claiming this sharp ray is a GGX integral,
            // but loses energy and may be too dark. Never leak distant light
            // through an occluder by blending it back into the local result.
            let local_weight = (1. - roughness).powi(2);
            radiance = bounced * local_weight;
        }
    }
    Some((diffuse + fresnel * radiance) * occlusion)
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

    struct EnvironmentProbe {
        environment: Option<EnvironmentSample>,
        metal: Option<MetallicSample>,
        blocker: u8,
        rays: std::sync::atomic::AtomicUsize,
    }
    impl EnvironmentProbe {
        fn open() -> Self {
            Self {
                environment: Some(EnvironmentSample {
                    diffuse_irradiance: Vec3::splat(std::f32::consts::PI * 0.25),
                    specular_radiance: Vec3::new(0.2, 0.4, 0.8),
                    max_distance: 5.,
                }),
                metal: None,
                blocker: 0,
                rays: std::sync::atomic::AtomicUsize::new(0),
            }
        }
        fn ray_count(&self) -> usize {
            self.rays.load(std::sync::atomic::Ordering::Relaxed)
        }
    }
    impl Scene for EnvironmentProbe {
        fn trace(&self, ray: Ray, _: f32, _: Stage, primary: bool) -> Option<Hit> {
            if primary {
                return Some(Hit {
                    position: ray.origin + ray.dir,
                    normal: -ray.dir,
                    ..local_hit()
                });
            }
            self.rays.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            (self.blocker != 0).then_some(Hit {
                distance: 2.,
                position: ray.origin + ray.dir * 2.,
                normal: -ray.dir,
                material: if self.blocker == 1 { EMISSIVE } else { 78 },
                object_id: 92,
                ..local_hit()
            })
        }
        fn light_position(&self, _: f32) -> Vec3 {
            Vec3::Z * 2.
        }
        fn pigment(&self, _: u32) -> Vec3 {
            Vec3::splat(0.3)
        }
        fn glyph(&self, _: u32, _: Vec3, _: Vec3, _: f32) -> u8 {
            b'#'
        }
        fn local_materials(&self) -> bool {
            true
        }
        fn material_sample(&self, hit: Hit, _: f32) -> Option<MaterialSample> {
            Some(MaterialSample {
                normal: hit.normal,
                f0: 0.,
                albedo: if self.blocker == 3 && hit.object_id == 92 {
                    Vec3::ZERO
                } else {
                    Vec3::splat(0.3)
                },
                ..local_sample()
            })
        }
        fn metallic_sample(&self, hit: Hit, _: f32) -> Option<MetallicSample> {
            (hit.material == 77).then_some(self.metal).flatten()
        }
        fn environment_sample(&self, query: EnvironmentQuery) -> Option<EnvironmentSample> {
            assert!(query.normal.is_finite() && query.reflected_direction.is_finite());
            assert!((0.16..=1.).contains(&query.roughness));
            self.environment
        }
        fn light_color(&self) -> Vec3 {
            Vec3::ONE
        }
    }

    #[test]
    fn environment_none_preserves_local_and_conductor_bits_without_extra_rays() {
        for metal in [
            None,
            Some(MetallicSample {
                f0: Vec3::splat(0.7),
                metalness: 1.,
            }),
        ] {
            let scene = EnvironmentProbe {
                environment: None,
                metal,
                ..EnvironmentProbe::open()
            };
            let old = metal_probe(metal);
            for stage in [Stage::Core, Stage::Light] {
                for view in [Vec3::Z, Vec3::new(0.6, 0., 0.8)] {
                    let a = sample_color(&scene, local_sample(), view, 0.01, stage);
                    let b = sample_color(&old, local_sample(), view, 0.01, stage);
                    assert_eq!(
                        a.to_array().map(f32::to_bits),
                        b.to_array().map(f32::to_bits)
                    );
                }
            }
            assert_eq!(scene.ray_count(), 2, "only the two direct shadows");
        }
    }

    #[test]
    fn environment_conductor_uses_colored_radiance_without_diffuse_or_legacy_ambient() {
        let scene = EnvironmentProbe {
            metal: Some(MetallicSample {
                f0: Vec3::new(0.8, 0.6, 0.3),
                metalness: 1.,
            }),
            ..EnvironmentProbe::open()
        };
        let a = sample_color(&scene, local_sample(), Vec3::Z, 0.01, Stage::Core);
        let expected = Vec3::new(0.8, 0.6, 0.3) * scene.environment.unwrap().specular_radiance;
        assert!((a - expected).abs().max_element() < 1e-6);
        let b = sample_color(
            &scene,
            MaterialSample {
                albedo: Vec3::ONE,
                ..local_sample()
            },
            Vec3::Z,
            0.01,
            Stage::Core,
        );
        assert_eq!(a, b);
        assert_eq!(
            scene.ray_count(),
            2,
            "one specular query per sample; no metal diffuse rays"
        );
    }

    #[test]
    fn environment_normal_visibility_blocks_diffuse_even_with_emissive_blocker() {
        let open = EnvironmentProbe::open();
        let blocked = EnvironmentProbe {
            blocker: 1,
            ..EnvironmentProbe::open()
        };
        let material = MaterialSample {
            f0: 0.,
            ..local_sample()
        };
        let a = sample_color(&open, material, Vec3::Z, 0.01, Stage::Core);
        let b = sample_color(&blocked, material, Vec3::Z, 0.01, Stage::Core);
        assert!((a - Vec3::splat(0.075)).abs().max_element() < 1e-6);
        assert_eq!(b, Vec3::ZERO);
        assert_eq!(open.ray_count(), 1);
        assert_eq!(blocked.ray_count(), 1);
    }

    #[test]
    fn environment_reflects_local_surface_once_and_bounds_roughness_blend() {
        let scene = EnvironmentProbe {
            metal: Some(MetallicSample {
                f0: Vec3::ONE,
                metalness: 1.,
            }),
            blocker: 2,
            ..EnvironmentProbe::open()
        };
        let material = MaterialSample {
            roughness: 0.25,
            ..local_sample()
        };
        let a = sample_color(&scene, material, Vec3::Z, 0.01, Stage::Core);
        let distant = scene.environment.unwrap().specular_radiance;
        let expected = Vec3::splat(0.075) * 0.75_f32.powi(2);
        assert!((a - expected).abs().max_element() < 1e-6);
        assert_eq!(
            scene.ray_count(),
            1,
            "secondary must not trace environment or recurse"
        );
        let rough = sample_color(
            &scene,
            MaterialSample {
                roughness: 1.,
                ..material
            },
            Vec3::Z,
            0.01,
            Stage::Core,
        );
        assert_ne!(distant, Vec3::ZERO);
        assert_eq!(rough, Vec3::ZERO);
    }

    #[test]
    fn environment_black_local_blocker_never_leaks_distant_sky() {
        let scene = EnvironmentProbe {
            metal: Some(MetallicSample {
                f0: Vec3::ONE,
                metalness: 1.,
            }),
            blocker: 3,
            ..EnvironmentProbe::open()
        };
        for roughness in [0., 0.25, 0.7, 1.] {
            let color = sample_color(
                &scene,
                MaterialSample {
                    roughness,
                    ..local_sample()
                },
                Vec3::Z,
                0.01,
                Stage::Core,
            );
            assert_eq!(color, Vec3::ZERO);
        }
        assert_eq!(scene.ray_count(), 4);
    }

    #[test]
    fn environment_invalid_inputs_are_finite_and_disabled_range_traces_nothing() {
        let scene = EnvironmentProbe {
            environment: Some(EnvironmentSample {
                diffuse_irradiance: Vec3::new(f32::NAN, -1., f32::INFINITY),
                specular_radiance: Vec3::new(f32::NAN, -1., f32::INFINITY),
                max_distance: f32::NAN,
            }),
            ..EnvironmentProbe::open()
        };
        let a = sample_color(
            &scene,
            MaterialSample {
                roughness: f32::NAN,
                normal: Vec3::ZERO,
                f0: f32::NAN,
                ..local_sample()
            },
            Vec3::Z,
            0.01,
            Stage::Core,
        );
        assert_eq!(a, Vec3::ZERO);
        assert_eq!(scene.ray_count(), 0);
    }

    #[test]
    fn environment_cells_and_history_are_worker_deterministic() {
        let scene = EnvironmentProbe {
            blocker: 2,
            ..EnvironmentProbe::open()
        };
        for mode in [Mode::First, Mode::Third, Mode::Overhead] {
            let mut serial = Composer::default();
            let mut parallel = Composer::with_threads(4).unwrap();
            for time in [0., 0., 1.] {
                let (a, sa) =
                    serial.compose(&scene, Camera::new(mode), time, Stage::Cloth, 9, 7, 1.);
                let (b, sb) =
                    parallel.compose(&scene, Camera::new(mode), time, Stage::Cloth, 9, 7, 1.);
                assert_eq!(
                    bytemuck::cast_slice::<GlyphCell, u8>(&a),
                    bytemuck::cast_slice::<GlyphCell, u8>(&b)
                );
                assert_stats_bits(&sa, &sb);
                assert_history_bits(&serial, &parallel);
            }
        }
    }

    #[derive(Clone, Copy)]
    struct LocalProbe {
        enabled: bool,
        sample: Option<MaterialSample>,
        blocked: bool,
    }
    #[derive(Clone, Copy)]
    struct MetalProbe {
        base: LocalProbe,
        metal: Option<MetallicSample>,
    }
    impl Scene for MetalProbe {
        fn trace(&self, ray: Ray, time: f32, stage: Stage, water: bool) -> Option<Hit> {
            self.base.trace(ray, time, stage, water)
        }
        fn occluded(&self, ray: Ray, time: f32, stage: Stage, max: f32) -> bool {
            self.base.occluded(ray, time, stage, max)
        }
        fn light_position(&self, time: f32) -> Vec3 {
            self.base.light_position(time)
        }
        fn pigment(&self, material: u32) -> Vec3 {
            self.base.pigment(material)
        }
        fn glyph(&self, m: u32, l: Vec3, n: Vec3, i: f32) -> u8 {
            self.base.glyph(m, l, n, i)
        }
        fn local_materials(&self) -> bool {
            self.base.local_materials()
        }
        fn material_sample(&self, hit: Hit, footprint: f32) -> Option<MaterialSample> {
            self.base.material_sample(hit, footprint)
        }
        fn metallic_sample(&self, _: Hit, _: f32) -> Option<MetallicSample> {
            assert!(self.base.enabled && self.base.sample.is_some());
            self.metal
        }
        fn light_color(&self) -> Vec3 {
            self.base.light_color()
        }
    }
    impl Scene for LocalProbe {
        fn trace(&self, _: Ray, _: f32, _: Stage, _: bool) -> Option<Hit> {
            Some(local_hit())
        }
        fn occluded(&self, ray: Ray, _: f32, _: Stage, max: f32) -> bool {
            assert_eq!(
                ray.origin,
                Vec3::Z * 0.012,
                "shadow offset must use geometric normal"
            );
            assert!((max - 1.91).abs() < 1e-6);
            self.blocked
        }
        fn light_position(&self, _: f32) -> Vec3 {
            Vec3::Z * 2.
        }
        fn pigment(&self, _: u32) -> Vec3 {
            Vec3::splat(0.3)
        }
        fn glyph(&self, _: u32, _: Vec3, _: Vec3, _: f32) -> u8 {
            b'#'
        }
        fn local_materials(&self) -> bool {
            self.enabled
        }
        fn material_sample(&self, _: Hit, footprint: f32) -> Option<MaterialSample> {
            assert!(self.enabled, "legacy must not sample materials");
            assert!(footprint.is_finite() && footprint > 0.);
            self.sample
        }
        fn light_color(&self) -> Vec3 {
            assert!(self.enabled, "legacy must not query local light color");
            Vec3::ONE
        }
    }
    fn local_hit() -> Hit {
        Hit {
            distance: 1.,
            position: Vec3::ZERO,
            normal: Vec3::Z,
            material: 77,
            object_id: 91,
            local: Vec3::new(0.1, 0.2, 0.),
        }
    }
    fn local_sample() -> MaterialSample {
        MaterialSample {
            albedo: Vec3::splat(0.3),
            normal: Vec3::Z,
            roughness: 0.25,
            f0: 0.04,
            occlusion: 1.,
        }
    }
    fn local_lighting() -> PreparedLighting {
        PreparedLighting {
            moon: Vec3::Y,
            light: Vec3::Z * 2.,
            color: Vec3::ONE,
        }
    }
    fn sample_color(
        scene: &impl Scene,
        sample: MaterialSample,
        view: Vec3,
        footprint: f32,
        stage: Stage,
    ) -> Vec3 {
        shade_material(
            scene,
            local_hit(),
            sample,
            view,
            footprint,
            0.,
            stage,
            &local_lighting(),
        )
    }

    fn metal_probe(metal: Option<MetallicSample>) -> MetalProbe {
        MetalProbe {
            base: LocalProbe {
                enabled: true,
                sample: Some(local_sample()),
                blocked: false,
            },
            metal,
        }
    }

    #[test]
    fn metallic_none_zero_and_invalid_weight_preserve_dielectric_bits() {
        let absent = metal_probe(None);
        for weight in [0., -1., f32::NAN, f32::INFINITY] {
            let zero = metal_probe(Some(MetallicSample {
                f0: Vec3::new(0.9, f32::NAN, 0.2),
                metalness: weight,
            }));
            for stage in [Stage::Core, Stage::Light, Stage::Water, Stage::Cloth] {
                for view in [Vec3::Z, Vec3::new(0.6, 0., 0.8)] {
                    let expected = sample_color(&absent.base, local_sample(), view, 0.03, stage);
                    for scene in [&absent, &zero] {
                        let actual = sample_color(scene, local_sample(), view, 0.03, stage);
                        assert_eq!(
                            actual.to_array().map(f32::to_bits),
                            expected.to_array().map(f32::to_bits)
                        );
                    }
                }
            }
        }
        // A conductor hook alone must not opt a legacy/None material into it.
        for enabled in [false, true] {
            let ignored = MetalProbe {
                base: LocalProbe {
                    enabled,
                    sample: None,
                    blocked: false,
                },
                metal: Some(MetallicSample {
                    f0: Vec3::ONE,
                    metalness: 1.,
                }),
            };
            Composer::default().compose(
                &ignored,
                Camera::new(Mode::First),
                0.,
                Stage::Light,
                3,
                3,
                1.,
            );
        }
    }

    #[test]
    fn pure_conductor_has_no_diffuse_or_ambient_and_has_colored_view_response() {
        let scene = metal_probe(Some(MetallicSample {
            f0: Vec3::new(0.9, 0.6, 0.3),
            metalness: 1.,
        }));
        let sample = MaterialSample {
            roughness: 0.55,
            ..local_sample()
        };
        assert_eq!(
            sample_color(&scene, sample, Vec3::Z, 0.01, Stage::Core),
            Vec3::ZERO
        );
        let blocked = MetalProbe {
            base: LocalProbe {
                blocked: true,
                ..scene.base
            },
            ..scene
        };
        assert_eq!(
            sample_color(&blocked, sample, Vec3::Z, 0.01, Stage::Light),
            Vec3::ZERO
        );
        let front = sample_color(&scene, sample, Vec3::Z, 0.01, Stage::Light);
        assert!(front.x > front.y && front.y > front.z && front.z > 0.);
        // At normal incidence, conductor channel ratios follow RGB F0.
        assert!((front.x / front.y - 1.5).abs() < 1e-5);
        for view in [Vec3::Z, Vec3::new(0.6, 0., 0.8)] {
            let color = sample_color(&scene, sample, view, 0.01, Stage::Light);
            let changed_albedo = sample_color(
                &scene,
                MaterialSample {
                    albedo: Vec3::new(0.8, 0.1, 0.4),
                    ..sample
                },
                view,
                0.01,
                Stage::Light,
            );
            assert_eq!(
                color.to_array().map(f32::to_bits),
                changed_albedo.to_array().map(f32::to_bits)
            );
            assert!(color.x > color.y && color.y > color.z);
        }
        let angled = sample_color(&scene, sample, Vec3::new(0.6, 0., 0.8), 0.01, Stage::Light);
        assert!(front.x > angled.x);
        let mixed = metal_probe(Some(MetallicSample {
            metalness: 0.5,
            ..scene.metal.unwrap()
        }));
        let dielectric = sample_color(&scene.base, sample, Vec3::Z, 0.01, Stage::Core);
        assert_eq!(
            sample_color(&mixed, sample, Vec3::Z, 0.01, Stage::Core),
            dielectric * 0.5
        );
    }

    #[test]
    fn conductor_invalid_channels_and_excess_weight_are_bounded() {
        let invalid = metal_probe(Some(MetallicSample {
            f0: Vec3::new(-2., f32::NAN, 3.),
            metalness: 2.,
        }));
        let sanitized = metal_probe(Some(MetallicSample {
            f0: Vec3::new(0., local_sample().f0, 1.),
            metalness: 1.,
        }));
        for view in [Vec3::Z, Vec3::new(0.6, 0., 0.8)] {
            let actual = sample_color(&invalid, local_sample(), view, 0.02, Stage::Light);
            let expected = sample_color(&sanitized, local_sample(), view, 0.02, Stage::Light);
            assert!(actual.is_finite() && actual.min_element() >= 0.);
            assert_eq!(
                actual.to_array().map(f32::to_bits),
                expected.to_array().map(f32::to_bits)
            );
        }
    }

    #[test]
    fn conductor_cells_and_history_are_worker_deterministic() {
        for metalness in [0., 0.5, 1.] {
            let scene = metal_probe(Some(MetallicSample {
                f0: Vec3::new(0.85, 0.65, 0.4),
                metalness,
            }));
            for mode in [Mode::First, Mode::Third, Mode::Overhead] {
                for workers in [2, 4] {
                    let mut serial = Composer::default();
                    let mut parallel = Composer::with_threads(workers).unwrap();
                    let mut dielectric = Composer::default();
                    for stage in [Stage::Core, Stage::Light, Stage::Light] {
                        let camera = Camera::new(mode);
                        let (a, sa) = serial.compose(&scene, camera, 0., stage, 9, 7, 16. / 9.);
                        let (b, sb) = parallel.compose(&scene, camera, 0., stage, 9, 7, 16. / 9.);
                        assert_eq!(
                            bytemuck::cast_slice::<GlyphCell, u8>(&a),
                            bytemuck::cast_slice::<GlyphCell, u8>(&b)
                        );
                        assert_stats_bits(&sa, &sb);
                        if metalness == 0. {
                            let (old, so) =
                                dielectric.compose(&scene.base, camera, 0., stage, 9, 7, 16. / 9.);
                            assert_eq!(
                                bytemuck::cast_slice::<GlyphCell, u8>(&a),
                                bytemuck::cast_slice::<GlyphCell, u8>(&old)
                            );
                            assert_stats_bits(&sa, &so);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn local_brdf_responds_to_view_roughness_footprint_shadow_and_cavity() {
        let scene = LocalProbe {
            enabled: true,
            sample: Some(local_sample()),
            blocked: false,
        };
        let sample = local_sample();
        let peak = sample_color(&scene, sample, Vec3::Z, 0.001, Stage::Light);
        let off_axis = sample_color(&scene, sample, Vec3::new(0.6, 0., 0.8), 0.001, Stage::Light);
        let rough = sample_color(
            &scene,
            MaterialSample {
                roughness: 0.8,
                ..sample
            },
            Vec3::Z,
            0.001,
            Stage::Light,
        );
        let filtered = sample_color(&scene, sample, Vec3::Z, 0.5, Stage::Light);
        assert!(peak.x > off_axis.x && peak.x > rough.x && peak.x > filtered.x);
        let ambient = sample_color(&scene, sample, Vec3::Z, 0.001, Stage::Core);
        let blocked = LocalProbe {
            blocked: true,
            ..scene
        };
        assert_eq!(
            sample_color(&blocked, sample, Vec3::Z, 0.001, Stage::Light),
            ambient
        );
        assert_eq!(
            sample_color(&blocked, sample, Vec3::X, 0.5, Stage::Core),
            ambient
        );
        let tilted = MaterialSample {
            normal: Vec3::new(0.4, 0., 1.).normalize(),
            ..sample
        };
        assert!(sample_color(&blocked, tilted, Vec3::Z, 0.001, Stage::Light).is_finite());
        let open = LocalProbe {
            blocked: false,
            ..blocked
        };
        assert_eq!(
            sample_color(
                &open,
                MaterialSample {
                    occlusion: 0.,
                    ..sample
                },
                Vec3::Z,
                0.001,
                Stage::Light
            ),
            Vec3::ZERO
        );
        let bright = sample_color(
            &open,
            MaterialSample {
                albedo: Vec3::splat(0.8),
                ..sample
            },
            Vec3::Z,
            0.001,
            Stage::Core,
        );
        assert!(bright.x > ambient.x);
        let invalid = MaterialSample {
            albedo: Vec3::splat(f32::NAN),
            normal: Vec3::ZERO,
            roughness: f32::NAN,
            f0: f32::INFINITY,
            occlusion: f32::NAN,
        };
        assert!(sample_color(&open, invalid, Vec3::Z, 0.001, Stage::Light).is_finite());
    }

    #[test]
    fn footprint_handles_perspective_and_orthographic_projection() {
        for mode in [Mode::First, Mode::Third, Mode::Overhead] {
            let camera = Camera::new(mode).prepare(120, 68, 16. / 9.);
            let ray = camera.ray(60, 34);
            let adjacent = [camera.ray(61, 34), camera.ray(60, 35)];
            let make_hit = |distance| Hit {
                position: ray.origin + ray.dir * distance,
                distance,
                normal: -ray.dir,
                ..local_hit()
            };
            let near = surface_footprint(ray, adjacent, make_hit(2.));
            let far = surface_footprint(ray, adjacent, make_hit(8.));
            assert!(near.is_finite() && near > 0. && far.is_finite());
            if mode == Mode::Overhead {
                assert!((near - far).abs() < 1e-5);
            } else {
                assert!(far > near * 3.9);
            }
        }
    }

    #[test]
    fn default_material_path_and_opt_in_none_preserve_legacy_cells() {
        let legacy = LocalProbe {
            enabled: false,
            sample: None,
            blocked: false,
        };
        let empty_opt_in = LocalProbe {
            enabled: true,
            sample: None,
            blocked: false,
        };
        for mode in [Mode::First, Mode::Third, Mode::Overhead] {
            for stage in [Stage::Core, Stage::Light, Stage::Water, Stage::Cloth] {
                let mut a = Composer::default();
                let mut b = Composer::default();
                for _ in 0..2 {
                    let (old, sa) =
                        a.compose(&legacy, Camera::new(mode), 0., stage, 7, 5, 16. / 9.);
                    let (new, sb) =
                        b.compose(&empty_opt_in, Camera::new(mode), 0., stage, 7, 5, 16. / 9.);
                    assert_eq!(
                        bytemuck::cast_slice::<GlyphCell, u8>(&old),
                        bytemuck::cast_slice::<GlyphCell, u8>(&new)
                    );
                    assert_stats_bits(&sa, &sb);
                }
            }
        }
    }

    #[test]
    fn local_material_frames_are_frozen_and_worker_deterministic() {
        let scene = LocalProbe {
            enabled: true,
            sample: Some(local_sample()),
            blocked: false,
        };
        let mut serial = Composer::default();
        let mut parallel = Composer::with_threads(2).unwrap();
        for mode in [Mode::First, Mode::Third, Mode::Overhead] {
            serial.reset();
            parallel.reset();
            let (first, _) =
                serial.compose(&scene, Camera::new(mode), 0., Stage::Light, 9, 7, 16. / 9.);
            let (again, _) =
                serial.compose(&scene, Camera::new(mode), 0., Stage::Light, 9, 7, 16. / 9.);
            let (other, _) =
                parallel.compose(&scene, Camera::new(mode), 0., Stage::Light, 9, 7, 16. / 9.);
            assert_eq!(
                bytemuck::cast_slice::<GlyphCell, u8>(&first),
                bytemuck::cast_slice::<GlyphCell, u8>(&again)
            );
            assert_eq!(
                bytemuck::cast_slice::<GlyphCell, u8>(&first),
                bytemuck::cast_slice::<GlyphCell, u8>(&other)
            );
            assert!(first.iter().all(|c| c.color.iter().all(|v| v.is_finite())));
        }
    }

    #[test]
    fn footprint_glyph_key_releases_history_when_lod_changes() {
        struct FilteredGlyph {
            cutoff: f32,
        }
        impl Scene for FilteredGlyph {
            fn trace(&self, ray: Ray, _: f32, _: Stage, _: bool) -> Option<Hit> {
                Some(Hit {
                    position: ray.origin + ray.dir,
                    normal: -ray.dir,
                    ..local_hit()
                })
            }
            fn light_position(&self, _: f32) -> Vec3 {
                Vec3::Y
            }
            fn pigment(&self, _: u32) -> Vec3 {
                Vec3::splat(0.4)
            }
            fn glyph(&self, _: u32, _: Vec3, _: Vec3, _: f32) -> u8 {
                b'!'
            }
            fn local_materials(&self) -> bool {
                true
            }
            fn glyph_sample(&self, _: Hit, _: f32, footprint: f32) -> u8 {
                if footprint > self.cutoff {
                    b'='
                } else {
                    b'.'
                }
            }
            fn glyph_sample_key(&self, _: Hit, footprint: f32) -> u32 {
                u32::from(footprint > self.cutoff)
            }
        }
        let mut near = Camera::new(Mode::Overhead);
        near.zoom = 0.1;
        let mut far = near;
        far.zoom = 0.2;
        // At 1x1, orthographic maximum-axis width = 10 * zoom * aspect.
        let scene = FilteredGlyph { cutoff: 1.5 };
        let mut composer = Composer::default();
        let (a, _) = composer.compose(&scene, near, 0., Stage::Core, 1, 1, 1.);
        let (b, stats) = composer.compose(&scene, far, 0., Stage::Core, 1, 1, 1.);
        assert_eq!(a[0].id, b[0].id);
        assert_eq!(a[0].glyph, b'.' as u32);
        assert_eq!(b[0].glyph, b'=' as u32);
        assert_eq!(stats.glyph_changes, 1);
    }

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
        assert_eq!(actual.primary_rays, expected.primary_rays);
        assert_eq!(actual.alternate_primary, expected.alternate_primary);
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
            assert_eq!((a.4, a.5, a.6), (b.4, b.5, b.6));
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

    fn sampling_camera() -> Camera {
        Camera::with_view(
            Mode::Overhead,
            crate::camera::CameraView::new(Vec3::new(0., 0., 4.), Vec3::ZERO, 60., 1.).unwrap(),
        )
    }

    // Nine isolated patches at the known sample positions of a 1x1 orthographic
    // cell. Arbitrary depths/identities exercise semantic resolving and history.
    struct SamplePatches {
        hits: [Option<Hit>; 9],
        queries: std::sync::atomic::AtomicUsize,
        shades: std::sync::atomic::AtomicUsize,
    }
    impl SamplePatches {
        fn empty() -> Self {
            Self {
                hits: [None; 9],
                queries: std::sync::atomic::AtomicUsize::new(0),
                shades: std::sync::atomic::AtomicUsize::new(0),
            }
        }
        fn set(&mut self, index: usize, object: u32, material: u32, distance: f32, local_x: f32) {
            let ray = sampling_camera()
                .ray_sample(0, 0, 1, 1, 1., PRIMARY_OFFSETS[index])
                .unwrap();
            self.hits[index] = Some(Hit {
                distance,
                position: ray.origin + ray.dir * distance,
                normal: Vec3::Z,
                material,
                object_id: object,
                local: Vec3::new(local_x, 0., 0.),
            });
        }
    }
    impl Scene for SamplePatches {
        fn trace(&self, ray: Ray, _: f32, _: Stage, _: bool) -> Option<Hit> {
            self.queries
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            // The independent geometric mapping is top-left to bottom-right,
            // while sample index 0 belongs to the central patch.
            let col = if ray.origin.x < -0.2 {
                0
            } else if ray.origin.x > 0.2 {
                2
            } else {
                1
            };
            let row = if ray.origin.y > 0.2 {
                0
            } else if ray.origin.y < -0.2 {
                2
            } else {
                1
            };
            self.hits[[1, 2, 3, 4, 0, 5, 6, 7, 8][row * 3 + col]]
        }
        fn light_position(&self, _: f32) -> Vec3 {
            Vec3::Y
        }
        fn pigment(&self, _: u32) -> Vec3 {
            self.shades
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Vec3::splat(0.7)
        }
        fn glyph(&self, _: u32, local: Vec3, _: Vec3, _: f32) -> u8 {
            if local.x < 0.53 {
                b'|'
            } else {
                b':'
            }
        }
    }
    fn sample_patch_frame(
        composer: &mut Composer,
        scene: &SamplePatches,
    ) -> (GlyphCell, FrameStats) {
        let (cells, stats) = composer.compose(scene, sampling_camera(), 0., Stage::Core, 1, 1, 1.);
        (cells[0], stats)
    }
    fn conservative_composer() -> Composer {
        let mut composer = Composer::default();
        composer.set_primary_sampling(PrimarySampling::Conservative9);
        composer
    }

    #[test]
    fn conservative_resolve_has_exact_depth_ties_and_center_group_preference() {
        let mut scene = SamplePatches::empty();
        scene.set(0, 20, 35, 4., 0.52);
        scene.set(1, 10, 34, 4., 0.52);
        scene.set(3, 10, 33, 4., 0.52);
        scene.set(6, 10, 33, 4., 0.52);
        let mut composer = conservative_composer();
        let (_, stats) = sample_patch_frame(&mut composer, &scene);
        assert_eq!(
            composer.previous_cells[0].6,
            PrimaryHistory {
                object: 10,
                material: 33,
                sample: 3,
                coverage_mask: (1 << 3) | (1 << 6),
            }
        );
        assert_eq!((stats.primary_rays, stats.alternate_primary), (9, 1));
        assert_eq!(scene.queries.load(std::sync::atomic::Ordering::Relaxed), 9);
        assert_eq!(scene.shades.load(std::sync::atomic::Ordering::Relaxed), 1);

        // Center can represent the winning group even when its own patch is
        // farther away; preserving its glyph/shading avoids interior shifts.
        scene.set(0, 10, 33, 4.1, 0.52);
        let (_, stats) = sample_patch_frame(&mut composer, &scene);
        assert_eq!(composer.previous_cells[0].6.sample, 0);
        assert_eq!(stats.alternate_primary, 0);

        // No epsilon may merge distinct layers and let a lower ID win.
        scene.set(0, 1, 33, f32::from_bits(4_f32.to_bits() + 1), 0.52);
        sample_patch_frame(&mut composer, &scene);
        assert_eq!(composer.previous_cells[0].6.object, 10);
        assert_eq!(composer.previous_cells[0].6.sample, 3);
    }

    #[test]
    fn conservative_invalid_depths_and_empty_cells_still_count_all_primary_queries() {
        let mut scene = SamplePatches::empty();
        for (index, depth) in [0., -1., f32::INFINITY, f32::NAN].into_iter().enumerate() {
            scene.set(index, 10, 32, depth, 0.52);
        }
        let mut composer = conservative_composer();
        let (cell, stats) = sample_patch_frame(&mut composer, &scene);
        assert_eq!(cell, GlyphCell::default());
        assert_eq!((stats.primary_rays, stats.alternate_primary), (9, 0));
        assert_eq!(scene.queries.load(std::sync::atomic::Ordering::Relaxed), 9);
        assert_eq!(scene.shades.load(std::sync::atomic::Ordering::Relaxed), 0);
    }

    #[test]
    fn conservative_history_releases_on_sample_or_coverage_mask_changes() {
        for change_sample in [true, false] {
            let mut scene = SamplePatches::empty();
            scene.set(1, 10, 32, 3., 0.52);
            scene.set(3, 10, 32, 4., 0.54);
            let mut composer = conservative_composer();
            let (first, _) = sample_patch_frame(&mut composer, &scene);
            let before = composer.previous_cells[0].6;
            if change_sample {
                scene.set(1, 10, 32, 4., 0.52);
                scene.set(3, 10, 32, 3., 0.54);
            } else {
                scene.hits[3] = None;
                scene.set(1, 10, 32, 3., 0.54);
            }
            let (updated, stats) = sample_patch_frame(&mut composer, &scene);
            let (fresh, _) = sample_patch_frame(&mut conservative_composer(), &scene);
            let after = composer.previous_cells[0].6;
            assert_eq!(
                first.id, fresh.id,
                "same surface bin isolates the extra history guard"
            );
            assert_ne!(first.glyph, fresh.glyph);
            assert_eq!(updated, fresh);
            assert_eq!(
                stats.matched, 1,
                "legacy matching statistics remain ID-only"
            );
            if change_sample {
                assert_ne!(before.sample, after.sample);
                assert_eq!(before.coverage_mask, after.coverage_mask);
            } else {
                assert_eq!(before.sample, after.sample);
                assert_ne!(before.coverage_mask, after.coverage_mask);
            }
        }
    }

    #[test]
    fn sampling_mode_change_resets_history_only_when_changed() {
        let mut composer = Composer::with_threads(2).unwrap();
        let pool = composer.pool.as_ref().unwrap() as *const rayon::ThreadPool;
        assert_eq!(composer.primary_sampling(), PrimarySampling::Center);
        let (legacy, _) = composer.compose(
            &SyntheticScene,
            Camera::new(Mode::Third),
            0.,
            Stage::Core,
            9,
            7,
            1.,
        );
        composer.set_primary_sampling(PrimarySampling::Center);
        assert_eq!(composer.previous_cells.len(), 63);
        composer.set_primary_sampling(PrimarySampling::Conservative9);
        assert!(composer.previous_cells.is_empty());
        composer.compose(
            &SyntheticScene,
            Camera::new(Mode::Third),
            0.,
            Stage::Core,
            9,
            7,
            1.,
        );
        composer.set_primary_sampling(PrimarySampling::Conservative9);
        assert_eq!(composer.previous_cells.len(), 63);
        composer.set_primary_sampling(PrimarySampling::Center);
        assert!(composer.previous_cells.is_empty());
        let (restored, stats) = composer.compose(
            &SyntheticScene,
            Camera::new(Mode::Third),
            0.,
            Stage::Core,
            9,
            7,
            1.,
        );
        assert_eq!(restored, legacy);
        assert_eq!((stats.primary_rays, stats.alternate_primary), (63, 0));
        assert_eq!(
            composer.pool.as_ref().unwrap() as *const rayon::ThreadPool,
            pool
        );
        for sampling in [PrimarySampling::Center, PrimarySampling::Conservative9] {
            assert_eq!(PrimarySampling::parse(sampling.name()), Some(sampling));
        }
        assert_eq!(PrimarySampling::parse("adaptive"), None);
    }

    #[test]
    fn conservative_motion_is_worker_deterministic_and_static_history_is_exact() {
        let mut serial = conservative_composer();
        let mut parallel = Composer::with_threads(8).unwrap();
        parallel.set_primary_sampling(PrimarySampling::Conservative9);
        for mode in [Mode::First, Mode::Third, Mode::Overhead] {
            serial.reset();
            parallel.reset();
            let mut previous = None;
            for time in [0., 0., 0.5, 1.] {
                let mut camera = Camera::new(mode);
                camera.orbit = time * 0.1;
                let (a, sa) = serial.compose(
                    &SyntheticScene,
                    camera,
                    time,
                    Stage::Cloth,
                    17,
                    11,
                    16. / 9.,
                );
                let (b, sb) = parallel.compose(
                    &SyntheticScene,
                    camera,
                    time,
                    Stage::Cloth,
                    17,
                    11,
                    16. / 9.,
                );
                assert_eq!(a, b);
                assert_stats_bits(&sa, &sb);
                assert_history_bits(&serial, &parallel);
                assert_eq!(sa.primary_rays, 17 * 11 * 9);
                if time == 0. {
                    if let Some(first) = previous.as_ref() {
                        assert_eq!(&a, first);
                    }
                    previous = Some(a);
                }
            }
        }
    }
}
