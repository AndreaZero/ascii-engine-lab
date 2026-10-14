//! Exact differential contract, using the unchanged original kernel as oracle.
use ascii_render::core::{intersect_box, intersect_box_before, intersect_box_before_counted, Ray};
use glam::Vec3;

const EPSILON: f32 = 0.0001;

fn bits(hit: Option<(f32, Vec3)>) -> Option<[u32; 4]> {
    hit.map(|(distance, normal)| {
        [
            distance.to_bits(),
            normal.x.to_bits(),
            normal.y.to_bits(),
            normal.z.to_bits(),
        ]
    })
}

// These neighbors cover finite values and infinities without an extra crate.
fn next_up(value: f32) -> f32 {
    if value.is_nan() || value == f32::INFINITY {
        value
    } else if value == 0.0 {
        f32::from_bits(1)
    } else if value > 0.0 {
        f32::from_bits(value.to_bits() + 1)
    } else {
        f32::from_bits(value.to_bits() - 1)
    }
}

fn next_down(value: f32) -> f32 {
    -next_up(-value)
}

fn assert_equivalent(ray: Ray, min: Vec3, max: Vec3, limit: f32) {
    let expected = intersect_box(ray, min, max).filter(|(distance, _)| *distance < limit);
    let actual = intersect_box_before(ray, min, max, limit);
    let (counted, axes) = intersect_box_before_counted(ray, min, max, limit);
    assert_eq!(
        bits(actual),
        bits(expected),
        "ray={ray:?}, min={min:?}, max={max:?}, limit={limit:?}/{:08x}",
        limit.to_bits()
    );
    assert_eq!(bits(counted), bits(expected));
    assert!(axes <= 3);
    if limit.is_nan() || limit <= EPSILON {
        assert_eq!(axes, 0);
    } else {
        let (_, unbounded_axes) = intersect_box_before_counted(ray, min, max, f32::INFINITY);
        assert!(axes <= unbounded_axes);
    }
}

fn assert_limits(ray: Ray, min: Vec3, max: Vec3) {
    for limit in [
        f32::NAN,
        f32::from_bits(0xffc0_1234),
        f32::NEG_INFINITY,
        -f32::MAX,
        -1.0,
        -0.0,
        0.0,
        next_down(EPSILON),
        EPSILON,
        next_up(EPSILON),
        0.5,
        1.0,
        2.0,
        4.0,
        100.0,
        f32::MAX,
        f32::INFINITY,
    ] {
        assert_equivalent(ray, min, max, limit);
    }
    if let Some((distance, _)) = intersect_box(ray, min, max) {
        for limit in [next_down(distance), distance, next_up(distance)] {
            assert_equivalent(ray, min, max, limit);
        }
    }
}

#[test]
fn strict_entry_exit_and_normal_ties_match_original_bits() {
    for ray in [
        Ray {
            origin: Vec3::new(0., 0., 3.),
            dir: -Vec3::Z,
        },
        Ray {
            origin: Vec3::new(3., 3., 3.),
            dir: -Vec3::ONE,
        },
        Ray {
            origin: Vec3::new(-3., -3., -3.),
            dir: Vec3::ONE,
        },
        Ray {
            origin: Vec3::ZERO,
            dir: Vec3::ONE,
        },
        Ray {
            origin: Vec3::ZERO,
            dir: -Vec3::ONE,
        },
        Ray {
            origin: Vec3::ONE,
            dir: -Vec3::ONE,
        },
        Ray {
            origin: Vec3::ONE,
            dir: Vec3::ONE,
        },
        Ray {
            origin: Vec3::new(1., 1., 3.),
            dir: -Vec3::Z,
        },
        Ray {
            origin: Vec3::new(1., 0., 0.),
            dir: Vec3::new(-0., 1., 0.),
        },
        Ray {
            origin: Vec3::new(3., 0., 0.),
            dir: Vec3::new(-2., -0., 0.),
        },
    ] {
        assert_limits(ray, -Vec3::ONE, Vec3::ONE);
    }
    let tied = Ray {
        origin: Vec3::splat(3.),
        dir: -Vec3::ONE,
    };
    assert_eq!(
        bits(intersect_box_before(
            tied,
            -Vec3::ONE,
            Vec3::ONE,
            next_up(2.)
        )),
        Some([2.0f32.to_bits(), 1.0f32.to_bits(), 0, 0])
    );
    assert!(intersect_box_before(tied, -Vec3::ONE, Vec3::ONE, 2.).is_none());
}

