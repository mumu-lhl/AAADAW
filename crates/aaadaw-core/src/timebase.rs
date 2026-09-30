use std::fmt;

/// Default project sample rate in samples per second.
pub const DEFAULT_SAMPLE_RATE: u32 = 48_000;
/// Default musical resolution in ticks per quarter note.
pub const DEFAULT_PPQ: u32 = 960;
/// Default project tempo in quarter notes per minute.
pub const DEFAULT_TEMPO_BPM: f64 = 120.0;

/// Immutable construction settings for a project's timebase.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProjectSettings {
    sample_rate: u32,
    ppq: u32,
    initial_tempo_bpm: f64,
}

impl ProjectSettings {
    /// Creates settings after validating the sample rate, PPQ resolution, and tempo.
    pub fn new(sample_rate: u32, ppq: u32, initial_tempo_bpm: f64) -> Result<Self, TimebaseError> {
        if sample_rate == 0 {
            return Err(TimebaseError::InvalidSampleRate);
        }
        if ppq == 0 {
            return Err(TimebaseError::InvalidPpq);
        }
        if !is_valid_tempo_for(sample_rate, ppq, initial_tempo_bpm) {
            return Err(TimebaseError::InvalidTempo);
        }
        Ok(Self {
            sample_rate,
            ppq,
            initial_tempo_bpm,
        })
    }

    /// Returns the project sample rate.
    pub fn sample_rate(self) -> u32 {
        self.sample_rate
    }

    /// Returns ticks per quarter note.
    pub fn ppq(self) -> u32 {
        self.ppq
    }

    /// Returns the initial tempo in quarter notes per minute.
    pub fn initial_tempo_bpm(self) -> f64 {
        self.initial_tempo_bpm
    }
}

impl Default for ProjectSettings {
    fn default() -> Self {
        Self {
            sample_rate: DEFAULT_SAMPLE_RATE,
            ppq: DEFAULT_PPQ,
            initial_tempo_bpm: DEFAULT_TEMPO_BPM,
        }
    }
}

/// An invalid timebase setting or a position that cannot be represented.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimebaseError {
    /// Sample rate must be greater than zero.
    InvalidSampleRate,
    /// PPQ resolution must be greater than zero.
    InvalidPpq,
    /// Tempo must be finite and greater than zero.
    InvalidTempo,
    /// Tick/sample position exceeds the supported range.
    PositionOutOfRange,
    /// The initial tempo point at tick zero cannot be removed.
    CannotRemoveInitialTempo,
    /// A tempo point expected to exist is missing.
    TempoPointNotFound,
}

impl fmt::Display for TimebaseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSampleRate => formatter.write_str("sample rate must be greater than zero"),
            Self::InvalidPpq => formatter.write_str("PPQ must be greater than zero"),
            Self::InvalidTempo => formatter.write_str("tempo must be finite and greater than zero"),
            Self::PositionOutOfRange => formatter.write_str("timebase position is out of range"),
            Self::CannotRemoveInitialTempo => {
                formatter.write_str("the initial tempo point cannot be removed")
            }
            Self::TempoPointNotFound => formatter.write_str("tempo point does not exist"),
        }
    }
}

impl std::error::Error for TimebaseError {}

#[derive(Clone, Copy, Debug, PartialEq)]
struct TempoPoint {
    start_tick: u64,
    bpm: f64,
    start_sample: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TempoMap {
    sample_rate: u32,
    ppq: u32,
    points: Vec<TempoPoint>,
}

impl TempoMap {
    pub(crate) fn new(settings: ProjectSettings) -> Self {
        Self {
            sample_rate: settings.sample_rate,
            ppq: settings.ppq,
            points: vec![TempoPoint {
                start_tick: 0,
                bpm: settings.initial_tempo_bpm,
                start_sample: 0.0,
            }],
        }
    }

    pub(crate) fn settings(&self) -> ProjectSettings {
        ProjectSettings {
            sample_rate: self.sample_rate,
            ppq: self.ppq,
            initial_tempo_bpm: self.points[0].bpm,
        }
    }

    pub(crate) fn point_at(&self, start_tick: u64) -> Option<f64> {
        self.points
            .binary_search_by_key(&start_tick, |point| point.start_tick)
            .ok()
            .map(|index| self.points[index].bpm)
    }

