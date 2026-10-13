//! Analytic demonstration figure: no meshes and no gameplay/locomotion controller.
//! Weighted idle pose and cloth are procedural; no cloth/body collision solver.
use crate::core::{
    intersect_ellipsoid, Hit, Ray, Stage, CAPE, CAPE_TRIM, CLOTH, EMISSIVE, LEATHER, METAL, SKIN,
};
use glam::{Quat, Vec3};

const CENTER_X: f32 = 1.0;
const CLOTH_BOTTOM: f32 = 0.25;
const CLOTH_TOP: f32 = 1.51;
const CAPE_ID: u32 = 130;
const EPS: f32 = 0.0001;
const ACTOR_MIN: Vec3 = Vec3::new(0.25, -0.01, -0.70);
const ACTOR_MAX: Vec3 = Vec3::new(1.75, 2.07, 0.55);
const CLOTH_MIN: Vec3 = Vec3::new(0.30, CLOTH_BOTTOM, -0.65);
const CLOTH_MAX: Vec3 = Vec3::new(1.70, CLOTH_TOP, -0.13);

#[derive(Clone, Copy)]
struct Part {
    center: Vec3,
    radii: Vec3,
    rotation: Quat,
    material: u32,
    object_id: u32,
}
impl Part {
    fn new(center: Vec3, radii: Vec3, material: u32, object_id: u32) -> Self {
        Self {
            center,
            radii,
            rotation: Quat::IDENTITY,
            material,
            object_id,
        }
    }
    fn limb(a: Vec3, b: Vec3, radius: f32, material: u32, object_id: u32) -> Self {
        let delta = b - a;
        Self {
            center: (a + b) * 0.5,
            radii: Vec3::new(radius, delta.length() * 0.5 + radius * 0.35, radius),
            rotation: Quat::from_rotation_arc(Vec3::Y, delta.normalize()),
            material,
            object_id,
        }
    }
    fn trace(&self, ray: Ray) -> Option<Hit> {
        let inverse = self.rotation.conjugate();
        let local_ray = Ray {
            origin: inverse * (ray.origin - self.center),
            dir: inverse * ray.dir,
        };
        let (distance, local_normal) = intersect_ellipsoid(local_ray, Vec3::ZERO, self.radii)?;
        let position = ray.origin + ray.dir * distance;
        Some(Hit {
            distance,
            position,
            normal: (self.rotation * local_normal).normalize(),
            material: self.material,
            object_id: self.object_id,
            local: (inverse * (position - self.center)) / self.radii,
        })
    }
}

