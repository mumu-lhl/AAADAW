use crate::ActionError;

/// Manual Item fade shapes, numbered as in REAPER's media Item API.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(u8)]
pub enum FadeShape {
    Linear = 0,
    #[default]
    FastStart = 1,
    SlowStart = 2,
    VeryFastStart = 3,
    VerySlowStart = 4,
    Smooth = 5,
    SteepSmooth = 6,
}

impl FadeShape {
    pub fn from_code(code: u8) -> Option<Self> {
        Some(match code {
            0 => Self::Linear,
            1 => Self::FastStart,
            2 => Self::SlowStart,
            3 => Self::VeryFastStart,
            4 => Self::VerySlowStart,
            5 => Self::Smooth,
            6 => Self::SteepSmooth,
            _ => return None,
        })
    }

    pub fn code(self) -> u8 {
        self as u8
    }

    /// Normalized fade-in gain; fade-out uses the remaining time instead.
    fn gain(self, x: f64) -> f64 {
        match self {
            Self::Linear => x,
            Self::FastStart => x * (2.0 - x),
            Self::SlowStart => x * x,
            Self::VeryFastStart => 1.0 - (1.0 - x).powi(4),
            Self::VerySlowStart => x.powi(4),
            Self::Smooth => x * x * (3.0 - 2.0 * x),
            Self::SteepSmooth if x < 0.5 => 8.0 * x.powi(4),
            Self::SteepSmooth => 1.0 - 8.0 * (1.0 - x).powi(4),
        }
    }
}

/// Continuous curvature and S controls introduced by REAPER 7.81.
/// These differ from compatibility presets: native S=0.5 is piecewise
/// quadratic, while the legacy Smooth preset remains cubic.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FadeCurveParameters {
    curvature: f64,
    s_parameter: f64,
}

// Validated finite values exclude NaN.
impl Eq for FadeCurveParameters {}

impl FadeCurveParameters {
    pub fn new(curvature: f64, s_parameter: f64) -> Result<Self, ActionError> {
        if !curvature.is_finite()
            || !s_parameter.is_finite()
            || !(-1.0..=1.0).contains(&curvature)
            || !(-1.0..=1.0).contains(&s_parameter)
        {
            return Err(ActionError::InvalidAudioItemFade);
        }
        Ok(Self {
            curvature,
            s_parameter,
        })
    }
    pub fn curvature(self) -> f64 {
        self.curvature
    }
    pub fn s_parameter(self) -> f64 {
        self.s_parameter
    }
    fn gain(self, x: f64) -> f64 {
        let x = if self.curvature < 0.0 {
            blend_power(x, -self.curvature)
        } else {
            1.0 - blend_power(1.0 - x, self.curvature)
        };
        if self.s_parameter >= 0.0 {
            if x < 0.5 {
                0.5 * blend_power(2.0 * x, self.s_parameter)
            } else {
                1.0 - 0.5 * blend_power(2.0 * (1.0 - x), self.s_parameter)
            }
        } else if x < 0.5 {
            0.5 * (1.0 - blend_power(1.0 - 2.0 * x, -self.s_parameter))
        } else {
            0.5 + 0.5 * blend_power(2.0 * x - 1.0, -self.s_parameter)
        }
    }
}

fn blend_power(x: f64, amount: f64) -> f64 {
    let squared = x * x;
    if amount <= 0.5 {
        x + (squared - x) * (amount * 2.0)
    } else {
        squared + (squared * squared - squared) * ((amount - 0.5) * 2.0)
    }
}

/// Preserve the rendering mode, not just approximate legacy shape codes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FadeCurve {
    Legacy(FadeShape),
    Native(FadeCurveParameters),
}

