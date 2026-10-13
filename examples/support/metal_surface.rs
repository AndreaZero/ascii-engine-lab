//! Worked steel study: overlapping hammer impressions, a few authored tool
//! scratches and persistent oxide. Object-local fields, no frame randomness.
//! These are artistic linear-color parameters, not measured alloy constants.
use ascii_engine_lab::core::{Hit, MaterialSample, MetallicSample};
use glam::{Vec2, Vec3};

const BODY: u32 = 50;
const RIM: u32 = 51;
const RIVET: u32 = 52;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Finish {
    Forged,
    Oxidized,
}

fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn width(footprint: f32) -> f32 {
    if footprint.is_finite() {
        footprint.max(0.0)
    } else {
        1.0
    }
}

fn hash(x: i32, y: i32, salt: u32) -> f32 {
    let mut v = (x as u32).wrapping_mul(0x9e37_79b9) ^ (y as u32).wrapping_mul(0x85eb_ca6b) ^ salt;
    v ^= v >> 16;
    v = v.wrapping_mul(0x7feb_352d);
    v ^= v >> 15;
    (v >> 8) as f32 / 16_777_215.0
}

#[derive(Clone, Copy)]
struct Surface {
    hammer: f32,
    scratch: f32,
    scratch_direction: u32,
    gradient: Vec3,
    oxide: f32,
    wear: f32,
}

fn surface(hit: Hit, finish: Finish, footprint: f32) -> Surface {
    let p = hit.local;
    let q = Vec2::new(p.x, p.y);
    let footprint = width(footprint);
    let hammer_filter = 1.0 - smooth(0.055, 0.16, footprint);
    let scratch_filter = 1.0 - smooth(0.006, 0.035, footprint);
    // Shallow, spatially irregular tool strikes. Each has finite support with
    // zero height and derivative at its edge; overlapping impressions add.
    let cell = (q / 0.19).floor();
    let mut hammer = 0.0;
    let mut gradient = Vec2::ZERO;
    for y in -1..=1 {
        for x in -1..=1 {
            let ix = cell.x as i32 + x;
            let iy = cell.y as i32 + y;
            let center = Vec2::new(
                ix as f32 + 0.18 + hash(ix, iy, 71) * 0.64,
                iy as f32 + 0.18 + hash(ix, iy, 193) * 0.64,
            ) * 0.19;
            let radius = 0.075 + hash(ix, iy, 317) * 0.038;
            let d = q - center;
            let u = d.length_squared() / (radius * radius);
            if u < 1.0 {
                let depression = (1.0 - u) * (1.0 - u);
                hammer += depression * hammer_filter;
                gradient += d * (0.006 * (1.0 - u) / (radius * radius) * hammer_filter);
            }
        }
    }
    hammer = hammer.min(1.0);

    // Isolated scoring marks with tapered ends; no continuous periodic stripes.
    let segments = [
        (Vec2::new(-0.60, 0.48), Vec2::new(-0.24, 0.62)),
        (Vec2::new(-0.55, 0.44), Vec2::new(-0.29, 0.54)),
        (Vec2::new(0.12, 0.28), Vec2::new(0.45, -0.05)),
        (Vec2::new(0.18, 0.27), Vec2::new(0.39, 0.03)),
        (Vec2::new(-0.15, -0.21), Vec2::new(0.12, -0.37)),
        (Vec2::new(-0.48, -0.53), Vec2::new(-0.36, -0.19)),
        (Vec2::new(0.40, -0.62), Vec2::new(0.59, -0.24)),
    ];
    let mut scratch = 0.0;
    let mut scratch_direction = 0;
    for (a, b) in segments {
        let direction = b - a;
        let t = ((q - a).dot(direction) / direction.length_squared()).clamp(0.0, 1.0);
        let d = q - (a + direction * t);
        let r2 = d.length_squared() / (0.012 * 0.012);
        let taper = smooth(0.0, 0.12, t) * (1.0 - smooth(0.82, 1.0, t));
        let strength = (1.0 - r2).max(0.0).powi(2) * taper * scratch_filter;
        if strength > scratch {
            scratch = strength;
            scratch_direction = u32::from(direction.y < 0.0);
        }
        if r2 < 1.0 {
            gradient += d * (0.0012 * (1.0 - r2) / (0.012 * 0.012) * taper * scratch_filter);
        }
    }

    // Broad oxide accumulation is authored separately from hammer detail.
    // It gathers near the lower edge and in three non-periodic weathered patches.
    let lens = |center: Vec2, radii: Vec2| {
        let d = (q - center) / radii;
        1.0 - smooth(0.25, 1.0, d.length_squared())
    };
    let patch = (lens(Vec2::new(-0.46, 0.24), Vec2::new(0.34, 0.42))
        + lens(Vec2::new(0.42, -0.42), Vec2::new(0.37, 0.47))
        + 0.6 * lens(Vec2::new(0.26, 0.69), Vec2::new(0.32, 0.23)))
    .min(1.0);
    let lower = 1.0 - smooth(-0.95, -0.42, p.y);
    let radius = (Vec2::new(p.x / 0.95, p.y / 1.1)).length();
    let wear = match hit.material {
        RIM => 1.0,
        RIVET => 1.0,
        BODY => smooth(0.77, 0.98, radius),
        _ => 0.0,
    };
    // Forged finish explicitly retains dark forge scale. Exposed scoring and
    // worn rim/rivets are pure conductor. No fake diffuse is assigned to them.
    let oxide = match finish {
        Finish::Forged => {
            (0.34 + hammer * 0.23 + patch * 0.12 + lower * 0.09) * (1.0 - wear) * (1.0 - scratch)
        }
        Finish::Oxidized => {
            (0.16 + patch * 0.64 + lower * 0.35 + hammer * 0.13).clamp(0.0, 1.0)
                * (1.0 - wear * 0.8)
                * (1.0 - scratch * 0.65)
        }
    };
    Surface {
        hammer,
        scratch,
        scratch_direction,
        gradient: Vec3::new(gradient.x, gradient.y, 0.0),
        oxide: oxide.clamp(0.0, 1.0),
        wear,
    }
}

