//! R10 integration fixture: R8 stone, R9 worked metal and the R5 actor share
//! one immutable snapshot. All surfaces are queried directly, without meshes.
//! Translations preserve authored local coordinates and geometric normals.
#[path = "metal_geometry.rs"]
mod metal_geometry;
#[path = "metal_surface.rs"]
mod metal_surface;
#[path = "stone_geometry.rs"]
pub(crate) mod stone_geometry;
#[path = "stone_surface.rs"]
mod stone_surface;

use ascii_engine_lab::{actor::ActorSnapshot, core::*, world};
use glam::Vec3;

pub const TARGET: Vec3 = Vec3::new(0., 1., 0.);
pub const STONE_OFFSET: Vec3 = Vec3::new(-2.25, -0.1, -0.35);
pub const METAL_OFFSET: Vec3 = Vec3::new(1.95, 0., -0.3);
pub const ACTOR_OFFSET: Vec3 = Vec3::new(-1., 0., 0.);
const FLOOR: u32 = 40;
const WALL: u32 = 41;

/// Bounds are conservative, in world metres, and identify the three subjects.
/// Actor IDs occupy 100..=142; 100 here identifies that entire diagnostic group.
pub fn subject_bounds() -> [(u32, Vec3, Vec3); 3] {
    [
        (
            700,
            stone_geometry::CENTER + STONE_OFFSET - stone_geometry::HALF_EXTENTS,
            stone_geometry::CENTER + STONE_OFFSET + stone_geometry::HALF_EXTENTS,
        ),
        (
            800,
            metal_geometry::CENTER + METAL_OFFSET - metal_geometry::HALF_EXTENTS,
            metal_geometry::CENTER + METAL_OFFSET + metal_geometry::HALF_EXTENTS,
        ),
        (
            100,
            Vec3::new(-0.75, -0.01, -0.55),
            Vec3::new(0.75, 2.07, 0.70),
        ),
    ]
}

#[derive(Clone, Copy)]
pub struct CommonScene {
    actor: ActorSnapshot,
    light: Vec3,
    environment: bool,
}
impl CommonScene {
    pub fn new(time: f32, environment: bool, moving_light: bool) -> Self {
        let light_time = if moving_light { time } else { 0. };
        Self {
            actor: ActorSnapshot::new(time, Stage::Cloth),
            light: Vec3::new(-1.6 + 3.2 * (light_time * 0.65).sin(), 3.7, 3.1),
            environment,
        }
    }
}

fn local_ray(ray: Ray, offset: Vec3) -> Ray {
    Ray {
        origin: ray.origin - offset,
        dir: ray.dir,
    }
}
fn translated(mut hit: Hit, offset: Vec3) -> Hit {
    hit.position += offset;
    hit
}
fn half_turn(v: Vec3) -> Vec3 {
    Vec3::new(-v.x, v.y, -v.z)
}
fn actor_ray(ray: Ray) -> Ray {
    // Face the retained R5 cape toward the positive-Z viewing side. This is a
    // rigid half-turn of the complete actor, including its shadow/reflections.
    Ray {
        origin: half_turn(ray.origin) - ACTOR_OFFSET,
        dir: half_turn(ray.dir),
    }
}
fn actor_hit(mut hit: Hit) -> Hit {
    hit.position = half_turn(hit.position + ACTOR_OFFSET);
    hit.normal = half_turn(hit.normal);
    hit
}
fn nearest(best: &mut Option<Hit>, candidate: Option<Hit>) {
    if let Some(hit) = candidate {
        if best.is_none_or(|previous| hit.distance < previous.distance) {
            *best = Some(hit);
        }
    }
}
fn wetness(hit: Hit) -> f32 {
    // Fixed damp half, connected to the stone itself, not to the camera/time.
    let t = ((hit.local.x + 0.2) / 0.5).clamp(0., 1.);
    t * t * (3. - 2. * t)
}
fn is_metal(material: u32) -> bool {
    matches!(
        material,
        metal_geometry::BODY | metal_geometry::RIM | metal_geometry::RIVET
    )
}
fn finish(hit: Hit) -> metal_surface::Finish {
    // The worked rim is worn clean; oxidation remains local on the R9 body.
    if hit.material == metal_geometry::RIM {
        metal_surface::Finish::Forged
    } else {
        metal_surface::Finish::Oxidized
    }
}

