//! Portable resource policy: a bounded starting budget, not a CPU speed test.
//! Quality stays explicit and fixed during a run; no hidden resolution changes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Profile {
    Economy,
    #[default]
    Balanced,
    Detail,
}
impl Profile {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "economy" => Some(Self::Economy),
            "balanced" => Some(Self::Balanced),
            "detail" => Some(Self::Detail),
            _ => None,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Economy => "economy",
            Self::Balanced => "balanced",
            Self::Detail => "detail",
        }
    }
    /// Eight output pixels per glyph by default, even in the dense preset.
    pub fn dimensions(self) -> (u32, u32, u32, u32) {
        match self {
            Self::Economy => (960, 544, 120, 68),
            Self::Balanced => (1280, 720, 160, 90),
            Self::Detail => (1920, 1080, 240, 135),
        }
    }
    pub fn worker_cap(self) -> usize {
        match self {
            Self::Economy => 2,
            Self::Balanced => 8,
            Self::Detail => 16,
        }
    }
    /// `available` is the runtime estimate, not the host's processor model.
    /// Auto leaves one logical slot outside this pool when possible. This is
    /// not core affinity, a physical-core reservation, or a frame-time promise.
    pub fn resolve_workers(
        self,
        available: usize,
        requested: Option<usize>,
    ) -> Result<usize, String> {
        let available = available.max(1);
        if let Some(count) = requested {
            if !(1..=64).contains(&count) {
                return Err("workers must be auto or 1..64".into());
            }
            if count > available {
                return Err(format!("requested {count} workers but runtime reports {available} available; use --workers auto or a smaller number"));
            }
            Ok(count)
        } else {
            Ok(available.saturating_sub(1).max(1).min(self.worker_cap()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn auto_budget_covers_small_and_large_runtime_capacities() {
        for available in [0, 1, 2, 4, 8, 16, 32, 64, 128] {
            for profile in [Profile::Economy, Profile::Balanced, Profile::Detail] {
                let n = profile.resolve_workers(available, None).unwrap();
                assert!(n >= 1 && n <= available.max(1) && n <= profile.worker_cap());
                if available > 1 {
                    assert!(n < available);
                }
            }
        }
        assert_eq!(Profile::Balanced.resolve_workers(16, None).unwrap(), 8);
        assert_eq!(Profile::Balanced.resolve_workers(2, None).unwrap(), 1);
        assert_eq!(Profile::Detail.resolve_workers(4, None).unwrap(), 3);
    }
    #[test]
    fn explicit_budget_respects_runtime_capacity() {
        assert_eq!(Profile::Economy.resolve_workers(16, Some(16)).unwrap(), 16);
        assert!(Profile::Detail.resolve_workers(2, Some(4)).is_err());
        assert!(Profile::Balanced.resolve_workers(16, Some(0)).is_err());
        assert!(Profile::Detail.resolve_workers(128, Some(65)).is_err());
    }
    #[test]
    fn profiles_change_density_without_changing_glyph_sampling_size() {
        for profile in [Profile::Economy, Profile::Balanced, Profile::Detail] {
            let (w, h, c, r) = profile.dimensions();
            assert_eq!((w / c, h / r), (8, 8));
        }
        assert!(Profile::Economy.dimensions().2 < Profile::Balanced.dimensions().2);
        assert!(Profile::Balanced.dimensions().2 < Profile::Detail.dimensions().2);
    }
}
