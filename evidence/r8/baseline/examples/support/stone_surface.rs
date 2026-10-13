//! Object-attached sedimentary stone study. No time, view vector, image texture,
//! or frame randomness enters these fields. Units match the analytic geometry.
//! Filtering suppresses unresolved detail; it is not exact pixel integration.
use ascii_engine_lab::core::{Hit, MaterialSample};
use glam::Vec3;

use crate::geometry;

fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn footprint_size(footprint: f32) -> f32 {
    if footprint.is_finite() {
        footprint.max(0.0)
    } else {
        1.0
    }
}

// Angular frequencies in radians/world-unit. Attenuation starts before a cell
// spans half a period and reaches zero before a complete oscillation fits in it.
fn resolved(frequency: f32, footprint: f32) -> f32 {
    1.0 - smooth(1.0, 3.0, frequency * footprint)
}

#[derive(Clone, Copy)]
struct Fields {
    bedding: f32,
    vein: f32,
    mineral: f32,
    grain: f32,
    aggregate: f32,
    pores: f32,
    gradient: Vec3,
    damp: f32,
}

// Fixed lattice values with smooth analytic derivatives. Hashing happens only
// in object space: moving light/camera never reseeds the surface.
fn mineral_noise(p: Vec3) -> (f32, Vec3) {
    fn value(x: i32, y: i32, z: i32) -> f32 {
        let mut h = (x as u32).wrapping_mul(0x8da6_b343)
            ^ (y as u32).wrapping_mul(0xd816_3841)
            ^ (z as u32).wrapping_mul(0xcb1a_b31f);
        h ^= h >> 16;
        h = h.wrapping_mul(0x7feb_352d);
        h ^= h >> 15;
        (h >> 8) as f32 / 16_777_215.0
    }
    let cell = p.floor();
    let t = p - cell;
    let u = t * t * t * (t * (t * 6.0 - Vec3::splat(15.0)) + Vec3::splat(10.0));
    let du = 30.0 * t * t * (t - Vec3::ONE) * (t - Vec3::ONE);
    let mut result = 0.0;
    let mut gradient = Vec3::ZERO;
    for z in 0..2 {
        for y in 0..2 {
            for x in 0..2 {
                let v = value(cell.x as i32 + x, cell.y as i32 + y, cell.z as i32 + z);
                let w = Vec3::new(
                    if x == 0 { 1.0 - u.x } else { u.x },
                    if y == 0 { 1.0 - u.y } else { u.y },
                    if z == 0 { 1.0 - u.z } else { u.z },
                );
                let d = du
                    * Vec3::new(
                        if x == 0 { -1.0 } else { 1.0 },
                        if y == 0 { -1.0 } else { 1.0 },
                        if z == 0 { -1.0 } else { 1.0 },
                    );
                result += v * w.x * w.y * w.z;
                gradient += v * Vec3::new(d.x * w.y * w.z, w.x * d.y * w.z, w.x * w.y * d.z);
            }
        }
    }
    (result, gradient)
}