/// Isotropic roughness plus decorative tool-mark normal. It is not an
/// anisotropic BRDF, and the hammered relief does not alter ray intersections.
pub fn sample(hit: Hit, finish: Finish, footprint: f32) -> MaterialSample {
    let s = surface(hit, finish, footprint);
    let n = hit.normal.normalize_or_zero();
    let n = if n.length_squared() < 0.5 { Vec3::Z } else { n };
    let slope = (s.gradient - n * s.gradient.dot(n)).clamp_length_max(0.20);
    let normal = (n - slope * (1.0 - s.wear * 0.75)).normalize();
    let oxide_color = match finish {
        Finish::Forged => Vec3::new(0.24, 0.265, 0.28),
        Finish::Oxidized => Vec3::new(0.32, 0.105, 0.035),
    };
    MaterialSample {
        albedo: oxide_color * (0.94 + s.hammer * 0.08),
        normal,
        roughness: (0.38 + s.oxide * 0.43 + s.hammer * 0.05 - s.wear * 0.14 - s.scratch * 0.10)
            .clamp(0.20, 0.90),
        f0: 0.04,
        occlusion: 1.0 - s.hammer * 0.06 * s.oxide,
    }
}

/// f0 is the linear RGB conductor endpoint; the renderer mixes it with the
/// dielectric endpoint using metalness. Chosen steel values are illustrative.
/// Pure-metal regions have no Lambert/ambient diffuse; no IBL is fabricated.
pub fn metallic(hit: Hit, finish: Finish, footprint: f32) -> MetallicSample {
    let s = surface(hit, finish, footprint);
    MetallicSample {
        f0: Vec3::new(0.55, 0.57, 0.60),
        metalness: 1.0 - s.oxide,
    }
}

// Exact discrete structural family, shared by ink selection and history.
fn region(hit: Hit, finish: Finish, footprint: f32) -> usize {
    let s = surface(hit, finish, footprint);
    if s.scratch > 0.25 {
        return 1 + s.scratch_direction as usize;
    }
    if hit.material == RIVET {
        return 3;
    }
    if hit.material == RIM {
        return 4;
    }
    if finish == Finish::Oxidized && s.oxide > 0.6 {
        return 5;
    }
    if s.hammer > 0.55 {
        6
    } else if s.hammer > 0.18 {
        7
    } else {
        8
    }
}

