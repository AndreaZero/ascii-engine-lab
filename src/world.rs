//! Small client fixture queried directly by the character-cell renderer.
//! No mesh, image of a scene, or Godot resource is used by these queries.
use glam::Vec3;

use crate::core::{
    intersect_box, intersect_ellipsoid, Hit, Ray, Stage, CAPE, CAPE_TRIM, CLOTH, EMISSIVE, GRASS,
    GROUND, LEATHER, METAL, SKIN, STONE, WATER,
};

// IDs 1..99 belong to the environment; actor.rs owns IDs >= 100.
const WATER_LEVEL: f32 = 0.035;

#[derive(Clone, Copy)]
struct Block {
    min: [f32; 3],
    max: [f32; 3],
    id: u32,
}

// A broken gate and its remaining wall. The opening is real empty space.
const MASONRY: &[Block] = &[
    Block {
        min: [-1.95, 0.0, -3.35],
        max: [-1.25, 2.65, -2.65],
        id: 10,
    },
    Block {
        min: [1.35, 0.0, -3.35],
        max: [2.05, 2.65, -2.65],
        id: 11,
    },
    Block {
        min: [-2.08, 2.45, -3.43],
        max: [-0.55, 3.05, -2.57],
        id: 12,
    },
    Block {
        min: [0.5, 2.45, -3.43],
        max: [2.16, 3.12, -2.57],
        id: 13,
    },
    Block {
        min: [-0.57, 2.72, -3.38],
        max: [0.42, 3.3, -2.62],
        id: 14,
    },
    Block {
        min: [-4.8, 0.0, -3.28],
        max: [-2.0, 1.28, -2.72],
        id: 15,
    },
    Block {
        min: [-4.3, 1.28, -3.28],
        max: [-2.6, 1.87, -2.72],
        id: 16,
    },
    Block {
        min: [2.08, 0.0, -3.28],
        max: [4.45, 0.8, -2.72],
        id: 17,
    },
    Block {
        min: [2.08, 0.8, -3.28],
        max: [2.95, 1.68, -2.72],
        id: 18,
    },
    Block {
        min: [-4.5, 0.0, -0.1],
        max: [-3.85, 0.36, 0.7],
        id: 19,
    },
    Block {
        min: [2.6, 0.0, -1.55],
        max: [3.18, 0.27, -0.8],
        id: 20,
    },
    Block {
        min: [-2.85, 0.0, 2.22],
        max: [-1.95, 0.16, 2.6],
        id: 21,
    },
];

// Sparse clumps rather than a solid wall around the whole courtyard.
const VEGETATION: &[[f32; 6]] = &[
    [-4.0, 0.48, 1.1, 0.75, 0.5, 0.75],
    [-4.5, 0.62, -1.35, 0.65, 0.68, 0.9],
    [3.85, 0.45, -1.3, 0.8, 0.48, 0.65],
    [4.45, 0.7, -4.3, 0.9, 0.75, 1.0],
    [-4.2, 0.75, -4.6, 1.0, 0.78, 0.85],
    [-0.5, 0.16, 3.15, 0.4, 0.22, 0.35],
];

fn retain_nearest(best: &mut Option<Hit>, candidate: Option<Hit>) {
    if let Some(hit) = candidate {
        if best.as_ref().is_none_or(|old| hit.distance < old.distance) {
            *best = Some(hit);
        }
    }
}

fn block_hit(ray: Ray, min: Vec3, max: Vec3, material: u32, object_id: u32) -> Option<Hit> {
    intersect_box(ray, min, max).map(|(distance, normal)| {
        let position = ray.origin + ray.dir * distance;
        Hit {
            distance,
            position,
            normal,
            material,
            object_id,
            local: position - (min + max) * 0.5,
        }
    })
}

fn ellipsoid_hit(
    ray: Ray,
    center: Vec3,
    radii: Vec3,
    material: u32,
    object_id: u32,
) -> Option<Hit> {
    intersect_ellipsoid(ray, center, radii).map(|(distance, normal)| {
        let position = ray.origin + ray.dir * distance;
        Hit {
            distance,
            position,
            normal,
            material,
            object_id,
            local: position - center,
        }
    })
}

