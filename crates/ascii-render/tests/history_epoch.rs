//! Event-boundary history acceptance, independent of client transaction storage.
//! The same analytic plane and surface identity deliberately survive each edit;
//! only the epoch can distinguish replacement content from a held glyph.
use ascii_render::{
    camera::{Camera, CameraView, Mode},
    core::{GlyphCell, Hit, Ray, Scene, Stage},
    frame::{Composer, FrameStats, PrimarySampling},
};
use glam::Vec3;
use std::sync::atomic::{AtomicUsize, Ordering};

const SAMPLING: [PrimarySampling; 2] = [PrimarySampling::Center, PrimarySampling::Conservative9];
const COLS: u32 = 19;
const ROWS: u32 = 9;

#[derive(Clone, Copy)]
struct Probe {
    glyph: Option<u8>,
    brightness: f32,
}
impl Probe {
    fn fixed(glyph: u8) -> Self {
        Self {
            glyph: Some(glyph),
            brightness: 0.66,
        }
    }
}
impl Scene for Probe {
    fn trace(&self, ray: Ray, _: f32, _: Stage, _: bool) -> Option<Hit> {
        let distance = -ray.origin.z / ray.dir.z;
        if !distance.is_finite() || distance <= 0.0001 {
            return None;
        }
        Some(Hit {
            distance,
            position: ray.origin + ray.dir * distance,
            normal: Vec3::Z,
            material: 32,
            object_id: 130,
            // A fixed authored region isolates history lifetime from bin changes.
            local: Vec3::new(0.54, 0.60, 0.),
        })
    }
    fn light_position(&self, _: f32) -> Vec3 {
        Vec3::Y
    }
    fn pigment(&self, _: u32) -> Vec3 {
        Vec3::splat(self.brightness)
    }
    fn glyph(&self, _: u32, _: Vec3, _: Vec3, illumination: f32) -> u8 {
        self.glyph
            .unwrap_or(if illumination < 0.12 { b',' } else { b':' })
    }
}

struct EpochProbe {
    inner: Probe,
    epoch: u64,
    reads: AtomicUsize,
}
impl EpochProbe {
    fn new(epoch: u64, glyph: u8) -> Self {
        Self {
            inner: Probe::fixed(glyph),
            epoch,
            reads: AtomicUsize::new(0),
        }
    }
}
impl Scene for EpochProbe {
    fn history_epoch(&self) -> u64 {
        self.reads.fetch_add(1, Ordering::Relaxed);
        self.epoch
    }
    fn trace(&self, ray: Ray, time: f32, stage: Stage, water: bool) -> Option<Hit> {
        assert!(
            self.reads.load(Ordering::Relaxed) > 0,
            "epoch must be read before composition queries begin"
        );
        self.inner.trace(ray, time, stage, water)
    }
    fn light_position(&self, time: f32) -> Vec3 {
        self.inner.light_position(time)
    }
    fn pigment(&self, material: u32) -> Vec3 {
        self.inner.pigment(material)
    }
    fn glyph(&self, material: u32, local: Vec3, normal: Vec3, illumination: f32) -> u8 {
        self.inner.glyph(material, local, normal, illumination)
    }
}

fn camera() -> Camera {
    Camera::with_view(
        Mode::Third,
        CameraView::new(Vec3::new(0., 0., 4.), Vec3::ZERO, 70., 1.).unwrap(),
    )
}

fn composer(workers: usize, sampling: PrimarySampling) -> Composer {
    let mut composer = Composer::with_threads(workers).unwrap();
    composer.set_primary_sampling(sampling);
    composer
}

fn render(composer: &mut Composer, scene: &(impl Scene + Sync)) -> (Vec<GlyphCell>, FrameStats) {
    composer.compose(scene, camera(), 0., Stage::Core, COLS, ROWS, 1.5)
}

fn render_counted(composer: &mut Composer, scene: &EpochProbe) -> (Vec<GlyphCell>, FrameStats) {
    let before = scene.reads.load(Ordering::Relaxed);
    let frame = render(composer, scene);
    assert_eq!(
        scene.reads.load(Ordering::Relaxed),
        before + 1,
        "one epoch read per frame, independent of cells, rays and workers"
    );
    frame
}

fn assert_frame_bits(
    actual: &(Vec<GlyphCell>, FrameStats),
    expected: &(Vec<GlyphCell>, FrameStats),
) {
    assert_eq!(
        bytemuck::cast_slice::<GlyphCell, u8>(&actual.0),
        bytemuck::cast_slice::<GlyphCell, u8>(&expected.0)
    );
    let (a, b) = (&actual.1, &expected.1);
    assert_eq!(a.visible, b.visible);
    assert_eq!(a.reflected, b.reflected);
    assert_eq!(a.matched, b.matched);
    assert_eq!(a.glyph_changes, b.glyph_changes);
    assert_eq!(a.mean_color_delta.to_bits(), b.mean_color_delta.to_bits());
    assert_eq!(a.primary_rays, b.primary_rays);
    assert_eq!(a.alternate_primary, b.alternate_primary);
}

fn assert_glyphs(frame: &(Vec<GlyphCell>, FrameStats), glyph: u8) {
    assert_eq!(frame.0.len(), (COLS * ROWS) as usize);
    assert!(frame.0.iter().all(|cell| cell.glyph == u32::from(glyph)));
}

