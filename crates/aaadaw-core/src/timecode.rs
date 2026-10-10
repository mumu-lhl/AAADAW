use crate::TimebaseError;

/// The factory video/timecode frame-rate presets in the fixed Linux reference.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum FrameRate {
    Fps23976,
    Fps24,
    Fps25,
    Fps2997Drop,
    Fps2997,
    #[default]
    Fps30,
    Fps48,
    Fps50,
    Fps60,
    Fps75,
}

impl FrameRate {
    /// All supported presets in menu order.
    pub const ALL: [Self; 10] = [
        Self::Fps23976,
        Self::Fps24,
        Self::Fps25,
        Self::Fps2997Drop,
        Self::Fps2997,
        Self::Fps30,
        Self::Fps48,
        Self::Fps50,
        Self::Fps60,
        Self::Fps75,
    ];

    /// Stable storage code; distinct drop/non-drop modes never alias.
    pub fn storage_code(self) -> u32 {
        Self::ALL
            .iter()
            .position(|rate| *rate == self)
            .expect("supported preset") as u32
    }

    /// Decodes a stored preset without silently replacing unknown settings.
    pub fn from_storage_code(code: u32) -> Option<Self> {
        Self::ALL.get(code as usize).copied()
    }

    /// Exact frames-per-second numerator and denominator.
    pub fn ratio(self) -> (u32, u32) {
        match self {
            Self::Fps23976 => (24000, 1001),
            Self::Fps2997Drop | Self::Fps2997 => (30000, 1001),
            Self::Fps24 => (24, 1),
            Self::Fps25 => (25, 1),
            Self::Fps30 => (30, 1),
            Self::Fps48 => (48, 1),
            Self::Fps50 => (50, 1),
            Self::Fps60 => (60, 1),
            Self::Fps75 => (75, 1),
        }
    }

    /// Whether the timecode skips two labels each non-tenth minute.
    pub fn is_drop_frame(self) -> bool {
        self == Self::Fps2997Drop
    }

    /// Returns the actual frame rate, including fractional presets.
    pub fn frames_per_second(self) -> f64 {
        let (numerator, denominator) = self.ratio();
        f64::from(numerator) / f64::from(denominator)
    }

    fn nominal(self) -> u64 {
        self.frames_per_second().round() as u64
    }

    /// Formats nonnegative seconds, retaining the frame-floor behavior.
    pub fn format_timecode(self, seconds: f64) -> Result<String, TimebaseError> {
        let frame = ((seconds + 1e-8) * self.frames_per_second()).floor();
        if !seconds.is_finite() || seconds < 0.0 || !(0.0..=(1_u64 << 53) as f64).contains(&frame) {
            return Err(TimebaseError::PositionOutOfRange);
        }
        let mut frame = frame as u64;
        if self.is_drop_frame() {
            let cycles = frame / 17982;
            let remainder = frame % 17982;
            frame += 18 * cycles + 2 * (remainder.saturating_sub(2) / 1798);
        }
        let nominal = self.nominal();
        let seconds = frame / nominal;
        let hours = seconds / 3600;
        let suffix = format!(
            ":{:02}:{:02}:{:02}",
            seconds / 60 % 60,
            seconds % 60,
            frame % nominal
        );
        Ok(if self.is_drop_frame() {
            format!("{hours}{suffix}")
        } else {
            format!("{hours:02}{suffix}")
        })
    }

    /// Parses canonical H:M:S:F; malformed or skipped drop-frame labels fail.
    pub fn parse_timecode(self, text: &str) -> Result<f64, TimebaseError> {
        let fields = text
            .trim()
            .split(':')
            .map(str::parse::<u64>)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| TimebaseError::PositionOutOfRange)?;
        let [hours, minutes, seconds, frames] = fields.as_slice() else {
            return Err(TimebaseError::PositionOutOfRange);
        };
        let nominal = self.nominal();
        if *minutes >= 60
            || *seconds >= 60
            || *frames >= nominal
            || (self.is_drop_frame() && minutes % 10 != 0 && *seconds == 0 && *frames < 2)
        {
            return Err(TimebaseError::PositionOutOfRange);
        }
        let total_minutes = hours
            .checked_mul(60)
            .and_then(|n| n.checked_add(*minutes))
            .ok_or(TimebaseError::PositionOutOfRange)?;
        let mut frame = total_minutes
            .checked_mul(60)
            .and_then(|n| n.checked_add(*seconds))
            .and_then(|n| n.checked_mul(nominal))
            .and_then(|n| n.checked_add(*frames))
            .ok_or(TimebaseError::PositionOutOfRange)?;
        if self.is_drop_frame() {
            frame = frame
                .checked_sub(2 * (total_minutes - total_minutes / 10))
                .ok_or(TimebaseError::PositionOutOfRange)?;
        }
        if frame > 1_u64 << 53 {
            return Err(TimebaseError::PositionOutOfRange);
        }
        Ok(frame as f64 / self.frames_per_second())
    }
}

impl std::fmt::Display for FrameRate {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Fps23976 => "23.976",
            Self::Fps24 => "24",
            Self::Fps25 => "25",
            Self::Fps2997Drop => "29.97DF",
            Self::Fps2997 => "29.97ND",
            Self::Fps30 => "30",
            Self::Fps48 => "48",
            Self::Fps50 => "50",
            Self::Fps60 => "60",
            Self::Fps75 => "75",
        })
    }
}