fn fields(p: Vec3, footprint: f32) -> Fields {
    let footprint = footprint_size(footprint);
    let (aggregate_value, _) = mineral_noise(p * 9.0 + Vec3::new(2.3, 5.1, -1.7));
    let aggregate = 0.5 + (aggregate_value - 0.5) * resolved(28.0, footprint);
    // Deposition is a sparse interrupted accent, never whole bands of hyphens.
    let bed_phase = p.y * 8.0 + p.x * 1.25 + (p.z * 1.4 + p.x * 0.7).sin() * 0.55;
    let bedding = smooth(0.93, 0.999, bed_phase.sin())
        * smooth(0.47, 0.70, aggregate)
        * resolved(32.0, footprint);
    let vein_phase = p.y * 3.8 + p.x * 0.85 + (p.z * 1.1).sin() * 0.25 + 1.1;
    let vein = smooth(0.96, 0.999, vein_phase.sin())
        * smooth(0.45, 0.72, aggregate)
        * resolved(32.0, footprint);

    // Three broad mineral lenses, not a full-field camouflage noise texture.
    let lens = |center: Vec3, radii: Vec3| {
        let q = (p - center) / radii;
        1.0 - smooth(0.15, 1.0, q.length_squared())
    };
    let mineral = (lens(Vec3::new(-0.72, 0.38, 0.72), Vec3::new(0.44, 0.17, 0.5))
        + 0.65 * lens(Vec3::new(0.68, -0.38, 0.64), Vec3::new(0.35, 0.24, 0.52))
        + 0.4 * lens(Vec3::new(0.28, 0.81, -0.25), Vec3::new(0.53, 0.3, 0.4)))
    .min(1.0);

    // Rounded fine grains and smaller pits share the height/normal field.
    // Octaves are attenuated before they become unresolved cell-scale speckle.
    let (fine, fine_gradient) = mineral_noise(p * 23.0 + Vec3::new(-3.4, 1.2, 4.7));
    let (micro, micro_gradient) = mineral_noise(p * 47.0 + Vec3::new(1.4, -2.6, 7.3));
    let fine_weight = resolved(72.0, footprint);
    let micro_weight = resolved(148.0, footprint);
    let grain = (fine - 0.5) * 2.0 * fine_weight;
    let pores = (1.0 - smooth(0.16, 0.34, micro)) * micro_weight;
    let gradient = fine_gradient * (0.0022 * 23.0 * fine_weight)
        + micro_gradient * (0.00065 * 47.0 * micro_weight);

    // Low, connected damp patches pool toward the base. No ticking noise or
    // view-dependent wet mask; wetness scales this same field continuously.
    let damp_wave =
        (p.x * 1.7 + p.z * 0.9).sin() * 0.16 + (p.z * 2.2 - p.y * 1.3 + 0.6).sin() * 0.12;
    let damp = (0.70 - p.y * 0.17 + damp_wave).clamp(0.35, 1.0);
    Fields {
        bedding,
        vein,
        mineral,
        grain,
        aggregate,
        pores,
        gradient,
        damp,
    }
}