pub fn glyph(hit: Hit, finish: Finish, illumination: f32, footprint: f32) -> u8 {
    let light = if illumination.is_finite() {
        illumination.max(0.0)
    } else {
        0.0
    };
    let level = if light < 0.018 {
        0
    } else if light < 0.045 {
        1
    } else if light < 0.10 {
        2
    } else if light < 0.20 {
        3
    } else if light < 0.38 {
        4
    } else if light < 0.60 {
        5
    } else if light < 0.85 {
        6
    } else {
        7
    };
    let palette: &[u8; 8] = match region(hit, finish, footprint) {
        1 => b" .:/77#@",
        2 => b" .:\\XX#@",
        3 => b" .:oO0#@",
        4 => b" .:+=O#@",
        5 => b" ,;cx%#@", // Rust is granular; exposed metal uses worked marks.
        6 => b" ,(uUO#@", // Hammer bowl.
        7 => b" .)nXO#@", // Shoulder of a tool impression.
        _ => b" .:xzX#@",
    };
    palette[level]
}

pub fn history_key(hit: Hit, finish: Finish, footprint: f32) -> u32 {
    region(hit, finish, footprint) as u32 | if finish == Finish::Oxidized { 32 } else { 16 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(x: f32, y: f32, material: u32) -> Hit {
        Hit {
            distance: 2.0,
            position: Vec3::new(x, y + 1.1, 0.25),
            local: Vec3::new(x, y, 0.25),
            normal: Vec3::Z,
            material,
            object_id: 800,
        }
    }

    #[test]
    fn finite_materials_and_shared_filtered_history() {
        let mut signatures = std::collections::BTreeMap::new();
        for i in -8..=8 {
            for j in -9..=9 {
                for material in [BODY, RIM, RIVET] {
                    let h = hit(i as f32 * 0.09, j as f32 * 0.09, material);
                    for finish in [Finish::Forged, Finish::Oxidized] {
                        for fp in [0.0, 0.008, 0.03, 0.15, 0.4] {
                            let m = sample(h, finish, fp);
                            let conductor = metallic(h, finish, fp);
                            assert!(m.albedo.is_finite() && m.normal.is_finite());
                            assert!((m.normal.length() - 1.0).abs() < 1e-5);
                            assert!(m.normal.dot(h.normal) > 0.97);
                            assert!((0.2..=0.9).contains(&m.roughness));
                            assert!((0.0..=1.0).contains(&conductor.metalness));
                            assert!(
                                conductor.f0.min_element() > 0.0
                                    && conductor.f0.max_element() < 1.0
                            );
                            let signature = [0.0, 0.03, 0.08, 0.15, 0.3, 0.5, 0.7, 1.0]
                                .map(|l| glyph(h, finish, l, fp));
                            assert!(signature.iter().all(|g| (32..=126).contains(g)));
                            if let Some(previous) =
                                signatures.insert(history_key(h, finish, fp), signature)
                            {
                                assert_eq!(previous, signature);
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn oxide_changes_conductor_coverage_and_roughness_with_exposed_edges() {
        let h = hit(-0.46, 0.24, BODY);
        let forged = metallic(h, Finish::Forged, 0.01);
        let oxide = metallic(h, Finish::Oxidized, 0.01);
        assert!(oxide.metalness < forged.metalness);
        assert!(
            sample(h, Finish::Oxidized, 0.01).roughness > sample(h, Finish::Forged, 0.01).roughness
        );
        assert_eq!(
            metallic(hit(0.6, 0.6, RIVET), Finish::Forged, 0.01).metalness,
            1.0
        );
        let translated = Hit {
            position: h.position + Vec3::X * 20.0,
            ..h
        };
        assert_eq!(
            sample(h, Finish::Forged, 0.01).normal,
            sample(translated, Finish::Forged, 0.01).normal
        );
    }

    #[test]
    fn tool_detail_filters_out_without_changing_geometry() {
        let h = hit(-0.40, 0.56, BODY);
        let far = surface(h, Finish::Forged, 0.4);
        assert_eq!(far.hammer, 0.0);
        assert_eq!(far.scratch, 0.0);
        assert_eq!(far.gradient, Vec3::ZERO);
        let a = sample(h, Finish::Forged, 0.024);
        let b = sample(h, Finish::Forged, 0.02401);
        assert!((a.normal - b.normal).length() < 0.001);
        assert!((a.roughness - b.roughness).abs() < 0.001);
        assert_eq!(h.normal, Vec3::Z);
    }
}
