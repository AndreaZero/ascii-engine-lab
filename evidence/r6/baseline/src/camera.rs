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
#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub mode: Mode,
    pub orbit: f32,
    pub offset: Vec3,
    pub zoom: f32,
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
        }
    }
    pub fn pose(self) -> (Vec3, Vec3) {
        let (eye, target) = match self.mode {
            Mode::First => (Vec3::new(1., 1.62, 3.1), Vec3::new(0., 1.2, -2.)),
            Mode::Third => (Vec3::new(4.5, 2.8, 6.8), Vec3::new(0., 0.95, -0.5)),
            Mode::Overhead => (Vec3::new(6., 9., 8.), Vec3::new(0., 0.1, -0.8)),
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

    pub(crate) fn prepare(self, cols: u32, rows: u32, aspect: f32) -> PreparedCamera {
        let (eye, target) = self.pose();
        let f = (target - eye).normalize();
        let right = f.cross(Vec3::Y).normalize();
        let up = right.cross(f);
        let perspective_scale = if self.mode == Mode::Overhead {
            0.
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
            cols,
            rows,
            aspect,
        }
    }
}

impl PreparedCamera {
    pub(crate) fn ray(self, x: u32, y: u32) -> Ray {
        let sx = ((x as f32 + 0.5) / self.cols as f32 * 2. - 1.) * self.aspect;
        let sy = 1. - (y as f32 + 0.5) / self.rows as f32 * 2.;
        if self.mode == Mode::Overhead {
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

    // Frozen R1 ray formula, deliberately independent from PreparedCamera.
    // This oracle catches reassociation/reciprocal changes that serial-vs-Rayon
    // comparisons alone would miss because both paths share the prepared data.
    fn r1_ray(camera: Camera, x: u32, y: u32, cols: u32, rows: u32, aspect: f32) -> Ray {
        let (eye, target) = camera.pose();
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
}
