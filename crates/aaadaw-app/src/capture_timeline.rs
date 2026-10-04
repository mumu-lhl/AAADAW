/// Maps a backend capture-frame clock to the project's sample clock.
///
/// The anchor pairs the backend frame position with the project sample at one instant. The two
/// sample rates may differ; frame positions are converted with symmetric nearest-sample rounding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CaptureTimelineAnchor {
    capture_frame: u64,
    project_sample: u64,
    capture_sample_rate: u32,
    project_sample_rate: u32,
}

impl CaptureTimelineAnchor {
    /// Creates an anchor when both clock rates are valid.
    pub fn new(
        capture_frame: u64,
        project_sample: u64,
        capture_sample_rate: u32,
        project_sample_rate: u32,
    ) -> Option<Self> {
        (capture_sample_rate > 0 && project_sample_rate > 0).then_some(Self {
            capture_frame,
            project_sample,
            capture_sample_rate,
            project_sample_rate,
        })
    }

    /// Converts a backend frame position to the project sample clock.
    ///
    /// Returns `None` when checked arithmetic fails or the mapped sample is outside the project
    /// timeline's unsigned sample range.
    pub fn project_sample_at(self, capture_frame: u64) -> Option<u64> {
        let frame_delta = i128::from(capture_frame) - i128::from(self.capture_frame);
        let scaled_delta = frame_delta.checked_mul(i128::from(self.project_sample_rate))?;
        let half_capture_rate = i128::from(self.capture_sample_rate) / 2;
        let rounded_delta = if scaled_delta < 0 {
            scaled_delta.checked_sub(half_capture_rate)?
        } else {
            scaled_delta.checked_add(half_capture_rate)?
        } / i128::from(self.capture_sample_rate);
        u64::try_from(i128::from(self.project_sample).checked_add(rounded_delta)?).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::CaptureTimelineAnchor;

    #[test]
    fn maps_jack_frames_around_the_anchor_at_common_rates() {
        for sample_rate in [44_100, 48_000] {
            let anchor = CaptureTimelineAnchor::new(10_000, 96_000, sample_rate, sample_rate)
                .expect("sample rates should be valid");
            assert_eq!(anchor.project_sample_at(10_000), Some(96_000));
            assert_eq!(anchor.project_sample_at(10_256), Some(96_256));
            assert_eq!(anchor.project_sample_at(9_872), Some(95_872));
        }
    }

    #[test]
    fn converts_clock_rates_with_symmetric_nearest_sample_rounding() {
        let anchor = CaptureTimelineAnchor::new(0, 100, 44_100, 48_000)
            .expect("sample rates should be valid");
        assert_eq!(anchor.project_sample_at(1), Some(101));
        assert_eq!(anchor.project_sample_at(44_100), Some(48_100));
        assert_eq!(anchor.project_sample_at(u64::MAX), None);

        let reverse = CaptureTimelineAnchor::new(10, 100, 48_000, 44_100)
            .expect("sample rates should be valid");
        assert_eq!(reverse.project_sample_at(11), Some(101));
        assert_eq!(reverse.project_sample_at(9), Some(99));
    }

    #[test]
    fn rejects_invalid_rates_underflow_and_overflow() {
        assert!(CaptureTimelineAnchor::new(0, 0, 0, 48_000).is_none());
        assert!(CaptureTimelineAnchor::new(0, 0, 48_000, 0).is_none());

        let underflow = CaptureTimelineAnchor::new(100, 1, 48_000, 48_000)
            .expect("sample rates should be valid");
        assert_eq!(underflow.project_sample_at(99), Some(0));
        assert_eq!(underflow.project_sample_at(98), None);

        let overflow = CaptureTimelineAnchor::new(0, u64::MAX, 48_000, 48_000)
            .expect("sample rates should be valid");
        assert_eq!(overflow.project_sample_at(0), Some(u64::MAX));
        assert_eq!(overflow.project_sample_at(1), None);
    }
}