/// Immutable analytic pose shared by primary, shadow and reflection queries.
/// Local coordinates rotate with each limb; no per-ray allocation or pose update.
#[derive(Clone, Copy)]
pub struct ActorSnapshot {
    parts: [Part; 27],
    time: f32,
    active: bool,
}
impl ActorSnapshot {
    pub fn new(time: f32, stage: Stage) -> Self {
        let active = matches!(stage, Stage::Cloth);
        let breath = if active {
            (time * 1.65).sin() * 0.006
        } else {
            0.0
        };
        let weight = if active {
            (time * 1.05).sin() * 0.012
        } else {
            0.0
        };
        let gesture = if active {
            (time * 1.8).sin() * 0.028
        } else {
            0.0
        };
        let mut parts = [Part::new(Vec3::ZERO, Vec3::ONE, 0, 0); 27];
        // Narrow waist, heavier shoulders, pointed hood and a recessed small face.
        parts[0] = Part::new(
            Vec3::new(1.0 + weight * 0.35, 1.16 + breath, 0.0),
            Vec3::new(0.235 + breath * 0.3, 0.295, 0.145 + breath * 0.4),
            CLOTH,
            100,
        );
        parts[1] = Part::new(
            Vec3::new(1.0 + weight, 0.79, -0.005),
            Vec3::new(0.195, 0.145, 0.14),
            CLOTH,
            103,
        );
        parts[2] = Part::new(
            Vec3::new(1.0, 1.70 + breath * 0.3, -0.025),
            Vec3::new(0.165, 0.20, 0.16),
            CLOTH,
            102,
        );
        parts[3] = Part::new(
            Vec3::new(1.0, 1.69 + breath * 0.3, 0.115),
            Vec3::new(0.078, 0.105, 0.043),
            SKIN,
            101,
        );
        parts[4] = Part::new(
            Vec3::new(1.0 + weight, 0.89, 0.015),
            Vec3::new(0.219, 0.047, 0.16),
            LEATHER,
            104,
        );
        parts[5] = Part::new(
            Vec3::new(1.0 + weight, 0.89, 0.174),
            Vec3::new(0.041, 0.034, 0.016),
            METAL,
            105,
        );
        parts[6] = Part::new(
            Vec3::new(1.0, 1.475, -0.02),
            Vec3::new(0.215, 0.065, 0.17),
            LEATHER,
            106,
        );
        parts[7] = Part::new(
            Vec3::new(1.0, 1.87 + breath * 0.3, -0.078),
            Vec3::new(0.10, 0.13, 0.095),
            CLOTH,
            107,
        );
        let mut lantern_hand = Vec3::ZERO;
        for (i, side) in [-1.0_f32, 1.0].into_iter().enumerate() {
            let index = i as u32;
            let hip = Vec3::new(1.0 + side * 0.122 + weight, 0.78, -0.01);
            let ankle = Vec3::new(1.0 + side * 0.14, 0.19, side * 0.032);
            let knee = Vec3::new(
                1.0 + side * 0.153 + weight * 0.3,
                0.47,
                0.027 + side * 0.018,
            );
            // Boots and ankles stay planted: this is a weight shift, not a walk cycle.
            parts[8 + i * 8] = Part::limb(hip, knee, 0.083, CLOTH, 110 + index);
            parts[9 + i * 8] = Part::limb(knee, ankle, 0.067, CLOTH, 114 + index);
            parts[10 + i * 8] = Part::new(
                Vec3::new(ankle.x, 0.105, ankle.z + 0.062),
                Vec3::new(0.105, 0.10, 0.17),
                LEATHER,
                112 + index,
            );
            let shoulder = Vec3::new(1.0 + side * 0.285, 1.395, 0.0);
            let elbow = Vec3::new(
                1.0 + side * (0.375 + gesture * 0.22),
                1.16,
                0.025 + side * gesture,
            );
            let hand = Vec3::new(
                1.0 + side * (0.425 + gesture * 0.3),
                0.94 + side * gesture * 0.4,
                0.105 + side * gesture * 1.7,
            );
            parts[11 + i * 8] =
                Part::new(shoulder, Vec3::new(0.12, 0.087, 0.125), METAL, 124 + index);
            parts[12 + i * 8] = Part::limb(shoulder, elbow, 0.074, CLOTH, 120 + index);
            parts[13 + i * 8] = Part::limb(elbow, hand, 0.063, CLOTH, 126 + index);
            parts[14 + i * 8] =
                Part::new(hand, Vec3::new(0.065, 0.074, 0.063), LEATHER, 122 + index);
            parts[15 + i * 8] = Part::limb(
                elbow.lerp(hand, 0.53),
                elbow.lerp(hand, 0.83),
                0.071,
                LEATHER,
                128 + index,
            );
            if i == 1 {
                lantern_hand = hand;
            }
        }
        let lantern = lantern_hand + Vec3::new(0.0, -0.19, 0.018);
        parts[24] = Part::new(lantern, Vec3::new(0.059, 0.075, 0.059), EMISSIVE, 140);
        parts[25] = Part::new(
            lantern + Vec3::Y * 0.081,
            Vec3::new(0.079, 0.021, 0.079),
            METAL,
            141,
        );
        parts[26] = Part::new(
            lantern - Vec3::Y * 0.081,
            Vec3::new(0.079, 0.021, 0.079),
            METAL,
            142,
        );
        Self {
            parts,
            time,
            active,
        }
    }
    pub fn trace(&self, ray: Ray) -> Option<Hit> {
        actor_interval(ray)?;
        self.trace_inside(ray)
    }
    fn trace_inside(&self, ray: Ray) -> Option<Hit> {
        let mut nearest = None;
        for part in &self.parts {
            if let Some(hit) = part.trace(ray) {
                keep_nearest(&mut nearest, hit);
            }
        }
        if let Some(hit) = trace_cloth(ray, self.time, self.active) {
            keep_nearest(&mut nearest, hit);
        }
        nearest
    }
}

/// Compatibility query; callers with many rays should prepare ActorSnapshot once.
pub fn trace(ray: Ray, time: f32, stage: Stage) -> Option<Hit> {
    actor_interval(ray)?;
    ActorSnapshot::new(time, stage).trace_inside(ray)
}
fn actor_interval(ray: Ray) -> Option<(f32, f32)> {
    interval(ray, ACTOR_MIN, ACTOR_MAX)
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
    0.285 + 0.22 * (1.0 - v)
}

