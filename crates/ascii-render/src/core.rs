//! Backend-independent semantic representation. No window or GPU handles here.
use glam::Vec3;

pub const STONE: u32 = 1;
pub const GROUND: u32 = 2;
pub const WATER: u32 = 3;
pub const CLOTH: u32 = 4;
pub const METAL: u32 = 5;
pub const SKIN: u32 = 6;
pub const GRASS: u32 = 7;
pub const EMISSIVE: u32 = 8;
/// R5 fixture materials. IDs remain semantic, independent of view and lighting.
pub const CAPE: u32 = 9;
pub const LEATHER: u32 = 10;
pub const CAPE_TRIM: u32 = 11;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Core,
    Light,
    Water,
    Cloth,
}
impl Stage {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "core" => Some(Self::Core),
            "light" => Some(Self::Light),
            "water" => Some(Self::Water),
            "cloth" => Some(Self::Cloth),
            _ => None,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Core => "core",
            Self::Light => "light",
            Self::Water => "water",
            Self::Cloth => "cloth",
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Ray {
    pub origin: Vec3,
    pub dir: Vec3,
}
#[derive(Clone, Copy, Debug)]
pub struct Hit {
    pub distance: f32,
    pub position: Vec3,
    pub normal: Vec3,
    pub material: u32,
    pub object_id: u32,
    pub local: Vec3,
}

/// Opt-in local, linear-color material response. `normal` is a world-space
/// shading normal; it never replaces Hit::normal for visibility/shadow offsets.
/// roughness is perceptual [0,1], f0 is scalar dielectric reflectance [0,1],
/// and occlusion [0,1] is authored local cavity visibility (not computed GI).
#[derive(Clone, Copy, Debug)]
pub struct MaterialSample {
    pub albedo: Vec3,
    pub normal: Vec3,
    pub roughness: f32,
    pub f0: f32,
    pub occlusion: f32,
}

/// Optional conductor endpoint for a local material. `f0` is linear RGB
/// reflectance at normal incidence [0,1], before blending with MaterialSample's
/// dielectric f0. `metalness` [0,1] blends those endpoints and removes diffuse
/// light, including the approximate ambient term. This is an isotropic RGB
/// approximation, not complex-IOR Fresnel, an oxide-layer model, or IBL.
#[derive(Clone, Copy, Debug)]
pub struct MetallicSample {
    pub f0: Vec3,
    pub metalness: f32,
}

/// World-space request for an analytic environment, never a rendered image.
/// Directions are normalized; roughness includes the cell-footprint floor.
#[derive(Clone, Copy, Debug)]
pub struct EnvironmentQuery {
    pub hit: Hit,
    pub normal: Vec3,
    pub reflected_direction: Vec3,
    pub roughness: f32,
    pub time: f32,
    pub stage: Stage,
}

/// Opt-in distant environment. Both colors are linear and contain neither
/// albedo nor Fresnel. Irradiance is the integrated cosine-weighted hemisphere
/// E (the renderer divides it by PI); specular radiance is an authored analytic
/// directional filter at the requested roughness, not an exact GGX convolution.
/// Finite channels are clamped to [0,16], invalid channels become zero.
/// max_distance is the local visibility/reflection range in world units,
/// clamped to [0,100]; zero/nonfinite disables local environment rays.
#[derive(Clone, Copy, Debug)]
pub struct EnvironmentSample {
    pub diffuse_irradiance: Vec3,
    pub specular_radiance: Vec3,
    pub max_distance: f32,
}

/// Small replaceable producer contract, independent from the BLACKWATER fixture.
/// Implementations used by Composer must be Sync and give deterministic answers
/// for one tick/stage. Update simulation before composing, never inside a query.
pub trait Scene {
    fn trace(&self, ray: Ray, time: f32, stage: Stage, include_water: bool) -> Option<Hit>;
    /// Legacy shadow semantics: query the nearest non-water surface, then test
    /// its distance and material. A nearest EMISSIVE surface returns false even
    /// if a non-emissive blocker lies behind it. Overrides must preserve this
    /// rule, the strict distance comparison, and trace's nearest-hit tie order;
    /// an arbitrary "any non-emissive hit" query is not equivalent.
    fn occluded(&self, ray: Ray, time: f32, stage: Stage, max_distance: f32) -> bool {
        self.trace(ray, time, stage, false)
            .is_some_and(|h| h.distance < max_distance && h.material != EMISSIVE)
    }
    fn light_position(&self, time: f32) -> Vec3;
    fn pigment(&self, material: u32) -> Vec3;
    fn glyph(&self, material: u32, local: Vec3, normal: Vec3, illumination: f32) -> u8;
    /// Opt in to footprint/material sampling. False keeps the legacy path and
    /// avoids generating neighboring camera rays. Return a stable value per tick.
    fn local_materials(&self) -> bool {
        false
    }
    /// Approximate world-space cell width (maximum axis), projected onto the
    /// geometric tangent plane. Reflections use a path-length cone estimate,
    /// without curved-surface ray differentials. None uses legacy shading.
    fn material_sample(&self, _hit: Hit, _footprint: f32) -> Option<MaterialSample> {
        None
    }
    /// Queried only for non-emissive local materials with material_sample=Some.
    /// None or zero metalness preserves the dielectric calculation exactly.
    /// Finite values are clamped; nonfinite metalness becomes zero and nonfinite
    /// f0 channels use the sanitized dielectric f0. Pure metal has no diffuse
    /// ambient contribution: without specular environment it is black in Core
    /// and in shadow. Producers must return deterministic answers per snapshot.
    fn metallic_sample(&self, _hit: Hit, _footprint: f32) -> Option<MetallicSample> {
        None
    }
    /// Local materials only. None preserves the R9 arithmetic exactly. Some
    /// replaces the old artistic ambient. The renderer uses one geometric-normal
    /// visibility ray as a coarse diffuse proxy (not hemispherical AO) and one
    /// reflected ray, with at most one secondary direct-light shadow query.
    /// Secondary shading has diffuse environment + direct light, no further
    /// environment rays/specular bounces. Rough local reflections are a weighted
    /// single-ray approximation, not a filtered multi-ray reflection integral.
    /// A local hit replaces distant radiance completely; its contribution is
    /// attenuated by (1-roughness)^2. Rough local reflections can thus be too dark.
    /// Queries must be deterministic for the immutable snapshot. As with other
    /// material changes, callers reset Composer history when toggling this mode.
    fn environment_sample(&self, _query: EnvironmentQuery) -> Option<EnvironmentSample> {
        None
    }
    fn glyph_sample(&self, hit: Hit, illumination: f32, _footprint: f32) -> u8 {
        self.glyph(hit.material, hit.local, hit.normal, illumination)
    }
    /// Opt-in structure/LOD identity; include footprint-dependent glyph regions
    /// here so history cannot retain fine detail after a filtering transition.
    fn glyph_sample_key(&self, hit: Hit, _footprint: f32) -> u32 {
        self.glyph_history_key(hit.material, hit.local, hit.normal)
    }
    /// Linear direct-light color for local materials only; legacy math is fixed.
    fn light_color(&self) -> Vec3 {
        Vec3::new(1., 0.58, 0.23)
    }
    /// Optional structural region key for glyph history. It must not depend on
    /// illumination: change it when a seam, fold or other authored region must
    /// replace a held glyph. Zero preserves legacy illumination hysteresis for
    /// producers that do not opt in; material changes always invalidate holds.
    fn glyph_history_key(&self, _material: u32, _local: Vec3, _normal: Vec3) -> u32 {
        0
    }
}

/// One output character, not a world-color pixel. ID identifies a local surface bin.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GlyphCell {
    pub glyph: u32,
    pub flags: u32,
    pub id: u32,
    pub pad: u32,
    pub color: [f32; 4],
}
impl Default for GlyphCell {
    fn default() -> Self {
        Self {
            glyph: 32,
            flags: 0,
            id: 0,
            pad: 0,
            color: [0., 0., 0., 1.],
        }
    }
}

