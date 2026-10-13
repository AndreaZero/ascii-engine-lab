//! Fixed authored stone: convex bevel planes minus finite analytic crack/chip volumes.
//! Ray intervals resolve actual cavity walls and their normals, without meshes or
//! ray marching. Cracks are shallow V grooves, not a fracture/erosion simulation.
use ascii_render::core::{Hit, Ray};
use glam::Vec3;
use std::sync::LazyLock;

pub const CENTER: Vec3 = Vec3::new(0.0, 1.05, 0.0);
pub const HALF_EXTENTS: Vec3 = Vec3::new(1.30, 0.95, 0.86);
pub const STONE_MATERIAL: u32 = 32;
const OBJECT_ID: u32 = 700;
const EPS: f32 = 0.0001;
const CRACK_COUNT: usize = 8;
const CUT_COUNT: usize = 10;
const SPAN_CAPACITY: usize = CUT_COUNT + 1;

#[derive(Clone, Copy)]
struct Plane {
    normal: Vec3,
    offset: f32,
}
impl Plane {
    const EMPTY: Self = Self {
        normal: Vec3::ZERO,
        offset: 0.0,
    };
    fn new(normal: Vec3, offset: f32) -> Self {
        let length = normal.length();
        Self {
            normal: normal / length,
            offset: offset / length,
        }
    }
    fn distance(self, point: Vec3) -> f32 {
        self.normal.dot(point) - self.offset
    }
}

#[derive(Clone, Copy)]
struct Convex<const N: usize> {
    planes: [Plane; N],
    count: usize,
}
impl<const N: usize> Convex<N> {
    const EMPTY: Self = Self {
        planes: [Plane::EMPTY; N],
        count: 0,
    };
    fn push(&mut self, normal: Vec3, offset: f32) {
        self.planes[self.count] = Plane::new(normal, offset);
        self.count += 1;
    }
    fn clip(&self, ray: Ray) -> Option<Span> {
        let mut span = Span {
            enter: -f32::INFINITY,
            exit: f32::INFINITY,
            ..Span::EMPTY
        };
        for plane in &self.planes[..self.count] {
            let denominator = plane.normal.dot(ray.dir);
            let origin_distance = plane.distance(ray.origin);
            if denominator.abs() < 1e-8 {
                if origin_distance > 0.0 {
                    return None;
                }
                continue;
            }
            let t = -origin_distance / denominator;
            if denominator < 0.0 {
                if t > span.enter {
                    span.enter = t;
                    span.enter_normal = plane.normal;
                }
            } else if t < span.exit {
                span.exit = t;
                span.exit_normal = plane.normal;
            }
            if span.enter > span.exit {
                return None;
            }
        }
        if span.enter.is_finite() && span.exit.is_finite() {
            Some(span)
        } else {
            None
        }
    }
    /// Outside distance lower bound from normalized planes; exact on each face.
    fn outside_distance(&self, point: Vec3) -> f32 {
        self.planes[..self.count]
            .iter()
            .map(|p| p.distance(point))
            .fold(0.0, f32::max)
    }
}

#[derive(Clone, Copy)]
struct Span {
    enter: f32,
    exit: f32,
    enter_normal: Vec3,
    exit_normal: Vec3,
}
impl Span {
    const EMPTY: Self = Self {
        enter: 0.0,
        exit: 0.0,
        enter_normal: Vec3::ZERO,
        exit_normal: Vec3::ZERO,
    };
}

struct Geometry {
    plain: Convex<32>,
    worn: Convex<32>,
    cuts: [Convex<8>; CUT_COUNT],
}

fn box_planes<const N: usize>(min: Vec3, max: Vec3) -> Convex<N> {
    let mut shape = Convex::EMPTY;
    for axis in 0..3 {
        let mut n = Vec3::ZERO;
        n[axis] = 1.0;
        shape.push(n, max[axis]);
        shape.push(-n, -min[axis]);
    }
    shape
}

/// Removes a finite wedge along a surface segment. At the mouth the half-width
/// is `width`; it narrows to zero at `depth`. End overlap joins adjacent wedges.
fn groove(a: Vec3, b: Vec3, outward: Vec3, width: f32, depth: f32) -> Convex<8> {
    let tangent = (b - a).normalize();
    let across = outward.cross(tangent).normalize();
    let length = a.distance(b);
    let mut cut = Convex::EMPTY;
    cut.push(-tangent, -tangent.dot(a) + 0.035);
    cut.push(tangent, tangent.dot(a) + length + 0.035);
    for side in [-1.0, 1.0] {
        let n = across * side - outward * (width / depth);
        cut.push(n, n.dot(a) + width);
    }
    cut.push(outward, outward.dot(a) + 0.30);
    cut.push(-outward, -outward.dot(a) + depth);
    cut
}