/// Immutable client state prepared once for all queries of one frame.
/// Geometry traversal and material selection are unchanged from the stateless
/// fixture; preparation only moves time-dependent light/actor work out of rays.
#[derive(Clone, Copy)]
pub struct WorldSnapshot {
    time: f32,
    stage: Stage,
    light: Vec3,
    actor: crate::actor::ActorSnapshot,
}

impl WorldSnapshot {
    pub fn new(time: f32, stage: Stage) -> Self {
        Self {
            time,
            stage,
            light: light_position(time),
            actor: crate::actor::ActorSnapshot::new(time, stage),
        }
    }

    pub fn time(&self) -> f32 {
        self.time
    }

    pub fn stage(&self) -> Stage {
        self.stage
    }

    pub fn light_position(&self) -> Vec3 {
        self.light
    }

    /// Nearest opaque support, independent of the selected glyph's ink coverage.
    /// Reflection queries can ignore water and share exactly the same snapshot
    /// as camera and shadow queries.
    pub fn trace(&self, ray: Ray, include_water: bool) -> Option<Hit> {
        trace_environment(
            ray,
            self.stage,
            include_water,
            self.light,
            self.actor.trace(ray),
        )
    }

    /// Equivalent to filtering the nearest `trace(ray, false)` hit, including
    /// its emissive masking behavior. This is not an "ignore all lights" query.
    /// Actor traversal is unchanged; only opaque environment traversal may exit
    /// early after checking both emissive distance limits.
    pub fn occluded(&self, ray: Ray, max_distance: f32) -> bool {
        let actor_hit = self.actor.trace(ray);
        let lamp_distance = if matches!(self.stage, Stage::Core) {
            None
        } else {
            intersect_ellipsoid(ray, self.light, Vec3::splat(0.11)).map(|(d, _)| d)
        };
        let actor_emissive_distance = actor_hit
            .filter(|hit| hit.material == EMISSIVE)
            .map(|hit| hit.distance);
        if actor_hit.is_some_and(|hit| {
            hit.material != EMISSIVE
                && opaque_distance_blocks(hit.distance, max_distance, None, lamp_distance)
        }) {
            return true;
        }

        let blocks = |distance| {
            opaque_distance_blocks(
                distance,
                max_distance,
                actor_emissive_distance,
                lamp_distance,
            )
        };
        if intersect_box(ray, Vec3::new(-8.0, -0.3, -8.0), Vec3::new(8.0, 0.0, 8.0))
            .is_some_and(|(distance, _)| blocks(distance))
        {
            return true;
        }
        for block in MASONRY {
            if intersect_box(
                ray,
                Vec3::from_array(block.min),
                Vec3::from_array(block.max),
            )
            .is_some_and(|(distance, _)| blocks(distance))
            {
                return true;
            }
        }
        for p in VEGETATION {
            if intersect_ellipsoid(
                ray,
                Vec3::new(p[0], p[1], p[2]),
                Vec3::new(p[3], p[4], p[5]),
            )
            .is_some_and(|(distance, _)| blocks(distance))
            {
                return true;
            }
        }
        false
    }
}

fn opaque_distance_blocks(
    distance: f32,
    max_distance: f32,
    actor_emissive_distance: Option<f32>,
    lamp_distance: Option<f32>,
) -> bool {
    // Actor is visited before the environment and wins exact-distance ties.
    // The world lamp is visited last and loses ties to opaque geometry.
    // Keep these distinct comparisons rather than combining bounds with min().
    distance < max_distance
        && actor_emissive_distance.is_none_or(|emissive| distance < emissive)
        && lamp_distance.is_none_or(|emissive| distance <= emissive)
}

