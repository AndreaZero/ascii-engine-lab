//! Direct material/visibility queries -> semantic glyphs. No RGB image input.
use crate::{camera::Camera, core::*};
use glam::Vec3;

#[derive(Default, Debug, Clone)]
pub struct FrameStats {
    pub visible: usize,
    pub reflected: usize,
    pub matched: usize,
    pub glyph_changes: usize,
    pub mean_color_delta: f32,
}
#[derive(Default)]
pub struct Composer {
    previous_cells: Vec<(u32, u32, f32, [f32; 4])>,
}
impl Composer {
    pub fn reset(&mut self) {
        self.previous_cells.clear();
    }
    // Explicit prototype inputs; no global window/fixture coupling in this API.
    #[allow(clippy::too_many_arguments)]
    pub fn compose(
        &mut self,
        scene: &impl Scene,
        camera: Camera,
        time: f32,
        stage: Stage,
        cols: u32,
        rows: u32,
        aspect: f32,
    ) -> (Vec<GlyphCell>, FrameStats) {
        let mut cells = Vec::with_capacity((cols * rows) as usize);
        let mut stats = FrameStats::default();
        let mut previous_cells = Vec::with_capacity((cols * rows) as usize);
        for y in 0..rows {
            for x in 0..cols {
                let ray = camera.ray(x, y, cols, rows, aspect);
                let Some(hit) = scene.trace(ray, time, stage, true) else {
                    cells.push(GlyphCell::default());
                    previous_cells.push((0, 32, 0., [0., 0., 0., 1.]));
                    continue;
                };
                let mut id = surface_id(hit);
                let mut color = shade(scene, hit, time, stage);
                let mut source = hit;
                if hit.material == WATER {
                    let nx = (hit.position.x * 3.2 + time * 1.4).cos() * 0.055;
                    let nz = (hit.position.z * 4.1 - time * 1.1).sin() * 0.045;
                    let n = Vec3::new(nx, 1., nz).normalize();
                    let reflected = Ray {
                        origin: hit.position + Vec3::Y * 0.012,
                        dir: ray.dir - 2. * ray.dir.dot(n) * n,
                    };
                    if let Some(r) = scene.trace(reflected, time, stage, false) {
                        let fresnel = 0.28 + 0.52 * (1. - (-ray.dir.dot(n)).clamp(0., 1.)).powi(3);
                        color =
                            shade(scene, r, time, stage) * Vec3::new(0.58, 0.76, 0.85) * fresnel
                                + color * 0.15;
                        source = r;
                        id = surface_id(r) ^ 0x80000000;
                        stats.reflected += 1;
                    } else {
                        color *= 0.5;
                    }
                }
                let illumination = color.max_element();
                let mut glyph =
                    scene.glyph(source.material, source.local, source.normal, illumination) as u32;
                if illumination < 0.012 {
                    glyph = 32;
                }
                let fog = (-hit.distance * 0.026).exp();
                color = (color * fog).min(Vec3::ONE);
                // Conservative hold only with matching ID in the same cell. Cross-cell
                // bin collisions must never copy a different glyph into a frozen frame.
                // Full history reprojection is intentionally outside this first spike.
                let mut anchor = illumination;
                if let Some(&(previous_id, old, old_i, old_color)) =
                    self.previous_cells.get(cells.len())
                {
                    if previous_id == id
                        && old != 32
                        && glyph != 32
                        && (illumination - old_i).abs() < 0.022
                    {
                        glyph = old;
                        anchor = old_i;
                    }
                    // Conservative correspondence: same screen cell AND surface bin.
                    // Exclude moved/unmatched cells; report coverage, not a full reprojected score.
                    if previous_id == id {
                        stats.matched += 1;
                        if old != glyph {
                            stats.glyph_changes += 1;
                        }
                        stats.mean_color_delta +=
                            (0..3).map(|c| (color[c] - old_color[c]).abs()).sum::<f32>() / 3.;
                    }
                }
                let color = [color.x.min(1.), color.y.min(1.), color.z.min(1.), 1.];
                let cell = GlyphCell {
                    glyph,
                    flags: if hit.material == WATER { 1 } else { 0 },
                    id,
                    pad: 0,
                    color,
                };
                previous_cells.push((id, glyph, anchor, color));
                if glyph != 32 {
                    stats.visible += 1;
                }
                cells.push(cell);
            }
        }
        if stats.matched > 0 {
            stats.mean_color_delta /= stats.matched as f32;
        }
        self.previous_cells = previous_cells;
        (cells, stats)
    }
}
fn shade(scene: &impl Scene, hit: Hit, time: f32, stage: Stage) -> Vec3 {
    if hit.material == EMISSIVE {
        return Vec3::new(1., 0.55, 0.12);
    }
    let base = scene.pigment(hit.material);
    let moon = Vec3::new(-0.5, 0.85, 0.3).normalize();
    let ambient = Vec3::new(0.16, 0.23, 0.31) * (0.13 + 0.36 * hit.normal.dot(moon).max(0.));
    if stage == Stage::Core {
        return base * (ambient + Vec3::splat(0.1));
    }
    let light = scene.light_position(time);
    let to = light - hit.position;
    let dist = to.length();
    let dir = to / dist;
    let diffuse = hit.normal.dot(dir).max(0.);
    let blocked = diffuse > 0.
        && scene
            .trace(
                Ray {
                    origin: hit.position + hit.normal * 0.012,
                    dir,
                },
                time,
                stage,
                false,
            )
            .is_some_and(|h| h.distance < dist - 0.09 && h.material != EMISSIVE);
    let response = if blocked {
        0.
    } else {
        diffuse * 4.5 / (1. + dist * dist * 0.45)
    };
    base * (ambient + Vec3::new(1., 0.58, 0.23) * response)
}
pub fn surface_id(hit: Hit) -> u32 {
    let q = (hit.local * 12.).floor().as_ivec3();
    let mut id = hit.object_id.wrapping_mul(16777619);
    for v in [q.x, q.y, q.z] {
        id = (id ^ (v as u32)).wrapping_mul(16777619);
    }
    id.max(1)
}
pub fn overlay(cells: &mut [GlyphCell], cols: u32, rows: u32, text: &str) {
    if rows < 2 {
        return;
    }
    for (i, ch) in text
        .bytes()
        .take(cols.saturating_sub(2) as usize)
        .enumerate()
    {
        cells[(cols + 1) as usize + i] = GlyphCell {
            glyph: ch as u32,
            flags: 2,
            id: 0,
            pad: 0,
            color: [0.5, 0.61, 0.64, 1.],
        };
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{camera::Mode, LabScene};
    #[test]
    fn frozen_scene_deterministic_and_printable() {
        for m in [Mode::First, Mode::Third, Mode::Overhead] {
            let mut a = Composer::default();
            let mut b = Composer::default();
            let c = Camera::new(m);
            let (x, _) = a.compose(&LabScene, c, 1., Stage::Cloth, 40, 24, 16. / 9.);
            let (y, _) = b.compose(&LabScene, c, 1., Stage::Cloth, 40, 24, 16. / 9.);
            assert_eq!(x, y);
            assert!(x.iter().all(|c| (32..=126).contains(&c.glyph)));
            assert!(x.iter().any(|c| c.glyph != 32));
            let (z, _) = a.compose(&LabScene, c, 1., Stage::Cloth, 40, 24, 16. / 9.);
            assert!(x == z, "history must not change a frozen image");
        }
    }
}
