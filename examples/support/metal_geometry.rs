//! Authored convex oval shield sample, queried as a union of analytic quadrics.
//! A broad, thin rim surrounds the domed body; eight front rivet heads protrude.
//! This is a solid material specimen, not a hollow sheet or a forging simulation.
use ascii_engine_lab::core::{Hit, Ray};
use glam::Vec3;
use std::sync::LazyLock;

pub const CENTER: Vec3 = Vec3::new(0.0, 1.1, 0.0);
pub const HALF_EXTENTS: Vec3 = Vec3::new(0.95, 1.1, 0.285);
pub const BODY: u32 = 50;
pub const RIM: u32 = 51;
pub const RIVET: u32 = 52;
const OBJECT_ID: u32 = 800;
const EPS: f64 = 0.0001;
const PART_COUNT: usize = 10;
const BODY_RADII: Vec3 = Vec3::new(0.865, 0.99, 0.285);

#[derive(Clone, Copy)]
struct Part {
    center: Vec3,
    radii: Vec3,
    material: u32,
}

static PARTS: LazyLock<[Part; PART_COUNT]> = LazyLock::new(|| {
    std::array::from_fn(|i| match i {
        0 => Part {
            center: Vec3::ZERO,
            radii: BODY_RADII,
            material: BODY,
        },
        1 => Part {
            center: Vec3::ZERO,
            radii: Vec3::new(0.95, 1.1, 0.115),
            material: RIM,
        },
        _ => {
            let angle = (i - 2) as f32 * std::f32::consts::FRAC_PI_4;
            let (sin, cos) = angle.sin_cos();
            let x = 0.79 * cos;
            let y = 0.92 * sin;
            let z = BODY_RADII.z
                * (1.0 - (x / BODY_RADII.x).powi(2) - (y / BODY_RADII.y).powi(2)).sqrt();
            Part {
                center: Vec3::new(x, y, z + 0.018),
                radii: Vec3::new(0.055, 0.055, 0.046),
                material: RIVET,
            }
        }
    })
});

#[derive(Clone, Copy)]
struct Interval {
    enter: f64,
    exit: f64,
    enter_part: usize,
    exit_part: usize,
}
impl Interval {
    const EMPTY: Self = Self {
        enter: 0.0,
        exit: 0.0,
        enter_part: 0,
        exit_part: 0,
    };
}

fn interval(ray: Ray, part: Part, index: usize) -> Option<Interval> {
    // Evaluate the quadratic in f64 to reduce cancellation near grazing hits.
    // Coordinates remain the caller's f32 data; this does not add geometry detail.
    let origin = (ray.origin.as_dvec3() - part.center.as_dvec3()) / part.radii.as_dvec3();
    let direction = ray.dir.as_dvec3() / part.radii.as_dvec3();
    let a = direction.length_squared();
    let b = origin.dot(direction);
    let c = origin.length_squared() - 1.0;
    let discriminant = b * b - a * c;
    if discriminant < 0.0 || a == 0.0 {
        return None;
    }
    let root = discriminant.sqrt();
    // Stable quadratic roots, including an exact tangent at the origin.
    let q = -b - root.copysign(b);
    let (first, second) = if q == 0.0 {
        (-b / a, -b / a)
    } else {
        (q / a, c / q)
    };
    let enter = first.min(second);
    let exit = first.max(second);
    (exit > EPS).then_some(Interval {
        enter,
        exit,
        enter_part: index,
        exit_part: index,
    })
}

/// Nearest exterior boundary of the complete union; internal overlapping part
/// boundaries cannot occlude rays launched inside the solid or inside a rivet.
/// Every finish uses this same shape, object ID and object-local coordinate frame.
pub fn trace(ray: Ray) -> Option<Hit> {
    if !ray.origin.is_finite() || !ray.dir.is_finite() || ray.dir == Vec3::ZERO {
        return None;
    }
    let local_ray = Ray {
        origin: ray.origin - CENTER,
        dir: ray.dir,
    };
    if !may_hit_bounds(local_ray) {
        return None;
    }
    trace_inner(ray, local_ray)
}

/// Conservative rejection only: do not move the ray to the box entry or reuse
/// its distances as surface hits. Inclusive slabs retain tangent/inside rays.
fn may_hit_bounds(ray: Ray) -> bool {
    let extents = HALF_EXTENTS.as_dvec3() + glam::DVec3::splat(1e-5);
    let origin = ray.origin.as_dvec3();
    let direction = ray.dir.as_dvec3();
    let mut near = f64::NEG_INFINITY;
    let mut far = f64::INFINITY;
    for axis in 0..3 {
        // A small nonzero direction can still enter a distant slab. Treat only
        // exact zero as parallel; this also handles negative zero without NaNs.
        if direction[axis] == 0.0 {
            if origin[axis] < -extents[axis] || origin[axis] > extents[axis] {
                return false;
            }
        } else {
            let a = (-extents[axis] - origin[axis]) / direction[axis];
            let b = (extents[axis] - origin[axis]) / direction[axis];
            near = near.max(a.min(b));
            far = far.min(a.max(b));
            if near > far {
                return false;
            }
        }
    }
    far > EPS
}