// Shared traversal keeps stateless clients and prepared frames equivalent without
// forcing off-actor legacy rays to prepare the actor's full pose.
fn trace_environment(
    ray: Ray,
    stage: Stage,
    include_water: bool,
    light: Vec3,
    actor_hit: Option<Hit>,
) -> Option<Hit> {
    let mut best = actor_hit;
    retain_nearest(
        &mut best,
        block_hit(
            ray,
            Vec3::new(-8.0, -0.3, -8.0),
            Vec3::new(8.0, 0.0, 8.0),
            GROUND,
            1,
        ),
    );

    for block in MASONRY {
        retain_nearest(
            &mut best,
            block_hit(
                ray,
                Vec3::from_array(block.min),
                Vec3::from_array(block.max),
                STONE,
                block.id,
            ),
        );
    }
    for (i, p) in VEGETATION.iter().enumerate() {
        retain_nearest(
            &mut best,
            ellipsoid_hit(
                ray,
                Vec3::new(p[0], p[1], p[2]),
                Vec3::new(p[3], p[4], p[5]),
                GRASS,
                30 + i as u32,
            ),
        );
    }

    if !matches!(stage, Stage::Core) {
        retain_nearest(
            &mut best,
            ellipsoid_hit(ray, light, Vec3::splat(0.11), EMISSIVE, 2),
        );
    }

    if include_water && matches!(stage, Stage::Water | Stage::Cloth) && ray.dir.y.abs() > 1e-6 {
        let distance = (WATER_LEVEL - ray.origin.y) / ray.dir.y;
        if distance > 1e-4 {
            let position = ray.origin + ray.dir * distance;
            if (-3.0..=0.0).contains(&position.x) && (-2.0..=2.0).contains(&position.z) {
                retain_nearest(
                    &mut best,
                    Some(Hit {
                        distance,
                        position,
                        normal: Vec3::Y,
                        material: WATER,
                        object_id: 3,
                        local: position - Vec3::new(-1.5, WATER_LEVEL, 0.0),
                    }),
                );
            }
        }
    }
    best
}

/// Compatibility path for independent stateless queries. Frame rendering should
/// create one `WorldSnapshot` and reuse it for camera, shadow and reflection rays.
pub fn trace(ray: Ray, time: f32, stage: Stage, include_water: bool) -> Option<Hit> {
    let actor_hit = crate::actor::trace(ray, time, stage);
    let light = if matches!(stage, Stage::Core) {
        Vec3::ZERO
    } else {
        light_position(time)
    };
    trace_environment(ray, stage, include_water, light, actor_hit)
}

/// A deterministic small moving light; the Core stage does not use it.
pub fn light_position(time: f32) -> Vec3 {
    let phase = time * 0.55;
    Vec3::new(
        -0.4 + 1.8 * phase.cos(),
        1.95 + 0.12 * (phase * 1.7).sin(),
        -0.6 + 1.8 * phase.sin(),
    )
}

/// Linear material pigment, before the common renderer's light response.
pub fn pigment(material: u32) -> Vec3 {
    match material {
        STONE => Vec3::new(0.48, 0.54, 0.52),
        GROUND => Vec3::new(0.19, 0.23, 0.22),
        WATER => Vec3::new(0.055, 0.12, 0.15),
        // Warm body fabric and a cool, subdued cape remain distinct in the
        // shared lighting. These are pigments, not actor-only emission/fill.
        CLOTH => Vec3::new(0.58, 0.39, 0.25),
        CAPE => Vec3::new(0.27, 0.43, 0.48),
        CAPE_TRIM => Vec3::new(0.44, 0.57, 0.56),
        LEATHER => Vec3::new(0.32, 0.23, 0.16),
        METAL => Vec3::new(0.61, 0.65, 0.63),
        SKIN => Vec3::new(0.66, 0.46, 0.31),
        GRASS => Vec3::new(0.23, 0.39, 0.24),
        EMISSIVE => Vec3::new(1.0, 0.57, 0.18),
        _ => Vec3::splat(0.45),
    }
}

fn cell_hash(local: Vec3, scale: f32) -> u32 {
    // Integer hash of fixed material coordinates: no frame/time RNG.
    let q = (local * scale).floor();
    let mut h = (q.x as i32 as u32).wrapping_mul(0x8da6_b343)
        ^ (q.y as i32 as u32).wrapping_mul(0xd816_3841)
        ^ (q.z as i32 as u32).wrapping_mul(0xcb1a_b31f);
    h ^= h >> 13;
    h = h.wrapping_mul(0x85eb_ca6b);
    h ^ (h >> 16)
}