    pub(crate) fn set_point(
        &mut self,
        start_tick: u64,
        bpm: Option<f64>,
    ) -> Result<(), TimebaseError> {
        if bpm.is_some_and(|value| !is_valid_tempo_for(self.sample_rate, self.ppq, value)) {
            return Err(TimebaseError::InvalidTempo);
        }
        if start_tick == 0 && bpm.is_none() {
            return Err(TimebaseError::CannotRemoveInitialTempo);
        }

        let mut points = self.points.clone();
        match (
            points.binary_search_by_key(&start_tick, |point| point.start_tick),
            bpm,
        ) {
            (Ok(index), Some(bpm)) => points[index].bpm = bpm,
            (Ok(index), None) => {
                points.remove(index);
            }
            (Err(index), Some(bpm)) => points.insert(
                index,
                TempoPoint {
                    start_tick,
                    bpm,
                    start_sample: 0.0,
                },
            ),
            (Err(_), None) => return Err(TimebaseError::TempoPointNotFound),
        }
        Self::recalculate_sample_anchors(&mut points, self.sample_rate, self.ppq)?;
        self.points = points;
        Ok(())
    }

    pub(crate) fn tempo_at_tick(&self, tick: u64) -> f64 {
        let index = self
            .points
            .partition_point(|point| point.start_tick <= tick)
            - 1;
        self.points[index].bpm
    }

    pub(crate) fn sample_at_tick(&self, tick: u64) -> Result<u64, TimebaseError> {
        let index = self
            .points
            .partition_point(|point| point.start_tick <= tick)
            - 1;
        let point = self.points[index];
        let samples = point.start_sample
            + (tick - point.start_tick) as f64 * self.samples_per_tick(point.bpm);
        rounded_position(samples)
    }

    pub(crate) fn tick_at_sample(&self, sample: u64) -> Result<u64, TimebaseError> {
        let sample = sample as f64;
        let index = self
            .points
            .partition_point(|point| point.start_sample <= sample)
            - 1;
        let point = self.points[index];
        let ticks = point.start_tick as f64
            + (sample - point.start_sample) / self.samples_per_tick(point.bpm);
        let mut tick = rounded_position(ticks)?;
        if let Some(next_point) = self.points.get(index + 1) {
            tick = tick.min(next_point.start_tick);
        }
        Ok(tick)
    }

    fn samples_per_tick(&self, bpm: f64) -> f64 {
        f64::from(self.sample_rate) * 60.0 / (bpm * f64::from(self.ppq))
    }

    fn recalculate_sample_anchors(
        points: &mut [TempoPoint],
        sample_rate: u32,
        ppq: u32,
    ) -> Result<(), TimebaseError> {
        points[0].start_sample = 0.0;
        for index in 1..points.len() {
            let previous = points[index - 1];
            let samples_per_tick = f64::from(sample_rate) * 60.0 / (previous.bpm * f64::from(ppq));
            let start_sample = previous.start_sample
                + (points[index].start_tick - previous.start_tick) as f64 * samples_per_tick;
            if !start_sample.is_finite()
                || start_sample <= previous.start_sample
                || start_sample >= u64::MAX as f64
            {
                return Err(TimebaseError::PositionOutOfRange);
            }
            points[index].start_sample = start_sample;
        }
        Ok(())
    }
}

impl Default for TempoMap {
    fn default() -> Self {
        Self::new(ProjectSettings::default())
    }
}

fn is_valid_tempo_for(sample_rate: u32, ppq: u32, bpm: f64) -> bool {
    if !bpm.is_finite() || bpm <= 0.0 {
        return false;
    }
    let samples_per_tick = f64::from(sample_rate) * 60.0 / (bpm * f64::from(ppq));
    samples_per_tick.is_finite() && samples_per_tick > 0.0
}

fn rounded_position(position: f64) -> Result<u64, TimebaseError> {
    let rounded = position.round();
    if !rounded.is_finite() || rounded < 0.0 || rounded >= u64::MAX as f64 {
        return Err(TimebaseError::PositionOutOfRange);
    }
    Ok(rounded as u64)
}