#[test]
fn replacement_in_the_same_surface_bin_requires_an_epoch_or_explicit_reset() {
    for sampling in SAMPLING {
        for workers in [1, 8] {
            let before = Probe::fixed(b'A');
            let after = Probe::fixed(b'B');
            assert_eq!(
                before.history_epoch(),
                0,
                "ordinary scenes retain default zero"
            );
            let mut history = composer(workers, sampling);
            let first = render(&mut history, &before);
            let held = render(&mut history, &after);
            let fresh = render(&mut composer(workers, sampling), &after);
            assert_glyphs(&first, b'A');
            assert_glyphs(&held, b'A');
            assert_glyphs(&fresh, b'B');
            assert_eq!(held.1.matched, (COLS * ROWS) as usize);
            assert_eq!(held.1.glyph_changes, 0);
            for (old, new) in first.0.iter().zip(&fresh.0) {
                assert_eq!(old.id, new.id);
                assert_eq!(old.color.map(f32::to_bits), new.color.map(f32::to_bits));
            }
            history.reset();
            assert_frame_bits(&render(&mut history, &after), &fresh);
        }
    }
}

#[test]
fn epoch_switches_include_zero_and_a_retained_older_snapshot() {
    for sampling in SAMPLING {
        let mut serial = composer(1, sampling);
        let mut parallel = composer(8, sampling);
        let original = EpochProbe::new(17, b'B');
        let newer = EpochProbe::new(23, b'D');
        let zero_a = EpochProbe::new(0, b'A');
        let zero_c = EpochProbe::new(0, b'C');
        let same_epoch_c = EpochProbe::new(17, b'C');
        let same_epoch_e = EpochProbe::new(23, b'E');
        // Reuse the actual old snapshot, not merely a freshly manufactured token.
        let cases = [
            (&zero_a, b'A', true),
            (&zero_c, b'A', false),
            (&original, b'B', true),
            (&same_epoch_c, b'B', false),
            (&newer, b'D', true),
            (&same_epoch_e, b'D', false),
            (&original, b'B', true),
            (&zero_c, b'C', true),
            (&zero_a, b'C', false),
        ];
        for (scene, expected_glyph, invalidated) in cases {
            let a = render_counted(&mut serial, scene);
            let b = render_counted(&mut parallel, scene);
            assert_frame_bits(&a, &b);
            assert_glyphs(&a, expected_glyph);
            assert_eq!(
                a.1.matched,
                if invalidated {
                    0
                } else {
                    (COLS * ROWS) as usize
                }
            );
            if invalidated {
                let fresh = render(&mut composer(1, sampling), &scene.inner);
                assert_frame_bits(&a, &fresh);
            }
        }
    }
}

#[test]
fn stable_epoch_preserves_lighting_hysteresis_and_legacy_bits() {
    for sampling in SAMPLING {
        for workers in [1, 8] {
            let mut legacy = composer(workers, sampling);
            let mut opted_in = composer(workers, sampling);
            let mut frames = Vec::new();
            for brightness in [0.66, 0.66, 0.74] {
                let inner = Probe {
                    glyph: None,
                    brightness,
                };
                let scene = EpochProbe {
                    inner,
                    epoch: 41,
                    reads: AtomicUsize::new(0),
                };
                let expected = render(&mut legacy, &inner);
                let actual = render_counted(&mut opted_in, &scene);
                assert_frame_bits(&actual, &expected);
                frames.push(actual);
            }
            assert_glyphs(&frames[0], b',');
            assert_glyphs(&frames[2], b',');
            assert_eq!(frames[2].1.glyph_changes, 0);
            let fresh = render(
                &mut composer(workers, sampling),
                &Probe {
                    glyph: None,
                    brightness: 0.74,
                },
            );
            assert_glyphs(&fresh, b':');
        }
    }
}

#[test]
fn explicit_reset_and_policy_changes_work_with_an_unchanged_epoch() {
    for sampling in SAMPLING {
        for workers in [1, 8] {
            let mut history = composer(workers, sampling);
            let a = EpochProbe::new(47, b'A');
            let b = EpochProbe::new(47, b'B');
            let c = EpochProbe::new(47, b'C');
            render_counted(&mut history, &a);
            assert_glyphs(&render_counted(&mut history, &b), b'A');

            history.set_primary_sampling(sampling);
            let same_policy = render_counted(&mut history, &b);
            assert_glyphs(&same_policy, b'A');
            assert_eq!(same_policy.1.matched, (COLS * ROWS) as usize);

            let other = match sampling {
                PrimarySampling::Center => PrimarySampling::Conservative9,
                PrimarySampling::Conservative9 => PrimarySampling::Center,
            };
            history.set_primary_sampling(other);
            let switched = render_counted(&mut history, &b);
            let fresh_b = render(&mut composer(workers, other), &b.inner);
            assert_frame_bits(&switched, &fresh_b);
            assert_glyphs(&render_counted(&mut history, &c), b'B');

            history.reset();
            let reset = render_counted(&mut history, &c);
            let fresh_c = render(&mut composer(workers, other), &c.inner);
            assert_frame_bits(&reset, &fresh_c);
            assert_eq!(history.threads(), workers);
            assert_eq!(history.primary_sampling(), other);
            let following = render_counted(&mut history, &c);
            assert_glyphs(&following, b'C');
            assert_eq!(following.1.matched, (COLS * ROWS) as usize);
        }
    }
}
