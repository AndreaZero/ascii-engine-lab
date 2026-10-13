//! Version 1 content: bounded analytic objects and object-attached glyph patterns.
//! Distances are world units; rotation, phase and yaw amplitudes are radians;
//! motion speed is radians/second. Ellipsoid half_extents are its three radii.
//! A snapshot requires finite time and owns immutable prepared transforms.
use ascii_render::core::{intersect_box, intersect_ellipsoid, Hit, Ray, Scene, Stage};
use glam::{Quat, Vec3};
use serde_json::{Map, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::Read,
    path::Path,
    sync::Arc,
};

const MAX_JSON_BYTES: usize = 1024 * 1024;
const MAX_OBJECTS: usize = 128;
const MAX_MATERIALS: usize = 32;

#[derive(Clone, Copy)]
enum PatternKind {
    Plain,
    Stone,
    Wood,
    Cloth,
}

#[derive(Clone)]
struct Material {
    pigment: Vec3,
    kind: PatternKind,
    scale: f32,
    palette: Vec<u8>,
}
impl Material {
    // The index describes authored structure, never the current brightness.
    fn region(&self, local: Vec3) -> usize {
        let p = local * self.scale;
        match self.kind {
            PatternKind::Plain => 0,
            PatternKind::Stone => {
                let course = p.y.floor();
                let stagger = course.rem_euclid(2.0) * 0.5;
                if p.y.rem_euclid(1.0) < 0.10 {
                    0
                } else if (p.x + stagger).rem_euclid(1.0) < 0.09 {
                    1
                } else if (p.x + p.z * 0.35).rem_euclid(1.0) < 0.42 {
                    2
                } else {
                    3
                }
            }
            PatternKind::Wood => {
                let grain =
                    (p.x + (p.y * 0.65).sin() * 0.17 + (p.z * 0.8).sin() * 0.12).rem_euclid(1.0);
                if grain < 0.13 {
                    0
                } else if grain < 0.25 {
                    1
                } else if grain < 0.65 {
                    2
                } else {
                    3
                }
            }
            PatternKind::Cloth => {
                let panel = (p.x + (p.y * 0.4).sin() * 0.14).rem_euclid(1.0);
                if panel < 0.075 {
                    0
                } else if panel < 0.34 {
                    1
                } else if panel < 0.68 {
                    2
                } else {
                    3
                }
            }
        }
    }

    fn glyph(&self, local: Vec3) -> u8 {
        self.palette[self.region(local) % self.palette.len()]
    }
}

#[derive(Clone, Copy)]
enum Shape {
    Box,
    Ellipsoid,
}

#[derive(Clone, Copy, Default)]
struct Translation {
    amplitude: Vec3,
    speed: f32,
    phase: f32,
}
impl Translation {
    fn offset(self, time: f32) -> Vec3 {
        // Finite f32 time can be very large. Evaluate phase in f64 to avoid
        // overflowing speed*time before sin; output remains bounded by amplitude.
        let phase = self.speed as f64 * time as f64 + self.phase as f64;
        self.amplitude * phase.sin() as f32
    }
}

#[derive(Clone, Copy, Default)]
struct Yaw {
    amplitude: f32,
    speed: f32,
    phase: f32,
}
impl Yaw {
    fn offset(self, time: f32) -> f32 {
        let phase = self.speed as f64 * time as f64 + self.phase as f64;
        self.amplitude * phase.sin() as f32
    }
}

#[derive(Clone)]
struct Object {
    id: u32,
    material: u32,
    shape: Shape,
    position: Vec3,
    half_extents: Vec3,
    rotation_y: f32,
    translation: Translation,
    yaw: Yaw,
}

struct DocumentData {
    source: Value,
    name: String,
    materials: BTreeMap<u32, Material>,
    objects: Vec<Object>,
    light_position: Vec3,
    light_translation: Translation,
}