// Unmodified exact union query, also used by the tests to bypass broad phase.
fn trace_inner(ray: Ray, local_ray: Ray) -> Option<Hit> {
    let mut intervals = [Interval::EMPTY; PART_COUNT];
    let mut count = 0;
    for (index, part) in PARTS.iter().enumerate() {
        if let Some(span) = interval(local_ray, *part, index) {
            // Small fixed-size insertion sort; no allocation or per-ray trig.
            let mut at = count;
            while at > 0 && intervals[at - 1].enter > span.enter {
                intervals[at] = intervals[at - 1];
                at -= 1;
            }
            intervals[at] = span;
            count += 1;
        }
    }
    if count == 0 {
        return None;
    }
    let mut visible = intervals[0];
    for span in &intervals[1..count] {
        if span.enter > visible.exit {
            break;
        }
        if span.exit > visible.exit {
            visible.exit = span.exit;
            visible.exit_part = span.exit_part;
        }
    }
    let (distance, index) = if visible.enter > EPS {
        (visible.enter, visible.enter_part)
    } else {
        (visible.exit, visible.exit_part)
    };
    let distance = distance as f32;
    if !distance.is_finite() {
        return None;
    }
    let position = ray.origin + ray.dir * distance;
    let local = position - CENTER;
    let part = PARTS[index];
    let normal = ((local - part.center) / (part.radii * part.radii)).normalize();
    Some(Hit {
        distance,
        position,
        normal,
        material: part.material,
        object_id: OBJECT_ID,
        local,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn front(x: f32, y: f32) -> Ray {
        Ray {
            origin: CENTER + Vec3::new(x, y, 2.0),
            dir: -Vec3::Z,
        }
    }
    fn solid(local: Vec3) -> bool {
        PARTS
            .iter()
            .any(|p| ((local - p.center) / p.radii).length_squared() < 1.0)
    }

    fn assert_unbounded_parity(ray: Ray) {
        let guarded = trace(ray);
        let exact = trace_inner(
            ray,
            Ray {
                origin: ray.origin - CENTER,
                dir: ray.dir,
            },
        );
        assert_eq!(
            guarded.is_some(),
            exact.is_some(),
            "hit/miss changed for {ray:?}"
        );
        if let (Some(a), Some(b)) = (guarded, exact) {
            assert_eq!(a.distance.to_bits(), b.distance.to_bits());
            assert_eq!(
                a.position.to_array().map(f32::to_bits),
                b.position.to_array().map(f32::to_bits)
            );
            assert_eq!(
                a.normal.to_array().map(f32::to_bits),
                b.normal.to_array().map(f32::to_bits)
            );
            assert_eq!(
                a.local.to_array().map(f32::to_bits),
                b.local.to_array().map(f32::to_bits)
            );
            assert_eq!((a.material, a.object_id), (b.material, b.object_id));
        }
    }

    #[test]
    fn bounds_preserve_exact_hits_for_world_grid_inside_grazing_and_shadows() {
        // World-space origins include sky, floor, either side and the solid.
        for x in -3..=3 {
            for y in -1..=4 {
                for z in -3..=3 {
                    let origin = Vec3::new(x as f32, y as f32 * 0.7, z as f32);
                    for dir in [
                        Vec3::X,
                        -Vec3::X,
                        Vec3::Y,
                        -Vec3::Y,
                        Vec3::Z,
                        -Vec3::Z,
                        (CENTER - origin).normalize_or_zero(),
                        Vec3::new(1e-9, 0.0, -1.0),
                    ] {
                        if dir != Vec3::ZERO {
                            assert_unbounded_parity(Ray { origin, dir });
                        }
                    }
                }
            }
        }
        for x in [
            0.0,
            0.79,
            HALF_EXTENTS.x - 1e-6,
            HALF_EXTENTS.x,
            HALF_EXTENTS.x + 1e-6,
        ] {
            for dir in [
                -Vec3::Z,
                Vec3::new(1e-9, -1e-9, -1.0),
                Vec3::new(-1e-9, 0.0, -1.0),
            ] {
                assert_unbounded_parity(Ray {
                    origin: front(x, 0.0).origin,
                    dir,
                });
            }
        }
        for part in PARTS.iter() {
            for dir in [Vec3::X, Vec3::Z, -Vec3::Z, Vec3::new(1e-9, 1.0, 0.0)] {
                assert_unbounded_parity(Ray {
                    origin: CENTER + part.center,
                    dir,
                });
            }
        }
        for x in -9..=9 {
            for y in -10..=10 {
                let ray = front(x as f32 * 0.1, y as f32 * 0.1);
                let local_ray = Ray {
                    origin: ray.origin - CENTER,
                    dir: ray.dir,
                };
                if let Some(hit) = trace_inner(ray, local_ray) {
                    for side in [-1.0, 1.0] {
                        let origin = hit.position + hit.normal * (0.001 * side);
                        for target in [
                            Vec3::new(-2.4, 3.4, 4.2),
                            Vec3::new(2.4, 3.4, 4.2),
                            Vec3::new(0.0, 2.0, -4.0),
                        ] {
                            assert_unbounded_parity(Ray {
                                origin,
                                dir: (target - origin).normalize(),
                            });
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn dome_rim_and_eight_rivets_have_distinct_real_surfaces() {
        let body = trace(front(0.0, 0.0)).unwrap();
        assert_eq!(body.material, BODY);
        assert!((body.local.z - BODY_RADII.z).abs() < 1e-5);
        assert_eq!(body.normal, Vec3::Z);
        assert_eq!(trace(front(0.91, 0.0)).unwrap().material, RIM);
        for rivet in &PARTS[2..] {
            let hit = trace(front(rivet.center.x, rivet.center.y)).unwrap();
            assert_eq!(hit.material, RIVET);
            assert!((hit.local.z - rivet.center.z - rivet.radii.z).abs() < 1e-5);
            assert_eq!(hit.object_id, 800);
            assert_eq!(hit.local, hit.position - CENTER);
        }
    }

    #[test]
    fn inside_rays_skip_all_internal_part_boundaries() {
        let center = trace(Ray {
            origin: CENTER,
            dir: Vec3::Z,
        })
        .unwrap();
        assert!((center.distance - BODY_RADII.z).abs() < 1e-5);
        let rivet = PARTS[2];
        let hit = trace(Ray {
            origin: CENTER + rivet.center,
            dir: -Vec3::Z,
        })
        .unwrap();
        assert_eq!(hit.material, BODY);
        assert!(hit.local.z < -0.10);
        assert!(hit.normal.z < 0.0);
    }

    #[test]
    fn tangent_axis_parallel_miss_and_nonunit_rays_are_well_defined() {
        let tangent = trace(front(HALF_EXTENTS.x, 0.0)).unwrap();
        assert!((tangent.distance - 2.0).abs() < 1e-5);
        assert!((tangent.normal - Vec3::X).length() < 1e-5);
        assert!(trace(front(HALF_EXTENTS.x + 0.001, 0.0)).is_none());
        let across = trace(Ray {
            origin: CENTER + Vec3::new(-2.0, 0.0, 0.0),
            dir: Vec3::X,
        })
        .unwrap();
        assert_eq!(across.material, RIM);
        assert!((across.distance - (2.0 - HALF_EXTENTS.x)).abs() < 1e-5);
        let mut ray = front(0.0, 0.0);
        ray.dir *= 2.0;
        assert!((trace(ray).unwrap().distance - (2.0 - BODY_RADII.z) * 0.5).abs() < 1e-5);
        assert!(trace(Ray {
            origin: CENTER,
            dir: Vec3::ZERO
        })
        .is_none());
        assert!(trace(Ray {
            origin: Vec3::splat(f32::NAN),
            dir: Vec3::Z
        })
        .is_none());
    }

    #[test]
    fn geometric_normals_separate_solid_and_support_shadow_offsets() {
        for (x, y) in [(0.0, 0.0), (0.5, 0.4), (0.91, 0.0), (0.79, 0.0)] {
            let hit = trace(front(x, y)).unwrap();
            assert!((hit.normal.length() - 1.0).abs() < 1e-5);
            assert!(solid(hit.local - hit.normal * 0.001));
            assert!(!solid(hit.local + hit.normal * 0.001));
            let outside = hit.position + hit.normal * 0.001;
            assert!(trace(Ray {
                origin: outside,
                dir: hit.normal
            })
            .is_none());
            assert!(
                trace(Ray {
                    origin: outside,
                    dir: -hit.normal
                })
                .unwrap()
                .distance
                    < 0.002
            );
        }
    }

    #[test]
    fn bounds_are_conservative_and_static_queries_repeat() {
        for part in PARTS.iter() {
            assert!((part.center.abs() + part.radii).cmple(HALF_EXTENTS).all());
        }
        for x in -10..=10 {
            for y in -11..=11 {
                let ray = front(x as f32 * 0.1, y as f32 * 0.1);
                if let Some(a) = trace(ray) {
                    let b = trace(ray).unwrap();
                    assert_eq!(a.distance.to_bits(), b.distance.to_bits());
                    assert_eq!(a.material, b.material);
                    assert!(a.local.abs().cmple(HALF_EXTENTS + Vec3::splat(1e-5)).all());
                    assert!(a.normal.is_finite());
                }
            }
        }
    }
}