/// Small material-specific vocabulary, chosen directly from semantic data.
/// Illumination is deliberately only a broad accent gate here. Temporal
/// hysteresis belongs to the shared cell renderer, not hidden per-world state.
pub fn glyph(material: u32, local: Vec3, normal: Vec3, illumination: f32) -> u8 {
    let hash = cell_hash(local, 7.0);
    match material {
        GROUND => match hash % 12 {
            0 => b':',
            1..=2 => b'.',
            3 => b'`',
            _ => b' ',
        },
        STONE => {
            let course = (local.y * 4.0).rem_euclid(1.0);
            if normal.y.abs() < 0.65 && course < 0.13 {
                return b'=';
            }
            if illumination > 0.75 && hash.is_multiple_of(5) {
                return b'#';
            }
            match hash % 7 {
                0 => b'o',
                1 => b'+',
                2 => b':',
                3 => b';',
                4 => b'c',
                _ => b'.',
            }
        }
        WATER => match hash % 7 {
            0 => b'~',
            1 => b'=',
            2..=4 => b'-',
            _ => b' ',
        },
        CLOTH => {
            // Actor body parts provide normalized local coordinates. A single
            // seam, shoulder and hem describe construction instead of a grid.
            if local.x.abs() < 0.10 && local.z.abs() > 0.35 && local.y < 0.45 {
                b'|'
            } else if local.y > 0.72 {
                b'_'
            } else if local.y < -0.64 {
                b'-'
            } else if local.x < -0.72 {
                b'('
            } else if local.x > 0.72 {
                b')'
            } else if local.y + local.x * 0.22 > 0.12 {
                b':'
            } else if illumination > 0.18 {
                b';'
            } else {
                b','
            }
        }
        CAPE => cape_glyph(local, normal, illumination),
        CAPE_TRIM => {
            // Sheet UV is (u * .5 + .5, v, 0); v=0 is the hanging hem.
            // Preserve connected side borders even when illumination is low.
            if local.y < 0.055 {
                b'_'
            } else {
                b'|'
            }
        }
        LEATHER => {
            // Broad edge bands and a central fastening, never random speckles.
            if local.x.abs() < 0.16 && local.y.abs() < 0.28 && local.z > 0.4 {
                b'+'
            } else if local.y.abs() > 0.65 {
                b'='
            } else if local.x.abs() < 0.10 {
                b':'
            } else {
                b'-'
            }
        }
        METAL => {
            if illumination > 0.8 {
                b'+'
            } else {
                b"/=|"[hash as usize % 3]
            }
        }
        SKIN => b"o:,"[hash as usize % 3],
        GRASS => b"/\\|'v"[hash as usize % 5],
        EMISSIVE => b'*',
        _ => b'?',
    }
}

