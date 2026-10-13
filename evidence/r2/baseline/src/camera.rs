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
        let (eye, target) = self.pose();
        let f = (target - eye).normalize();
        let right = f.cross(Vec3::Y).normalize();
        let up = right.cross(f);
        let sx = ((x as f32 + 0.5) / cols as f32 * 2. - 1.) * aspect;
        let sy = 1. - (y as f32 + 0.5) / rows as f32 * 2.;
        if self.mode == Mode::Overhead {
            Ray {
                origin: eye + right * sx * 5. * self.zoom + up * sy * 5. * self.zoom,
                dir: f,
            }
        } else {
            let scale = (if self.mode == Mode::First {
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
}
#[cfg(test)]
mod tests {
    use super::*;
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