/// Validated, shareable content, independent of render/window state.
#[derive(Clone)]
pub struct SceneDocument {
    data: Arc<DocumentData>,
}
impl SceneDocument {
    pub fn from_json(text: &str) -> Result<Self, String> {
        if text.len() > MAX_JSON_BYTES {
            return Err("$: JSON exceeds 1048576 bytes".into());
        }
        let value: Value =
            serde_json::from_str(text).map_err(|e| format!("$: invalid JSON: {e}"))?;
        reject_duplicate_keys(text)?;
        let root = fields(
            &value,
            "$",
            &["version", "name", "materials", "objects", "light"],
        )?;
        let version = required(root, "version", "$")?;
        if version.as_u64() != Some(1) {
            return Err("$.version: expected supported integer version 1".into());
        }
        let document_name = name(required(root, "name", "$")?, "$.name")?;
        let material_values = array(
            required(root, "materials", "$")?,
            "$.materials",
            MAX_MATERIALS,
        )?;
        let mut materials = BTreeMap::new();
        for (i, value) in material_values.iter().enumerate() {
            let path = format!("$.materials[{i}]");
            let map = fields(value, &path, &["id", "name", "pigment", "pattern"])?;
            let id = id(required(map, "id", &path)?, &format!("{path}.id"), 32)?;
            name(required(map, "name", &path)?, &format!("{path}.name"))?;
            let pigment = vector(
                required(map, "pigment", &path)?,
                &format!("{path}.pigment"),
                0.0,
                1.0,
            )?;
            let pattern_path = format!("{path}.pattern");
            let pattern = fields(
                required(map, "pattern", &path)?,
                &pattern_path,
                &["kind", "scale", "palette"],
            )?;
            let kind = match string(
                required(pattern, "kind", &pattern_path)?,
                &format!("{pattern_path}.kind"),
            )? {
                "plain" => PatternKind::Plain,
                "stone" => PatternKind::Stone,
                "wood" => PatternKind::Wood,
                "cloth" => PatternKind::Cloth,
                _ => {
                    return Err(format!(
                        "{pattern_path}.kind: expected plain|stone|wood|cloth"
                    ))
                }
            };
            let scale = number(
                required(pattern, "scale", &pattern_path)?,
                &format!("{pattern_path}.scale"),
                0.0001,
                64.0,
            )?;
            let palette = string(
                required(pattern, "palette", &pattern_path)?,
                &format!("{pattern_path}.palette"),
            )?
            .as_bytes()
            .to_vec();
            if palette.is_empty()
                || palette.len() > 16
                || !palette.iter().all(|b| (32..=126).contains(b))
            {
                return Err(format!(
                    "{pattern_path}.palette: expected 1..16 printable ASCII bytes (32..126)"
                ));
            }
            if materials
                .insert(
                    id,
                    Material {
                        pigment,
                        kind,
                        scale,
                        palette,
                    },
                )
                .is_some()
            {
                return Err(format!("{path}.id: duplicate material id {id}"));
            }
        }

        let object_values = array(required(root, "objects", "$")?, "$.objects", MAX_OBJECTS)?;
        let mut objects = Vec::with_capacity(object_values.len());
        let mut object_ids = BTreeSet::new();
        for (i, value) in object_values.iter().enumerate() {
            let path = format!("$.objects[{i}]");
            let map = fields(
                value,
                &path,
                &[
                    "id",
                    "name",
                    "shape",
                    "material",
                    "position",
                    "half_extents",
                    "rotation_y",
                    "motion",
                ],
            )?;
            let id = id(required(map, "id", &path)?, &format!("{path}.id"), 1)?;
            if !object_ids.insert(id) {
                return Err(format!("{path}.id: duplicate object id {id}"));
            }
            name(required(map, "name", &path)?, &format!("{path}.name"))?;
            let material = id_field(map, "material", &path, 32)?;
            if !materials.contains_key(&material) {
                return Err(format!("{path}.material: unknown material id {material}"));
            }
            let shape = match string(required(map, "shape", &path)?, &format!("{path}.shape"))? {
                "box" => Shape::Box,
                "ellipsoid" => Shape::Ellipsoid,
                _ => return Err(format!("{path}.shape: expected box|ellipsoid")),
            };
            let position = vector(
                required(map, "position", &path)?,
                &format!("{path}.position"),
                -10000.0,
                10000.0,
            )?;
            let half_extents = vector(
                required(map, "half_extents", &path)?,
                &format!("{path}.half_extents"),
                0.0001,
                10000.0,
            )?;
            let rotation_y = optional_number(map, "rotation_y", &path, 0.0, -1024.0, 1024.0)?;
            let (translation, yaw) = if let Some(motion) = map.get("motion") {
                parse_motion(motion, &format!("{path}.motion"), true)?
            } else {
                (Translation::default(), Yaw::default())
            };
            objects.push(Object {
                id,
                material,
                shape,
                position,
                half_extents,
                rotation_y,
                translation,
                yaw,
            });
        }

        let light = fields(
            required(root, "light", "$")?,
            "$.light",
            &["position", "motion"],
        )?;
        let light_position = vector(
            required(light, "position", "$.light")?,
            "$.light.position",
            -10000.0,
            10000.0,
        )?;
        let light_translation = if let Some(motion) = light.get("motion") {
            parse_motion(motion, "$.light.motion", false)?.0
        } else {
            Translation::default()
        };
        Ok(Self {
            data: Arc::new(DocumentData {
                source: value,
                name: document_name,
                materials,
                objects,
                light_position,
                light_translation,
            }),
        })
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        let file =
            File::open(path).map_err(|e| format!("{}: cannot open scene: {e}", path.display()))?;
        let mut bytes = Vec::new();
        file.take(MAX_JSON_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| format!("{}: cannot read scene: {e}", path.display()))?;
        if bytes.len() > MAX_JSON_BYTES {
            return Err("$: JSON exceeds 1048576 bytes".into());
        }
        let text = String::from_utf8(bytes).map_err(|e| format!("$: scene is not UTF-8: {e}"))?;
        Self::from_json(&text)
    }