fn cape_glyph(local: Vec3, normal: Vec3, illumination: f32) -> u8 {
    // One central seam and a short shoulder yoke use material/rest UVs, so they
    // follow the deformation. No per-cell hash or periodic checker is involved.
    if (local.x - 0.5).abs() < 0.025 && local.y > 0.45 {
        return b'|';
    }
    if local.y > 0.90 && (0.25..0.75).contains(&local.x) {
        return b'_';
    }
    // The sheet normal may be flipped toward the observer. Its component
    // product is invariant to that flip; front/back views do not swap folds.
    // Parentheses describe broad sloping fold faces instead of tiled chevrons.
    let fold_direction = normal.x * normal.z;
    if fold_direction > 0.18 {
        b')'
    } else if fold_direction < -0.18 {
        b'('
    } else if normal.y.abs() > 0.38 {
        b'~'
    } else if illumination > 0.12 {
        b':'
    } else {
        b','
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn occlusion_preserves_nearest_emissive_masking() {
        let actor_ray = Ray {
            origin: Vec3::new(1.425, 0.75, 1.0),
            dir: -Vec3::Z,
        };
        let core = WorldSnapshot::new(0.0, Stage::Core);
        assert_eq!(core.trace(actor_ray, false).unwrap().object_id, 140);
        assert!(!core.occluded(actor_ray, 10.0));

        let lamp_ray = Ray {
            origin: Vec3::new(1.4, 2.45, -0.6),
            dir: -Vec3::Y,
        };
        let lit = WorldSnapshot::new(0.0, Stage::Light);
        assert_eq!(lit.trace(lamp_ray, false).unwrap().object_id, 2);
        assert!(!lit.occluded(lamp_ray, 10.0));
        // With the world lamp absent, the opaque fixture behind it occludes.
        // Its exact material may change with the actor's silhouette.
        assert_ne!(core.trace(lamp_ray, false).unwrap().material, EMISSIVE);
        assert!(core.occluded(lamp_ray, 10.0));
    }

    #[test]
    fn occlusion_matches_trace_at_segment_boundaries() {
        let rays = [
            Ray {
                origin: Vec3::new(-1.5, 4.0, 0.0),
                dir: -Vec3::Y,
            },
            Ray {
                origin: Vec3::new(-1.6, 1.2, 2.0),
                dir: -Vec3::Z,
            },
            Ray {
                origin: Vec3::new(1.0, 1.1, 0.0),
                dir: Vec3::X,
            },
            Ray {
                origin: Vec3::new(0.1, 1.2, 2.0),
                dir: -Vec3::Z,
            },
            Ray {
                origin: Vec3::new(-1.5, 4.0, 0.0),
                dir: -Vec3::Y * 2.0,
            },
        ];
        for stage in [Stage::Core, Stage::Light, Stage::Water, Stage::Cloth] {
            for time in [0.0, 1.0, 2.5] {
                let snapshot = WorldSnapshot::new(time, stage);
                for ray in rays {
                    let nearest = snapshot.trace(ray, false);
                    let distance = nearest.map_or(4.0, |hit| hit.distance);
                    for limit in [
                        0.0,
                        -1.0,
                        f32::NAN,
                        f32::INFINITY,
                        f32::from_bits(distance.to_bits() - 1),
                        distance,
                        f32::from_bits(distance.to_bits() + 1),
                    ] {
                        let expected = nearest
                            .is_some_and(|hit| hit.distance < limit && hit.material != EMISSIVE);
                        assert_eq!(
                            snapshot.occluded(ray, limit),
                            expected,
                            "stage={stage:?} time={time} ray={ray:?} limit={limit}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn occlusion_ties_respect_original_actor_and_lamp_order() {
        assert!(!opaque_distance_blocks(2.0, 3.0, Some(2.0), None));
        assert!(opaque_distance_blocks(2.0, 3.0, None, Some(2.0)));
        assert!(!opaque_distance_blocks(2.0, 3.0, Some(2.0), Some(2.0)));
        assert!(!opaque_distance_blocks(2.0, 2.0, None, None));
        assert!(!opaque_distance_blocks(2.0, f32::NAN, None, None));
    }

    #[test]
    fn snapshots_keep_independent_light_and_stage_state() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<WorldSnapshot>();

        let snapshot = WorldSnapshot::new(0.0, Stage::Water);
        let lamp_ray = Ray {
            origin: Vec3::new(1.4, 2.45, -0.6),
            dir: -Vec3::Y,
        };
        let initial = snapshot.trace(lamp_ray, false).expect("prepared lamp");
        assert_eq!(initial.object_id, 2);
        assert!((initial.distance - 0.39).abs() < 1e-5);

        let other = WorldSnapshot::new(3.0, Stage::Core);
        assert_ne!(snapshot.light_position(), other.light_position());
        assert_eq!(snapshot.time(), 0.0);
        assert_eq!(snapshot.stage(), Stage::Water);
        assert!(other
            .trace(lamp_ray, false)
            .is_none_or(|h| h.object_id != 2));
        let retained = snapshot.trace(lamp_ray, false).expect("same prepared lamp");
        assert_eq!(initial.distance, retained.distance);
        assert_eq!(initial.position, retained.position);
        assert_eq!(initial.normal, retained.normal);
        assert_eq!(initial.local, retained.local);

        let pool_ray = Ray {
            origin: Vec3::new(-1.5, 4.0, 0.0),
            dir: -Vec3::Y,
        };
        assert_eq!(snapshot.trace(pool_ray, true).unwrap().material, WATER);
        assert_eq!(other.trace(pool_ray, true).unwrap().material, GROUND);
    }

    #[test]
    fn gate_pillar_occludes_but_opening_remains_open() {
        let pillar = trace(
            Ray {
                origin: Vec3::new(-1.6, 1.2, 2.0),
                dir: -Vec3::Z,
            },
            0.0,
            Stage::Core,
            true,
        )
        .expect("pillar support");
        assert_eq!(pillar.object_id, 10);
        assert!((pillar.distance - 4.65).abs() < 1e-4);
        assert!(trace(
            Ray {
                origin: Vec3::new(0.1, 1.2, 2.0),
                dir: -Vec3::Z
            },
            0.0,
            Stage::Core,
            true
        )
        .is_none());
    }

    #[test]
    fn water_is_stage_gated_and_secondary_rays_can_ignore_it() {
        let ray = Ray {
            origin: Vec3::new(-1.5, 4.0, 0.0),
            dir: -Vec3::Y,
        };
        for stage in [Stage::Core, Stage::Light] {
            assert_eq!(trace(ray, 0.0, stage, true).unwrap().material, GROUND);
        }
        for stage in [Stage::Water, Stage::Cloth] {
            let hit = trace(ray, 0.0, stage, true).unwrap();
            assert_eq!(hit.material, WATER);
            assert!((hit.position.y - WATER_LEVEL).abs() < 1e-5);
            assert_eq!(trace(ray, 0.0, stage, false).unwrap().material, GROUND);
        }
        let outside = Ray {
            origin: Vec3::new(-3.3, 4.0, 0.0),
            dir: -Vec3::Y,
        };
        assert_eq!(
            trace(outside, 0.0, Stage::Water, true).unwrap().material,
            GROUND
        );
    }

    #[test]
    fn material_vocabulary_is_printable_ascii_and_deterministic() {
        for material in [
            STONE, GROUND, WATER, CLOTH, METAL, SKIN, GRASS, EMISSIVE, CAPE, CAPE_TRIM, LEATHER,
        ] {
            for i in -20..20 {
                let p = Vec3::new(i as f32 * 0.137, 0.41, -0.77);
                let code = glyph(material, p, Vec3::Z, 0.6);
                assert!((32..=126).contains(&code));
                assert_eq!(code, glyph(material, p, Vec3::Z, 0.6));
            }
        }
    }

    #[test]
    fn actor_materials_have_distinct_palette_and_construction_marks() {
        let tunic = pigment(CLOTH);
        let cape = pigment(CAPE);
        assert!(tunic.x > tunic.z && cape.z > cape.x);
        assert!(pigment(CAPE_TRIM).min_element() > cape.min_element());
        assert_ne!(pigment(LEATHER), tunic);
        assert_eq!(
            glyph(CAPE_TRIM, Vec3::new(0.5, 0.02, 0.0), Vec3::Z, 0.05),
            b'_'
        );
        assert_eq!(
            glyph(CAPE_TRIM, Vec3::new(0.01, 0.5, 0.0), Vec3::Z, 0.05),
            b'|'
        );
        assert_eq!(glyph(CLOTH, Vec3::new(0.0, 0.0, 0.8), Vec3::Z, 0.2), b'|');
        assert_eq!(glyph(LEATHER, Vec3::new(0.0, 0.0, 0.8), Vec3::Z, 0.2), b'+');
    }

    #[test]
    fn cape_folds_follow_direction_without_flipping_between_sides() {
        let uv = Vec3::new(0.3, 0.5, 0.0);
        let right = Vec3::new(0.6, 0.0, 0.8);
        let left = Vec3::new(-0.6, 0.0, 0.8);
        assert_ne!(glyph(CAPE, uv, right, 0.2), glyph(CAPE, uv, left, 0.2));
        for normal in [right, left, Vec3::Z, Vec3::new(0.0, 0.6, 0.8)] {
            for light in [0.0, 0.05, 0.2, 1.0] {
                assert_eq!(
                    glyph(CAPE, uv, normal, light),
                    glyph(CAPE, uv, -normal, light)
                );
            }
        }
    }
}
