//! Persist interactive camera controls without baking rounded world-space poses.
//! The scene document owns the three base views and their projection parameters.
use crate::SceneDocument;
use ascii_render::camera::{Camera, CameraView, Mode};
use glam::Vec3;
use serde_json::{json, Map, Value};

const MODES: [Mode; 3] = [Mode::First, Mode::Third, Mode::Overhead];

/// Three independent client cameras. Saving the controls alongside the unchanged
/// scene views preserves the exact camera arithmetic, including near-vertical
/// directions that `Camera::pose()` may lose after world-coordinate rounding.
#[derive(Clone, Debug)]
pub struct CameraBank {
    cameras: [Camera; 3],
}

impl CameraBank {
    pub fn defaults(document: &SceneDocument) -> Self {
        Self {
            cameras: MODES.map(|mode| document.camera(mode)),
        }
    }

    /// Read the project's `camera_controls` value. All modes and fields are
    /// required; unknown fields, nonfinite values and out-of-range controls fail.
    /// Controls replace the document camera's controls rather than accumulating.
    pub fn from_json(document: &SceneDocument, value: &Value) -> Result<Self, String> {
        let root = fields(value, "$.camera_controls", &["first", "third", "overhead"])?;
        let mut bank = Self::defaults(document);
        for mode in MODES {
            let path = format!("$.camera_controls.{}", mode.name());
            let record = fields(
                required(root, mode.name(), "$.camera_controls")?,
                &path,
                &["orbit", "offset", "zoom"],
            )?;
            let mut camera = bank.get(mode);
            camera.orbit = number(
                required(record, "orbit", &path)?,
                &format!("{path}.orbit"),
                -1024.,
                1024.,
            )?;
            let offset_path = format!("{path}.offset");
            let offset = required(record, "offset", &path)?
                .as_array()
                .filter(|components| components.len() == 3)
                .ok_or_else(|| format!("{offset_path}: expected exactly three numbers"))?;
            camera.offset = Vec3::new(
                number(&offset[0], &format!("{offset_path}[0]"), -10000., 10000.)?,
                number(&offset[1], &format!("{offset_path}[1]"), -10000., 10000.)?,
                number(&offset[2], &format!("{offset_path}[2]"), -10000., 10000.)?,
            );
            camera.zoom = number(
                required(record, "zoom", &path)?,
                &format!("{path}.zoom"),
                0.01,
                100.,
            )?;
            bank.set(camera)?;
        }
        Ok(bank)
    }

    pub fn to_json(&self) -> Value {
        let mut records = Map::new();
        for mode in MODES {
            let camera = self.get(mode);
            records.insert(
                mode.name().into(),
                json!({
                    "orbit": camera.orbit,
                    "offset": camera.offset.to_array(),
                    "zoom": camera.zoom,
                }),
            );
        }
        Value::Object(records)
    }

    pub fn get(&self, mode: Mode) -> Camera {
        self.cameras[index(mode)]
    }

    /// Atomically replace one slot's controls. Orbit is bounded to +/-1024
    /// radians, offset components to +/-10000 and positive zoom to [0.01,100].
    /// Changing the base view here would be lost by the controls-only format;
    /// edit `scene.cameras` and reconstruct the bank for such changes instead.
    pub fn set(&mut self, camera: Camera) -> Result<(), String> {
        let path = format!("$.camera_controls.{}", camera.mode.name());
        validate(camera.orbit, &format!("{path}.orbit"), -1024., 1024.)?;
        for (axis, value) in camera.offset.to_array().into_iter().enumerate() {
            validate(value, &format!("{path}.offset[{axis}]"), -10000., 10000.)?;
        }
        validate(camera.zoom, &format!("{path}.zoom"), 0.01, 100.)?;
        let slot = index(camera.mode);
        if !same_view(self.cameras[slot].view, camera.view) {
            return Err(format!(
                "{path}: base view differs from the scene; edit scene.cameras and rebuild the camera bank"
            ));
        }
        self.cameras[slot] = camera;
        Ok(())
    }
}

fn index(mode: Mode) -> usize {
    match mode {
        Mode::First => 0,
        Mode::Third => 1,
        Mode::Overhead => 2,
    }
}

fn same_view(a: Option<CameraView>, b: Option<CameraView>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            a.eye().to_array().map(f32::to_bits) == b.eye().to_array().map(f32::to_bits)
                && a.target().to_array().map(f32::to_bits)
                    == b.target().to_array().map(f32::to_bits)
                && a.vertical_fov_degrees().to_bits() == b.vertical_fov_degrees().to_bits()
                && a.orthographic_half_height().to_bits() == b.orthographic_half_height().to_bits()
        }
        _ => false,
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
    if let Some(key) = map.keys().find(|key| !allowed.contains(&key.as_str())) {
        return Err(format!("{path}.{key}: unknown field"));
    }
    Ok(map)
}

fn required<'a>(map: &'a Map<String, Value>, key: &str, path: &str) -> Result<&'a Value, String> {
    map.get(key)
        .ok_or_else(|| format!("{path}.{key}: required field"))
}

fn number(value: &Value, path: &str, min: f32, max: f32) -> Result<f32, String> {
    let value = value
        .as_f64()
        .ok_or_else(|| format!("{path}: expected finite number"))?;
    // Check the authored number before narrowing too: 10000.0001 must not pass
    // the offset bound merely because it rounds to 10000 as an f32.
    if !value.is_finite() || value < min as f64 || value > max as f64 {
        return Err(format!("{path}: expected finite f32 in [{min}, {max}]"));
    }
    let value = value as f32;
    validate(value, path, min, max)?;
    Ok(value)
}

fn validate(value: f32, path: &str, min: f32, max: f32) -> Result<(), String> {
    if !value.is_finite() || value < min || value > max {
        return Err(format!("{path}: expected finite f32 in [{min}, {max}]"));
    }
    Ok(())
}