    pub fn name(&self) -> &str {
        &self.data.name
    }
    /// The exact validated JSON value used to construct this document.
    pub fn source(&self) -> &Value {
        &self.data.source
    }
    pub fn object_count(&self) -> usize {
        self.data.objects.len()
    }

    /// `time` must be finite. Pose/light preparation occurs once, before queries.
    /// Scene calls using a different finite time rebuild a temporary snapshot to
    /// preserve the renderer's time contract; normal clients reuse the same tick.
    pub fn snapshot(&self, time: f32) -> Snapshot {
        Snapshot::new(self.data.clone(), time)
    }
}

#[derive(Clone)]
struct PreparedObject {
    id: u32,
    material: u32,
    shape: Shape,
    position: Vec3,
    half_extents: Vec3,
    rotation: Quat,
}
impl PreparedObject {
    fn trace(&self, ray: Ray) -> Option<Hit> {
        let inverse = self.rotation.conjugate();
        let local_ray = Ray {
            origin: inverse * (ray.origin - self.position),
            dir: inverse * ray.dir,
        };
        let (distance, normal) = match self.shape {
            Shape::Box => intersect_box(local_ray, -self.half_extents, self.half_extents),
            Shape::Ellipsoid => intersect_ellipsoid(local_ray, Vec3::ZERO, self.half_extents),
        }?;
        let local = local_ray.origin + local_ray.dir * distance;
        let position = ray.origin + ray.dir * distance;
        let normal = (self.rotation * normal).normalize();
        if !distance.is_finite()
            || !position.is_finite()
            || !local.is_finite()
            || !normal.is_finite()
        {
            return None;
        }
        Some(Hit {
            distance,
            position,
            normal,
            material: self.material,
            object_id: self.id,
            local,
        })
    }
}

/// A client-owned immutable producer; all renderer queries share this pose.
/// Every material ID is >=32, so no object opts into WATER/EMISSIVE semantics.
#[derive(Clone)]
pub struct Snapshot {
    data: Arc<DocumentData>,
    time: f32,
    objects: Vec<PreparedObject>,
    light: Vec3,
}
impl Snapshot {
    fn new(data: Arc<DocumentData>, time: f32) -> Self {
        assert!(
            time.is_finite(),
            "material-room snapshot time must be finite"
        );
        let objects = data
            .objects
            .iter()
            .map(|object| PreparedObject {
                id: object.id,
                material: object.material,
                shape: object.shape,
                position: object.position + object.translation.offset(time),
                half_extents: object.half_extents,
                rotation: Quat::from_rotation_y(object.rotation_y + object.yaw.offset(time)),
            })
            .collect();
        let light = data.light_position + data.light_translation.offset(time);
        Self {
            data,
            time,
            objects,
            light,
        }
    }

