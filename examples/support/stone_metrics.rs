//! Untimed stone-only diagnostics. Normal buckets describe geometric orientation,
//! not semantic faces. Atlas occupancy is not output-pixel coverage: resizing can
//! sample font texels unevenly. RGB averages/energy are not perceptual luminance.
use ascii_engine_lab::core::{GlyphCell, Hit};
use font8x8::UnicodeFonts;
use serde_json::{json, Value};
use std::collections::BTreeMap;

const GROUPS: [&str; 7] = ["+X", "-X", "+Y", "-Y", "+Z", "-Z", "invalid_normal"];
const SPARSE_BITS: u32 = 8;

#[derive(Default)]
struct Bucket {
    cells: u64,
    blank: u64,
    sparse: u64,
    invalid_glyph: u64,
    invalid_color: u64,
    color_cells: u64,
    ink_sum: f64,
    effective_ink_sum: f64,
    rgb_sum: [f64; 3],
    energy_sum: [f64; 3],
    glyphs: BTreeMap<u32, u64>,
}

impl Bucket {
    fn add(&mut self, cell: &GlyphCell, bits: u32) {
        self.cells += 1;
        self.blank += u64::from(bits == 0);
        self.sparse += u64::from(bits > 0 && bits <= SPARSE_BITS);
        self.invalid_glyph += u64::from(!(32..=126).contains(&cell.glyph));
        *self.glyphs.entry(cell.glyph).or_default() += 1;
        let ink = f64::from(bits) / 64.0;
        self.ink_sum += ink;
        // Non-finite input has no claimed GPU equivalent. Keep geometry/font
        // counts, but explicitly exclude the complete RGBA sample from means.
        if !cell.color.iter().all(|c| c.is_finite()) {
            self.invalid_color += 1;
            return;
        }
        self.color_cells += 1;
        let effective = ink * f64::from(cell.color[3].clamp(0.0, 1.0));
        self.effective_ink_sum += effective;
        for (channel, raw) in cell.color[..3].iter().enumerate() {
            let rgb = f64::from(raw.clamp(0.0, 1.0));
            self.rgb_sum[channel] += rgb;
            self.energy_sum[channel] += rgb * effective;
        }
    }

    fn finish(self) -> Value {
        if self.cells == 0 {
            return Value::Null;
        }
        let count = self.cells as f64;
        let color = (self.color_cells > 0).then(|| {
            let n = self.color_cells as f64;
            let rgb = self.rgb_sum.map(|v| v / n);
            let energy = self.energy_sum.map(|v| v / n);
            json!({
                "samples": self.color_cells,
                "mean_linear_rgb_clamped": rgb,
                "mean_effective_ink": self.effective_ink_sum / n,
                "mean_linear_rgb_times_effective_ink": energy,
                "mean_channel_energy_proxy": energy.iter().sum::<f64>() / 3.0
            })
        });
        json!({
            "cells": self.cells,
            "blank_cells": self.blank,
            "blank_fraction": self.blank as f64 / count,
            "sparse_nonblank_cells": self.sparse,
            "sparse_nonblank_fraction": self.sparse as f64 / count,
            "invalid_glyph_cells": self.invalid_glyph,
            "nonfinite_rgba_cells": self.invalid_color,
            "mean_atlas_ink": self.ink_sum / count,
            "color": color,
            "glyph_code_histogram": self.glyphs
        })
    }
}

/// Accumulates only primary hits with the requested object ID. Use outside the
/// compose timer, passing the corresponding output cell (including history).
pub struct StoneMetrics {
    object_id: u32,
    total: Bucket,
    groups: [Bucket; 7],
}

impl StoneMetrics {
    pub fn new(object_id: u32) -> Self {
        Self {
            object_id,
            total: Bucket::default(),
            groups: std::array::from_fn(|_| Bucket::default()),
        }
    }

    pub fn add(&mut self, hit: Option<Hit>, cell: &GlyphCell) {
        let Some(hit) = hit.filter(|hit| hit.object_id == self.object_id) else {
            return;
        };
        let bits = glyph_bits(cell.glyph);
        let normal = hit.normal;
        let group = if !normal.is_finite() || normal == glam::Vec3::ZERO {
            6
        } else {
            let abs = normal.abs();
            let axis = if abs.x >= abs.y && abs.x >= abs.z {
                0
            } else if abs.y >= abs.z {
                1
            } else {
                2
            };
            axis * 2 + usize::from(normal[axis] < 0.0)
        };
        self.total.add(cell, bits);
        self.groups[group].add(cell, bits);
    }

    pub fn finish(self) -> Value {
        let groups: BTreeMap<_, _> = GROUPS
            .into_iter()
            .zip(self.groups.map(Bucket::finish))
            .collect();
        json!({
            "object_id": self.object_id,
            "definitions": {
                "grouping": "Dominant absolute component of geometric Hit.normal; sign retained; ties X then Y then Z. Not semantic faces; zero/nonfinite normals have their own group.",
                "blank": "Zero lit font texels; controls and codes outside 32..126 are blank like glyph.wgsl.",
                "sparse_nonblank": "1..=8 lit texels out of 64; excludes blank; fraction denominator is all matching cells.",
                "ink": "BASIC_FONTS lit texels / 64; effective ink multiplies clamped alpha. Atlas occupancy, not actual framebuffer-pixel coverage.",
                "color": "Finite RGBA only; RGB and alpha clamped to 0..1 as in shader. Linear RGB times effective ink is an unweighted energy proxy, not perceptual luminance, sRGB output, or measured GPU pixels.",
                "empty": "Empty buckets and absent color samples are null, never a fabricated zero mean.",
                "nonfinite": "Nonfinite RGBA excluded from color/effective-ink averages and counted; font/geometry counts retained. No GPU behavior assumed for NaN/Infinity."
            },
            "total": self.total.finish(),
            "geometric_normal_groups": groups
        })
    }
}