fn cloth_width(v: f32, time: f32, active: bool) -> f32 {
    half_width(v)
        + if active {
            (1.0 - v).powi(2) * (time * 1.8 + v * 5.0).sin() * 0.05
        } else {
            0.0
        }
}

fn cloth_shift(v: f32, time: f32, active: bool) -> f32 {
    if active {
        (1.0 - v).powi(2) * (time * 1.55 + v * 4.0).sin() * 0.065
    } else {
        0.0
    }
}

fn cloth_u(x: f32, v: f32, time: f32, active: bool) -> f32 {
    (x - CENTER_X - cloth_shift(v, time, active)) / cloth_width(v, time, active)
}

/// A material point moves sideways and in depth; its (u,v) identity persists.
#[cfg(test)]
fn cloth_point(u: f32, v: f32, time: f32, active: bool) -> Vec3 {
    Vec3::new(
        CENTER_X + cloth_shift(v, time, active) + u * cloth_width(v, time, active),
        CLOTH_BOTTOM + v * (CLOTH_TOP - CLOTH_BOTTOM),
        cloth_z(u, v, time, active),
    )
}

/// Lagrangian (u,v) remains attached to the sheet while its depth changes.
/// u spans -1..1, v spans 0..1. Shoulders stay fixed; lower regions deform locally.
fn cloth_z(u: f32, v: f32, time: f32, active: bool) -> f32 {
    let fall = 1.0 - v;
    let folds = (u * std::f32::consts::PI * 3.0 + v * 0.6).sin() * 0.038;
    let wave = if active {
        fall * fall
            * ((time * 2.1 + v * 4.3 + u * 2.0).sin() * 0.115
                + (time * 3.7 - v * 5.7 + u * 3.4).sin() * 0.045)
    } else {
        0.0
    };
    -0.215 - fall * 0.16 + folds + wave
}

fn sheet_depth(x: f32, y: f32, time: f32, active: bool) -> f32 {
    let v = cloth_v(y);
    cloth_z(cloth_u(x, v, time, active), v, time, active)
}

fn field(point: Vec3, time: f32, active: bool) -> f32 {
    point.z - sheet_depth(point.x, point.y, time, active)
}