    fn trace_prepared(&self, ray: Ray) -> Option<Hit> {
        if !ray.origin.is_finite()
            || !ray.dir.is_finite()
            || !ray.dir.length_squared().is_finite()
            || ray.dir.length_squared() <= 0.0
        {
            return None;
        }
        let mut nearest: Option<Hit> = None;
        for object in &self.objects {
            if let Some(hit) = object.trace(ray) {
                if nearest
                    .as_ref()
                    .is_none_or(|old| hit.distance < old.distance)
                {
                    nearest = Some(hit);
                }
            }
        }
        nearest
    }
}
impl Scene for Snapshot {
    fn trace(&self, ray: Ray, time: f32, _stage: Stage, _include_water: bool) -> Option<Hit> {
        if time.to_bits() == self.time.to_bits() {
            self.trace_prepared(ray)
        } else {
            Self::new(self.data.clone(), time).trace_prepared(ray)
        }
    }
    fn light_position(&self, time: f32) -> Vec3 {
        assert!(time.is_finite(), "material-room query time must be finite");
        if time.to_bits() == self.time.to_bits() {
            self.light
        } else {
            self.data.light_position + self.data.light_translation.offset(time)
        }
    }
    fn pigment(&self, material: u32) -> Vec3 {
        self.data
            .materials
            .get(&material)
            .map_or(Vec3::ZERO, |m| m.pigment)
    }
    fn glyph(&self, material: u32, local: Vec3, _normal: Vec3, _illumination: f32) -> u8 {
        // Lighting remains the renderer's responsibility. Structural choices
        // stay attached to the moving object and are independent of brightness.
        self.data
            .materials
            .get(&material)
            .map_or(b' ', |m| m.glyph(local))
    }
    fn glyph_history_key(&self, material: u32, local: Vec3, _normal: Vec3) -> u32 {
        self.data
            .materials
            .get(&material)
            .map_or(0, |m| m.region(local) as u32 + 1)
    }
}

fn fields<'a>(
    value: &'a Value,
    path: &str,
    allowed: &[&str],
) -> Result<&'a Map<String, Value>, String> {
    let map = value
        .as_object()
        .ok_or_else(|| format!("{path}: expected object"))?;
    for key in map.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(format!("{path}.{key}: unknown field"));
        }
    }
    Ok(map)
}
fn required<'a>(map: &'a Map<String, Value>, key: &str, path: &str) -> Result<&'a Value, String> {
    map.get(key)
        .ok_or_else(|| format!("{path}.{key}: required field missing"))
}
fn array<'a>(value: &'a Value, path: &str, max: usize) -> Result<&'a [Value], String> {
    let values = value
        .as_array()
        .ok_or_else(|| format!("{path}: expected array"))?;
    if values.len() > max {
        return Err(format!("{path}: exceeds {max} entries"));
    }
    Ok(values)
}
fn string<'a>(value: &'a Value, path: &str) -> Result<&'a str, String> {
    value
        .as_str()
        .ok_or_else(|| format!("{path}: expected string"))
}
fn name(value: &Value, path: &str) -> Result<String, String> {
    let text = string(value, path)?;
    if text.trim().is_empty() || text.chars().count() > 80 || text.chars().any(char::is_control) {
        return Err(format!(
            "{path}: expected nonempty name of at most 80 characters, without control characters"
        ));
    }
    Ok(text.to_owned())
}
fn id(value: &Value, path: &str, min: u32) -> Result<u32, String> {
    let id = value
        .as_u64()
        .filter(|v| *v >= min as u64 && *v <= u32::MAX as u64)
        .ok_or_else(|| format!("{path}: expected integer id in {min}..={}", u32::MAX))?;
    Ok(id as u32)
}
fn id_field(map: &Map<String, Value>, key: &str, path: &str, min: u32) -> Result<u32, String> {
    id(required(map, key, path)?, &format!("{path}.{key}"), min)
}
fn number(value: &Value, path: &str, min: f32, max: f32) -> Result<f32, String> {
    let value = value
        .as_f64()
        .ok_or_else(|| format!("{path}: expected finite number"))?;
    let number = value as f32;
    if !number.is_finite() || value < min as f64 || value > max as f64 {
        return Err(format!("{path}: expected finite f32 in [{min}, {max}]"));
    }
    Ok(number)
}
fn optional_number(
    map: &Map<String, Value>,
    key: &str,
    path: &str,
    default: f32,
    min: f32,
    max: f32,
) -> Result<f32, String> {
    map.get(key).map_or(Ok(default), |value| {
        number(value, &format!("{path}.{key}"), min, max)
    })
}
fn vector(value: &Value, path: &str, min: f32, max: f32) -> Result<Vec3, String> {
    let values = array(value, path, 3)?;
    if values.len() != 3 {
        return Err(format!("{path}: expected exactly 3 components"));
    }
    Ok(Vec3::new(
        number(&values[0], &format!("{path}[0]"), min, max)?,
        number(&values[1], &format!("{path}[1]"), min, max)?,
        number(&values[2], &format!("{path}[2]"), min, max)?,
    ))
}
fn parse_motion(value: &Value, path: &str, allow_yaw: bool) -> Result<(Translation, Yaw), String> {
    let allowed = if allow_yaw {
        &["translation", "yaw"][..]
    } else {
        &["translation"][..]
    };
    let map = fields(value, path, allowed)?;
    if map.is_empty() {
        return Err(format!("{path}: expected translation or yaw motion"));
    }
    let translation = if let Some(value) = map.get("translation") {
        let p = format!("{path}.translation");
        let m = fields(value, &p, &["amplitude", "speed", "phase"])?;
        Translation {
            amplitude: vector(
                required(m, "amplitude", &p)?,
                &format!("{p}.amplitude"),
                -1000.0,
                1000.0,
            )?,
            speed: number(
                required(m, "speed", &p)?,
                &format!("{p}.speed"),
                -64.0,
                64.0,
            )?,
            phase: optional_number(m, "phase", &p, 0.0, -10000.0, 10000.0)?,
        }
    } else {
        Translation::default()
    };
    let yaw = if let Some(value) = map.get("yaw") {
        let p = format!("{path}.yaw");
        let m = fields(value, &p, &["amplitude", "speed", "phase"])?;
        Yaw {
            amplitude: number(
                required(m, "amplitude", &p)?,
                &format!("{p}.amplitude"),
                -1024.0,
                1024.0,
            )?,
            speed: number(
                required(m, "speed", &p)?,
                &format!("{p}.speed"),
                -64.0,
                64.0,
            )?,
            phase: optional_number(m, "phase", &p, 0.0, -10000.0, 10000.0)?,
        }
    } else {
        Yaw::default()
    };
    Ok((translation, yaw))
}