#[test]
fn bounded_entry_saves_axes_without_pruning_inside_exit() {
    let outside = Ray {
        origin: Vec3::new(-5., 0., 0.),
        dir: Vec3::X,
    };
    let (rejected, axes) = intersect_box_before_counted(outside, -Vec3::ONE, Vec3::ONE, 4.);
    assert!(rejected.is_none());
    assert_eq!(axes, 1);
    let (accepted, axes) =
        intersect_box_before_counted(outside, -Vec3::ONE, Vec3::ONE, next_up(4.));
    assert_eq!(accepted.unwrap().0.to_bits(), 4.0f32.to_bits());
    assert_eq!(axes, 3); // Includes the two parallel axes.
    let inside = Ray {
        origin: Vec3::ZERO,
        dir: Vec3::X,
    };
    let (rejected, axes) = intersect_box_before_counted(inside, -Vec3::ONE, Vec3::ONE, 0.5);
    assert!(rejected.is_none());
    assert_eq!(axes, 3); // A negative entry does not justify an early rejection.
}

#[test]
fn infinite_limit_filters_raw_infinity_after_all_original_axes() {
    let ray = Ray {
        origin: Vec3::new(-f32::MAX, 0., 0.),
        dir: Vec3::new(1e-8, 0., 0.),
    };
    // The x slab overflows to +Inf. The original then examines both parallel
    // axes and returns this raw infinite hit; the strict +Inf filter rejects it.
    assert_eq!(
        intersect_box(ray, -Vec3::ONE, Vec3::ONE).unwrap().0,
        f32::INFINITY
    );
    let (hit, axes) = intersect_box_before_counted(ray, -Vec3::ONE, Vec3::ONE, f32::INFINITY);
    assert!(hit.is_none());
    assert_eq!(axes, 3);
    assert_equivalent(ray, -Vec3::ONE, Vec3::ONE, f32::INFINITY);
    // A finite limit still permits rejecting immediately after the x slab.
    let (hit, axes) = intersect_box_before_counted(ray, -Vec3::ONE, Vec3::ONE, f32::MAX);
    assert!(hit.is_none());
    assert_eq!(axes, 1);
}

#[test]
fn parallel_threshold_nonunit_and_legacy_outside_slab_hit_are_preserved() {
    for transverse in [
        -next_up(1e-8),
        -1e-8,
        -next_down(1e-8),
        -0.0,
        0.0,
        next_down(1e-8),
        1e-8,
        next_up(1e-8),
    ] {
        for depth in [1e-8, next_up(1e-8), 0.25, 1., 1e10] {
            for origin in [Vec3::ZERO, Vec3::new(0., 0., 10000.), Vec3::new(1., 0., 3.)] {
                let ray = Ray {
                    origin,
                    dir: Vec3::new(transverse, 0., -depth),
                };
                assert_limits(ray, -Vec3::ONE, Vec3::ONE);
            }
        }
    }
    // The original treats x as parallel. Its accepted hit lies far outside the
    // mathematical x slab; no new geometric bounding approximation may drop it.
    let ray = Ray {
        origin: Vec3::new(0., 0., 10000.),
        dir: Vec3::new(9e-9, 0., -1e-8),
    };
    let (distance, _) = intersect_box(ray, -Vec3::ONE, Vec3::ONE).unwrap();
    assert!((ray.origin + ray.dir * distance).x > 1000.);
    assert_eq!(
        bits(intersect_box_before(
            ray,
            -Vec3::ONE,
            Vec3::ONE,
            next_up(distance)
        )),
        bits(intersect_box(ray, -Vec3::ONE, Vec3::ONE))
    );
}