static GEOMETRY: LazyLock<Geometry> = LazyLock::new(|| {
    let plain = box_planes(-HALF_EXTENTS, HALF_EXTENTS);
    let mut worn = plain;
    // Unequal bevel widths avoid a perfectly rounded/chamfered cube. These are
    // actual silhouette planes, not shading normals painted on a box.
    for (a, b) in [(0, 1), (0, 2), (1, 2)] {
        for sa in [-1.0, 1.0] {
            for sb in [-1.0, 1.0] {
                let mut n = Vec3::ZERO;
                n[a] = sa;
                n[b] = sb;
                let bevel =
                    0.105 + if sa > 0.0 { 0.05 } else { 0.0 } + if sb < 0.0 { 0.035 } else { 0.0 };
                worn.push(n, HALF_EXTENTS[a] + HALF_EXTENTS[b] - bevel);
            }
        }
    }
    for sx in [-1.0, 1.0] {
        for sy in [-1.0, 1.0] {
            for sz in [-1.0, 1.0] {
                let n = Vec3::new(sx, sy, sz);
                let corner = 0.27 + if sx * sy * sz > 0.0 { 0.09 } else { 0.0 };
                worn.push(n, HALF_EXTENTS.element_sum() - corner);
            }
        }
    }
    for (n, bite) in [
        (Vec3::new(1.0, 0.36, 1.0), 0.28),
        (Vec3::new(-0.31, 1.0, 1.0), 0.23),
        (Vec3::new(-1.0, -0.45, -1.0), 0.24),
    ] {
        worn.push(n, n.abs().dot(HALF_EXTENTS) - bite);
    }

    let front = HALF_EXTENTS.z;
    let a = Vec3::new(-0.47, 0.99, front);
    let b = Vec3::new(-0.37, 0.53, front);
    let c = Vec3::new(-0.10, 0.19, front);
    let d = Vec3::new(-0.21, -0.37, front);
    let e = Vec3::new(0.025, -0.87, front);
    let branch = Vec3::new(0.34, 0.38, front);
    let mut cuts = [Convex::EMPTY; CUT_COUNT];
    cuts[0] = groove(a, b, Vec3::Z, 0.042, 0.16);
    cuts[1] = groove(b, c, Vec3::Z, 0.038, 0.15);
    cuts[2] = groove(c, d, Vec3::Z, 0.033, 0.14);
    cuts[3] = groove(d, e, Vec3::Z, 0.026, 0.115);
    cuts[4] = groove(c, branch, Vec3::Z, 0.025, 0.11);
    cuts[5] = groove(branch, Vec3::new(0.74, 0.16, front), Vec3::Z, 0.019, 0.085);
    cuts[6] = groove(
        Vec3::new(-0.47, HALF_EXTENTS.y, 0.94),
        Vec3::new(-0.55, HALF_EXTENTS.y, 0.48),
        Vec3::Y,
        0.039,
        0.13,
    );
    cuts[7] = groove(
        Vec3::new(-0.55, HALF_EXTENTS.y, 0.48),
        Vec3::new(-0.30, HALF_EXTENTS.y, 0.13),
        Vec3::Y,
        0.028,
        0.10,
    );
    // Two local missing chips, with oblique internal faces rather than only a
    // convex bevel. They are distinct from the long structural crack field.
    cuts[8] = box_planes(Vec3::new(1.04, 0.57, 0.67), Vec3::new(1.43, 0.77, 1.12));
    cuts[8].push(Vec3::new(-0.3, -1.0, -0.5), -1.24);
    cuts[8].push(Vec3::new(0.5, 1.0, -0.2), 1.20);
    // Tilt the complete upper chip about its own center: its mouth no longer
    // resembles a horizontal machined notch. Plane count and depth stay fixed.
    let chip_center = Vec3::new(1.235, 0.67, 0.895);
    let (sin, cos) = (-0.42_f32).sin_cos();
    for plane in &mut cuts[8].planes {
        let previous = plane.normal;
        plane.normal = Vec3::new(
            cos * previous.x - sin * previous.y,
            sin * previous.x + cos * previous.y,
            previous.z,
        );
        plane.offset += (plane.normal - previous).dot(chip_center);
    }
    cuts[9] = box_planes(Vec3::new(-1.43, -0.77, 0.63), Vec3::new(-1.09, -0.55, 1.12));
    cuts[9].push(Vec3::new(0.4, 1.0, -0.3), -1.34);
    cuts[9].push(Vec3::new(-0.6, -1.0, -0.1), 1.40);
    Geometry { plain, worn, cuts }
});