// serde_json::Value overwrites repeated object keys. A second bounded lexical
// walk rejects them (including escaped aliases) without adding a serde derive
// dependency. Grammar/escapes have already been validated by serde_json above.
fn reject_duplicate_keys(text: &str) -> Result<(), String> {
    fn whitespace(bytes: &[u8], at: &mut usize) {
        while bytes.get(*at).is_some_and(u8::is_ascii_whitespace) {
            *at += 1;
        }
    }
    fn quoted_end(bytes: &[u8], at: &mut usize) {
        *at += 1;
        while *at < bytes.len() {
            match bytes[*at] {
                b'\\' => *at += 2,
                b'"' => {
                    *at += 1;
                    return;
                }
                _ => *at += 1,
            }
        }
    }
    fn walk(text: &str, at: &mut usize, path: &str) -> Result<(), String> {
        let bytes = text.as_bytes();
        whitespace(bytes, at);
        match bytes[*at] {
            b'{' => {
                *at += 1;
                whitespace(bytes, at);
                let mut keys = BTreeSet::new();
                while bytes[*at] != b'}' {
                    let start = *at;
                    quoted_end(bytes, at);
                    let key: String = serde_json::from_str(&text[start..*at])
                        .map_err(|e| format!("{path}: invalid key: {e}"))?;
                    let child = format!("{path}.{key}");
                    if !keys.insert(key) {
                        return Err(format!("{child}: duplicate field"));
                    }
                    whitespace(bytes, at);
                    *at += 1; // colon
                    walk(text, at, &child)?;
                    whitespace(bytes, at);
                    if bytes[*at] == b',' {
                        *at += 1;
                        whitespace(bytes, at);
                    } else {
                        break;
                    }
                }
                *at += 1;
            }
            b'[' => {
                *at += 1;
                whitespace(bytes, at);
                let mut index = 0;
                while bytes[*at] != b']' {
                    walk(text, at, &format!("{path}[{index}]"))?;
                    index += 1;
                    whitespace(bytes, at);
                    if bytes[*at] == b',' {
                        *at += 1;
                        whitespace(bytes, at);
                    } else {
                        break;
                    }
                }
                *at += 1;
            }
            b'"' => quoted_end(bytes, at),
            _ => {
                while bytes
                    .get(*at)
                    .is_some_and(|b| !b.is_ascii_whitespace() && !b",]}".contains(b))
                {
                    *at += 1;
                }
            }
        }
        Ok(())
    }
    walk(text, &mut 0, "$")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fixture() -> Value {
        json!({"version":1,"name":"Query fixture","materials":[
            {"id":32,"name":"Wood","pigment":[0.4,0.3,0.2],"pattern":{"kind":"wood","scale":2.0,"palette":"|:-."}}
        ],"objects":[
            {"id":1,"name":"Block","shape":"box","material":32,"position":[0,1,0],"half_extents":[0.5,1,0.25]}
        ],"light":{"position":[0,4,2]}})
    }
    fn load_value(value: &Value) -> Result<SceneDocument, String> {
        SceneDocument::from_json(&value.to_string())
    }
    fn rejected(value: Value, path: &str) {
        let error = match load_value(&value) {
            Ok(_) => panic!("expected rejection at {path}"),
            Err(error) => error,
        };
        assert!(error.contains(path), "{error}, expected path {path}");
    }
    fn bits(hit: Option<Hit>) -> Option<[u32; 12]> {
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
    fn rejects_schema_errors_with_field_paths() {
        let mut v = fixture();
        v["version"] = json!(2);
        rejected(v, "$.version");
        let mut v = fixture();
        v["surprise"] = json!(true);
        rejected(v, "$.surprise");
        let mut v = fixture();
        v["objects"][0]["material"] = json!(99);
        rejected(v, "$.objects[0].material");
        let mut v = fixture();
        v["objects"][0]["shape"] = json!("mesh");
        rejected(v, "$.objects[0].shape");
        let mut v = fixture();
        v["materials"][0]["pattern"]["kind"] = json!("rgb");
        rejected(v, "$.materials[0].pattern.kind");
        let mut v = fixture();
        v["materials"][0]["id"] = json!(8);
        rejected(v, "$.materials[0].id");
        let mut v = fixture();
        v["materials"][0]["pigment"][0] = json!(1.1);
        rejected(v, "$.materials[0].pigment[0]");
        let mut v = fixture();
        v["materials"][0]["pigment"][0] = json!(1.0000000001);
        rejected(v, "$.materials[0].pigment[0]");
        let mut v = fixture();
        v["objects"][0]["half_extents"][1] = json!(0);
        rejected(v, "$.objects[0].half_extents[1]");
        let mut v = fixture();
        v["objects"][0]["position"][0] = json!(1e100);
        rejected(v, "$.objects[0].position[0]");
        let mut v = fixture();
        v["objects"][0]["motion"] = json!({"yaw":{"amplitude":1,"speed":1,"unknown":0}});
        rejected(v, "$.objects[0].motion.yaw.unknown");
        for palette in ["", "é", "a\n", "abcdefghijklmnopq"] {
            let mut v = fixture();
            v["materials"][0]["pattern"]["palette"] = json!(palette);
            rejected(v, "$.materials[0].pattern.palette");
        }
    }

    #[test]
    fn rejects_duplicate_ids_keys_and_resource_limits() {
        let mut v = fixture();
        let duplicate = v["objects"][0].clone();
        v["objects"].as_array_mut().unwrap().push(duplicate);
        rejected(v, "$.objects[1].id");
        let mut v = fixture();
        let duplicate = v["materials"][0].clone();
        v["materials"].as_array_mut().unwrap().push(duplicate);
        rejected(v, "$.materials[1].id");
        let mut v = fixture();
        v["objects"] = Value::Array(vec![v["objects"][0].clone(); 129]);
        rejected(v, "$.objects");
        let mut v = fixture();
        v["materials"] = Value::Array(vec![v["materials"][0].clone(); 33]);
        rejected(v, "$.materials");
        assert!(SceneDocument::from_json(&" ".repeat(MAX_JSON_BYTES + 1)).is_err());
        let duplicate = r#"{"version":1,"\u0076ersion":1}"#;
        let error = SceneDocument::from_json(duplicate).err().unwrap();
        assert!(error.contains("$.version: duplicate field"));
        assert!(SceneDocument::from_json("{bad json}").is_err());
    }

    #[test]
    fn static_and_transformed_queries_keep_object_local_coordinates() {
        let mut v = fixture();
        v["objects"][0]["rotation_y"] = json!(std::f32::consts::FRAC_PI_2);
        let document = load_value(&v).unwrap();
        let ray = Ray {
            origin: Vec3::new(2.0, 1.0, 0.0),
            dir: -Vec3::X,
        };
        let a = document.snapshot(0.0);
        let b = document.snapshot(100.0);
        let hit = a.trace(ray, 0.0, Stage::Cloth, true).unwrap();
        assert!((hit.distance - 1.75).abs() < 1e-5);
        assert!(hit.local.abs_diff_eq(Vec3::new(0.0, 0.0, 0.25), 1e-5));
        assert_eq!(
            bits(Some(hit)),
            bits(b.trace(ray, 100.0, Stage::Core, false))
        );
        assert!(a.occluded(ray, 0.0, Stage::Light, 2.0));
        assert!(!a.occluded(ray, 0.0, Stage::Light, hit.distance));
    }

    #[test]
    fn moving_snapshots_are_independent_and_time_fallback_is_consistent() {
        fn send_sync<T: Send + Sync>() {}
        send_sync::<Snapshot>();
        let mut v = fixture();
        v["objects"][0]["motion"] = json!({"translation":{"amplitude":[2,0,0],"speed":1},"yaw":{"amplitude":0.4,"speed":1}});
        v["light"]["motion"] = json!({"translation":{"amplitude":[0,0,1],"speed":1}});
        let document = load_value(&v).unwrap();
        let t = std::f32::consts::FRAC_PI_2;
        let a = document.snapshot(0.0);
        let ray = Ray {
            origin: Vec3::new(0.0, 1.0, 3.0),
            dir: -Vec3::Z,
        };
        let initial = bits(a.trace(ray, 0.0, Stage::Cloth, true));
        assert!(initial.is_some());
        let b = document.snapshot(t);
        assert!(b.trace(ray, t, Stage::Cloth, true).is_none());
        assert_eq!(initial, bits(a.trace(ray, 0.0, Stage::Cloth, true)));
        assert_eq!(
            bits(a.trace(ray, t, Stage::Cloth, true)),
            bits(b.trace(ray, t, Stage::Cloth, true))
        );
        assert_eq!(a.light_position(t), b.light_position(t));
        assert_ne!(a.light_position(0.0), b.light_position(t));
    }

    #[test]
    fn packaged_scene_and_material_queries_are_finite_and_structurally_stable() {
        let document = SceneDocument::from_json(crate::DEFAULT_SCENE_JSON).unwrap();
        assert!(document.object_count() >= 4);
        for time in [0.0, 1.0, f32::MAX] {
            let snapshot = document.snapshot(time);
            assert!(snapshot.light_position(time).is_finite());
            for material in document.data.materials.keys() {
                assert!(snapshot.pigment(*material).is_finite());
                for x in 0..20 {
                    let local = Vec3::new(x as f32 * 0.073, 0.42, 0.17);
                    let dark = snapshot.glyph(*material, local, Vec3::Z, 0.02);
                    assert!((32..=126).contains(&dark));
                    assert_eq!(dark, snapshot.glyph(*material, local, Vec3::Z, 1.0));
                    assert_eq!(
                        snapshot.glyph_history_key(*material, local, Vec3::Z),
                        snapshot.glyph_history_key(*material, local, -Vec3::Z)
                    );
                }
            }
            for x in -5..6 {
                let ray = Ray {
                    origin: Vec3::new(x as f32, 4.0, 5.0),
                    dir: Vec3::new(0.0, -0.5, -1.0),
                };
                if let Some(hit) = snapshot.trace(ray, time, Stage::Cloth, true) {
                    assert!(
                        hit.distance.is_finite()
                            && hit.position.is_finite()
                            && hit.normal.is_finite()
                            && hit.local.is_finite()
                    );
                    assert!(document.data.materials.contains_key(&hit.material));
                }
            }
        }
    }
}
