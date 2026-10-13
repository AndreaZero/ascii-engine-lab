//! Analytic demonstration figure: no meshes and no gameplay/locomotion controller.
//! Cloth is a two-sided procedural sheet, not a physical cloth simulation.
use glam::Vec3;

use crate::core::{intersect_ellipsoid, Hit, Ray, Stage, CLOTH, EMISSIVE, METAL, SKIN};

const CENTER_X: f32 = 1.0;
const CLOTH_BOTTOM: f32 = 0.32;
const CLOTH_TOP: f32 = 1.50;
const CAPE_ID: u32 = 130;
const EPS: f32 = 0.0001;

/// The same query is used for camera, shadow and reflection rays at the same time.
/// The figure faces +Z. Coordinates are metres; its height is about 1.84 m.
pub fn trace(ray: Ray, time: f32, stage: Stage) -> Option<Hit> {
    interval(
        ray,
        Vec3::new(0.45, 0.0, -0.50),
        Vec3::new(1.55, 1.90, 0.30),
    )?;
    let active = matches!(stage, Stage::Cloth);
    let breath = if active {
        (time * 1.6).sin() * 0.008
    } else {
        0.0
    };
    let sway = if active {
        (time * 1.9).sin() * 0.032
    } else {
        0.0
    };
    let mut nearest = None;
    // Persistent object IDs and local coordinates do not depend on the camera.
    ellipsoid(
        &mut nearest,
        ray,
        Vec3::new(1.0, 1.08 + breath, 0.0),
        Vec3::new(0.25, 0.34, 0.17),
        CLOTH,
        100,
    );
    ellipsoid(
        &mut nearest,
        ray,
        Vec3::new(1.0, 0.75, 0.0),
        Vec3::new(0.22, 0.16, 0.15),
        CLOTH,
        103,
    );
    ellipsoid(
        &mut nearest,
        ray,
        Vec3::new(1.0, 1.65 + breath, -0.015),
        Vec3::new(0.15, 0.19, 0.15),
        CLOTH,
        102,
    );
    ellipsoid(
        &mut nearest,
        ray,
        Vec3::new(1.0, 1.64 + breath, 0.11),
        Vec3::new(0.095, 0.125, 0.065),
        SKIN,
        101,
    );

    for (i, side) in [-1.0_f32, 1.0].into_iter().enumerate() {
        let index = i as u32;
        let arm_z = 0.015 + side * sway;
        ellipsoid(
            &mut nearest,
            ray,
            Vec3::new(1.0 + side * 0.125, 0.40, 0.0),
            Vec3::new(0.085, 0.30, 0.095),
            CLOTH,
            110 + index,
        );
        ellipsoid(
            &mut nearest,
            ray,
            Vec3::new(1.0 + side * 0.125, 0.095, 0.07),
            Vec3::new(0.105, 0.09, 0.16),
            METAL,
            112 + index,
        );
        ellipsoid(
            &mut nearest,
            ray,
            Vec3::new(1.0 + side * 0.285, 1.36 + breath, 0.0),
            Vec3::new(0.115, 0.11, 0.145),
            METAL,
            124 + index,
        );
        ellipsoid(
            &mut nearest,
            ray,
            Vec3::new(1.0 + side * 0.345, 1.07 + breath * 0.5, arm_z),
            Vec3::new(0.075, 0.26, 0.075),
            CLOTH,
            120 + index,
        );
        ellipsoid(
            &mut nearest,
            ray,
            Vec3::new(1.0 + side * 0.36, 0.79 + breath * 0.5, arm_z * 1.6),
            Vec3::new(0.072, 0.085, 0.075),
            SKIN,
            122 + index,
        );
    }
    // A small carried glow is semantic geometry; world lighting owns its lights.
    ellipsoid(
        &mut nearest,
        ray,
        Vec3::new(1.36, 0.59 + breath * 0.5, 0.09 + sway * 1.6),
        Vec3::new(0.06, 0.085, 0.06),
        EMISSIVE,
        140,
    );
    if let Some(hit) = trace_cloth(ray, time, active) {
        keep_nearest(&mut nearest, hit);
    }
    nearest
}