/// Local distance to the authored fracture cavities. Zero means on/inside a
/// removed crack wedge. Outside, this is a conservative distance in world units
/// (plane-distance bound, not exact Euclidean distance at wedge corners).
pub fn crack_distance(local: Vec3) -> f32 {
    GEOMETRY.cuts[..CRACK_COUNT]
        .iter()
        .map(|cut| cut.outside_distance(local))
        .fold(f32::INFINITY, f32::min)
}

/// Exposure near real bevels and local missing chips, without a screen-space mask.
pub fn edge_wear(local: Vec3) -> f32 {
    let bevel = GEOMETRY.worn.planes[6..GEOMETRY.worn.count]
        .iter()
        .map(|p| (1.0 + p.distance(local) / 0.13).clamp(0.0, 1.0))
        .fold(0.0, f32::max);
    let chips = GEOMETRY.cuts[CRACK_COUNT..]
        .iter()
        .map(|cut| (1.0 - cut.outside_distance(local) / 0.10).clamp(0.0, 1.0))
        .fold(0.0, f32::max);
    bevel.max(chips)
}

/// Dry/wet share this exact geometry; material choice is outside this module.
pub fn trace(ray: Ray, detailed: bool) -> Option<Hit> {
    let local_ray = Ray {
        origin: ray.origin - CENTER,
        dir: ray.dir,
    };
    let base = if detailed {
        &GEOMETRY.worn
    } else {
        &GEOMETRY.plain
    };
    let base_span = base.clip(local_ray)?;
    if base_span.exit <= EPS {
        return None;
    }
    let mut spans = [Span::EMPTY; SPAN_CAPACITY];
    spans[0] = base_span;
    let mut count = 1;
    if detailed {
        for cutter in &GEOMETRY.cuts {
            let Some(cut) = cutter.clip(local_ray) else {
                continue;
            };
            if cut.enter >= cut.exit {
                continue;
            }
            let mut next = [Span::EMPTY; SPAN_CAPACITY];
            let mut next_count = 0;
            for &span in &spans[..count] {
                if cut.exit <= span.enter || cut.enter >= span.exit {
                    next[next_count] = span;
                    next_count += 1;
                    continue;
                }
                if cut.enter > span.enter {
                    next[next_count] = Span {
                        exit: cut.enter,
                        exit_normal: -cut.enter_normal,
                        ..span
                    };
                    next_count += 1;
                }
                if cut.exit < span.exit {
                    next[next_count] = Span {
                        enter: cut.exit,
                        enter_normal: -cut.exit_normal,
                        ..span
                    };
                    next_count += 1;
                }
            }
            spans = next;
            count = next_count;
            if count == 0 {
                return None;
            }
        }
    }
    for span in &spans[..count] {
        let (distance, normal) = if span.enter > EPS {
            (span.enter, span.enter_normal)
        } else if span.exit > EPS {
            (span.exit, span.exit_normal)
        } else {
            continue;
        };
        let position = ray.origin + ray.dir * distance;
        return Some(Hit {
            distance,
            position,
            normal,
            material: STONE_MATERIAL,
            object_id: OBJECT_ID,
            local: position - CENTER,
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn front_ray(x: f32, y: f32) -> Ray {
        Ray {
            origin: CENTER + Vec3::new(x, y, 2.0),
            dir: -Vec3::Z,
        }
    }
    fn is_solid(local: Vec3) -> bool {
        GEOMETRY.worn.outside_distance(local) == 0.0
            && GEOMETRY
                .cuts
                .iter()
                .all(|cut| cut.outside_distance(local) > 0.0)
    }

    #[test]
    fn baseline_is_a_box_with_stable_coordinates() {
        let hit = trace(front_ray(0.6, 0.1), false).unwrap();
        assert!((hit.distance - (2.0 - HALF_EXTENTS.z)).abs() < 1e-6);
        assert_eq!(hit.normal, Vec3::Z);
        assert_eq!(hit.local, hit.position - CENTER);
        assert_eq!((hit.material, hit.object_id), (32, 700));
    }

    #[test]
    fn main_crack_and_branch_are_real_recesses() {
        for (x, y, min_depth) in [
            (-0.235, 0.36, 0.10),
            (-0.10, 0.19, 0.08),
            (0.54, 0.27, 0.055),
        ] {
            let ray = front_ray(x, y);
            let plain = trace(ray, false).unwrap();
            let worn = trace(ray, true).unwrap();
            assert!(
                worn.distance - plain.distance > min_depth,
                "no cavity at {x},{y}"
            );
            assert!(crack_distance(worn.local) < 1e-5);
            assert!((worn.normal.length() - 1.0).abs() < 1e-5);
        }
    }

    #[test]
    fn top_crack_and_missing_corner_change_geometry() {
        let ray = Ray {
            origin: CENTER + Vec3::new(-0.425, 2.0, 0.305),
            dir: -Vec3::Y,
        };
        assert!(trace(ray, true).unwrap().distance - trace(ray, false).unwrap().distance > 0.075);
        let corner = front_ray(1.26, 0.92);
        assert!(trace(corner, false).is_some());
        assert!(trace(corner, true).is_none());
    }

    #[test]
    fn inside_solid_and_inside_cavity_use_correct_boundaries() {
        let inside = trace(
            Ray {
                origin: CENTER,
                dir: -Vec3::Z,
            },
            true,
        )
        .unwrap();
        assert!((inside.distance - HALF_EXTENTS.z).abs() < 1e-5);
        assert_eq!(inside.normal, -Vec3::Z);
        let origin = CENTER + Vec3::new(-0.235, 0.36, 0.81);
        let into_stone = trace(
            Ray {
                origin,
                dir: -Vec3::Z,
            },
            true,
        )
        .unwrap();
        assert!(into_stone.distance > 0.05 && into_stone.distance < 0.15);
        assert!(into_stone.normal.dot(-Vec3::Z) < 0.0);
        assert!(trace(
            Ray {
                origin,
                dir: Vec3::Z
            },
            true
        )
        .is_none());
    }

    #[test]
    fn cavity_walls_cast_local_shadow_and_normals_point_out_of_solid() {
        let origin = CENTER + Vec3::new(-0.235, 0.36, 0.81);
        let tangent = Vec3::new(0.27, -0.34, 0.0).normalize();
        let ray = Ray {
            origin,
            dir: Vec3::Z.cross(tangent).normalize(),
        };
        let hit = trace(ray, true).unwrap();
        assert!(hit.distance > 0.005 && hit.distance < 0.06);
        assert!(is_solid(hit.local - hit.normal * 0.002));
        assert!(!is_solid(hit.local + hit.normal * 0.002));
    }

    #[test]
    fn parallel_grazing_and_outside_rays_are_finite_and_deterministic() {
        let rays = [
            front_ray(1.30, 0.0),
            front_ray(1.301, 0.0),
            Ray {
                origin: CENTER + Vec3::new(-3.0, 0.8, 0.7),
                dir: Vec3::X,
            },
            Ray {
                origin: CENTER + Vec3::new(-0.24, 0.36, 0.82),
                dir: Vec3::new(1.0, 0.0, -0.001).normalize(),
            },
        ];
        for ray in rays {
            let a = trace(ray, true);
            let b = trace(ray, true);
            assert_eq!(a.is_some(), b.is_some());
            if let (Some(a), Some(b)) = (a, b) {
                assert_eq!(a.distance.to_bits(), b.distance.to_bits());
                assert!(a.distance > 0.0 && a.distance.is_finite());
                assert!(a.normal.is_finite());
                assert!((a.normal.length() - 1.0).abs() < 1e-5);
            }
        }
    }

    #[test]
    fn material_fields_follow_actual_cuts_and_bevels() {
        assert_eq!(crack_distance(Vec3::new(-0.235, 0.36, 0.83)), 0.0);
        assert!(crack_distance(Vec3::new(0.9, -0.2, -0.8)) > 0.3);
        let edge = trace(front_ray(1.20, 0.0), true).unwrap();
        assert!(edge_wear(edge.local) > 0.7);
        assert!(edge_wear(Vec3::new(0.0, 0.0, HALF_EXTENTS.z)) < 0.1);
        for p in [Vec3::ZERO, HALF_EXTENTS, -HALF_EXTENTS, Vec3::splat(5.0)] {
            assert!((0.0..=1.0).contains(&edge_wear(p)));
            assert!(crack_distance(p) >= 0.0);
        }
    }
}
