use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

/// Default final-output sample-peak ceiling in dBFS.
pub const MASTER_OUTPUT_DEFAULT_CEILING_DBFS: i8 = -1;

/// A validated integer dBFS ceiling for the final Master sample-peak guard.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MasterOutputCeiling(i8);

impl MasterOutputCeiling {
    /// Creates a ceiling from -12 through 0 dBFS.
    pub fn new(dbfs: i8) -> Result<Self, MasterOutputSafetyError> {
        (-12..=0)
            .contains(&dbfs)
            .then_some(Self(dbfs))
            .ok_or(MasterOutputSafetyError)
    }

    /// Returns this ceiling's dBFS value.
    pub const fn as_dbfs(self) -> i8 {
        self.0
    }

    fn linear_amplitude(self) -> f32 {
        10.0_f32.powf(f32::from(self.0) / 20.0)
    }
}

impl Default for MasterOutputCeiling {
    fn default() -> Self {
        Self(MASTER_OUTPUT_DEFAULT_CEILING_DBFS)
    }
}

impl std::fmt::Display for MasterOutputCeiling {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{} dBFS", self.0)
    }
}

/// Invalid ceiling values are rejected before they can reach the audio callback.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MasterOutputSafetyError;

impl std::fmt::Display for MasterOutputSafetyError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Master sample-peak ceiling must be between -12 and 0 dBFS")
    }
}

impl std::error::Error for MasterOutputSafetyError {}

/// A control-thread handle for the graph's final sample-peak ceiling.
///
/// Updates are one validated atomic store, so the audio callback sees either the old or new
/// ceiling without locking or rebuilding the render graph.
#[derive(Clone, Debug)]
pub struct MasterOutputSafetyController {
    ceiling_linear: Arc<AtomicU32>,
}

impl MasterOutputSafetyController {
    /// Sets the final sample-peak ceiling. The supported range is -12 through 0 dBFS.
    pub fn set_ceiling(&self, ceiling: MasterOutputCeiling) {
        self.ceiling_linear
            .store(ceiling.linear_amplitude().to_bits(), Ordering::Relaxed);
    }

    /// Returns the currently active ceiling.
    pub fn ceiling(&self) -> MasterOutputCeiling {
        let linear = f32::from_bits(self.ceiling_linear.load(Ordering::Relaxed));
        let dbfs = (20.0 * linear.log10()).round() as i8;
        MasterOutputCeiling::new(dbfs).expect("controller stores only validated ceilings")
    }
}

#[derive(Debug)]
pub(super) struct MasterOutputSafety {
    ceiling_linear: Arc<AtomicU32>,
}

impl Default for MasterOutputSafety {
    fn default() -> Self {
        Self {
            ceiling_linear: Arc::new(AtomicU32::new(
                MasterOutputCeiling::default().linear_amplitude().to_bits(),
            )),
        }
    }
}

impl MasterOutputSafety {
    pub(super) fn controller(&self) -> MasterOutputSafetyController {
        MasterOutputSafetyController {
            ceiling_linear: Arc::clone(&self.ceiling_linear),
        }
    }

    pub(super) fn process(&self, output: &mut [[f32; 2]]) -> MasterOutputGuardStats {
        let ceiling = f32::from_bits(self.ceiling_linear.load(Ordering::Relaxed));
        let mut stats = MasterOutputGuardStats::default();
        for sample in output.iter_mut().flatten() {
            if !sample.is_finite() {
                *sample = 0.0;
                stats.non_finite_samples += 1;
            } else if *sample > ceiling {
                *sample = ceiling;
                stats.guarded_samples += 1;
            } else if *sample < -ceiling {
                *sample = -ceiling;
                stats.guarded_samples += 1;
            }
        }
        stats
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct MasterOutputGuardStats {
    pub(super) guarded_samples: usize,
    pub(super) non_finite_samples: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guard_preserves_audio_below_ceiling_and_clamps_both_polarities() {
        let safety = MasterOutputSafety::default();
        let mut output = [[0.5, -0.5], [1.0, -1.0]];
        let stats = safety.process(&mut output);
        let ceiling = 10.0_f32.powf(-1.0 / 20.0);

        assert_eq!(stats.guarded_samples, 2);
        assert_eq!(stats.non_finite_samples, 0);
        assert_eq!(output[0], [0.5, -0.5]);
        assert_eq!(output[1], [ceiling, -ceiling]);
    }

    #[test]
    fn guard_silences_non_finite_samples_and_counts_them_separately() {
        let safety = MasterOutputSafety::default();
        let mut output = [[f32::NAN, f32::INFINITY], [f32::NEG_INFINITY, 0.25]];

        let stats = safety.process(&mut output);

        assert_eq!(stats.guarded_samples, 0);
        assert_eq!(stats.non_finite_samples, 3);
        assert_eq!(output, [[0.0, 0.0], [0.0, 0.25]]);
    }

    #[test]
    fn controller_updates_ceiling_and_rejects_values_outside_the_supported_range() {
        let safety = MasterOutputSafety::default();
        let controller = safety.controller();
        let mut output = [[0.8, -0.8]];
        assert_eq!(safety.process(&mut output).guarded_samples, 0);

        controller.set_ceiling(MasterOutputCeiling::new(-6).unwrap());
        assert_eq!(controller.ceiling(), MasterOutputCeiling::new(-6).unwrap());
        let stats = safety.process(&mut output);
        let ceiling = 10.0_f32.powf(-6.0 / 20.0);
        assert_eq!(stats.guarded_samples, 2);
        assert_eq!(output, [[ceiling, -ceiling]]);

        assert!(MasterOutputCeiling::new(-13).is_err());
        assert!(MasterOutputCeiling::new(1).is_err());
        assert_eq!(controller.ceiling(), MasterOutputCeiling::new(-6).unwrap());
    }
}