pub fn intersect_box(ray: Ray, min: Vec3, max: Vec3) -> Option<(f32, Vec3)> {
    let mut near = -f32::INFINITY;
    let mut far = f32::INFINITY;
    let mut near_normal = Vec3::ZERO;
    let mut far_normal = Vec3::ZERO;
    for a in 0..3 {
        if ray.dir[a].abs() < 1e-8 {
            if ray.origin[a] < min[a] || ray.origin[a] > max[a] {
                return None;
            }
            continue;
        }
        let mut t0 = (min[a] - ray.origin[a]) / ray.dir[a];
        let mut t1 = (max[a] - ray.origin[a]) / ray.dir[a];
        let mut n0 = Vec3::ZERO;
        n0[a] = -1.;
        let mut n1 = Vec3::ZERO;
        n1[a] = 1.;
        if t0 > t1 {
            std::mem::swap(&mut t0, &mut t1);
            std::mem::swap(&mut n0, &mut n1);
        }
        if t0 > near {
            near = t0;
            near_normal = n0;
        }
        if t1 < far {
            far = t1;
            far_normal = n1;
        }
        if near > far {
            return None;
        }
    }
    if near > 0.0001 {
        Some((near, near_normal))
    } else if far > 0.0001 && far.is_finite() {
        Some((far, far_normal))
    } else {
        None
    }
}
pub fn intersect_ellipsoid(ray: Ray, center: Vec3, radii: Vec3) -> Option<(f32, Vec3)> {
    let o = (ray.origin - center) / radii;
    let d = ray.dir / radii;
    let a = d.length_squared();
    let b = o.dot(d);
    let c = o.length_squared() - 1.;
    let disc = b * b - a * c;
    if disc < 0. || a < 1e-12 {
        return None;
    }
    let root = disc.sqrt();
    let near = (-b - root) / a;
    let far = (-b + root) / a;
    let t = if near > 0.0001 {
        near
    } else if far > 0.0001 {
        far
    } else {
        return None;
    };
    let n = ((ray.origin + ray.dir * t - center) / (radii * radii)).normalize();
    Some((t, n))
}
#[cfg(test)]
mod tests {
    use super::*;
    struct ShadowFixture(Vec<Hit>);
    impl Scene for ShadowFixture {
        fn trace(&self, _: Ray, time: f32, stage: Stage, water: bool) -> Option<Hit> {
            assert_eq!(time, 1.);
            assert_eq!(stage, Stage::Cloth);
            assert!(!water);
            self.0
                .iter()
                .copied()
                .min_by(|a, b| a.distance.total_cmp(&b.distance))
        }
        fn light_position(&self, _: f32) -> Vec3 {
            Vec3::ZERO
        }
        fn pigment(&self, _: u32) -> Vec3 {
            Vec3::ZERO
        }
        fn glyph(&self, _: u32, _: Vec3, _: Vec3, _: f32) -> u8 {
            b' '
        }
    }