/// Decorative micro-normal never replaces the geometry normal used for ray
/// offsets, visibility or the actual carved fracture. Wet coating is an artistic
/// single-layer approximation, not a measured/energy-complete layered BSDF.
pub fn sample(hit: Hit, wetness: f32, footprint: f32) -> MaterialSample {
    let f = fields(hit.local, footprint);
    let wetness = if wetness.is_finite() {
        wetness.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let wet = wetness * f.damp;
    let crack = 1.0 - smooth(0.008, 0.065, geometry::crack_distance(hit.local));
    let wear = geometry::edge_wear(hit.local).clamp(0.0, 1.0);
    let base = Vec3::new(0.48, 0.445, 0.375);
    let tone =
        1.0 + f.bedding * 0.025 + (f.aggregate - 0.5) * 0.10 + f.grain * 0.035 - f.pores * 0.13;
    let mineral_tint = Vec3::new(0.045, 0.014, -0.022) * f.mineral;
    let albedo = ((base * tone + mineral_tint + Vec3::splat(f.vein * 0.065 + wear * 0.06))
        * (1.0 - crack * 0.20)
        * (1.0 - wet * 0.43))
        .clamp(Vec3::splat(0.01), Vec3::ONE);
    let n = hit.normal.normalize_or_zero();
    let n = if n.length_squared() < 0.5 { Vec3::Y } else { n };
    let tangent_gradient = (f.gradient - n * f.gradient.dot(n)).clamp_length_max(0.22);
    let normal = (n - tangent_gradient * (1.0 - wet * 0.48)).normalize();
    MaterialSample {
        albedo,
        normal,
        roughness: (0.86 - wet * 0.76 - wear * 0.05 + f.pores * 0.05).clamp(0.20, 0.92),
        f0: 0.035 + wet * 0.012,
        occlusion: (1.0 - crack * 0.24 - f.pores * 0.07).clamp(0.6, 1.0),
    }
}

// One filtered structural classification drives both glyphs and history. A
// change of pore/aggregate/stratum family invalidates a held glyph even when the
// light did not change; brightness itself is deliberately absent from the key.
fn structural_region(hit: Hit, footprint: f32) -> usize {
    if geometry::crack_distance(hit.local) < 0.03 {
        return 0;
    }
    if geometry::edge_wear(hit.local) > 0.65 {
        return 1;
    }
    let f = fields(hit.local, footprint);
    if f.vein > 0.30 || f.bedding > 0.38 {
        return 2;
    }
    if f.pores > 0.2 {
        return 3;
    }
    if f.mineral > 0.5 {
        return 4;
    }
    let grain = (f.aggregate + f.grain * 0.14).clamp(0.0, 1.0);
    if grain < 0.35 {
        5
    } else if grain < 0.49 {
        6
    } else if grain < 0.63 {
        7
    } else {
        8
    }
}

/// Native glyph families describe grain mass rather than drawing continuous
/// lines across deposition bands. The common 0.15-0.4 light range has rounded
/// and angular ink shapes; dense marks are reserved for stronger illumination.
pub fn glyph(hit: Hit, illumination: f32, footprint: f32) -> u8 {
    let light = if illumination.is_finite() {
        illumination.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let level = if light < 0.025 {
        0
    } else if light < 0.07 {
        1
    } else if light < 0.13 {
        2
    } else if light < 0.23 {
        3
    } else if light < 0.38 {
        4
    } else if light < 0.58 {
        5
    } else if light < 0.8 {
        6
    } else {
        7
    };
    let family: &[u8; 8] = match structural_region(hit, footprint) {
        0 => b"  ..:;|#", // Actual cavities remain visually recessive.
        1 => b" .:cxO*@", // Worn/chipped edges.
        2 => b" .:cxo/@", // Sparse interrupted sediment accents.
        3 => b" .:coO0@", // Small porous inclusions.
        4 => b" ,;oxO%@", // Localized mineral lenses.
        5 => b" ,:coO%@",
        6 => b" .:oxO%@",
        7 => b" ,;xoO%@",
        _ => b" .:co0O@",
    };
    family[level]
}

/// Exact filtered family signature used by glyph(), independent of brightness,
/// view and wetness. The footprint is part of the classification, not quantized
/// separately into arbitrary LOD bins.
pub fn history_key(hit: Hit, footprint: f32) -> u32 {
    structural_region(hit, footprint) as u32 + 1
}
#[cfg(test)]
mod tests {
    use super::*;

    fn hit(local: Vec3, normal: Vec3) -> Hit {
        Hit {
            distance: 2.0,
            position: geometry::CENTER + local,
            local,
            normal,
            material: geometry::STONE_MATERIAL,
            object_id: 700,
        }
    }

    #[test]
    fn finite_bounded_fields_and_ascii_on_all_faces() {
        for i in -12..=12 {
            for j in -8..=8 {
                let u = i as f32 * 0.09;
                let v = j as f32 * 0.09;
                for (p, n) in [
                    (Vec3::new(u, v, 0.86), Vec3::Z),
                    (Vec3::new(u, v, -0.86), -Vec3::Z),
                    (Vec3::new(1.3, v, u * 0.6), Vec3::X),
                    (Vec3::new(-1.3, v, u * 0.6), -Vec3::X),
                    (Vec3::new(u, 0.95, v), Vec3::Y),
                    (Vec3::new(u, -0.95, v), -Vec3::Y),
                ] {
                    let h = hit(p, n);
                    for footprint in [0.0, 0.005, 0.03, 0.15, 1.0] {
                        for wet in [0.0, 0.5, 1.0] {
                            let m = sample(h, wet, footprint);
                            assert!(m.albedo.is_finite() && m.normal.is_finite());
                            assert!(m.albedo.min_element() >= 0.0 && m.albedo.max_element() <= 1.0);
                            assert!((m.normal.length() - 1.0).abs() < 1e-5);
                            assert!(m.normal.dot(n) > 0.95);
                            assert!((0.2..=0.92).contains(&m.roughness));
                            assert!((0.0..=1.0).contains(&m.f0));
                            assert!((0.6..=1.0).contains(&m.occlusion));
                            for light in [0.0, 0.08, 0.3, 0.8, 1.0] {
                                assert!((32..=126).contains(&glyph(h, light, footprint)));
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn wet_stone_changes_reflectance_not_only_tint() {
        let h = hit(Vec3::new(-0.65, -0.4, 0.86), Vec3::Z);
        let dry = sample(h, 0.0, 0.008);
        let wet = sample(h, 1.0, 0.008);
        assert!(wet.albedo.max_element() < dry.albedo.max_element());
        assert!(wet.roughness < dry.roughness - 0.15);
        assert!(wet.f0 > dry.f0);
        assert_eq!(sample(h, 1.0, 0.008).albedo, wet.albedo);
        assert_eq!(sample(h, 1.0, 0.008).normal, wet.normal);
        let moved = Hit {
            position: h.position + Vec3::X * 10.0,
            distance: 20.0,
            ..h
        };
        assert_eq!(history_key(h, 0.008), history_key(moved, 0.008));
        assert_eq!(sample(moved, 1.0, 0.008).albedo, wet.albedo);
    }

    #[test]
    fn unresolved_microdetail_vanishes_smoothly() {
        let p = Vec3::new(0.24, 0.32, 0.86);
        let coarse = fields(p, 0.2);
        assert_eq!(coarse.grain, 0.0);
        assert_eq!(coarse.pores, 0.0);
        assert_eq!(coarse.gradient, Vec3::ZERO);
        let a = sample(hit(p, Vec3::Z), 0.5, 0.025);
        let b = sample(hit(p, Vec3::Z), 0.5, 0.02501);
        assert!((a.albedo - b.albedo).length() < 0.001);
        assert!((a.normal - b.normal).length() < 0.001);
        assert_eq!(fields(p, 0.008).gradient, fields(p, 0.008).gradient);
    }

    #[test]
    fn history_signature_covers_every_light_level_and_filtered_family() {
        let mut families = std::collections::BTreeMap::new();
        let levels = [0.0, 0.05, 0.10, 0.18, 0.3, 0.5, 0.7, 1.0];
        let mut body_glyphs = std::collections::BTreeSet::new();
        for i in -20..=20 {
            for j in -14..=14 {
                let h = hit(Vec3::new(i as f32 * 0.057, j as f32 * 0.059, 0.86), Vec3::Z);
                for footprint in [0.004, 0.02, 0.05, 0.15] {
                    let signature = levels.map(|light| glyph(h, light, footprint));
                    if let Some(previous) = families.insert(history_key(h, footprint), signature) {
                        assert_eq!(
                            previous, signature,
                            "held family would hide a low-light change"
                        );
                    }
                    body_glyphs.insert(glyph(h, 0.18, footprint));
                }
            }
        }
        assert!(
            body_glyphs.contains(&b'c')
                && body_glyphs.contains(&b'o')
                && body_glyphs.contains(&b'x')
        );
        assert!(!body_glyphs.contains(&b'-'));
    }

    #[test]
    fn granular_height_derivative_matches_field_and_crosses_cells_continuously() {
        let p = Vec3::new(1.27, -0.48, 2.71);
        let (_, derivative) = mineral_noise(p);
        for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
            let numeric =
                (mineral_noise(p + axis * 0.0005).0 - mineral_noise(p - axis * 0.0005).0) / 0.001;
            assert!((numeric - derivative.dot(axis)).abs() < 0.003);
        }
        let a = mineral_noise(Vec3::new(0.99999, -0.48, 2.71));
        let b = mineral_noise(Vec3::new(1.00001, -0.48, 2.71));
        assert!((a.0 - b.0).abs() < 0.0001);
        assert!((a.1 - b.1).length() < 0.001);
    }
}
