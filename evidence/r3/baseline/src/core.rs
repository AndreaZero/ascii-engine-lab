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

/// Small replaceable producer contract, independent from the BLACKWATER fixture.
pub trait Scene {
    fn trace(&self, ray: Ray, time: f32, stage: Stage, include_water: bool) -> Option<Hit>;
    fn light_position(&self, time: f32) -> Vec3;
    fn pigment(&self, material: u32) -> Vec3;
    fn glyph(&self, material: u32, local: Vec3, normal: Vec3, illumination: f32) -> u8;
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
