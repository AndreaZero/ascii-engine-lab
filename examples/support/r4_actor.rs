//! Analytic demonstration figure: no meshes and no gameplay/locomotion controller.
//! Cloth is a two-sided procedural sheet, not a physical cloth simulation.
use glam::Vec3;

use crate::core::{intersect_ellipsoid, Hit, Ray, Stage, CLOTH, EMISSIVE, METAL, SKIN};

const CENTER_X: f32 = 1.0;
const CLOTH_BOTTOM: f32 = 0.32;
const CLOTH_TOP: f32 = 1.50;
const CAPE_ID: u32 = 130;
const EPS: f32 = 0.0001;

#[derive(Clone, Copy)]
struct Part {
    center: Vec3,
    radii: Vec3,
    material: u32,
    object_id: u32,
}

impl Part {
    fn new(center: Vec3, radii: Vec3, material: u32, object_id: u32) -> Self {
        Self {
            center,
            radii,
            material,
            object_id,
        }
    }
}

/// Immutable analytic pose, prepared once per frame and shared by all rays.
/// The fixed-size part array has no heap allocation. Cloth retains the R2 solver.
#[derive(Clone, Copy)]
pub struct ActorSnapshot {
    parts: [Part; 15],
    time: f32,
    active: bool,
}

impl ActorSnapshot {
    pub fn new(time: f32, stage: Stage) -> Self {
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
        let mut parts = [Part::new(Vec3::ZERO, Vec3::ONE, 0, 0); 15];
        // Persistent object IDs and local coordinates do not depend on the camera.
        parts[0] = Part::new(
            Vec3::new(1.0, 1.08 + breath, 0.0),
            Vec3::new(0.25, 0.34, 0.17),
            CLOTH,
            100,
        );
        parts[1] = Part::new(
            Vec3::new(1.0, 0.75, 0.0),
            Vec3::new(0.22, 0.16, 0.15),
            CLOTH,
            103,
        );
        parts[2] = Part::new(
            Vec3::new(1.0, 1.65 + breath, -0.015),
            Vec3::new(0.15, 0.19, 0.15),
            CLOTH,
            102,
        );
        parts[3] = Part::new(
            Vec3::new(1.0, 1.64 + breath, 0.11),
            Vec3::new(0.095, 0.125, 0.065),
            SKIN,
            101,
        );

        for (i, side) in [-1.0_f32, 1.0].into_iter().enumerate() {
            let index = i as u32;
            let arm_z = 0.015 + side * sway;
            parts[4 + i * 5] = Part::new(
                Vec3::new(1.0 + side * 0.125, 0.40, 0.0),
                Vec3::new(0.085, 0.30, 0.095),
                CLOTH,
                110 + index,
            );
            parts[5 + i * 5] = Part::new(
                Vec3::new(1.0 + side * 0.125, 0.095, 0.07),
                Vec3::new(0.105, 0.09, 0.16),
                METAL,
                112 + index,
            );
            parts[6 + i * 5] = Part::new(
                Vec3::new(1.0 + side * 0.285, 1.36 + breath, 0.0),
                Vec3::new(0.115, 0.11, 0.145),
                METAL,
                124 + index,
            );
            parts[7 + i * 5] = Part::new(
                Vec3::new(1.0 + side * 0.345, 1.07 + breath * 0.5, arm_z),
                Vec3::new(0.075, 0.26, 0.075),
                CLOTH,
                120 + index,
            );
            parts[8 + i * 5] = Part::new(
                Vec3::new(1.0 + side * 0.36, 0.79 + breath * 0.5, arm_z * 1.6),
                Vec3::new(0.072, 0.085, 0.075),
                SKIN,
                122 + index,
            );
        }
        // A small carried glow is semantic geometry; world lighting owns its lights.
        parts[14] = Part::new(
            Vec3::new(1.36, 0.59 + breath * 0.5, 0.09 + sway * 1.6),
            Vec3::new(0.06, 0.085, 0.06),
            EMISSIVE,
            140,
        );
        Self {
            parts,
            time,
            active,
        }
    }

    /// The same immutable pose serves primary, shadow and reflection queries.
    /// The figure faces +Z. Coordinates are metres; height is about 1.84 m.
    pub fn trace(&self, ray: Ray) -> Option<Hit> {
        actor_interval(ray)?;
        self.trace_inside(ray)
    }

    fn trace_inside(&self, ray: Ray) -> Option<Hit> {
        let mut nearest = None;
        // Preserve R2 visitation order, including equal-distance tie breaking.
        for part in &self.parts {
            ellipsoid(
                &mut nearest,
                ray,
                part.center,
                part.radii,
                part.material,
                part.object_id,
            );
        }
        if let Some(hit) = trace_cloth(ray, self.time, self.active) {
            keep_nearest(&mut nearest, hit);
        }
        nearest
    }
}

/// Compatibility query; callers with many rays should prepare ActorSnapshot once.
pub fn trace(ray: Ray, time: f32, stage: Stage) -> Option<Hit> {
    // Preserve cheap rejection before pose preparation for legacy one-ray callers.
    actor_interval(ray)?;
    ActorSnapshot::new(time, stage).trace_inside(ray)
}

fn actor_interval(ray: Ray) -> Option<(f32, f32)> {
    interval(
        ray,
        Vec3::new(0.45, 0.0, -0.50),
        Vec3::new(1.55, 1.90, 0.30),
    )
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