impl FadeCurve {
    /// Values shown by the current curve editor, retaining compatibility
    /// mode in `self` until the user actually changes either parameter.
    pub fn parameters(self) -> FadeCurveParameters {
        match self {
            Self::Native(parameters) => parameters,
            Self::Legacy(shape) => {
                let (curvature, s_parameter) = match shape {
                    FadeShape::Linear => (0.0, 0.0),
                    FadeShape::FastStart => (0.5, 0.0),
                    FadeShape::SlowStart => (-0.5, 0.0),
                    FadeShape::VeryFastStart => (1.0, 0.0),
                    FadeShape::VerySlowStart => (-1.0, 0.0),
                    FadeShape::Smooth => (0.0, 0.5),
                    FadeShape::SteepSmooth => (0.0, 1.0),
                };
                FadeCurveParameters {
                    curvature,
                    s_parameter,
                }
            }
        }
    }
    /// REAPER 7.82's curve menu preserves compatibility mode except for
    /// Smooth, which selects the current piecewise-quadratic S preset.
    pub fn current_preset(shape: FadeShape) -> Self {
        if shape == FadeShape::Smooth {
            Self::Native(FadeCurveParameters {
                curvature: 0.0,
                s_parameter: 0.5,
            })
        } else {
            Self::Legacy(shape)
        }
    }

    pub fn current_preset_shape(self) -> Option<FadeShape> {
        match self {
            Self::Legacy(shape) => Some(shape),
            Self::Native(parameters) => (0..=6).find_map(|code| {
                let shape = FadeShape::from_code(code)?;
                let expected = match shape {
                    FadeShape::Linear => (0.0, 0.0),
                    FadeShape::FastStart => (0.5, 0.0),
                    FadeShape::SlowStart => (-0.5, 0.0),
                    FadeShape::VeryFastStart => (1.0, 0.0),
                    FadeShape::VerySlowStart => (-1.0, 0.0),
                    FadeShape::Smooth => (0.0, 0.5),
                    FadeShape::SteepSmooth => (0.0, 1.0),
                };
                ((parameters.curvature(), parameters.s_parameter()) == expected).then_some(shape)
            }),
        }
    }

    fn gain(self, x: f64) -> f64 {
        match self {
            Self::Legacy(shape) => shape.gain(x),
            Self::Native(parameters) => parameters.gain(x),
        }
    }
}

/// A validated manual fade duration in fractional project sample frames.
/// Zero disables the fade without discarding its selected shape.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AudioFade {
    length_samples: f64,
    curve: FadeCurve,
}

// Construction excludes NaN, so equality is reflexive for every valid value.
impl Eq for AudioFade {}

impl Default for AudioFade {
    fn default() -> Self {
        Self {
            length_samples: 0.0,
            curve: FadeCurve::Legacy(FadeShape::default()),
        }
    }
}

impl AudioFade {
    pub fn new(length_samples: f64, shape: FadeShape) -> Result<Self, ActionError> {
        Self::with_curve(length_samples, FadeCurve::Legacy(shape))
    }

    pub fn with_curve(length_samples: f64, curve: FadeCurve) -> Result<Self, ActionError> {
        if !length_samples.is_finite() || length_samples < 0.0 {
            return Err(ActionError::InvalidAudioItemFade);
        }
        Ok(Self {
            length_samples,
            curve,
        })
    }

    pub fn length_samples(self) -> f64 {
        self.length_samples
    }
    pub fn curve(self) -> FadeCurve {
        self.curve
    }

    /// Samples the curve for visual feedback, clamping to its unit interval.
    pub fn gain_at_progress(self, progress: f64) -> f32 {
        if !progress.is_finite() {
            return 0.0;
        }
        self.curve.gain(progress.clamp(0.0, 1.0)) as f32
    }
}

/// Manual fades retain requested lengths even when they exceed the Item.
/// Rendering gives fade-in priority, shortening fade-out to the remaining
/// duration. This follows measured REAPER 7.82 behavior; overlapping manual
/// fades do not multiply together.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AudioItemFades {
    pub fade_in: AudioFade,
    pub fade_out: AudioFade,
}

