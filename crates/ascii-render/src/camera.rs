use crate::core::Ray;
use glam::Vec3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    First,
    Third,
    Overhead,
}
impl Mode {
    pub fn name(self) -> &'static str {
        match self {
            Self::First => "first",
            Self::Third => "third",
            Self::Overhead => "overhead",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "first" => Some(Self::First),
            "third" => Some(Self::Third),
            "overhead" => Some(Self::Overhead),
            _ => None,
        }
    }
}

/// A client-owned observer pose and projection, independent of the lab fixture.
///
/// Coordinates are finite and bounded to +/-10,000 world units. The eye must
/// be more than 0.0001 units from the target, and its direction must have a
/// squared sine greater than 1e-8 relative to the Y axis used as world-up.
/// This rejects near-degenerate frames as well as exactly vertical views.
/// First/third modes use the vertical field of view; overhead uses the
/// orthographic half-height. Both projection parameters are always validated.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraView {
    eye: Vec3,
    target: Vec3,
    vertical_fov_degrees: f32,
    orthographic_half_height: f32,
}

impl CameraView {
    pub fn new(
        eye: Vec3,
        target: Vec3,
        vertical_fov_degrees: f32,
        orthographic_half_height: f32,
    ) -> Result<Self, String> {
        for (name, point) in [("eye", eye), ("target", target)] {
            if !point.is_finite() || point.abs().max_element() > 10_000. {
                return Err(format!(
                    "camera {name} coordinates must be finite and within [-10000, 10000]"
                ));
            }
        }
        let direction = target - eye;
        let distance_squared = direction.length_squared();
        if distance_squared <= 1e-8 {
            return Err("camera eye and target must be more than 0.0001 units apart".into());
        }
        if direction.cross(Vec3::Y).length_squared() <= distance_squared * 1e-8 {
            return Err("camera direction must not be parallel or nearly parallel to Y".into());
        }
        if !vertical_fov_degrees.is_finite() || !(1.0..=179.0).contains(&vertical_fov_degrees) {
            return Err("camera vertical FOV must be finite and within [1, 179] degrees".into());
        }
        if !orthographic_half_height.is_finite()
            || !(0.01..=10_000.).contains(&orthographic_half_height)
        {
            return Err(
                "camera orthographic half-height must be finite and within [0.01, 10000]".into(),
            );
        }
        Ok(Self {
            eye,
            target,
            vertical_fov_degrees,
            orthographic_half_height,
        })
    }

    pub fn eye(self) -> Vec3 {
        self.eye
    }

    pub fn target(self) -> Vec3 {
        self.target
    }

    pub fn vertical_fov_degrees(self) -> f32 {
        self.vertical_fov_degrees
    }

    pub fn orthographic_half_height(self) -> f32 {
        self.orthographic_half_height
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub mode: Mode,
    pub orbit: f32,
    pub offset: Vec3,
    pub zoom: f32,
    /// None preserves the original observer poses and projection arithmetic.
    pub view: Option<CameraView>,
}

/// Camera invariants prepared once per composition. Pixel arithmetic keeps the
/// R1 operation order: in particular do not replace division with a reciprocal
/// multiply, or combine the orthographic scale factors.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PreparedCamera {
    eye: Vec3,
    forward: Vec3,
    right: Vec3,
    up: Vec3,
    mode: Mode,
    zoom: f32,
    perspective_scale: f32,
    orthographic_half_height: Option<f32>,
    cols: u32,
    rows: u32,
    aspect: f32,
}

impl Camera {
    pub fn new(mode: Mode) -> Self {
        Self {
            mode,
            orbit: 0.,
            offset: Vec3::ZERO,
            zoom: 1.,
            view: None,
        }
    }

    /// Start from a validated client pose with the usual interactive controls.
    /// Orbit rotates around world Y; offset translates both eye and target.
    /// First mode keeps the eye fixed and rotates/scales the target distance;
    /// other modes keep the target fixed and rotate/scale the eye distance.
    /// Overhead zoom also scales the orthographic half-height.
    /// As with the legacy camera, callers maintain finite controls and zoom > 0.
    pub fn with_view(mode: Mode, view: CameraView) -> Self {
        Self {
            view: Some(view),
            ..Self::new(mode)
        }
    }