impl Scene for CommonScene {
    fn trace(&self, ray: Ray, _time: f32, _stage: Stage, _include_water: bool) -> Option<Hit> {
        let mut hit = self.actor.trace(actor_ray(ray)).map(actor_hit);
        // Avoid the more expensive crack query outside its conservative bounds.
        let stone_ray = local_ray(ray, STONE_OFFSET);
        if intersect_box(
            stone_ray,
            stone_geometry::CENTER - stone_geometry::HALF_EXTENTS,
            stone_geometry::CENTER + stone_geometry::HALF_EXTENTS,
        )
        .is_some()
        {
            nearest(
                &mut hit,
                stone_geometry::trace(stone_ray, true).map(|h| translated(h, STONE_OFFSET)),
            );
        }
        nearest(
            &mut hit,
            metal_geometry::trace(local_ray(ray, METAL_OFFSET))
                .map(|h| translated(h, METAL_OFFSET)),
        );
        // Three simple structural blocks frame a broad opening. They participate
        // in primary, shadow and environment queries with the same geometry.
        for (min, max, object_id) in [
            (Vec3::new(-4.3, 0., -2.8), Vec3::new(-1.15, 3.5, -2.55), 901),
            (Vec3::new(1.15, 0., -2.8), Vec3::new(4.3, 3.5, -2.55), 902),
            (
                Vec3::new(-1.15, 0., -2.8),
                Vec3::new(1.15, 0.65, -2.55),
                903,
            ),
        ] {
            nearest(
                &mut hit,
                intersect_box(ray, min, max).map(|(distance, normal)| {
                    let position = ray.origin + ray.dir * distance;
                    Hit {
                        distance,
                        position,
                        normal,
                        local: position,
                        material: WALL,
                        object_id,
                    }
                }),
            );
        }
        if ray.dir.y.abs() > 1e-7 {
            let distance = -ray.origin.y / ray.dir.y;
            let position = ray.origin + ray.dir * distance;
            if distance > 0.0001 && position.x.abs() < 4.8 && (-2.8..4.2).contains(&position.z) {
                nearest(
                    &mut hit,
                    Some(Hit {
                        distance,
                        position,
                        normal: Vec3::Y,
                        local: position,
                        material: FLOOR,
                        object_id: 900,
                    }),
                );
            }
        }
        hit
    }
    fn light_position(&self, _time: f32) -> Vec3 {
        self.light
    }
    fn light_color(&self) -> Vec3 {
        Vec3::new(1., 0.79, 0.54) * 5.5
    }
    fn local_materials(&self) -> bool {
        true
    }
    fn pigment(&self, material: u32) -> Vec3 {
        match material {
            FLOOR => Vec3::new(0.085, 0.092, 0.10),
            WALL => Vec3::new(0.22, 0.23, 0.25),
            _ => world::pigment(material),
        }
    }
    fn glyph(&self, material: u32, local: Vec3, normal: Vec3, i: f32) -> u8 {
        if matches!(material, FLOOR | WALL) {
            if i > 0.10 {
                b':'
            } else {
                b'.'
            }
        } else {
            world::glyph(material, local, normal, i)
        }
    }
    fn material_sample(&self, hit: Hit, footprint: f32) -> Option<MaterialSample> {
        Some(if hit.material == stone_geometry::STONE_MATERIAL {
            stone_surface::sample(hit, wetness(hit), footprint)
        } else if is_metal(hit.material) {
            metal_surface::sample(hit, finish(hit), footprint)
        } else {
            // R5 geometry and glyph vocabulary are reused; these local roughness
            // values are an R10 integration approximation, not measured fabrics.
            MaterialSample {
                albedo: self.pigment(hit.material),
                normal: hit.normal,
                roughness: match hit.material {
                    METAL => 0.4,
                    LEATHER => 0.7,
                    SKIN => 0.76,
                    _ => 0.92,
                },
                f0: if matches!(hit.material, FLOOR | WALL) {
                    0.025
                } else {
                    0.035
                },
                occlusion: 1.,
            }
        })
    }
    fn metallic_sample(&self, hit: Hit, footprint: f32) -> Option<MetallicSample> {
        if is_metal(hit.material) {
            Some(metal_surface::metallic(hit, finish(hit), footprint))
        } else if hit.material == METAL {
            Some(MetallicSample {
                f0: Vec3::new(0.55, 0.57, 0.60),
                metalness: 1.,
            })
        } else {
            None
        }
    }
    fn glyph_sample(&self, hit: Hit, i: f32, footprint: f32) -> u8 {
        if hit.material == stone_geometry::STONE_MATERIAL {
            if wetness(hit) >= 0.5 {
                stone_surface::wet_glyph(hit, i, footprint)
            } else {
                stone_surface::glyph(hit, i, footprint)
            }
        } else if is_metal(hit.material) {
            metal_surface::glyph(hit, finish(hit), i, footprint)
        } else {
            self.glyph(hit.material, hit.local, hit.normal, i)
        }
    }
    fn glyph_sample_key(&self, hit: Hit, footprint: f32) -> u32 {
        if hit.material == stone_geometry::STONE_MATERIAL {
            stone_surface::history_key(hit, footprint) | (u32::from(wetness(hit) >= 0.5) << 31)
        } else if is_metal(hit.material) {
            metal_surface::history_key(hit, finish(hit), footprint)
        } else if matches!(hit.material, CLOTH | CAPE | CAPE_TRIM | LEATHER) {
            world::glyph(hit.material, hit.local, hit.normal, 1.) as u32
        } else {
            0
        }
    }
    fn environment_sample(&self, query: EnvironmentQuery) -> Option<EnvironmentSample> {
        if !self.environment {
            return None;
        }
        // Analytic distant radiance, not an image captured from the scene.
        // Values are authored linear lighting units. This broad cool sky/window
        // lobe deliberately has no sharp sun. Roughness widens and lowers the
        // directional lobe; it is not an exact convolution or physical daylight.
        let n = query.normal;
        let r = query.reflected_direction;
        let sky = Vec3::new(0.16, 0.23, 0.34);
        let ground = Vec3::new(0.025, 0.019, 0.013);
        let diffuse = ground.lerp(sky, (n.y * 0.5 + 0.5).clamp(0., 1.));
        let opening = Vec3::new(-0.28, 0.52, 0.81).normalize();
        let exponent = 14. * (1. - query.roughness) + 1.;
        let lobe = r.dot(opening).max(0.).powf(exponent) / (1. + 2. * query.roughness);
        let radiance =
            ground.lerp(sky, (r.y * 0.5 + 0.5).clamp(0., 1.)) + Vec3::new(0.70, 0.85, 1.10) * lobe;
        Some(EnvironmentSample {
            diffuse_irradiance: diffuse * std::f32::consts::PI,
            specular_radiance: radiance,
            max_distance: 12.,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn translation_preserves_material_coordinates_and_query_distance() {
        for (offset, center, material, expected_id) in [
            (STONE_OFFSET, stone_geometry::CENTER, 32, 700),
            (METAL_OFFSET, metal_geometry::CENTER, 50, 800),
        ] {
            let local = Ray {
                origin: center + Vec3::Z * 4.,
                dir: -Vec3::Z,
            };
            let original = if expected_id == 700 {
                stone_geometry::trace(local, true)
            } else {
                metal_geometry::trace(local)
            }
            .unwrap();
            let ray = Ray {
                origin: local.origin + offset,
                dir: local.dir,
            };
            let actual = CommonScene::new(0., true, false)
                .trace(ray, 0., Stage::Cloth, false)
                .unwrap();
            assert_eq!(actual.object_id, expected_id);
            assert_eq!(actual.material, material);
            assert!((actual.distance - original.distance).abs() < 1e-5);
            assert!(actual.local.abs_diff_eq(original.local, 1e-5));
            assert!(actual.normal.abs_diff_eq(original.normal, 1e-5));
            assert!(actual
                .position
                .abs_diff_eq(original.position + offset, 1e-5));
        }
    }
    #[test]
    fn old_snapshot_does_not_change_when_a_new_pose_is_built() {
        let old = CommonScene::new(0., true, true);
        let ray = Ray {
            origin: Vec3::new(0., 1.3, 2.),
            dir: -Vec3::Z,
        };
        let before = old.trace(ray, 0., Stage::Cloth, false).unwrap();
        let later = CommonScene::new(1.7, true, true);
        let after = old.trace(ray, 0., Stage::Cloth, false).unwrap();
        assert_eq!(before.distance.to_bits(), after.distance.to_bits());
        assert_eq!(before.local.to_array(), after.local.to_array());
        assert_ne!(old.light_position(0.), later.light_position(1.7));
        assert!(matches!(before.material, CAPE | CAPE_TRIM));
    }
}