impl AudioItemFades {
    /// Returns the gain at an Item-relative frame, independently of source offset.
    pub fn gain_at(self, offset_samples: u64, item_length_samples: u64) -> f32 {
        if offset_samples >= item_length_samples {
            return 0.0;
        }
        let length = item_length_samples as f64;
        let offset = offset_samples as f64;
        let fade_in = self.fade_in.length_samples.min(length);
        let fade_out = self.fade_out.length_samples.min(length - fade_in);
        if fade_in > 0.0 && offset < fade_in {
            self.fade_in.curve.gain(offset / fade_in) as f32
        } else if fade_out > 0.0 && length - offset < fade_out {
            self.fade_out.curve.gain((length - offset) / fade_out) as f32
        } else {
            1.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fades(shape: FadeShape, fade_in: f64, fade_out: f64) -> AudioItemFades {
        AudioItemFades {
            fade_in: AudioFade::new(fade_in, shape).unwrap(),
            fade_out: AudioFade::new(fade_out, shape).unwrap(),
        }
    }

    #[test]
    fn seven_shapes_match_native_pcm_measurements() {
        // Normalized 24-bit PCM renders of a constant 0.125 signal, 48 kHz,
        // 12,000-frame fades. Tolerance accounts for one PCM quantization step.
        let measured = [
            [0.125, 0.25, 0.5, 0.75],
            [0.234375, 0.4375, 0.75, 0.9375],
            [0.015625, 0.0625, 0.25, 0.5625],
            [0.4138184, 0.6835938, 0.9375, 0.9960938],
            [0.0002441, 0.0039062, 0.0625, 0.3164062],
            [0.0429688, 0.15625, 0.5, 0.84375],
            [0.0019531, 0.03125, 0.5, 0.96875],
        ];
        for (code, expected) in measured.into_iter().enumerate() {
            let params = fades(
                FadeShape::from_code(code as u8).unwrap(),
                12_000.0,
                12_000.0,
            );
            for (offset, gain) in [1500, 3000, 6000, 9000].into_iter().zip(expected) {
                assert!((params.gain_at(offset, 48_000) - gain).abs() < 0.000001);
                assert!((params.gain_at(48_000 - offset, 48_000) - gain).abs() < 0.000001);
            }
            assert_eq!(params.gain_at(0, 48_000), 0.0);
            assert_eq!(params.gain_at(12_000, 48_000), 1.0);
            assert_eq!(params.gain_at(36_000, 48_000), 1.0);
            assert_eq!(params.gain_at(48_000, 48_000), 0.0);
        }
    }

    #[test]
    fn overlapping_and_oversized_fades_follow_native_in_priority() {
        let overlap = fades(FadeShape::Linear, 36_000.0, 36_000.0);
        assert_eq!(overlap.gain_at(24_000, 48_000), 2.0 / 3.0);
        assert_eq!(overlap.gain_at(36_000, 48_000), 1.0);
        assert_eq!(overlap.gain_at(42_000, 48_000), 0.5);
        assert_eq!(overlap.fade_out.length_samples(), 36_000.0);
        let oversized = fades(FadeShape::Linear, 96_000.0, 96_000.0);
        assert_eq!(oversized.gain_at(24_000, 48_000), 0.5);
        assert_eq!(oversized.gain_at(47_999, 48_000), 47_999.0 / 48_000.0);
        assert_eq!(
            fades(FadeShape::Linear, 0.0, 96_000.0).gain_at(12_000, 48_000),
            0.75
        );
    }

    #[test]
    fn disabled_fractional_and_invalid_lengths_are_handled() {
        assert_eq!(AudioItemFades::default().gain_at(0, 48_000), 1.0);
        assert_eq!(AudioItemFades::default().gain_at(47_999, 48_000), 1.0);
        assert_eq!(
            AudioFade::default().curve(),
            FadeCurve::Legacy(FadeShape::FastStart)
        );
        assert_eq!(fades(FadeShape::Linear, 2.5, 0.0).gain_at(1, 48_000), 0.4);
        for invalid in [-1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(
                AudioFade::new(invalid, FadeShape::Linear),
                Err(ActionError::InvalidAudioItemFade)
            );
        }
        assert!(FadeShape::from_code(7).is_none());
    }
}