    /// Return world-space observer points using the original control semantics.
    /// At large coordinates, f32 rounding can erase a small eye-target delta.
    /// Explicit views therefore prepare ray orientation from the relative
    /// authored direction, rather than subtracting these rounded world points.
    pub fn pose(self) -> (Vec3, Vec3) {
        let (eye, target) = match self.view {
            Some(view) => (view.eye, view.target),
            None => match self.mode {
                Mode::First => (Vec3::new(1., 1.62, 3.1), Vec3::new(0., 1.2, -2.)),
                Mode::Third => (Vec3::new(4.5, 2.8, 6.8), Vec3::new(0., 0.95, -0.5)),
                Mode::Overhead => (Vec3::new(6., 9., 8.), Vec3::new(0., 0.1, -0.8)),
            },
        };
        let d = eye - target;
        let c = self.orbit.cos();
        let s = self.orbit.sin();
        let d = Vec3::new(c * d.x + s * d.z, d.y, -s * d.x + c * d.z) * self.zoom;
        if self.mode == Mode::First {
            return (eye + self.offset, eye - d + self.offset);
        }
        (target + d + self.offset, target + self.offset)
    }
    pub fn ray(self, x: u32, y: u32, cols: u32, rows: u32, aspect: f32) -> Ray {
        self.prepare(cols, rows, aspect).ray(x, y)
    }

    /// Sample within a cell: offsets are relative to its center in cell units,
    /// positive right/down, and must be finite in [-0.5, 0.5]. Zero offset is
    /// bitwise identical to `ray`. Grid dimensions and aspect must be positive
    /// (aspect finite). Like `ray`, x/y may extend beyond the grid for footprint
    /// queries; callers maintain valid camera controls.
    #[allow(clippy::too_many_arguments)]
    pub fn ray_sample(
        self,
        x: u32,
        y: u32,
        cols: u32,
        rows: u32,
        aspect: f32,
        offset: [f32; 2],
    ) -> Result<Ray, String> {
        if cols == 0 || rows == 0 || !aspect.is_finite() || aspect <= 0. {
            return Err(
                "camera sampling requires positive grid dimensions and finite positive aspect"
                    .into(),
            );
        }
        if offset
            .into_iter()
            .any(|v| !v.is_finite() || !(-0.5..=0.5).contains(&v))
        {
            return Err("camera sample offsets must be finite and within [-0.5, 0.5]".into());
        }
        Ok(self.prepare(cols, rows, aspect).ray_sample(x, y, offset))
    }

    pub(crate) fn prepare(self, cols: u32, rows: u32, aspect: f32) -> PreparedCamera {
        let (eye, target) = self.pose();
        let f = if let Some(view) = self.view {
            // Form the basis before translating into world coordinates: a
            // valid near-vertical direction can otherwise round to exactly Y.
            // Positive zoom changes distance/extent, never orientation; omit
            // it here to avoid losing the direction at very small distances.
            let d = view.target - view.eye;
            let c = self.orbit.cos();
            let s = self.orbit.sin();
            Vec3::new(c * d.x + s * d.z, d.y, -s * d.x + c * d.z).normalize()
        } else {
            (target - eye).normalize()
        };
        let right = f.cross(Vec3::Y).normalize();
        let up = right.cross(f);
        let perspective_scale = if self.mode == Mode::Overhead {
            0.
        } else if let Some(view) = self.view {
            view.vertical_fov_degrees
                .to_radians()
                .mul_add(0.5, 0.)
                .tan()
        } else {
            (if self.mode == Mode::First {
                66_f32
            } else {
                53_f32
            })
            .to_radians()
            .mul_add(0.5, 0.)
            .tan()
        };
        PreparedCamera {
            eye,
            forward: f,
            right,
            up,
            mode: self.mode,
            zoom: self.zoom,
            perspective_scale,
            orthographic_half_height: self.view.map(|view| view.orthographic_half_height),
            cols,
            rows,
            aspect,
        }
    }
}

impl PreparedCamera {
    pub(crate) fn forward_depth(self, position: Vec3) -> f32 {
        (position - self.eye).dot(self.forward)
    }

    /// Trusted internal offsets use the same projection as the center ray.
    pub(crate) fn ray_sample(self, x: u32, y: u32, offset: [f32; 2]) -> Ray {
        if offset == [0., 0.] {
            return self.ray(x, y);
        }
        let sx = ((x as f32 + 0.5 + offset[0]) / self.cols as f32 * 2. - 1.) * self.aspect;
        let sy = 1. - (y as f32 + 0.5 + offset[1]) / self.rows as f32 * 2.;
        if self.mode == Mode::Overhead {
            let half_height = self.orthographic_half_height.unwrap_or(5.);
            Ray {
                origin: self.eye
                    + self.right * sx * half_height * self.zoom
                    + self.up * sy * half_height * self.zoom,
                dir: self.forward,
            }
        } else {
            let scale = self.perspective_scale;
            Ray {
                origin: self.eye,
                dir: (self.forward + self.right * sx * scale + self.up * sy * scale).normalize(),
            }
        }
    }