#[test]
fn nonfinite_local_inputs_signed_zero_and_extreme_bounds_match_original() {
    let special = [
        f32::NEG_INFINITY,
        -f32::MAX,
        -1e20,
        -1.,
        -f32::MIN_POSITIVE,
        -0.,
        0.,
        f32::from_bits(1),
        f32::MIN_POSITIVE,
        1.,
        1e20,
        f32::MAX,
        f32::INFINITY,
        f32::NAN,
        f32::from_bits(0xffc0_1234),
    ];
    for axis in 0..3 {
        for value in special {
            let mut origin = Vec3::new(0.25, -0.25, 3.);
            origin[axis] = value;
            assert_limits(
                Ray {
                    origin,
                    dir: Vec3::new(-0., 0.25, -1.),
                },
                -Vec3::ONE,
                Vec3::ONE,
            );
            let mut dir = Vec3::new(0.25, -0.25, -1.);
            dir[axis] = value;
            assert_limits(
                Ray {
                    origin: Vec3::new(0., 0., 3.),
                    dir,
                },
                -Vec3::ONE,
                Vec3::ONE,
            );
            let mut bound = Vec3::ONE;
            bound[axis] = value;
            let ray = Ray {
                origin: Vec3::new(0., 0., 3.),
                dir: Vec3::new(0.25, -0.25, -1.),
            };
            assert_limits(ray, -Vec3::ONE, bound);
            assert_limits(ray, bound, Vec3::ONE);
        }
    }
    for origin in [Vec3::ZERO, Vec3::ONE, Vec3::splat(f32::MAX)] {
        for dir in [
            Vec3::ZERO,
            Vec3::splat(-0.),
            Vec3::splat(f32::INFINITY),
            Vec3::splat(f32::NAN),
        ] {
            assert_limits(Ray { origin, dir }, -Vec3::ONE, Vec3::ONE);
        }
    }
}

struct Random(u64);
impl Random {
    fn bits(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 as u32
    }
    fn finite(&mut self, scale: f32) -> f32 {
        (self.bits() as f64 / u32::MAX as f64 * 2. - 1.) as f32 * scale
    }
    fn vector(&mut self, scale: f32) -> Vec3 {
        Vec3::new(self.finite(scale), self.finite(scale), self.finite(scale))
    }
    fn raw_vector(&mut self) -> Vec3 {
        Vec3::new(
            f32::from_bits(self.bits()),
            f32::from_bits(self.bits()),
            f32::from_bits(self.bits()),
        )
    }
}

#[test]
fn deterministic_random_finite_and_raw_bits_match_original_filter() {
    let mut random = Random(0x218c_f31b_72a5_640d);
    for index in 0..20000 {
        let (ray, min, max) = if index % 2 == 0 {
            let center = random.vector(20.);
            let half = random.vector(4.).abs() + Vec3::splat(0.001);
            let origin = center + random.vector(10.);
            let dir = if index % 4 == 0 {
                center - origin
            } else {
                random.vector(3.)
            };
            (Ray { origin, dir }, center - half, center + half)
        } else {
            // Includes unvalidated local arithmetic inputs, unordered bounds,
            // subnormals, infinities and NaNs; the original kernel is the oracle.
            (
                Ray {
                    origin: random.raw_vector(),
                    dir: random.raw_vector(),
                },
                random.raw_vector(),
                random.raw_vector(),
            )
        };
        assert_limits(ray, min, max);
        assert_equivalent(ray, min, max, f32::from_bits(random.bits()));
    }
}
