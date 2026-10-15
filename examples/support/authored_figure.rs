//! R16 bounded content producer. No mesh, renderer artwork or cloth physics.
//! Panels are two-sided graph sheets: fixed bracketing can miss tangent/double
//! roots. They cannot fold over themselves. Coordinates follow deformation.
//! This experimental bounded schema parses through serde_json::Value: duplicate
//! JSON member names use serde_json's last value. Duplicate numeric object and
//! material identities are rejected. This is not a general asset validator.
use ascii_render::core::{intersect_ellipsoid, Hit, MaterialSample, Ray};
use glam::{EulerRot, Quat, Vec3};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

#[derive(Clone)]
struct Material {
    color: Vec3,
    roughness: f32,
    scale: f32,
    structure: bool,
}
#[derive(Clone)]
struct Part {
    id: u32,
    material: u32,
    center: Vec3,
    radii: Vec3,
    rotation: Quat,
    motion: Vec3,
    speed: f32,
}
#[derive(Clone)]
struct Panel {
    id: u32,
    material: u32,
    origin: Vec3,
    rotation: Quat,
    height: f32,
    bottom: f32,
    top: f32,
    slope: f32,
    folds: f32,
    amplitude: f32,
    phase: f32,
    sway: f32,
    speed: f32,
}
struct Data {
    materials: BTreeMap<u32, Material>,
    parts: Vec<Part>,
    panels: Vec<Panel>,
}
#[derive(Clone)]
pub struct FigureDefinition {
    data: Arc<Data>,
}
#[derive(Clone)]
pub struct FigureSnapshot {
    data: Arc<Data>,
    parts: Vec<Part>,
    time: f32,
}