fn glyph_bits(code: u32) -> u32 {
    if !(32..=126).contains(&code) {
        return 0;
    }
    font8x8::BASIC_FONTS
        .get(char::from_u32(code).unwrap())
        .map(|bitmap| bitmap.iter().map(|row| row.count_ones()).sum())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    fn hit(object_id: u32, normal: Vec3) -> Option<Hit> {
        Some(Hit {
            distance: 1.0,
            position: Vec3::ZERO,
            normal,
            material: 32,
            object_id,
            local: Vec3::ZERO,
        })
    }

    fn cell(glyph: u32, color: [f32; 4]) -> GlyphCell {
        GlyphCell {
            glyph,
            color,
            ..GlyphCell::default()
        }
    }

    #[test]
    fn only_requested_hit_id_enters_totals_and_empty_means_are_null() {
        let mut metrics = StoneMetrics::new(700);
        let cell = cell(b'#' as u32, [1.0; 4]);
        metrics.add(None, &cell);
        metrics.add(hit(1, Vec3::Y), &cell);
        let empty = metrics.finish();
        assert!(empty["total"].is_null());
        assert!(empty["geometric_normal_groups"]["+Y"].is_null());
        let mut metrics = StoneMetrics::new(700);
        metrics.add(hit(700, Vec3::Y), &cell);
        assert_eq!(metrics.finish()["total"]["cells"], 1);
    }

    #[test]
    fn font_occupancy_and_alpha_follow_atlas_and_shader_rules() {
        // font8x8 period is two 0x0c rows: four lit texels, not a solid cell.
        assert_eq!(glyph_bits(b'.' as u32), 4);
        assert_eq!(glyph_bits(b' ' as u32), 0);
        assert_eq!(glyph_bits(0), 0);
        assert_eq!(glyph_bits(127), 0);
        assert_eq!(glyph_bits(u32::MAX), 0);
        let mut metrics = StoneMetrics::new(700);
        metrics.add(hit(700, Vec3::Z), &cell(b'.' as u32, [2.0, -1.0, 0.5, 0.5]));
        let v = metrics.finish();
        let total = &v["total"];
        assert_eq!(total["sparse_nonblank_cells"], 1);
        assert_eq!(total["mean_atlas_ink"], 0.0625);
        assert_eq!(total["color"]["mean_effective_ink"], 0.03125);
        assert_eq!(
            total["color"]["mean_linear_rgb_times_effective_ink"],
            json!([0.03125, 0.0, 0.015625])
        );
    }

    #[test]
    fn blank_and_invalid_glyphs_count_without_inventing_energy() {
        let mut metrics = StoneMetrics::new(700);
        for glyph in [32, 127, u32::MAX] {
            metrics.add(hit(700, Vec3::Z), &cell(glyph, [1.0; 4]));
        }
        let v = metrics.finish();
        assert_eq!(v["total"]["blank_cells"], 3);
        assert_eq!(v["total"]["invalid_glyph_cells"], 2);
        assert_eq!(v["total"]["sparse_nonblank_cells"], 0);
        assert_eq!(v["total"]["color"]["mean_channel_energy_proxy"], 0.0);
    }

    #[test]
    fn geometric_axes_signs_ties_and_invalid_normals_are_explicit() {
        let mut metrics = StoneMetrics::new(700);
        for normal in [
            Vec3::X,
            -Vec3::X,
            Vec3::Y,
            -Vec3::Y,
            Vec3::Z,
            -Vec3::Z,
            Vec3::ONE,
            Vec3::new(0.0, -2.0, 2.0),
            Vec3::ZERO,
            Vec3::new(f32::NAN, 0.0, 0.0),
        ] {
            metrics.add(hit(700, normal), &GlyphCell::default());
        }
        let v = metrics.finish();
        let groups = &v["geometric_normal_groups"];
        for (name, expected) in [
            ("+X", 2),
            ("-X", 1),
            ("+Y", 1),
            ("-Y", 2),
            ("+Z", 1),
            ("-Z", 1),
            ("invalid_normal", 2),
        ] {
            assert_eq!(groups[name]["cells"], expected);
        }
        assert_eq!(v["total"]["cells"], 10);
    }

    #[test]
    fn nonfinite_rgba_is_counted_and_cannot_poison_valid_averages() {
        let mut metrics = StoneMetrics::new(700);
        metrics.add(hit(700, Vec3::Z), &cell(46, [f32::NAN, 1.0, 1.0, 1.0]));
        metrics.add(hit(700, Vec3::X), &cell(46, [1.0, 1.0, 1.0, f32::INFINITY]));
        metrics.add(hit(700, Vec3::Z), &cell(46, [1.0; 4]));
        let v = metrics.finish();
        assert_eq!(v["total"]["nonfinite_rgba_cells"], 2);
        assert_eq!(v["total"]["color"]["samples"], 1);
        assert_eq!(v["total"]["color"]["mean_channel_energy_proxy"], 0.0625);
        assert!(v["geometric_normal_groups"]["+X"]["color"].is_null());
        assert!(serde_json::to_string(&v).is_ok());
    }
}