fn ellipsoid(
    nearest: &mut Option<Hit>,
    ray: Ray,
    center: Vec3,
    radii: Vec3,
    material: u32,
    object_id: u32,
) {
    if let Some((distance, normal)) = intersect_ellipsoid(ray, center, radii) {
        let position = ray.origin + ray.dir * distance;
        keep_nearest(
            nearest,
            Hit {
                distance,
                position,
                normal,
                material,
                object_id,
                local: (position - center) / radii,
            },
        );
    }
}

fn keep_nearest(nearest: &mut Option<Hit>, hit: Hit) {
    if nearest
        .as_ref()
        .is_none_or(|current| hit.distance < current.distance)
    {
        *nearest = Some(hit);
    }
}

/// A full slab interval is needed for bounded sheet root finding, including rays
/// that originate inside the bounds. Unlike a box hit, this returns both ends.
fn interval(ray: Ray, min: Vec3, max: Vec3) -> Option<(f32, f32)> {
    let mut near = EPS;
    let mut far = f32::INFINITY;
    for axis in 0..3 {
        if ray.dir[axis].abs() < 1e-8 {
            if ray.origin[axis] < min[axis] || ray.origin[axis] > max[axis] {
                return None;
            }
        } else {
            let a = (min[axis] - ray.origin[axis]) / ray.dir[axis];
            let b = (max[axis] - ray.origin[axis]) / ray.dir[axis];
            near = near.max(a.min(b));
            far = far.min(a.max(b));
            if near > far {
                return None;
            }
        }
    }
    if far.is_finite() && far >= near {
        Some((near, far))
    } else {
        None
    }
}

fn cloth_v(y: f32) -> f32 {
    (y - CLOTH_BOTTOM) / (CLOTH_TOP - CLOTH_BOTTOM)
}
fn half_width(v: f32) -> f32 {
    0.27 + 0.22 * (1.0 - v)
}

/// Lagrangian (u,v) remains attached to the sheet while its depth changes.
/// u spans -1..1, v spans 0..1. Shoulders stay fixed; lower regions deform locally.
fn cloth_z(u: f32, v: f32, time: f32, active: bool) -> f32 {
    let fall = 1.0 - v;
    let folds = (u * std::f32::consts::PI * 3.0).sin() * 0.018;
    let wave = if active {
        fall * fall
            * ((time * 2.6 + v * 5.0 + u * 2.0).sin() * 0.075
                + (time * 4.1 - v * 8.0 + u * 3.0).sin() * 0.035)
    } else {
        0.0
    };
    -0.24 - fall * 0.09 + folds + wave
}

fn sheet_depth(x: f32, y: f32, time: f32, active: bool) -> f32 {
    let v = cloth_v(y);
    cloth_z((x - CENTER_X) / half_width(v), v, time, active)
}

fn field(point: Vec3, time: f32, active: bool) -> f32 {
    point.z - sheet_depth(point.x, point.y, time, active)
}