fn trace_cloth(ray: Ray, time: f32, active: bool) -> Option<Hit> {
    let (near, far) = interval(ray, CLOTH_MIN, CLOTH_MAX)?;
    let mut left = near;
    let mut left_value = field(ray.origin + ray.dir * left, time, active);
    // Fixed work: conservative bounds, 32 brackets and 8 bisections per crossing.
    // More pronounced folds need finer brackets than R4. Grazing double roots
    // can still be missed: this is a graph sheet, not self-colliding cloth physics.
    for step in 1..=32 {
        let right = near + (far - near) * step as f32 / 32.0;
        let right_value = field(ray.origin + ray.dir * right, time, active);
        if left_value * right_value <= 0.0 {
            let mut a = left;
            let mut b = right;
            let mut fa = left_value;
            for _ in 0..8 {
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
            let u = cloth_u(position.x, v, time, active);
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
                    material: if v < 0.055 || u.abs() > 0.94 {
                        CAPE_TRIM
                    } else {
                        CAPE
                    },
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

    fn hit_bits(hit: Option<Hit>) -> Option<[u32; 12]> {
        hit.map(|h| {
            [
                h.distance.to_bits(),
                h.position.x.to_bits(),
                h.position.y.to_bits(),
                h.position.z.to_bits(),
                h.normal.x.to_bits(),
                h.normal.y.to_bits(),
                h.normal.z.to_bits(),
                h.material,
                h.object_id,
                h.local.x.to_bits(),
                h.local.y.to_bits(),
                h.local.z.to_bits(),
            ]
        })
    }

    #[test]
    fn snapshot_and_compatibility_query_match_hits_bitwise() {
        let eyes = [
            Vec3::new(1.0, 1.0, 3.0),
            Vec3::new(1.0, 1.0, -3.0),
            Vec3::new(3.0, 1.1, 0.2),
            Vec3::new(1.0, 3.0, -0.1),
        ];
        for stage in [Stage::Core, Stage::Light, Stage::Water, Stage::Cloth] {
            for time in [0.0, 0.016, 1.3, 2.75] {
                let snapshot = ActorSnapshot::new(time, stage);
                for eye in eyes {
                    for y in 0..15 {
                        for x in 0..11 {
                            let target = Vec3::new(0.45 + x as f32 * 0.11, y as f32 * 0.135, 0.0);
                            let ray = Ray {
                                origin: eye,
                                dir: (target - eye).normalize(),
                            };
                            let expected = hit_bits(snapshot.trace(ray));
                            assert_eq!(
                                hit_bits(trace(ray, time, stage)),
                                expected,
                                "wrapper stage={stage:?}, time={time}, ray={ray:?}"
                            );
                        }
                    }
                }
            }
        }
    }

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
        for u in [-1.0, -0.3, 0.3, 1.0] {
            assert_eq!(
                cloth_point(u, 1.0, 0.0, true),
                cloth_point(u, 1.0, 1.0, true)
            );
        }
        let delta_a = cloth_z(-0.4, 0.1, 1.0, true) - cloth_z(-0.4, 0.1, 0.0, true);
        let delta_b = cloth_z(0.4, 0.1, 1.0, true) - cloth_z(0.4, 0.1, 0.0, true);
        assert!(delta_a.abs() > 0.01);
        assert!((delta_a - delta_b).abs() > 0.01);
        let hem_start = cloth_point(0.9, 0.02, 0.0, true);
        let hem_later = cloth_point(0.9, 0.02, 1.0, true);
        assert!((hem_start.x - hem_later.x).abs() > 0.025);
        assert!((hem_start.z - hem_later.z).abs() > 0.025);
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
        for time in [0.0, 0.25, 1.0, 2.5] {
            let point = cloth_point(u, v, time, true);
            let hit = trace_cloth(
                Ray {
                    origin: Vec3::new(point.x, point.y, -2.0),
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
    fn bounds_cover_all_parts_and_deformed_sheet() {
        for tick in 0..120 {
            let time = tick as f32 * 0.19;
            let snapshot = ActorSnapshot::new(time, Stage::Cloth);
            for part in snapshot.parts {
                let extent = (part.rotation * Vec3::X).abs() * part.radii.x
                    + (part.rotation * Vec3::Y).abs() * part.radii.y
                    + (part.rotation * Vec3::Z).abs() * part.radii.z;
                assert!((part.center - extent).cmpge(ACTOR_MIN).all());
                assert!((part.center + extent).cmple(ACTOR_MAX).all());
            }
            for j in 0..11 {
                for i in 0..21 {
                    let point = cloth_point(-1.0 + i as f32 * 0.1, j as f32 * 0.1, time, true);
                    assert!(point.cmpge(CLOTH_MIN).all());
                    assert!(point.cmple(CLOTH_MAX).all());
                }
            }
        }
    }

    #[test]
    fn cape_border_has_its_own_material_and_stable_uv() {
        for (u, v, material) in [
            (0.98, 0.4, CAPE_TRIM),
            (0.1, 0.02, CAPE_TRIM),
            (0.4, 0.5, CAPE),
        ] {
            let point = cloth_point(u, v, 0.75, true);
            let hit = trace_cloth(
                Ray {
                    origin: point - Vec3::Z,
                    dir: Vec3::Z,
                },
                0.75,
                true,
            )
            .unwrap();
            assert_eq!(hit.material, material);
            assert_eq!(hit.object_id, CAPE_ID);
            assert!(hit
                .local
                .abs_diff_eq(Vec3::new(u * 0.5 + 0.5, v, 0.0), 1e-5));
        }
    }

    #[test]
    fn articulated_pose_keeps_boots_planted_and_local_material_coordinates() {
        let a = ActorSnapshot::new(0.0, Stage::Cloth);
        let b = ActorSnapshot::new(1.0, Stage::Cloth);
        for index in [10, 18] {
            assert_eq!(a.parts[index].center, b.parts[index].center);
            assert_eq!(a.parts[index].rotation, b.parts[index].rotation);
            assert_eq!(a.parts[index].material, LEATHER);
        }
        assert!(a.parts[13].center.distance(b.parts[13].center) > 0.01);
        for part in [a.parts[13], b.parts[13]] {
            let ray = Ray {
                origin: part.center + part.rotation * Vec3::Z,
                dir: -(part.rotation * Vec3::Z),
            };
            let hit = part.trace(ray).unwrap();
            assert!(hit.local.abs_diff_eq(Vec3::Z, 0.0001));
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
