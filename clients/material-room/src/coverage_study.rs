//! Fixed six-second observer path for the R12 geometry workload.
//! Content/simulation remain in the client; density and sampling never change.
use ascii_render::camera::{Camera, Mode};
use glam::Vec3;

/// Apply the same bounded observer motion in the window, captures and audit.
/// `elapsed` is finite seconds from the authored camera, not accumulated deltas.
/// First/third move away by up to 0.9 world units; overhead also widens by 25%.
pub fn coverage_camera(base: Camera, elapsed: f32) -> Camera {
    assert!(elapsed.is_finite(), "coverage path time must be finite");
    let phase = elapsed as f64 * std::f64::consts::TAU / 6.0;
    let sway = phase.sin() as f32;
    let distance = ((1.0 - phase.cos()) * 0.5) as f32;
    let (eye, target) = base.pose();
    let mut camera = base;
    camera.offset +=
        Vec3::X * (0.22 * sway) + (eye - target).normalize_or_zero() * (0.9 * distance);
    camera.orbit += 0.045 * sway;
    if base.mode == Mode::Overhead {
        camera.zoom *= 1.0 + 0.25 * distance;
    }
    camera
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_starts_at_authored_pose_and_is_periodic() {
        for mode in [Mode::First, Mode::Third, Mode::Overhead] {
            let base = Camera::new(mode);
            let start = coverage_camera(base, 0.0);
            assert_eq!(start.pose(), base.pose());
            assert_eq!(start.zoom, base.zoom);
            for t in [0.0, 0.5, 1.5, 3.0, 5.5] {
                let a = coverage_camera(base, t);
                let b = coverage_camera(base, t + 6.0);
                assert!(a.offset.abs_diff_eq(b.offset, 1e-6));
                assert!((a.orbit - b.orbit).abs() < 1e-6);
                assert!((a.zoom - b.zoom).abs() < 1e-6);
            }
        }
    }
}