fn number(v: &Value, name: &str, default: Option<f32>) -> Result<f32, String> {
    let n = match v.get(name) {
        Some(v) => v
            .as_f64()
            .ok_or_else(|| format!("{name}: expected number"))? as f32,
        None => default.ok_or_else(|| format!("missing {name}"))?,
    };
    if n.is_finite() && n.abs() <= 100. {
        Ok(n)
    } else {
        Err(format!("{name}: finite value within +/-100 required"))
    }
}
fn vector(v: &Value, name: &str, default: Option<Vec3>) -> Result<Vec3, String> {
    let Some(a) = v.get(name) else {
        return default.ok_or_else(|| format!("missing {name}"));
    };
    let a = a
        .as_array()
        .filter(|a| a.len() == 3)
        .ok_or_else(|| format!("{name}: expected three numbers"))?;
    let mut out = [0.; 3];
    for (i, n) in a.iter().enumerate() {
        out[i] = n
            .as_f64()
            .ok_or_else(|| format!("{name}: expected number"))? as f32;
    }
    let p = Vec3::from_array(out);
    if p.is_finite() && p.abs().max_element() <= 100. {
        Ok(p)
    } else {
        Err(format!("{name}: invalid vector"))
    }
}
fn id(v: &Value, name: &str, minimum: u32) -> Result<u32, String> {
    v.get(name)
        .and_then(Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
        .filter(|n| *n >= minimum)
        .ok_or_else(|| format!("{name}: integer >= {minimum} required"))
}
fn rotation(v: &Value) -> Result<Quat, String> {
    let r = vector(v, "rotation", Some(Vec3::ZERO))?;
    Ok(Quat::from_euler(EulerRot::XYZ, r.x, r.y, r.z))
}
fn array<'a>(v: &'a Value, name: &str, max: usize) -> Result<&'a Vec<Value>, String> {
    v.get(name)
        .and_then(Value::as_array)
        .filter(|a| a.len() <= max)
        .ok_or_else(|| format!("{name}: array with at most {max} entries required"))
}
fn fields(v: &Value, allowed: &[&str]) -> Result<(), String> {
    let object = v.as_object().ok_or("expected object")?;
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(format!("unknown field {key}"));
        }
    }
    Ok(())
}
impl FigureDefinition {
    pub fn from_json(text: &str) -> Result<Self, String> {
        if text.len() > 262144 {
            return Err("figure exceeds 256 KiB".into());
        }
        let v: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        fields(&v, &["version", "name", "materials", "parts", "panels"])?;
        if v["version"].as_u64() != Some(1) {
            return Err("figure version must be 1".into());
        }
        let mut materials = BTreeMap::new();
        for m in array(&v, "materials", 16)? {
            fields(
                m,
                &["id", "color", "roughness", "feature_scale", "structure"],
            )?;
            let key = id(m, "id", 32)?;
            let color = vector(m, "color", None)?;
            let roughness = number(m, "roughness", Some(0.8))?;
            let scale = number(m, "feature_scale", Some(0.08))?;
            let structure = match m.get("structure") {
                None => true,
                Some(v) => v.as_bool().ok_or("structure must be boolean")?,
            };
            if color.min_element() < 0.
                || color.max_element() > 1.
                || !(0.04..=1.).contains(&roughness)
                || !(0.01..=1.).contains(&scale)
            {
                return Err("invalid material range".into());
            }
            if materials
                .insert(
                    key,
                    Material {
                        color,
                        roughness,
                        scale,
                        structure,
                    },
                )
                .is_some()
            {
                return Err("duplicate material ID".into());
            }
        }
        let mut ids = BTreeSet::new();
        let mut parts = Vec::new();
        for p in array(&v, "parts", 48)? {
            fields(
                p,
                &[
                    "id", "material", "center", "radii", "rotation", "motion", "speed",
                ],
            )?;
            let part = Part {
                id: id(p, "id", 100)?,
                material: id(p, "material", 32)?,
                center: vector(p, "center", None)?,
                radii: vector(p, "radii", None)?,
                rotation: rotation(p)?,
                motion: vector(p, "motion", Some(Vec3::ZERO))?,
                speed: number(p, "speed", Some(1.6))?,
            };
            if part.radii.min_element() < 0.005
                || part.radii.max_element() > 2.
                || part.center.abs().max_element() > 4.
                || part.motion.abs().max_element() > 0.15
                || part.speed.abs() > 8.
            {
                return Err("part outside bounded figure range".into());
            }
            if !ids.insert(part.id) || !materials.contains_key(&part.material) {
                return Err("duplicate object ID or missing material".into());
            }
            parts.push(part);
        }
        let mut panels = Vec::new();
        for p in array(&v, "panels", 6)? {
            fields(
                p,
                &[
                    "id",
                    "material",
                    "origin",
                    "rotation",
                    "height",
                    "bottom",
                    "top",
                    "slope",
                    "folds",
                    "amplitude",
                    "phase",
                    "sway",
                    "speed",
                ],
            )?;
            let panel = Panel {
                id: id(p, "id", 100)?,
                material: id(p, "material", 32)?,
                origin: vector(p, "origin", None)?,
                rotation: rotation(p)?,
                height: number(p, "height", None)?,
                bottom: number(p, "bottom", None)?,
                top: number(p, "top", None)?,
                slope: number(p, "slope", Some(0.))?,
                folds: number(p, "folds", Some(2.))?,
                amplitude: number(p, "amplitude", Some(0.02))?,
                phase: number(p, "phase", Some(0.))?,
                sway: number(p, "sway", Some(0.025))?,
                speed: number(p, "speed", Some(1.7))?,
            };
            if !(0.05..=2.).contains(&panel.height)
                || !(0.02..=1.).contains(&panel.bottom)
                || !(0.02..=1.).contains(&panel.top)
                || panel.slope.abs() > 0.5
                || !(0.0..=6.).contains(&panel.folds)
                || panel.amplitude.abs() > 0.08
                || panel.sway.abs() > 0.10
                || panel.speed.abs() > 8.
                || panel.origin.abs().max_element() > 4.
            {
                return Err("panel outside bounded figure range".into());
            }
            if !ids.insert(panel.id) || !materials.contains_key(&panel.material) {
                return Err("duplicate object ID or missing material".into());
            }
            panels.push(panel);
        }
        if parts.is_empty() {
            return Err("at least one body part required".into());
        }
        Ok(Self {
            data: Arc::new(Data {
                materials,
                parts,
                panels,
            }),
        })
    }
    /// Callers supply finite simulation time; invalid values freeze at rest.
    pub fn snapshot(&self, time: f32) -> FigureSnapshot {
        let time = if time.is_finite() { time } else { 0. };
        let mut parts = self.data.parts.clone();
        for p in &mut parts {
            p.center += p.motion * ((time as f64 * p.speed as f64).sin() as f32);
        }
        FigureSnapshot {
            data: self.data.clone(),
            parts,
            time,
        }
    }
}
fn nearest(best: &mut Option<Hit>, hit: Option<Hit>) {
    if let Some(h) = hit {
        if h.distance.is_finite()
            && h.distance > 0.0001
            && h.position.is_finite()
            && h.normal.is_finite()
            && h.local.is_finite()
            && best.is_none_or(|b| h.distance < b.distance)
        {
            *best = Some(h);
        }
    }
}
fn interval(ray: Ray, min: Vec3, max: Vec3) -> Option<(f32, f32)> {
    let (mut near, mut far) = (0.0001_f32, f32::INFINITY);
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
        }
    }
    (far >= near && far.is_finite()).then_some((near, far))
}
impl Panel {
    fn width(&self, v: f32) -> f32 {
        self.bottom + (self.top - self.bottom) * v
    }
    fn depth(&self, x: f32, y: f32, time: f32) -> f32 {
        let v = y / self.height;
        let u = x / self.width(v);
        let t = (time as f64 * self.speed as f64).rem_euclid(std::f64::consts::TAU) as f32;
        self.slope * (1. - v)
            + self.amplitude * (u * std::f32::consts::PI * self.folds + self.phase + v * 0.4).sin()
            + self.sway * (1. - v).powi(2) * (t + u * 1.3 + v * 2.).sin()
    }
    fn local_bounds(&self) -> (Vec3, Vec3) {
        let w = self.bottom.max(self.top);
        let d = self.slope.abs() + self.amplitude.abs() + self.sway.abs() + 0.002;
        (Vec3::new(-w, 0., -d), Vec3::new(w, self.height, d))
    }
    fn trace(&self, ray: Ray, time: f32) -> Option<Hit> {
        let inv = self.rotation.conjugate();
        let local = Ray {
            origin: inv * (ray.origin - self.origin),
            dir: inv * ray.dir,
        };
        let (min, max) = self.local_bounds();
        let (near, far) = interval(local, min, max)?;
        let field = |t: f32| {
            let p = local.origin + local.dir * t;
            p.z - self.depth(p.x, p.y, time)
        };
        let mut left = near;
        let mut fl = field(left);
        for step in 1..=32 {
            let right = near + (far - near) * step as f32 / 32.;
            let fr = field(right);
            if fl * fr <= 0. {
                let (mut a, mut b, mut fa) = (left, right, fl);
                for _ in 0..8 {
                    let m = (a + b) * 0.5;
                    let fm = field(m);
                    if fa * fm <= 0. {
                        b = m;
                    } else {
                        a = m;
                        fa = fm;
                    }
                }
                let distance = (a + b) * 0.5;
                let p = local.origin + local.dir * distance;
                let v = p.y / self.height;
                let u = p.x / self.width(v);
                if u.abs() <= 1. && (0.0..=1.).contains(&v) {
                    let h = 0.0005;
                    let dx = (self.depth(p.x + h, p.y, time) - self.depth(p.x - h, p.y, time))
                        / (2. * h);
                    let dy = (self.depth(p.x, p.y + h, time) - self.depth(p.x, p.y - h, time))
                        / (2. * h);
                    let mut normal = (self.rotation * Vec3::new(-dx, -dy, 1.)).normalize();
                    if normal.dot(ray.dir) > 0. {
                        normal = -normal;
                    }
                    return Some(Hit {
                        distance,
                        position: ray.origin + ray.dir * distance,
                        normal,
                        material: self.material,
                        object_id: self.id,
                        local: Vec3::new(u, v * 2. - 1., 0.),
                    });
                }
            }
            left = right;
            fl = fr;
        }
        None
    }
}
impl FigureSnapshot {
    pub fn bounds(&self) -> (Vec3, Vec3) {
        let (mut min, mut max) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
        for p in &self.parts {
            let e = (p.rotation * Vec3::X).abs() * p.radii.x
                + (p.rotation * Vec3::Y).abs() * p.radii.y
                + (p.rotation * Vec3::Z).abs() * p.radii.z;
            min = min.min(p.center - e);
            max = max.max(p.center + e);
        }
        for p in &self.data.panels {
            let (a, b) = p.local_bounds();
            for x in [a.x, b.x] {
                for y in [a.y, b.y] {
                    for z in [a.z, b.z] {
                        let q = p.origin + p.rotation * Vec3::new(x, y, z);
                        min = min.min(q);
                        max = max.max(q);
                    }
                }
            }
        }
        (min, max)
    }
    pub fn trace(&self, ray: Ray) -> Option<Hit> {
        if !ray.origin.is_finite() || !ray.dir.is_finite() || ray.dir.length_squared() < 1e-12 {
            return None;
        }
        let mut best = None;
        for p in &self.parts {
            let inverse = p.rotation.conjugate();
            let local = Ray {
                origin: inverse * (ray.origin - p.center),
                dir: inverse * ray.dir,
            };
            if let Some((distance, n)) = intersect_ellipsoid(local, Vec3::ZERO, p.radii) {
                let position = ray.origin + ray.dir * distance;
                nearest(
                    &mut best,
                    Some(Hit {
                        distance,
                        position,
                        normal: (p.rotation * n).normalize(),
                        material: p.material,
                        object_id: p.id,
                        local: (inverse * (position - p.center)) / p.radii,
                    }),
                );
            }
        }
        for p in &self.data.panels {
            nearest(&mut best, p.trace(ray, self.time));
        }
        best
    }
    pub fn pigment(&self, material: u32) -> Vec3 {
        self.data
            .materials
            .get(&material)
            .map_or(Vec3::splat(0.5), |m| m.color)
    }
    pub fn material_sample(&self, hit: Hit, _footprint: f32) -> Option<MaterialSample> {
        let m = self.data.materials.get(&hit.material)?;
        Some(MaterialSample {
            albedo: m.color,
            normal: hit.normal,
            roughness: m.roughness,
            f0: 0.04,
            occlusion: 1.,
        })
    }
    fn region(&self, hit: Hit, footprint: f32) -> (u32, u32) {
        let Some(m) = self.data.materials.get(&hit.material) else {
            return (0, 2);
        };
        let width = if footprint.is_finite() {
            footprint.max(0.)
        } else {
            f32::INFINITY
        };
        if !m.structure || width > m.scale {
            return (0, 2);
        }
        if width > m.scale * 0.4 {
            return (0, 1);
        }
        let r = if hit.local.y.abs() > 0.88 {
            1
        } else if hit.local.x.abs() > 0.86 {
            2
        } else {
            0
        };
        (r, 0)
    }
    pub fn glyph_sample_key(&self, hit: Hit, footprint: f32) -> u32 {
        let (r, l) = self.region(hit, footprint);
        1 + r + (l << 8)
    }
    pub fn glyph_sample(&self, hit: Hit, illumination: f32, footprint: f32) -> u8 {
        match self.region(hit, footprint).0 {
            1 => b'=',
            2 => b'|',
            _ => {
                if illumination > 0.12 {
                    b'o'
                } else if illumination > 0.035 {
                    b':'
                } else {
                    b'.'
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const WARDEN: &str = include_str!("../assets/figures/warden.json");
    const COURIER: &str = include_str!("../assets/figures/courier.json");
    #[test]
    fn authored_variants_have_different_geometry() {
        let a = FigureDefinition::from_json(WARDEN).unwrap().snapshot(0.);
        let b = FigureDefinition::from_json(COURIER).unwrap().snapshot(0.);
        assert_ne!(a.bounds(), b.bounds());
    }
    #[test]
    fn rejects_missing_material_duplicate_identity_and_bad_size() {
        for mutation in 0..3 {
            let mut v: Value = serde_json::from_str(WARDEN).unwrap();
            match mutation {
                0 => v["parts"][0]["material"] = 999.into(),
                1 => v["parts"][1]["id"] = v["parts"][0]["id"].clone(),
                _ => v["parts"][0]["radii"] = serde_json::json!([0., 0.1, 0.1]),
            };
            assert!(FigureDefinition::from_json(&v.to_string()).is_err());
        }
    }
    #[test]
    fn snapshots_are_independent_and_surface_keys_track_lod() {
        let d = FigureDefinition::from_json(WARDEN).unwrap();
        let a = d.snapshot(0.);
        let _b = d.snapshot(1.);
        let ray = Ray {
            origin: Vec3::new(0., 1.2, 3.),
            dir: -Vec3::Z,
        };
        let h = a.trace(ray).unwrap();
        assert_eq!(
            h.distance.to_bits(),
            d.snapshot(0.).trace(ray).unwrap().distance.to_bits()
        );
        assert_ne!(a.glyph_sample_key(h, 0.), a.glyph_sample_key(h, 1.));
    }
    #[test]
    fn panel_coordinates_follow_deformation_on_both_faces() {
        let d = FigureDefinition::from_json(WARDEN).unwrap();
        let p = &d.data.panels[0];
        for t in [0., 0.7, 1.8] {
            let y = p.height * 0.5;
            let x = p.width(0.5) * 0.25;
            let q = Vec3::new(x, y, p.depth(x, y, t));
            for side in [-1., 1.] {
                let origin = p.origin + p.rotation * (q + Vec3::Z * side);
                let dir = p.rotation * (-Vec3::Z * side);
                let h = p.trace(Ray { origin, dir }, t).unwrap();
                assert!((h.local.x - 0.25).abs() < 0.002);
                assert!(h.local.y.abs() < 0.002);
                assert!(h.normal.dot(dir) < 0.);
            }
        }
    }
}