    #[test]
    fn default_occlusion_preserves_nearest_emissive_and_strict_distance() {
        let ray = Ray {
            origin: Vec3::ZERO,
            dir: Vec3::Z,
        };
        let hit = |distance, material| Hit {
            distance,
            material,
            position: Vec3::ZERO,
            normal: Vec3::Y,
            object_id: 1,
            local: Vec3::ZERO,
        };
        let emissive_first = ShadowFixture(vec![hit(1., EMISSIVE), hit(2., STONE)]);
        assert!(!emissive_first.occluded(ray, 1., Stage::Cloth, 3.));
        let stone_first = ShadowFixture(vec![hit(1., STONE), hit(2., EMISSIVE)]);
        assert!(stone_first.occluded(ray, 1., Stage::Cloth, 3.));
        for max_distance in [1., 0.5, f32::NAN] {
            assert!(!stone_first.occluded(ray, 1., Stage::Cloth, max_distance));
        }
        assert!(!ShadowFixture(vec![]).occluded(ray, 1., Stage::Cloth, 3.));
    }
    #[test]
    fn parallel_and_inside_box() {
        assert!(intersect_box(
            Ray {
                origin: Vec3::new(2., 0., 0.),
                dir: Vec3::Y
            },
            -Vec3::ONE,
            Vec3::ONE
        )
        .is_none());
        let (t, n) = intersect_box(
            Ray {
                origin: Vec3::ZERO,
                dir: Vec3::X,
            },
            -Vec3::ONE,
            Vec3::ONE,
        )
        .unwrap();
        assert_eq!(t, 1.);
        assert_eq!(n, Vec3::X);
    }
    #[test]
    fn ellipsoid_distance_and_normal() {
        let (t, n) = intersect_ellipsoid(
            Ray {
                origin: Vec3::new(0., 0., 3.),
                dir: -Vec3::Z,
            },
            Vec3::ZERO,
            Vec3::new(1., 2., 1.),
        )
        .unwrap();
        assert!((t - 2.).abs() < 1e-5);
        assert!(n.abs_diff_eq(Vec3::Z, 1e-5));
    }
}
