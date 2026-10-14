//! Versioned content: bounded analytic objects and object-attached glyph patterns.
//! V1 keeps the R6 appearance. V2 adds client cameras, uniform local surfaces and
//! an optional constant authored environment; it is not an image/HDR importer.
//! Distances are world units; rotation, phase and yaw amplitudes are radians;
//! motion speed is radians/second. Ellipsoid half_extents are its three radii.
//! A snapshot requires finite time and owns immutable prepared transforms.
use ascii_render::{
    camera::{Camera, CameraView, Mode},
    core::{
        intersect_box, intersect_box_before, intersect_box_before_counted, intersect_ellipsoid,
        EnvironmentQuery, EnvironmentSample, Hit, MaterialSample, MetallicSample, Ray, Scene,
        Stage,
    },
};
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
    surface: Option<Surface>,
}

#[derive(Clone, Copy)]
struct Surface {
    roughness: f32,
    f0: f32,
    occlusion: f32,
    metallic: Option<MetallicSample>,
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

    // V2 filters the existing pattern vocabulary, not the object's geometry or
    // color. Fine seams need at least two cell widths; broader bands survive
    // one more level, then collapse to a uniform authored palette entry.
    // Both glyph and history key MUST use this same classification, including
    // LOD even when the author repeats characters in the palette.
    fn filtered_region(&self, local: Vec3, footprint: f32) -> (usize, u32) {
        if matches!(self.kind, PatternKind::Plain) {
            return (0, 1);
        }
        let width = if footprint.is_finite() {
            footprint.max(0.0) * self.scale
        } else {
            f32::INFINITY
        };
        let fine_limit = match self.kind {
            PatternKind::Stone => 0.04,
            PatternKind::Wood => 0.05,
            PatternKind::Cloth => 0.03,
            PatternKind::Plain => unreachable!(),
        };
        let (region, lod) = if width <= fine_limit {
            (self.region(local), 0)
        } else if width <= 0.20 {
            let p = local * self.scale;
            let phase = match self.kind {
                PatternKind::Stone => p.x + p.z * 0.35,
                PatternKind::Wood => p.x + (p.y * 0.65).sin() * 0.17 + (p.z * 0.8).sin() * 0.12,
                PatternKind::Cloth => p.x + (p.y * 0.4).sin() * 0.14,
                PatternKind::Plain => unreachable!(),
            };
            (if phase.rem_euclid(1.0) < 0.5 { 2 } else { 3 }, 1)
        } else {
            (3, 2)
        };
        (region, ((lod as u32) << 8) | (region as u32 + 1))
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
    version: u64,
    name: String,
    materials: BTreeMap<u32, Material>,
    objects: Vec<Object>,
    light_position: Vec3,
    light_translation: Translation,
    light_color: Vec3,
    cameras: Option<[CameraView; 3]>,
    environment: Option<EnvironmentSample>,
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
        let root = value.as_object().ok_or("$: expected object")?;
        let version = required(root, "version", "$")?
            .as_u64()
            .filter(|version| matches!(version, 1 | 2))
            .ok_or("$.version: expected supported integer version 1 or 2")?;
        fields(
            &value,
            "$",
            if version == 1 {
                &["version", "name", "materials", "objects", "light"]
            } else {
                &[
                    "version",
                    "name",
                    "materials",
                    "objects",
                    "light",
                    "cameras",
                    "environment",
                ]
            },
        )?;
        let document_name = name(required(root, "name", "$")?, "$.name")?;
        let material_values = array(
            required(root, "materials", "$")?,
            "$.materials",
            MAX_MATERIALS,
        )?;
        let mut materials = BTreeMap::new();
        for (i, value) in material_values.iter().enumerate() {
            let path = format!("$.materials[{i}]");
            let map = fields(
                value,
                &path,
                if version == 1 {
                    &["id", "name", "pigment", "pattern"]
                } else {
                    &["id", "name", "pigment", "pattern", "surface"]
                },
            )?;
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
            let surface = if version == 2 {
                Some(parse_surface(
                    required(map, "surface", &path)?,
                    &format!("{path}.surface"),
                )?)
            } else {
                None
            };
            if materials
                .insert(
                    id,
                    Material {
                        pigment,
                        kind,
                        scale,
                        palette,
                        surface,
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

        // Stable explicit identity also defines equal-distance query priority.
        // V1 retains its authored array order for exact R6 compatibility.
        if version == 2 {
            objects.sort_by_key(|object| object.id);
        }

        let light = fields(
            required(root, "light", "$")?,
            "$.light",
            if version == 1 {
                &["position", "motion"]
            } else {
                &["position", "motion", "color"]
            },
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
        let light_color = light
            .get("color")
            .map_or(Ok(Vec3::new(1.0, 0.58, 0.23)), |value| {
                vector(value, "$.light.color", 0.0, 16.0)
            })?;
        let cameras = if version == 2 {
            Some(parse_cameras(required(root, "cameras", "$")?)?)
        } else {
            None
        };
        let environment = root
            .get("environment")
            .map(|value| parse_environment(value, "$.environment"))
            .transpose()?;
        Ok(Self {
            data: Arc::new(DocumentData {
                source: value,
                version,
                name: document_name,
                materials,
                objects,
                light_position,
                light_translation,
                light_color,
                cameras,
                environment,
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
    pub fn version(&self) -> u64 {
        self.data.version
    }

    /// Fresh client-owned camera state for this document and selected view.
    /// V1 presets reproduce the original Material Room CLI/window camera.
    pub fn camera(&self, mode: Mode) -> Camera {
        if let Some(views) = &self.data.cameras {
            let index = match mode {
                Mode::First => 0,
                Mode::Third => 1,
                Mode::Overhead => 2,
            };
            Camera::with_view(mode, views[index])
        } else {
            let mut camera = Camera::new(mode);
            match mode {
                Mode::First => {
                    camera.offset = Vec3::new(-1.0, 0.0, 1.4);
                    camera.orbit = -0.194;
                }
                Mode::Third => camera.zoom = 0.85,
                Mode::Overhead => {}
            }
            camera
        }
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

    #[inline]
    fn trace_bounded<const COUNT: bool>(
        &self,
        ray: Ray,
        max_distance: f32,
        stats: &mut QueryStats,
    ) -> Option<Hit> {
        let inverse = self.rotation.conjugate();
        let local_ray = Ray {
            origin: inverse * (ray.origin - self.position),
            dir: inverse * ray.dir,
        };
        if COUNT {
            stats.direction_transforms += 1;
        }
        let (distance, normal) = match self.shape {
            Shape::Box => {
                if COUNT {
                    let (hit, axes) = intersect_box_before_counted(
                        local_ray,
                        -self.half_extents,
                        self.half_extents,
                        max_distance,
                    );
                    // Untimed reference work only: count the axes of the same
                    // visited box without a distance bound, not unvisited
                    // shadow objects after an early return.
                    let (_, unbounded_axes) = intersect_box_before_counted(
                        local_ray,
                        -self.half_extents,
                        self.half_extents,
                        f32::INFINITY,
                    );
                    stats.box_axes_examined += usize::from(axes);
                    stats.box_axes_unbounded_visited += usize::from(unbounded_axes);
                    hit
                } else {
                    intersect_box_before(
                        local_ray,
                        -self.half_extents,
                        self.half_extents,
                        max_distance,
                    )
                }
            }
            Shape::Ellipsoid => intersect_ellipsoid(local_ray, Vec3::ZERO, self.half_extents),
        }?;
        if COUNT {
            stats.primitive_hits += 1;
            stats.full_hit_builds += 1;
        }
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
        if COUNT {
            stats.valid_hits += 1;
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

/// Work performed by the explicit bounded-query experiment, outside frame
/// timing. These counters do not describe ordinary `Scene` queries, which
/// retain the original R12 nearest-hit and default occlusion paths.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct QueryStats {
    /// Objects reached in authored query order, before testing their primitive.
    pub objects_considered: usize,
    /// Analytic primitive calls, excluding the counted-only unbounded replay.
    /// There is no broadphase in this producer.
    pub primitive_tests: usize,
    /// Quaternion multiplications of this ray's direction, one per object.
    pub direction_transforms: usize,
    /// Slab axes examined by bounded box calls, including parallel axes.
    pub box_axes_examined: usize,
    /// Slab axes of those same visited boxes replayed with an infinite limit.
    /// This reference replay exists only in counted queries, outside timing.
    pub box_axes_unbounded_visited: usize,
    /// Active solver results of `Some`, including nonfinite Hits rejected later.
    /// Box results already satisfy the strict distance bound; ellipsoids use
    /// the original unbounded solver. These are not R12's raw candidate counts.
    pub primitive_hits: usize,
    /// Attempts to reconstruct a complete Hit and validate its finite fields.
    /// Every primitive result is reconstructed, so this equals primitive_hits.
    pub full_hit_builds: usize,
    /// Reconstructed Hits accepted by the finite-field checks.
    pub valid_hits: usize,
    /// One when an occlusion query returns at its first valid blocker, else zero.
    pub shadow_early_exits: usize,
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

    /// Explicit untimed bounded nearest-hit experiment, separate from the
    /// ordinary `Scene::trace` path. Counts describe only this experiment.
    /// Stage and water flags do not change this client's authored objects.
    pub fn trace_bounded_counted(
        &self,
        ray: Ray,
        time: f32,
        _stage: Stage,
        _include_water: bool,
    ) -> (Option<Hit>, QueryStats) {
        let mut stats = QueryStats::default();
        let hit = self.trace_bounded_query::<true>(ray, time, &mut stats);
        (hit, stats)
    }

    /// Explicit untimed bounded any-hit experiment, separate from the default
    /// `Scene::occluded` path. Every blocker passes full Hit validation; counts
    /// describe only this experiment, including its early shadow returns.
    pub fn occluded_bounded_counted(
        &self,
        ray: Ray,
        time: f32,
        _stage: Stage,
        max_distance: f32,
    ) -> (bool, QueryStats) {
        let mut stats = QueryStats::default();
        let blocked = self.occluded_bounded_query::<true>(ray, time, max_distance, &mut stats);
        (blocked, stats)
    }

    fn trace_bounded_query<const COUNT: bool>(
        &self,
        ray: Ray,
        time: f32,
        stats: &mut QueryStats,
    ) -> Option<Hit> {
        if time.to_bits() == self.time.to_bits() {
            self.trace_bounded_prepared::<COUNT>(ray, stats)
        } else {
            Self::new(self.data.clone(), time).trace_bounded_prepared::<COUNT>(ray, stats)
        }
    }

    fn trace_bounded_prepared<const COUNT: bool>(
        &self,
        ray: Ray,
        stats: &mut QueryStats,
    ) -> Option<Hit> {
        if !valid_ray(ray) {
            return None;
        }
        let mut nearest: Option<Hit> = None;
        for object in &self.objects {
            if COUNT {
                stats.objects_considered += 1;
                stats.primitive_tests += 1;
            }
            // Only a previously validated Hit may narrow the distance bound.
            let limit = nearest.as_ref().map_or(f32::INFINITY, |hit| hit.distance);
            if let Some(hit) = object.trace_bounded::<COUNT>(ray, limit, stats) {
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

    fn occluded_bounded_query<const COUNT: bool>(
        &self,
        ray: Ray,
        time: f32,
        max_distance: f32,
        stats: &mut QueryStats,
    ) -> bool {
        if time.to_bits() == self.time.to_bits() {
            self.occluded_bounded_prepared::<COUNT>(ray, max_distance, stats)
        } else {
            Self::new(self.data.clone(), time).occluded_bounded_prepared::<COUNT>(
                ray,
                max_distance,
                stats,
            )
        }
    }

    fn occluded_bounded_prepared<const COUNT: bool>(
        &self,
        ray: Ray,
        max_distance: f32,
        stats: &mut QueryStats,
    ) -> bool {
        if !valid_ray(ray) {
            return false;
        }
        // This any-hit path is specific to the validated client format: all
        // materials are >=32, so none is WATER or EMISSIVE. The core's nearest
        // emissive shadow semantics must not be replaced by a generic any-hit.
        for object in &self.objects {
            if COUNT {
                stats.objects_considered += 1;
                stats.primitive_tests += 1;
            }
            if let Some(hit) = object.trace_bounded::<COUNT>(ray, max_distance, stats) {
                if hit.distance < max_distance {
                    if COUNT {
                        stats.shadow_early_exits += 1;
                    }
                    return true;
                }
            }
        }
        false
    }
}

fn valid_ray(ray: Ray) -> bool {
    ray.origin.is_finite()
        && ray.dir.is_finite()
        && ray.dir.length_squared().is_finite()
        && ray.dir.length_squared() > 0.0
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
    fn local_materials(&self) -> bool {
        self.data.version == 2
    }
    fn material_sample(&self, hit: Hit, _footprint: f32) -> Option<MaterialSample> {
        let material = self.data.materials.get(&hit.material)?;
        let surface = material.surface?;
        Some(MaterialSample {
            albedo: material.pigment,
            normal: hit.normal,
            roughness: surface.roughness,
            f0: surface.f0,
            occlusion: surface.occlusion,
        })
    }
    fn metallic_sample(&self, hit: Hit, _footprint: f32) -> Option<MetallicSample> {
        self.data.materials.get(&hit.material)?.surface?.metallic
    }
    fn environment_sample(&self, _query: EnvironmentQuery) -> Option<EnvironmentSample> {
        self.data.environment
    }
    fn light_color(&self) -> Vec3 {
        self.data.light_color
    }
    fn glyph_sample(&self, hit: Hit, illumination: f32, footprint: f32) -> u8 {
        if self.data.version == 1 {
            return self.glyph(hit.material, hit.local, hit.normal, illumination);
        }
        self.data
            .materials
            .get(&hit.material)
            .map_or(b' ', |material| {
                let (region, _) = material.filtered_region(hit.local, footprint);
                material.palette[region % material.palette.len()]
            })
    }
    fn glyph_sample_key(&self, hit: Hit, footprint: f32) -> u32 {
        if self.data.version == 1 {
            return self.glyph_history_key(hit.material, hit.local, hit.normal);
        }
        self.data
            .materials
            .get(&hit.material)
            .map_or(0, |material| {
                material.filtered_region(hit.local, footprint).1
            })
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

fn parse_surface(value: &Value, path: &str) -> Result<Surface, String> {
    let map = fields(value, path, &["roughness", "f0", "occlusion", "metallic"])?;
    let scalar = |key| {
        number(
            required(map, key, path)?,
            &format!("{path}.{key}"),
            0.0,
            1.0,
        )
    };
    let metallic = map
        .get("metallic")
        .map(|value| {
            let path = format!("{path}.metallic");
            let map = fields(value, &path, &["f0", "metalness"])?;
            Ok::<_, String>(MetallicSample {
                f0: vector(required(map, "f0", &path)?, &format!("{path}.f0"), 0.0, 1.0)?,
                metalness: number(
                    required(map, "metalness", &path)?,
                    &format!("{path}.metalness"),
                    0.0,
                    1.0,
                )?,
            })
        })
        .transpose()?;
    Ok(Surface {
        roughness: scalar("roughness")?,
        f0: scalar("f0")?,
        occlusion: scalar("occlusion")?,
        metallic,
    })
}

fn parse_cameras(value: &Value) -> Result<[CameraView; 3], String> {
    let map = fields(value, "$.cameras", &["first", "third", "overhead"])?;
    let parse = |key| {
        let path = format!("$.cameras.{key}");
        let view = fields(
            required(map, key, "$.cameras")?,
            &path,
            &["eye", "target", "fov_y_degrees", "ortho_half_height"],
        )?;
        CameraView::new(
            vector(
                required(view, "eye", &path)?,
                &format!("{path}.eye"),
                -10000.0,
                10000.0,
            )?,
            vector(
                required(view, "target", &path)?,
                &format!("{path}.target"),
                -10000.0,
                10000.0,
            )?,
            number(
                required(view, "fov_y_degrees", &path)?,
                &format!("{path}.fov_y_degrees"),
                1.0,
                179.0,
            )?,
            number(
                required(view, "ortho_half_height", &path)?,
                &format!("{path}.ortho_half_height"),
                0.01,
                10000.0,
            )?,
        )
        .map_err(|error| format!("{path}: {error}"))
    };
    Ok([parse("first")?, parse("third")?, parse("overhead")?])
}

fn parse_environment(value: &Value, path: &str) -> Result<EnvironmentSample, String> {
    let map = fields(
        value,
        path,
        &["diffuse_irradiance", "specular_radiance", "max_distance"],
    )?;
    Ok(EnvironmentSample {
        diffuse_irradiance: vector(
            required(map, "diffuse_irradiance", path)?,
            &format!("{path}.diffuse_irradiance"),
            0.0,
            16.0,
        )?,
        specular_radiance: vector(
            required(map, "specular_radiance", path)?,
            &format!("{path}.specular_radiance"),
            0.0,
            16.0,
        )?,
        max_distance: number(
            required(map, "max_distance", path)?,
            &format!("{path}.max_distance"),
            0.0,
            100.0,
        )?,
    })
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
pub(crate) fn reject_duplicate_keys(text: &str) -> Result<(), String> {
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
    fn fixture_v2() -> Value {
        let mut value = fixture();
        value["version"] = json!(2);
        value["materials"][0]["surface"] = json!({"roughness":0.6,"f0":0.04,"occlusion":0.8});
        let view = json!({"eye":[0,2,5],"target":[0,1,0],"fov_y_degrees":60,"ortho_half_height":4});
        value["cameras"] = json!({"first":view,"third":view,"overhead":view});
        value
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
    fn diagnostic_counts_bounded_box_axes_and_shadow_prefix_work() {
        let mut value = fixture_v2();
        let mut tied = value["objects"][0].clone();
        tied["id"] = json!(2);
        let mut farther = tied.clone();
        farther["id"] = json!(3);
        farther["position"][0] = json!(-2);
        value["objects"]
            .as_array_mut()
            .unwrap()
            .extend([tied, farther]);
        let snapshot = load_value(&value).unwrap().snapshot(0.0);
        let ray = Ray {
            origin: Vec3::new(3.0, 1.0, 0.0),
            dir: -Vec3::X,
        };
        for time in [0.0, 2.0] {
            let (hit, stats) = snapshot.trace_bounded_counted(ray, time, Stage::Light, true);
            assert_eq!(
                bits(hit),
                bits(snapshot.trace(ray, time, Stage::Light, true))
            );
            assert_eq!(hit.unwrap().object_id, 1);
            assert_eq!(
                stats,
                QueryStats {
                    objects_considered: 3,
                    primitive_tests: 3,
                    direction_transforms: 3,
                    box_axes_examined: 5,
                    box_axes_unbounded_visited: 9,
                    primitive_hits: 1,
                    full_hit_builds: 1,
                    valid_hits: 1,
                    shadow_early_exits: 0,
                }
            );
            let (blocked, stats) = snapshot.occluded_bounded_counted(ray, time, Stage::Light, 3.0);
            assert!(blocked);
            assert_eq!(blocked, snapshot.occluded(ray, time, Stage::Light, 3.0));
            assert_eq!(
                stats,
                QueryStats {
                    objects_considered: 1,
                    primitive_tests: 1,
                    direction_transforms: 1,
                    box_axes_examined: 3,
                    box_axes_unbounded_visited: 3,
                    primitive_hits: 1,
                    full_hit_builds: 1,
                    valid_hits: 1,
                    shadow_early_exits: 1,
                }
            );
            for limit in [hit.unwrap().distance, 0.0, f32::NEG_INFINITY, f32::NAN] {
                let (blocked, stats) =
                    snapshot.occluded_bounded_counted(ray, time, Stage::Light, limit);
                assert!(!blocked);
                assert_eq!(blocked, snapshot.occluded(ray, time, Stage::Light, limit));
                assert_eq!(stats.primitive_tests, 3);
                assert_eq!(stats.direction_transforms, 3);
                assert_eq!(stats.box_axes_examined, if limit > 0.0001 { 3 } else { 0 });
                assert_eq!(stats.box_axes_unbounded_visited, 9);
                assert_eq!(stats.primitive_hits, 0);
                assert_eq!(stats.full_hit_builds, 0);
                assert_eq!(stats.valid_hits, 0);
                assert_eq!(stats.shadow_early_exits, 0);
            }
        }
    }

    #[test]
    fn diagnostic_invalid_rays_never_enter_primitive_queries() {
        let snapshot = load_value(&fixture()).unwrap().snapshot(0.0);
        for ray in [
            Ray {
                origin: Vec3::ZERO,
                dir: Vec3::ZERO,
            },
            Ray {
                origin: Vec3::splat(f32::INFINITY),
                dir: Vec3::Z,
            },
            Ray {
                origin: Vec3::ZERO,
                dir: Vec3::splat(f32::NAN),
            },
            Ray {
                origin: Vec3::ZERO,
                dir: Vec3::splat(f32::MAX),
            },
        ] {
            let (hit, stats) = snapshot.trace_bounded_counted(ray, 0.0, Stage::Core, false);
            assert!(hit.is_none());
            assert_eq!(stats, QueryStats::default());
            let (blocked, stats) =
                snapshot.occluded_bounded_counted(ray, 0.0, Stage::Core, f32::INFINITY);
            assert!(!blocked);
            assert_eq!(stats, QueryStats::default());
        }
    }

    #[test]
    fn ellipsoid_queries_keep_original_full_hit_validation_and_no_box_axis_work() {
        let mut value = fixture_v2();
        value["objects"][0]["shape"] = json!("ellipsoid");
        let snapshot = load_value(&value).unwrap().snapshot(0.0);
        let ray = Ray {
            origin: Vec3::new(0.0, 1.0, 3.0),
            dir: -Vec3::Z,
        };
        let (hit, stats) = snapshot.trace_bounded_counted(ray, 0.0, Stage::Light, true);
        assert!(hit.is_some());
        assert_eq!(stats.primitive_tests, 1);
        assert_eq!(stats.direction_transforms, 1);
        assert_eq!(stats.primitive_hits, 1);
        assert_eq!(stats.full_hit_builds, 1);
        assert_eq!(stats.box_axes_examined, 0);
        assert_eq!(stats.box_axes_unbounded_visited, 0);
        for limit in [0.0, hit.unwrap().distance, f32::NAN, f32::INFINITY] {
            let (blocked, stats) = snapshot.occluded_bounded_counted(ray, 0.0, Stage::Light, limit);
            assert_eq!(blocked, hit.unwrap().distance < limit);
            assert_eq!(stats.primitive_hits, 1);
            assert_eq!(stats.full_hit_builds, 1);
            assert_eq!(stats.valid_hits, 1);
            assert_eq!(stats.box_axes_examined, 0);
            assert_eq!(stats.box_axes_unbounded_visited, 0);
        }
    }

    #[test]
    fn rotated_misses_do_not_change_the_later_valid_hit() {
        let mut value = fixture_v2();
        let base = value["objects"][0].clone();
        let mut objects = Vec::new();
        // A, A, B(miss), B(hit), A. All objects except ID 4 miss this ray.
        for (index, angle) in [0.0, 0.0, 0.4, 0.4, 0.0].into_iter().enumerate() {
            let mut object = base.clone();
            object["id"] = json!(index + 1);
            object["rotation_y"] = json!(angle);
            object["position"][0] = json!(if index == 3 { 0 } else { 10 });
            objects.push(object);
        }
        value["objects"] = json!(objects);
        let snapshot = load_value(&value).unwrap().snapshot(0.0);
        value["objects"] = json!([objects[3]]);
        let isolated = load_value(&value).unwrap().snapshot(0.0);
        let ray = Ray {
            origin: Vec3::new(0.0, 1.0, 3.0),
            dir: -Vec3::Z,
        };
        let expected = isolated.trace(ray, 0.0, Stage::Light, true).unwrap();
        let (hit, stats) = snapshot.trace_bounded_counted(ray, 0.0, Stage::Light, true);
        assert_eq!(bits(hit), bits(Some(expected)));
        assert_eq!(expected.object_id, 4);
        assert_eq!(stats.primitive_tests, 5);
        assert_eq!(stats.direction_transforms, 5);
        assert_eq!(stats.primitive_hits, 1);
        let (blocked, stats) = snapshot.occluded_bounded_counted(ray, 0.0, Stage::Light, 4.0);
        assert!(blocked);
        assert_eq!(stats.primitive_tests, 4);
        assert_eq!(stats.direction_transforms, 4);
        let (blocked, stats) =
            snapshot.occluded_bounded_counted(ray, 0.0, Stage::Light, expected.distance);
        assert!(!blocked);
        assert_eq!(stats.primitive_tests, 5);
        assert_eq!(stats.direction_transforms, 5);
        // A new ray must remain independent of the preceding query.
        let other = Ray {
            dir: -Vec3::X,
            ..ray
        };
        let (hit, stats) = snapshot.trace_bounded_counted(other, 0.0, Stage::Light, true);
        assert!(hit.is_none());
        assert_eq!(stats.direction_transforms, 5);
    }

    #[test]
    fn moving_tied_objects_keep_snapshot_and_query_time_parity() {
        let mut value = fixture_v2();
        let mut moving = value["objects"][0].clone();
        moving["id"] = json!(2);
        moving["motion"] = json!({"yaw":{"amplitude":0.5,"speed":1}});
        let mut same_moving = moving.clone();
        same_moving["id"] = json!(3);
        value["objects"]
            .as_array_mut()
            .unwrap()
            .extend([moving, same_moving]);
        let document = load_value(&value).unwrap();
        let initial = document.snapshot(0.0);
        let time = std::f32::consts::FRAC_PI_2;
        let moved = document.snapshot(time);
        let ray = Ray {
            origin: Vec3::new(0.0, 1.0, 3.0),
            dir: -Vec3::Z,
        };
        assert_eq!(
            initial
                .trace_bounded_counted(ray, 0.0, Stage::Light, true)
                .1
                .direction_transforms,
            3
        );
        let (direct, direct_stats) = moved.trace_bounded_counted(ray, time, Stage::Light, true);
        let (fallback, fallback_stats) =
            initial.trace_bounded_counted(ray, time, Stage::Light, true);
        assert_eq!(direct_stats.direction_transforms, 3);
        assert_eq!(direct_stats, fallback_stats);
        assert_eq!(bits(direct), bits(fallback));
        assert_eq!(
            initial
                .trace_bounded_counted(ray, 0.0, Stage::Light, true)
                .1
                .direction_transforms,
            3
        );
    }

    #[test]
    fn strict_ties_preserve_first_hit_across_signed_zero_rotations() {
        let mut value = fixture_v2();
        let positive = value["objects"][0].clone();
        let mut negative = positive.clone();
        negative["id"] = json!(2);
        negative["rotation_y"] = json!(-0.0);
        // -0 + -0 keeps the negative angle when preparing the quaternion.
        negative["motion"] = json!({"yaw":{"amplitude":0,"speed":0,"phase":-1}});
        let mut negative_again = negative.clone();
        negative_again["id"] = json!(3);
        let mut positive_again = positive.clone();
        positive_again["id"] = json!(4);
        value["objects"] = json!([positive, negative, negative_again, positive_again]);
        let snapshot = load_value(&value).unwrap().snapshot(0.0);
        value["objects"] = json!([value["objects"][0]]);
        let isolated = load_value(&value).unwrap().snapshot(0.0);
        let ray = Ray {
            origin: Vec3::new(0.0, 1.0, 3.0),
            dir: -Vec3::Z,
        };
        let (hit, stats) = snapshot.trace_bounded_counted(ray, 0.0, Stage::Light, true);
        assert_eq!(
            bits(hit),
            bits(isolated.trace(ray, 0.0, Stage::Light, true))
        );
        assert_eq!(hit.unwrap().object_id, 1);
        assert_eq!(stats.primitive_tests, 4);
        assert_eq!(stats.direction_transforms, 4);
    }

    #[test]
    fn rejects_schema_errors_with_field_paths() {
        let mut v = fixture();
        v["version"] = json!(3);
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
    fn v1_rejects_new_fields_and_retains_client_camera_presets() {
        let document = load_value(&fixture()).unwrap();
        assert_eq!(document.version(), 1);
        for mode in [Mode::First, Mode::Third, Mode::Overhead] {
            let mut old = Camera::new(mode);
            match mode {
                Mode::First => {
                    old.offset = Vec3::new(-1.0, 0.0, 1.4);
                    old.orbit = -0.194;
                }
                Mode::Third => old.zoom = 0.85,
                Mode::Overhead => {}
            }
            let restored = document.camera(mode);
            assert_eq!(restored.pose(), old.pose());
            for (x, y) in [(0, 0), (7, 13), (30, 18)] {
                let a = restored.ray(x, y, 31, 19, 16.0 / 9.0);
                let b = old.ray(x, y, 31, 19, 16.0 / 9.0);
                assert_eq!(a.origin, b.origin);
                assert_eq!(a.dir, b.dir);
            }
        }
        for key in ["cameras", "environment"] {
            let mut value = fixture();
            value[key] = json!({});
            rejected(value, &format!("$.{key}"));
        }
        let mut value = fixture();
        value["materials"][0]["surface"] = json!({});
        rejected(value, "$.materials[0].surface");
        let mut value = fixture();
        value["light"]["color"] = json!([1, 1, 1]);
        rejected(value, "$.light.color");
    }

    #[test]
    fn v2_rejects_missing_unknown_and_out_of_range_fields() {
        let valid = fixture_v2();
        assert!(load_value(&valid).is_ok());
        for path in [
            "/cameras",
            "/cameras/first",
            "/cameras/third/eye",
            "/cameras/overhead/fov_y_degrees",
            "/materials/0/surface",
        ] {
            let mut value = valid.clone();
            let (parent, key) = path.rsplit_once('/').unwrap();
            value
                .pointer_mut(parent)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove(key);
            assert!(load_value(&value).is_err(), "missing {path}");
        }
        for key in ["roughness", "f0", "occlusion"] {
            for invalid in [
                json!(-0.001),
                json!(1.0000000001),
                Value::Null,
                json!("0.5"),
            ] {
                let mut value = valid.clone();
                value["materials"][0]["surface"][key] = invalid;
                rejected(value, &format!("$.materials[0].surface.{key}"));
            }
        }
        for (pointer, invalid, path) in [
            (
                "/cameras/first/eye/0",
                json!(10000.0001),
                "$.cameras.first.eye[0]",
            ),
            (
                "/cameras/first/target/0",
                json!(-10001),
                "$.cameras.first.target[0]",
            ),
            (
                "/cameras/third/fov_y_degrees",
                json!(179.1),
                "$.cameras.third.fov_y_degrees",
            ),
            (
                "/cameras/third/fov_y_degrees",
                json!(0.99),
                "$.cameras.third.fov_y_degrees",
            ),
            (
                "/cameras/overhead/ortho_half_height",
                json!(0),
                "$.cameras.overhead.ortho_half_height",
            ),
            (
                "/cameras/overhead/ortho_half_height",
                json!(10001),
                "$.cameras.overhead.ortho_half_height",
            ),
        ] {
            let mut value = valid.clone();
            *value.pointer_mut(pointer).unwrap() = invalid;
            rejected(value, path);
        }
        let mut value = valid.clone();
        value["cameras"]["first"]["target"] = value["cameras"]["first"]["eye"].clone();
        rejected(value, "$.cameras.first");
        let mut value = valid.clone();
        value["cameras"]["third"]["target"] = json!([0, 3, 5]);
        rejected(value, "$.cameras.third");
        for (pointer, path) in [
            ("/cameras", "$.cameras.surprise"),
            ("/cameras/first", "$.cameras.first.surprise"),
            ("/materials/0/surface", "$.materials[0].surface.surprise"),
        ] {
            let mut value = valid.clone();
            value.pointer_mut(pointer).unwrap()["surprise"] = json!(0);
            rejected(value, path);
        }
        let mut value = valid;
        value["materials"][0]["surface"]["metallic"] = json!({"f0":[0.5,1.01,0.5],"metalness":1});
        rejected(value, "$.materials[0].surface.metallic.f0[1]");
        let duplicate = fixture_v2().to_string().replace(
            "\"roughness\":0.6",
            "\"roughness\":0.6,\"\\u0072oughness\":0.5",
        );
        assert!(SceneDocument::from_json(&duplicate)
            .err()
            .unwrap()
            .contains("$.materials[0].surface.roughness: duplicate field"));
    }

    #[test]
    fn v2_exposes_authored_surface_conductor_light_and_constant_environment() {
        let mut value = fixture_v2();
        value["materials"][0]["surface"]["metallic"] = json!({"f0":[0.6,0.7,0.8],"metalness":0.9});
        value["environment"] = json!({"diffuse_irradiance":[0.2,0.3,0.4],"specular_radiance":[0.5,0.6,0.7],"max_distance":12});
        value["light"]["color"] = json!([0.4, 0.6, 0.8]);
        let document = load_value(&value).unwrap();
        let snapshot = document.snapshot(0.0);
        let hit = snapshot
            .trace(
                Ray {
                    origin: Vec3::new(0.0, 1.0, 3.0),
                    dir: -Vec3::Z,
                },
                0.0,
                Stage::Light,
                true,
            )
            .unwrap();
        assert!(snapshot.local_materials());
        let surface = snapshot.material_sample(hit, 0.1).unwrap();
        assert_eq!(surface.albedo, Vec3::new(0.4, 0.3, 0.2));
        assert_eq!(surface.normal, hit.normal);
        assert_eq!(
            (surface.roughness, surface.f0, surface.occlusion),
            (0.6, 0.04, 0.8)
        );
        let metallic = snapshot.metallic_sample(hit, 0.1).unwrap();
        assert_eq!(metallic.f0, Vec3::new(0.6, 0.7, 0.8));
        assert_eq!(metallic.metalness, 0.9);
        assert_eq!(snapshot.light_color(), Vec3::new(0.4, 0.6, 0.8));
        for (normal, direction, roughness, time) in
            [(Vec3::Y, Vec3::Z, 0.0, 0.0), (-Vec3::Y, -Vec3::Z, 1.0, 5.0)]
        {
            let sample = snapshot
                .environment_sample(EnvironmentQuery {
                    hit,
                    normal,
                    reflected_direction: direction,
                    roughness,
                    time,
                    stage: Stage::Light,
                })
                .unwrap();
            assert_eq!(sample.diffuse_irradiance, Vec3::new(0.2, 0.3, 0.4));
            assert_eq!(sample.specular_radiance, Vec3::new(0.5, 0.6, 0.7));
            assert_eq!(sample.max_distance, 12.0);
        }
        let legacy = load_value(&fixture()).unwrap().snapshot(0.0);
        assert!(!legacy.local_materials());
        assert!(legacy.material_sample(hit, 0.1).is_none());
        assert!(legacy.metallic_sample(hit, 0.1).is_none());
        assert_eq!(legacy.light_color(), Vec3::new(1.0, 0.58, 0.23));
        let defaults = load_value(&fixture_v2()).unwrap().snapshot(0.0);
        assert!(defaults.metallic_sample(hit, 0.1).is_none());
        assert_eq!(defaults.light_color(), legacy.light_color());
        for (key, invalid) in [
            ("diffuse_irradiance", json!([0, 0, 16.00001])),
            ("specular_radiance", json!([-0.1, 0, 0])),
            ("max_distance", json!(100.00001)),
            ("unexpected", json!(1)),
        ] {
            let mut bad = value.clone();
            bad["environment"][key] = invalid;
            rejected(bad, &format!("$.environment.{key}"));
        }
        value["light"]["color"] = json!([0, 16.01, 0]);
        rejected(value, "$.light.color[1]");
    }

    #[test]
    fn v2_query_ties_follow_stable_ids_while_v1_keeps_array_order() {
        for version in [1, 2] {
            let mut value = if version == 1 {
                fixture()
            } else {
                fixture_v2()
            };
            let mut first = value["objects"][0].clone();
            first["id"] = json!(7);
            value["objects"].as_array_mut().unwrap().insert(0, first);
            let forward = load_value(&value).unwrap();
            value["objects"].as_array_mut().unwrap().reverse();
            let reversed = load_value(&value).unwrap();
            let ray = Ray {
                origin: Vec3::new(0.0, 1.0, 3.0),
                dir: -Vec3::Z,
            };
            let a = forward
                .snapshot(0.0)
                .trace(ray, 0.0, Stage::Light, true)
                .unwrap();
            let b = reversed
                .snapshot(0.0)
                .trace(ray, 0.0, Stage::Light, true)
                .unwrap();
            assert_eq!(b.object_id, 1);
            if version == 2 {
                assert_eq!(bits(Some(a)), bits(Some(b)));
            } else {
                assert_eq!(a.object_id, 7);
            }
        }
    }

    #[test]
    fn v2_filtered_glyphs_share_lod_and_region_identity() {
        for kind in ["stone", "wood", "cloth"] {
            let mut value = fixture_v2();
            value["materials"][0]["pattern"] = json!({"kind":kind,"scale":1,"palette":"abcd"});
            let scene = load_value(&value).unwrap().snapshot(0.0);
            let mut seen = BTreeMap::new();
            for footprint in [0.0, 0.001, 0.04, 0.1, 0.2, 0.201, 10.0, f32::INFINITY] {
                for x in 0..80 {
                    let hit = Hit {
                        distance: 1.0,
                        position: Vec3::ZERO,
                        normal: Vec3::Z,
                        material: 32,
                        object_id: 1,
                        local: Vec3::new(x as f32 * 0.029, 0.023, 0.23),
                    };
                    let glyph = scene.glyph_sample(hit, 1.0, footprint);
                    let key = scene.glyph_sample_key(hit, footprint);
                    assert_eq!(glyph, scene.glyph_sample(hit, 0.0, footprint));
                    if let Some(previous) = seen.insert(key, glyph) {
                        assert_eq!(
                            previous, glyph,
                            "one key cannot hold a different filtered region"
                        );
                    }
                    if footprint > 0.2 {
                        assert_eq!(glyph, b'd');
                        assert_eq!(key, 0x204);
                    }
                }
            }
            // Repeated glyphs must not erase a structural/LOD transition.
            value["materials"][0]["pattern"]["palette"] = json!("xxxx");
            let scene = load_value(&value).unwrap().snapshot(0.0);
            let hit = Hit {
                distance: 1.0,
                position: Vec3::ZERO,
                normal: Vec3::Z,
                material: 32,
                object_id: 1,
                local: Vec3::ZERO,
            };
            assert_eq!(
                scene.glyph_sample(hit, 1.0, 0.0),
                scene.glyph_sample(hit, 1.0, 1.0)
            );
            assert_ne!(
                scene.glyph_sample_key(hit, 0.0),
                scene.glyph_sample_key(hit, 1.0)
            );
        }
    }

    #[test]
    fn packaged_v2_scene_is_visible_from_all_client_views() {
        let document = SceneDocument::from_json(crate::DEFAULT_SCENE_V2_JSON).unwrap();
        assert_eq!(document.version(), 2);
        let scene = document.snapshot(0.0);
        for mode in [Mode::First, Mode::Third, Mode::Overhead] {
            let camera = document.camera(mode);
            let mut ids = BTreeSet::new();
            for y in 0..45 {
                for x in 0..80 {
                    if let Some(hit) = scene.trace(
                        camera.ray(x, y, 80, 45, 16.0 / 9.0),
                        0.0,
                        Stage::Light,
                        true,
                    ) {
                        ids.insert(hit.object_id);
                    }
                }
            }
            assert!(ids.contains(&5), "textile panel absent in {}", mode.name());
            assert!(
                ids.contains(&8),
                "conductor specimen absent in {}",
                mode.name()
            );
            assert!(
                ids.contains(&2) && ids.contains(&3),
                "piers absent in {}",
                mode.name()
            );
        }
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