fn trace_cloth(ray: Ray, time: f32, active: bool) -> Option<Hit> {
    let (near, far) = interval(
        ray,
        Vec3::new(0.51, CLOTH_BOTTOM, -0.49),
        Vec3::new(1.49, CLOTH_TOP, -0.18),
    )?;
    let mut left = near;
    let mut left_value = field(ray.origin + ray.dir * left, time, active);
    // Fixed work: narrow bounds, 24 brackets and at most 6 bisections per crossing.
    // This is an approximate root search: grazing double roots can be missed.
    for step in 1..=24 {
        let right = near + (far - near) * step as f32 / 24.0;
        let right_value = field(ray.origin + ray.dir * right, time, active);
        if left_value * right_value <= 0.0 {
            let mut a = left;
            let mut b = right;
            let mut fa = left_value;
            for _ in 0..6 {
                let middle = (a + b) * 0.5;
                let fm = field(ray.origin + ray.dir * middle, time, active);
                if fa * fm <= 0.0 {
                    b = middle;
                } else {
                    a = middle;
                    fa = fm;
                }
            }
            let distance = (a + b) * 0.5;
            let position = ray.origin + ray.dir * distance;
            let v = cloth_v(position.y);
            let u = (position.x - CENTER_X) / half_width(v);
            if u.abs() <= 1.0 && (0.0..=1.0).contains(&v) {
                let h = 0.001;
                let dx = (sheet_depth(position.x + h, position.y, time, active)
                    - sheet_depth(position.x - h, position.y, time, active))
                    / (2.0 * h);
                let dy = (sheet_depth(position.x, position.y + h, time, active)
                    - sheet_depth(position.x, position.y - h, time, active))
                    / (2.0 * h);
                let mut normal = Vec3::new(-dx, -dy, 1.0).normalize();
                // Both faces are opaque and receive correctly facing lighting.
                if normal.dot(ray.dir) > 0.0 {
                    normal = -normal;
                }
                return Some(Hit {
                    distance,
                    position,
                    normal,
                    material: CLOTH,
                    object_id: CAPE_ID,
                    local: Vec3::new(u * 0.5 + 0.5, v, 0.0),
                });
            }
        }
        left = right;
        left_value = right_value;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_stages_and_repeated_time_are_deterministic() {
        let ray = Ray {
            origin: Vec3::new(1.35, 0.65, -2.0),
            dir: Vec3::Z,
        };
        let a = trace(ray, 0.0, Stage::Core).unwrap();
        for stage in [Stage::Core, Stage::Light, Stage::Water] {
            let b = trace(ray, 123.0, stage).unwrap();
            assert_eq!(a.distance, b.distance);
            assert_eq!(a.local, b.local);
        }
        let a = trace(ray, 1.3, Stage::Cloth).unwrap();
        let b = trace(ray, 1.3, Stage::Cloth).unwrap();
        assert_eq!(a.position, b.position);
        assert_eq!(a.normal, b.normal);
        assert_eq!(a.local, b.local);
    }

    #[test]
    fn cloth_deforms_locally_and_keeps_shoulders_anchored() {
        assert_eq!(cloth_z(0.3, 1.0, 0.0, true), cloth_z(0.3, 1.0, 1.0, true));
        let delta_a = cloth_z(-0.4, 0.1, 1.0, true) - cloth_z(-0.4, 0.1, 0.0, true);
        let delta_b = cloth_z(0.4, 0.1, 1.0, true) - cloth_z(0.4, 0.1, 0.0, true);
        assert!(delta_a.abs() > 0.01);
        assert!((delta_a - delta_b).abs() > 0.01);
    }

    #[test]
    fn cloth_hits_from_both_sides_and_from_nearby() {
        let x = 1.35;
        let y = 0.65;
        let z = sheet_depth(x, y, 1.3, true);
        for (offset, direction) in [(1.0, -Vec3::Z), (-1.0, Vec3::Z), (0.005, -Vec3::Z)] {
            let ray = Ray {
                origin: Vec3::new(x, y, z + offset),
                dir: direction,
            };
            let hit = trace_cloth(ray, 1.3, true).unwrap();
            assert_eq!(hit.object_id, CAPE_ID);
            assert!((hit.distance - offset.abs()).abs() < 0.0003);
            assert!(hit.normal.dot(direction) < 0.0);
            assert!(hit.normal.is_finite());
        }
    }

    #[test]
    fn material_coordinates_follow_the_same_moving_point() {
        let u = 0.72;
        let v = 0.26;
        let x = CENTER_X + u * half_width(v);
        let y = CLOTH_BOTTOM + v * (CLOTH_TOP - CLOTH_BOTTOM);
        for time in [0.0, 0.25, 1.0, 2.5] {
            let hit = trace_cloth(
                Ray {
                    origin: Vec3::new(x, y, -2.0),
                    dir: Vec3::Z,
                },
                time,
                true,
            )
            .unwrap();
            assert!(hit.local.abs_diff_eq(Vec3::new(0.86, v, 0.0), 1e-5));
        }
    }

    #[test]
    fn empty_space_and_thin_leg_gap_remain_empty() {
        assert!(trace(
            Ray {
                origin: Vec3::new(3.0, 1.0, 2.0),
                dir: -Vec3::Z
            },
            0.0,
            Stage::Core
        )
        .is_none());
        assert!(trace(
            Ray {
                origin: Vec3::new(1.0, 0.15, 2.0),
                dir: -Vec3::Z
            },
            0.0,
            Stage::Core
        )
        .is_none());
    }
}