    pub(crate) fn ray(self, x: u32, y: u32) -> Ray {
        let sx = ((x as f32 + 0.5) / self.cols as f32 * 2. - 1.) * self.aspect;
        let sy = 1. - (y as f32 + 0.5) / self.rows as f32 * 2.;
        if self.mode == Mode::Overhead {
            if let Some(half_height) = self.orthographic_half_height {
                return Ray {
                    origin: self.eye
                        + self.right * sx * half_height * self.zoom
                        + self.up * sy * half_height * self.zoom,
                    dir: self.forward,
                };
            }
            Ray {
                origin: self.eye + self.right * sx * 5. * self.zoom + self.up * sy * 5. * self.zoom,
                dir: self.forward,
            }
        } else {
            let scale = self.perspective_scale;
            Ray {
                origin: self.eye,
                dir: (self.forward + self.right * sx * scale + self.up * sy * scale).normalize(),
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    // Keep the fixture pose independent too: a new default must not silently
    // change both the implementation and the ray oracle through Camera::pose.
    fn r1_pose(camera: Camera) -> (Vec3, Vec3) {
        let (eye, target) = match camera.mode {
            Mode::First => (Vec3::new(1., 1.62, 3.1), Vec3::new(0., 1.2, -2.)),
            Mode::Third => (Vec3::new(4.5, 2.8, 6.8), Vec3::new(0., 0.95, -0.5)),
            Mode::Overhead => (Vec3::new(6., 9., 8.), Vec3::new(0., 0.1, -0.8)),
        };
        let d = eye - target;
        let c = camera.orbit.cos();
        let s = camera.orbit.sin();
        let d = Vec3::new(c * d.x + s * d.z, d.y, -s * d.x + c * d.z) * camera.zoom;
        if camera.mode == Mode::First {
            return (eye + camera.offset, eye - d + camera.offset);
        }
        (target + d + camera.offset, target + camera.offset)
    }

    // Frozen R1 ray formula, deliberately independent from PreparedCamera.
    // This oracle catches reassociation/reciprocal changes that serial-vs-Rayon
    // comparisons alone would miss because both paths share the prepared data.
    fn r1_ray(camera: Camera, x: u32, y: u32, cols: u32, rows: u32, aspect: f32) -> Ray {
        let (eye, target) = r1_pose(camera);
        let f = (target - eye).normalize();
        let right = f.cross(Vec3::Y).normalize();
        let up = right.cross(f);
        let sx = ((x as f32 + 0.5) / cols as f32 * 2. - 1.) * aspect;
        let sy = 1. - (y as f32 + 0.5) / rows as f32 * 2.;
        if camera.mode == Mode::Overhead {
            Ray {
                origin: eye + right * sx * 5. * camera.zoom + up * sy * 5. * camera.zoom,
                dir: f,
            }
        } else {
            let scale = (if camera.mode == Mode::First {
                66_f32
            } else {
                53_f32
            })
            .to_radians()
            .mul_add(0.5, 0.)
            .tan();
            Ray {
                origin: eye,
                dir: (f + right * sx * scale + up * sy * scale).normalize(),
            }
        }
    }

    fn ray_bits(ray: Ray) -> [u32; 6] {
        [
            ray.origin.x.to_bits(),
            ray.origin.y.to_bits(),
            ray.origin.z.to_bits(),
            ray.dir.x.to_bits(),
            ray.dir.y.to_bits(),
            ray.dir.z.to_bits(),
        ]
    }

    #[test]
    fn prepared_rays_are_bitwise_identical_to_r1() {
        for mode in [Mode::First, Mode::Third, Mode::Overhead] {
            for orbit in [0., -0.173, 1.73] {
                for zoom in [0.65, 1., 1.7] {
                    let mut camera = Camera::new(mode);
                    assert_eq!(camera.view, None);
                    camera.orbit = orbit;
                    camera.zoom = zoom;
                    camera.offset = Vec3::new(-0.37, 0.19, 0.73);
                    for (cols, rows, aspect) in [(31, 19, 16. / 9.), (17, 11, 4. / 3.)] {
                        let prepared = camera.prepare(cols, rows, aspect);
                        for y in 0..rows {
                            for x in 0..cols {
                                let expected = ray_bits(r1_ray(camera, x, y, cols, rows, aspect));
                                assert_eq!(ray_bits(prepared.ray(x, y)), expected);
                                assert_eq!(
                                    ray_bits(camera.ray(x, y, cols, rows, aspect)),
                                    expected
                                );
                            }
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn camera_modes_share_world_and_valid_rays() {
        for mode in [Mode::First, Mode::Third, Mode::Overhead] {
            let r = Camera::new(mode).ray(10, 10, 20, 20, 16. / 9.);
            assert!(r.dir.is_finite());
            assert!((r.dir.length() - 1.).abs() < 1e-5);
        }
    }
    #[test]
    fn orbit_preserves_distance() {
        let a = Camera::new(Mode::Third);
        let mut b = a;
        b.orbit = 1.;
        let (e, t) = a.pose();
        let (e2, t2) = b.pose();
        assert!(((e - t).length() - (e2 - t2).length()).abs() < 1e-5);
    }

    fn assert_vec_close(actual: Vec3, expected: Vec3) {
        assert!(
            (actual - expected).abs().max_element() < 2e-5,
            "expected {expected:?}, got {actual:?}"
        );
    }

    #[test]
    fn explicit_view_centers_all_modes_on_its_target() {
        let eye = Vec3::new(-2., 3., 7.);
        let target = Vec3::new(1., 1., -1.);
        let view = CameraView::new(eye, target, 71., 3.).unwrap();
        assert_eq!(view.eye(), eye);
        assert_eq!(view.target(), target);
        assert_eq!(view.vertical_fov_degrees(), 71.);
        assert_eq!(view.orthographic_half_height(), 3.);
        for mode in [Mode::First, Mode::Third, Mode::Overhead] {
            let camera = Camera::with_view(mode, view);
            assert_eq!(camera.view, Some(view));
            let center = camera.ray(2, 1, 5, 3, 2.);
            assert_vec_close(center.origin, eye);
            assert_vec_close(center.dir, (target - eye).normalize());
        }
    }

    #[test]
    fn explicit_fov_controls_vertical_and_horizontal_edge_rays() {
        for mode in [Mode::First, Mode::Third] {
            for (fov, tangent) in [(60., 1. / 3_f32.sqrt()), (90., 1.)] {
                let view = CameraView::new(Vec3::ZERO, Vec3::NEG_Z, fov, 3.).unwrap();
                let camera = Camera::with_view(mode, view);
                let top = camera.ray(2, 0, 5, 5, 2.);
                let right = camera.ray(4, 2, 5, 5, 2.);
                let bottom_left = camera.ray(0, 4, 5, 5, 2.);
                assert_vec_close(top.origin, Vec3::ZERO);
                assert_vec_close(top.dir, Vec3::new(0., 0.8 * tangent, -1.).normalize());
                assert_vec_close(right.dir, Vec3::new(1.6 * tangent, 0., -1.).normalize());
                assert_vec_close(
                    bottom_left.dir,
                    Vec3::new(-1.6 * tangent, -0.8 * tangent, -1.).normalize(),
                );
            }
        }
    }

    #[test]
    fn explicit_orthographic_extent_scales_origins_with_aspect_and_zoom() {
        let view = CameraView::new(Vec3::ZERO, Vec3::NEG_Z, 60., 3.).unwrap();
        for zoom in [1., 2.] {
            let mut camera = Camera::with_view(Mode::Overhead, view);
            camera.zoom = zoom;
            let eye = Vec3::new(0., 0., zoom - 1.);
            let center = camera.ray(2, 2, 5, 5, 2.);
            let top = camera.ray(2, 0, 5, 5, 2.);
            let right = camera.ray(4, 2, 5, 5, 2.);
            assert_vec_close(center.origin, eye);
            assert_vec_close(top.origin, eye + Vec3::Y * 2.4 * zoom);
            assert_vec_close(right.origin, eye + Vec3::X * 4.8 * zoom);
            for y in 0..5 {
                for x in 0..5 {
                    assert_vec_close(camera.ray(x, y, 5, 5, 2.).dir, Vec3::NEG_Z);
                }
            }
        }
    }

    #[test]
    fn explicit_controls_keep_the_first_eye_or_other_target_as_pivot() {
        let view = CameraView::new(Vec3::new(3., 4., 5.), Vec3::new(3., 4., 1.), 60., 3.).unwrap();
        for mode in [Mode::First, Mode::Third, Mode::Overhead] {
            let mut camera = Camera::with_view(mode, view);
            camera.orbit = std::f32::consts::FRAC_PI_2;
            camera.offset = Vec3::new(1., -2., 0.5);
            camera.zoom = 2.;
            let (eye, target) = camera.pose();
            if mode == Mode::First {
                assert_vec_close(eye, Vec3::new(4., 2., 5.5));
                assert_vec_close(target, Vec3::new(-4., 2., 5.5));
            } else {
                assert_vec_close(eye, Vec3::new(12., 2., 1.5));
                assert_vec_close(target, Vec3::new(4., 2., 1.5));
            }
            let center = camera.ray(2, 2, 5, 5, 2.);
            assert_vec_close(center.origin, eye);
            assert_vec_close(center.dir, Vec3::NEG_X);
        }
    }

    #[test]
    fn explicit_basis_survives_world_pose_rounding_after_orbit() {
        let view = CameraView::new(
            Vec3::new(10_000., 1., 0.),
            Vec3::new(10_000., 0., 0.0003),
            60.,
            3.,
        )
        .unwrap();
        for mode in [Mode::First, Mode::Third, Mode::Overhead] {
            let mut camera = Camera::with_view(mode, view);
            camera.orbit = std::f32::consts::FRAC_PI_2;
            if mode != Mode::First {
                let (eye, target) = camera.pose();
                // Regression trigger: these f32 world points alone cannot
                // recover the authored horizontal direction after rotation.
                assert_eq!((target - eye).cross(Vec3::Y), Vec3::ZERO);
            }
            assert_vec_close(
                camera.ray(2, 2, 5, 5, 2.).dir,
                Vec3::new(0.0003, -1., 0.).normalize(),
            );
            for y in 0..5 {
                for x in 0..5 {
                    let ray = camera.ray(x, y, 5, 5, 2.);
                    assert!(ray.origin.is_finite());
                    assert!(ray.dir.is_finite());
                    assert!((ray.dir.length() - 1.).abs() < 1e-5);
                }
            }
        }
    }

    #[test]
    fn explicit_ray_directions_rotate_and_ignore_translation_and_positive_zoom() {
        let view = CameraView::new(Vec3::Y, Vec3::new(0., 0., 0.0003), 60., 3.).unwrap();
        for mode in [Mode::First, Mode::Third, Mode::Overhead] {
            let base = Camera::with_view(mode, view);
            for orbit in [0.37, std::f32::consts::FRAC_PI_2, -2.4] {
                let rotation = glam::Quat::from_rotation_y(orbit);
                for zoom in [0.5, 1., 2.] {
                    let mut camera = base;
                    camera.orbit = orbit;
                    camera.zoom = zoom;
                    for offset in [
                        Vec3::new(10_000., -10_000., 10_000.),
                        Vec3::new(-10_000., 10_000., -10_000.),
                    ] {
                        let mut shifted = camera;
                        shifted.offset = offset;
                        for (x, y) in [(0, 0), (2, 2), (4, 4)] {
                            let original = base.ray(x, y, 5, 5, 2.);
                            let rotated = camera.ray(x, y, 5, 5, 2.);
                            let translated = shifted.ray(x, y, 5, 5, 2.);
                            assert_vec_close(rotated.dir, rotation * original.dir);
                            assert_eq!(
                                rotated.dir.to_array().map(f32::to_bits),
                                translated.dir.to_array().map(f32::to_bits),
                            );
                            // Origins retain normal f32 world-position precision.
                            assert!(
                                (translated.origin - rotated.origin - offset)
                                    .abs()
                                    .max_element()
                                    < 0.004
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn view_rejects_invalid_coordinates_and_projection_values() {
        for value in [
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            -10_001.,
            10_001.,
        ] {
            for axis in 0..3 {
                let mut coordinates = [0.; 3];
                coordinates[axis] = value;
                let point = Vec3::from_array(coordinates);
                assert!(CameraView::new(point, Vec3::NEG_Z, 60., 3.).is_err());
                assert!(CameraView::new(Vec3::ZERO, point, 60., 3.).is_err());
            }
        }
        for fov in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 0.999, 179.001] {
            assert!(CameraView::new(Vec3::ZERO, Vec3::NEG_Z, fov, 3.).is_err());
        }
        for extent in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 0.009, 10_001.] {
            assert!(CameraView::new(Vec3::ZERO, Vec3::NEG_Z, 60., extent).is_err());
        }
    }

    #[test]
    fn view_rejects_degenerate_or_nearly_vertical_frames_at_any_distance() {
        for target in [Vec3::ZERO, Vec3::NEG_Z * 0.00001, Vec3::Y, Vec3::NEG_Y] {
            assert!(CameraView::new(Vec3::ZERO, target, 60., 3.).is_err());
        }
        for distance in [0.001, 1., 10_000.] {
            let nearly_vertical = Vec3::new(0.00001, 1., 0.) * distance;
            assert!(CameraView::new(Vec3::ZERO, nearly_vertical, 60., 3.).is_err());
            let usable = Vec3::new(0.001, 1., 0.) * distance;
            let view = CameraView::new(Vec3::ZERO, usable, 60., 3.).unwrap();
            for mode in [Mode::First, Mode::Third, Mode::Overhead] {
                let ray = Camera::with_view(mode, view).ray(2, 0, 5, 5, 2.);
                assert!(ray.origin.is_finite());
                assert!(ray.dir.is_finite());
                assert!((ray.dir.length() - 1.).abs() < 1e-5);
            }
        }
    }

    #[test]
    fn view_accepts_inclusive_coordinate_and_projection_bounds() {
        for (fov, extent) in [(1., 0.01), (179., 10_000.)] {
            let view =
                CameraView::new(Vec3::splat(-10_000.), Vec3::splat(10_000.), fov, extent).unwrap();
            for mode in [Mode::First, Mode::Third, Mode::Overhead] {
                let ray = Camera::with_view(mode, view).ray(0, 0, 5, 5, 2.);
                assert!(ray.origin.is_finite());
                assert!(ray.dir.is_finite());
                assert!((ray.dir.length() - 1.).abs() < 1e-5);
            }
        }
    }

    #[test]
    fn sample_center_preserves_legacy_and_explicit_ray_bits() {
        let view = CameraView::new(Vec3::new(2., 3., 4.), Vec3::ZERO, 70., 2.).unwrap();
        for mode in [Mode::First, Mode::Third, Mode::Overhead] {
            for mut camera in [Camera::new(mode), Camera::with_view(mode, view)] {
                camera.orbit = -0.31;
                camera.zoom = 1.7;
                camera.offset = Vec3::new(0.1, -0.2, 0.3);
                for y in 0..7 {
                    for x in 0..11 {
                        assert_eq!(
                            ray_bits(camera.ray(x, y, 11, 7, 16. / 9.)),
                            ray_bits(camera.ray_sample(x, y, 11, 7, 16. / 9., [0., 0.]).unwrap()),
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn sample_offsets_move_right_and_down_in_cell_units() {
        let view = CameraView::new(Vec3::new(0., 0., 4.), Vec3::ZERO, 90., 1.).unwrap();
        for mode in [Mode::First, Mode::Third, Mode::Overhead] {
            let camera = Camera::with_view(mode, view);
            let ray = camera
                .ray_sample(2, 1, 4, 4, 2., [1. / 3., 1. / 3.])
                .unwrap();
            if mode == Mode::Overhead {
                assert_vec_close(ray.origin, Vec3::new(5. / 6., 1. / 12., 4.));
                assert_vec_close(ray.dir, Vec3::NEG_Z);
            } else {
                assert_vec_close(ray.origin, Vec3::new(0., 0., 4.));
                assert_vec_close(ray.dir, Vec3::new(5. / 6., 1. / 12., -1.).normalize());
            }
            // Cell boundaries coincide, including a neighbor beyond the grid.
            assert_eq!(
                ray_bits(camera.ray_sample(3, 2, 4, 4, 2., [0.5, 0.]).unwrap()),
                ray_bits(camera.ray_sample(4, 2, 4, 4, 2., [-0.5, 0.]).unwrap()),
            );
        }
    }

    #[test]
    fn sample_rejects_invalid_grid_aspect_and_offsets() {
        let camera = Camera::new(Mode::Third);
        for (cols, rows, aspect) in [
            (0, 4, 1.),
            (4, 0, 1.),
            (4, 4, 0.),
            (4, 4, -1.),
            (4, 4, f32::NAN),
            (4, 4, f32::INFINITY),
        ] {
            assert!(camera
                .ray_sample(0, 0, cols, rows, aspect, [0., 0.])
                .is_err());
        }
        for value in [f32::NAN, f32::INFINITY, -0.5001, 0.5001] {
            for offset in [[value, 0.], [0., value]] {
                assert!(camera.ray_sample(0, 0, 4, 4, 1., offset).is_err());
            }
        }
    }
}
