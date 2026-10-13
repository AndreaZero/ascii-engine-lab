//! R3 traversal is frozen independently of the optimized shadow queries.
//! Both paths use current R5 actor/materials: intentional artwork changes are
//! not a parity failure. The current compositor is used on both paths because
//! R5 deliberately fixes history invalidation for structural glyph changes.
pub use ascii_engine_lab::{actor, core};
#[allow(dead_code)]
#[path = "../evidence/r4/baseline/src/world.rs"]
mod r3_world;

use ascii_engine_lab::{
    camera::{Camera, Mode},
    core::{Hit, Ray, Scene, Stage, EMISSIVE},
    frame::Composer,
    LabScene,
};
use glam::Vec3;

struct Reference(r3_world::WorldSnapshot);
impl Scene for Reference {
    fn trace(&self, r: Ray, _: f32, _: Stage, w: bool) -> Option<Hit> {
        self.0.trace(r, w)
    }
    fn light_position(&self, _: f32) -> Vec3 {
        self.0.light_position()
    }
    fn pigment(&self, m: u32) -> Vec3 {
        ascii_engine_lab::world::pigment(m)
    }
    fn glyph(&self, m: u32, l: Vec3, n: Vec3, i: f32) -> u8 {
        ascii_engine_lab::world::glyph(m, l, n, i)
    }
    fn glyph_history_key(&self, m: u32, l: Vec3, n: Vec3) -> u32 {
        LabScene.glyph_history_key(m, l, n)
    }
}

#[test]
fn optimized_queries_preserve_r3_cells_and_history_across_views() {
    for mode in [Mode::First, Mode::Third, Mode::Overhead] {
        for stage in [Stage::Core, Stage::Light, Stage::Water, Stage::Cloth] {
            for workers in [1, 2, 8] {
                let mut old = Composer::with_threads(workers).unwrap();
                let mut new = Composer::with_threads(workers).unwrap();
                for (frame, t) in [0., 0.17, 0.5, 1., 1.6, 3.2].into_iter().enumerate() {
                    if frame == 4 {
                        old.reset();
                        new.reset();
                    }
                    let mut camera = Camera::new(mode);
                    camera.orbit = frame as f32 * 0.031;
                    camera.zoom = 0.85;
                    let reference = Reference(r3_world::WorldSnapshot::new(t, stage));
                    let (a, sa) = old.compose(&reference, camera, t, stage, 53, 31, 16. / 9.);
                    let (b, sb) = new.compose(
                        &LabScene::snapshot(t, stage),
                        camera,
                        t,
                        stage,
                        53,
                        31,
                        16. / 9.,
                    );
                    assert!(
                        bytemuck::cast_slice::<_, u8>(&a) == bytemuck::cast_slice::<_, u8>(&b),
                        "{} {} workers={workers} frame={frame}",
                        mode.name(),
                        stage.name()
                    );
                    assert_eq!(
                        (
                            sa.visible,
                            sa.reflected,
                            sa.matched,
                            sa.glyph_changes,
                            sa.mean_color_delta.to_bits()
                        ),
                        (
                            sb.visible,
                            sb.reflected,
                            sb.matched,
                            sb.glyph_changes,
                            sb.mean_color_delta.to_bits()
                        )
                    );
                }
            }
        }
    }
}

fn sample(seed: &mut u32) -> f32 {
    *seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
    (*seed >> 8) as f32 / 16777216.
}

#[test]
fn finite_shadow_queries_match_nearest_hit_including_distance_boundaries() {
    let mut seed = 19_u32;
    for stage in [Stage::Core, Stage::Light, Stage::Water, Stage::Cloth] {
        for time in [0., 1., 3.2] {
            let old = Reference(r3_world::WorldSnapshot::new(time, stage));
            let new = LabScene::snapshot(time, stage);
            for i in 0..2400 {
                let origin = Vec3::new(
                    sample(&mut seed) * 16. - 8.,
                    sample(&mut seed) * 5. - 0.5,
                    sample(&mut seed) * 16. - 8.,
                );
                let target = if i % 2 == 0 {
                    old.light_position(time)
                } else {
                    Vec3::new(
                        sample(&mut seed) * 8. - 4.,
                        sample(&mut seed) * 3.,
                        sample(&mut seed) * 8. - 4.,
                    )
                };
                let ray = Ray {
                    origin,
                    dir: (target - origin).normalize_or_zero(),
                };
                let hit = old.trace(ray, time, stage, false);
                let distance = hit.map_or(2., |h| h.distance);
                for limit in [
                    0.,
                    0.1,
                    0.7,
                    2.5,
                    12.,
                    distance,
                    distance - 0.00001,
                    distance + 0.00001,
                ] {
                    let expected =
                        hit.is_some_and(|h| h.distance < limit && h.material != EMISSIVE);
                    assert_eq!(
                        Scene::occluded(&new, ray, time, stage, limit),
                        expected,
                        "stage={stage:?} time={time} ray={i} limit={limit} hit={hit:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn reused_snapshot_keeps_requested_time_and_stage_for_shadows() {
    let reused = LabScene::snapshot(0., Stage::Core);
    for stage in [Stage::Core, Stage::Cloth] {
        let reference = Reference(r3_world::WorldSnapshot::new(2., stage));
        let mut a = Composer::default();
        let mut b = Composer::default();
        let camera = Camera::new(Mode::First);
        assert_eq!(
            a.compose(
                &reference,
                Camera::new(Mode::First),
                2.,
                stage,
                47,
                29,
                16. / 9.
            )
            .0,
            b.compose(&reused, camera, 2., stage, 47, 29, 16. / 9.).0
        );
    }
}
