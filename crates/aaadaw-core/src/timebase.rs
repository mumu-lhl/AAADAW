use std::fmt;

/// Default project sample rate in samples per second.
pub const DEFAULT_SAMPLE_RATE: u32 = 48_000;
/// Default musical resolution in ticks per quarter note.
pub const DEFAULT_PPQ: u32 = 960;
/// Default project tempo in quarter notes per minute.
pub const DEFAULT_TEMPO_BPM: f64 = 120.0;
// Tempo segment integration uses f64 positions; reject integers beyond its exact range.
const MAX_EXACT_FLOAT_POSITION: u64 = 1 << 53;

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
    /// Tick/sample position exceeds the exact range of the floating-point integrator.
    PositionOutOfRange,
    /// The initial tempo point at tick zero cannot be removed.
    CannotRemoveInitialTempo,
    /// A tempo point expected to exist is missing.
    TempoPointNotFound,
    /// Time signature numerator/denominator is invalid or unrepresentable.
    InvalidTimeSignature,
    /// A musical grid does not map to a positive integral number of ticks.
    InvalidGrid,
    /// A meter change is not aligned to the prior meter's bar line.
    MeterChangeNotOnBarBoundary,
    /// The initial meter point at tick zero cannot be removed.
    CannotRemoveInitialMeter,
    /// A meter point expected to exist is missing.
    MeterPointNotFound,
    /// The measure number cannot be represented.
    MusicalPositionOutOfRange,
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
            Self::InvalidTimeSignature => formatter.write_str("time signature is invalid"),
            Self::InvalidGrid => formatter.write_str("grid does not map to a positive tick count"),
            Self::MeterChangeNotOnBarBoundary => {
                formatter.write_str("meter changes must align with a bar line")
            }
            Self::CannotRemoveInitialMeter => {
                formatter.write_str("the initial meter point cannot be removed")
            }
            Self::MeterPointNotFound => formatter.write_str("meter point does not exist"),
            Self::MusicalPositionOutOfRange => {
                formatter.write_str("musical measure position is out of range")
            }
        }
    }
}

impl std::error::Error for TimebaseError {}

/// A musical quantization grid expressed as a fraction of a whole note.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GridFraction {
    numerator: u32,
    denominator: u32,
}

impl GridFraction {
    /// Creates a positive musical grid fraction, such as `1/16`.
    pub fn new(numerator: u32, denominator: u32) -> Result<Self, TimebaseError> {
        if numerator == 0 || denominator == 0 {
            return Err(TimebaseError::InvalidGrid);
        }
        Ok(Self {
            numerator,
            denominator,
        })
    }

    pub(crate) fn ticks(self, ppq: u32) -> Result<u64, TimebaseError> {
        let numerator = u64::from(ppq)
            .checked_mul(4)
            .and_then(|ticks| ticks.checked_mul(u64::from(self.numerator)))
            .ok_or(TimebaseError::InvalidGrid)?;
        let denominator = u64::from(self.denominator);
        if numerator == 0 || numerator % denominator != 0 {
            return Err(TimebaseError::InvalidGrid);
        }
        Ok(numerator / denominator)
    }
}

/// A musical meter such as 4/4 or 7/8.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TimeSignature {
    numerator: u32,
    denominator: u32,
}

impl TimeSignature {
    /// Creates a meter with a positive numerator and power-of-two denominator.
    pub fn new(numerator: u32, denominator: u32) -> Result<Self, TimebaseError> {
        if numerator == 0 || !denominator.is_power_of_two() {
            return Err(TimebaseError::InvalidTimeSignature);
        }
        Ok(Self {
            numerator,
            denominator,
        })
    }

    /// Returns the number of beats in each measure.
    pub fn numerator(self) -> u32 {
        self.numerator
    }

    /// Returns the note value that receives one beat.
    pub fn denominator(self) -> u32 {
        self.denominator
    }

    fn ticks_per_beat(self, ppq: u32) -> Result<u64, TimebaseError> {
        let quarter_note_ticks = u64::from(ppq) * 4;
        let denominator = u64::from(self.denominator);
        if quarter_note_ticks % denominator != 0 {
            return Err(TimebaseError::InvalidTimeSignature);
        }
        Ok(quarter_note_ticks / denominator)
    }

    fn ticks_per_measure(self, ppq: u32) -> Result<u64, TimebaseError> {
        self.ticks_per_beat(ppq)?
            .checked_mul(u64::from(self.numerator))
            .ok_or(TimebaseError::MusicalPositionOutOfRange)
    }
}

/// A one-based bar/beat position and zero-based tick offset within the beat.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MusicalPosition {
    measure: u64,
    beat: u32,
    tick_in_beat: u64,
}

impl MusicalPosition {
    /// Returns the one-based measure number.
    pub fn measure(self) -> u64 {
        self.measure
    }

    /// Returns the one-based beat number within the measure.
    pub fn beat(self) -> u32 {
        self.beat
    }

    /// Returns the tick offset within the beat.
    pub fn tick_in_beat(self) -> u64 {
        self.tick_in_beat
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct MeterPoint {
    start_tick: u64,
    signature: TimeSignature,
    start_measure: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MeterMap {
    ppq: u32,
    points: Vec<MeterPoint>,
}

impl Default for MeterMap {
    fn default() -> Self {
        Self::new(DEFAULT_PPQ)
    }
}

impl MeterMap {
    pub(crate) fn new(ppq: u32) -> Self {
        Self {
            ppq,
            points: vec![MeterPoint {
                start_tick: 0,
                signature: TimeSignature {
                    numerator: 4,
                    denominator: 4,
                },
                start_measure: 0,
            }],
        }
    }

    pub(crate) fn points(&self) -> impl Iterator<Item = (u64, TimeSignature)> + '_ {
        self.points
            .iter()
            .map(|point| (point.start_tick, point.signature))
    }

    pub(crate) fn point_at(&self, start_tick: u64) -> Option<TimeSignature> {
        self.points
            .binary_search_by_key(&start_tick, |point| point.start_tick)
            .ok()
            .map(|index| self.points[index].signature)
    }

    pub(crate) fn set_point(
        &mut self,
        start_tick: u64,
        signature: Option<TimeSignature>,
    ) -> Result<(), TimebaseError> {
        if let Some(signature) = signature {
            signature.ticks_per_measure(self.ppq)?;
        }
        if start_tick == 0 && signature.is_none() {
            return Err(TimebaseError::CannotRemoveInitialMeter);
        }

        let mut points = self.points.clone();
        match (
            points.binary_search_by_key(&start_tick, |point| point.start_tick),
            signature,
        ) {
            (Ok(index), Some(signature)) => points[index].signature = signature,
            (Ok(index), None) => {
                points.remove(index);
            }
            (Err(index), Some(signature)) => points.insert(
                index,
                MeterPoint {
                    start_tick,
                    signature,
                    start_measure: 0,
                },
            ),
            (Err(_), None) => return Err(TimebaseError::MeterPointNotFound),
        }
        Self::recalculate_measures(&mut points, self.ppq)?;
        self.points = points;
        Ok(())
    }

    pub(crate) fn position_at_tick(&self, tick: u64) -> Result<MusicalPosition, TimebaseError> {
        let index = self
            .points
            .partition_point(|point| point.start_tick <= tick)
            - 1;
        let point = self.points[index];
        let ticks_per_beat = point.signature.ticks_per_beat(self.ppq)?;
        let ticks_per_measure = point.signature.ticks_per_measure(self.ppq)?;
        let offset = tick - point.start_tick;
        let measure_offset = offset / ticks_per_measure;
        let measure = point
            .start_measure
            .checked_add(measure_offset)
            .and_then(|value| value.checked_add(1))
            .ok_or(TimebaseError::MusicalPositionOutOfRange)?;
        let within_measure = offset % ticks_per_measure;
        Ok(MusicalPosition {
            measure,
            beat: (within_measure / ticks_per_beat + 1) as u32,
            tick_in_beat: within_measure % ticks_per_beat,
        })
    }

    pub(crate) fn signature_at_tick(&self, tick: u64) -> TimeSignature {
        let index = self
            .points
            .partition_point(|point| point.start_tick <= tick)
            - 1;
        self.points[index].signature
    }

    fn recalculate_measures(points: &mut [MeterPoint], ppq: u32) -> Result<(), TimebaseError> {
        points[0].start_measure = 0;
        for index in 1..points.len() {
            let previous = points[index - 1];
            let ticks_per_measure = previous.signature.ticks_per_measure(ppq)?;
            let offset = points[index].start_tick - previous.start_tick;
            if offset % ticks_per_measure != 0 {
                return Err(TimebaseError::MeterChangeNotOnBarBoundary);
            }
            points[index].start_measure = previous
                .start_measure
                .checked_add(offset / ticks_per_measure)
                .ok_or(TimebaseError::MusicalPositionOutOfRange)?;
        }
        Ok(())
    }
}

/// The interpolation mode from one tempo point to the next.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TempoCurve {
    /// Hold the tempo until the next point.
    #[default]
    Step,
    /// Change BPM linearly over the interval to the next point.
    Linear,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct TempoPoint {
    start_tick: u64,
    bpm: f64,
    curve_to_next: TempoCurve,
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
                curve_to_next: TempoCurve::Step,
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

    pub(crate) fn points(&self) -> impl Iterator<Item = (u64, f64, TempoCurve)> + '_ {
        self.points
            .iter()
            .map(|point| (point.start_tick, point.bpm, point.curve_to_next))
    }

    pub(crate) fn point_at(&self, start_tick: u64) -> Option<f64> {
        self.points
            .binary_search_by_key(&start_tick, |point| point.start_tick)
            .ok()
            .map(|index| self.points[index].bpm)
    }

    pub(crate) fn curve_at(&self, start_tick: u64) -> Option<TempoCurve> {
        self.points
            .binary_search_by_key(&start_tick, |point| point.start_tick)
            .ok()
            .map(|index| self.points[index].curve_to_next)
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
                    curve_to_next: TempoCurve::Step,
                    start_sample: 0.0,
                },
            ),
            (Err(_), None) => return Err(TimebaseError::TempoPointNotFound),
        }
        Self::recalculate_sample_anchors(&mut points, self.sample_rate, self.ppq)?;
        self.points = points;
        Ok(())
    }

    pub(crate) fn set_curve(
        &mut self,
        start_tick: u64,
        curve: TempoCurve,
    ) -> Result<(), TimebaseError> {
        let mut points = self.points.clone();
        let index = points
            .binary_search_by_key(&start_tick, |point| point.start_tick)
            .map_err(|_| TimebaseError::TempoPointNotFound)?;
        points[index].curve_to_next = curve;
        Self::recalculate_sample_anchors(&mut points, self.sample_rate, self.ppq)?;
        self.points = points;
        Ok(())
    }

    pub(crate) fn tempo_at_tick(&self, tick: u64) -> f64 {
        let index = self
            .points
            .partition_point(|point| point.start_tick <= tick)
            - 1;
        let point = self.points[index];
        let Some(next) = self.points.get(index + 1) else {
            return point.bpm;
        };
        if point.curve_to_next == TempoCurve::Step {
            return point.bpm;
        }
        let progress =
            (tick - point.start_tick) as f64 / (next.start_tick - point.start_tick) as f64;
        point.bpm + (next.bpm - point.bpm) * progress
    }

    pub(crate) fn sample_at_tick(&self, tick: u64) -> Result<u64, TimebaseError> {
        if tick > MAX_EXACT_FLOAT_POSITION {
            return Err(TimebaseError::PositionOutOfRange);
        }
        let index = self
            .points
            .partition_point(|point| point.start_tick <= tick)
            - 1;
        let point = self.points[index];
        let (end_tick, end_bpm, curve) = self
            .points
            .get(index + 1)
            .map_or((tick, point.bpm, TempoCurve::Step), |next| {
                (next.start_tick, next.bpm, point.curve_to_next)
            });
        let offset = tick - point.start_tick;
        let segment_length = end_tick - point.start_tick;
        let elapsed = segment_sample_offset(
            self.sample_rate,
            self.ppq,
            point.bpm,
            end_bpm,
            curve,
            segment_length,
            offset,
        )?;
        rounded_position(point.start_sample + elapsed)
    }

    pub(crate) fn tick_at_sample(&self, sample: u64) -> Result<u64, TimebaseError> {
        if sample > MAX_EXACT_FLOAT_POSITION {
            return Err(TimebaseError::PositionOutOfRange);
        }
        let sample = sample as f64;
        let index = self
            .points
            .partition_point(|point| point.start_sample <= sample)
            - 1;
        let point = self.points[index];
        let next = self.points.get(index + 1);
        let offset = if let Some(next) = next {
            let segment_length = next.start_tick - point.start_tick;
            ticks_from_sample_offset(
                self.sample_rate,
                self.ppq,
                point.bpm,
                next.bpm,
                point.curve_to_next,
                segment_length,
                sample - point.start_sample,
            )?
            .min(segment_length as f64)
        } else {
            sample_offset_to_ticks(
                self.sample_rate,
                self.ppq,
                point.bpm,
                sample - point.start_sample,
            )?
        };
        let offset = rounded_position(offset)?;
        let tick = point
            .start_tick
            .checked_add(offset)
            .ok_or(TimebaseError::PositionOutOfRange)?;
        if tick > MAX_EXACT_FLOAT_POSITION {
            return Err(TimebaseError::PositionOutOfRange);
        }
        Ok(next.map_or(tick, |next| tick.min(next.start_tick)))
    }

    fn recalculate_sample_anchors(
        points: &mut [TempoPoint],
        sample_rate: u32,
        ppq: u32,
    ) -> Result<(), TimebaseError> {
        points[0].start_sample = 0.0;
        for index in 1..points.len() {
            let previous = points[index - 1];
            let next = points[index];
            if next.start_tick > MAX_EXACT_FLOAT_POSITION {
                return Err(TimebaseError::PositionOutOfRange);
            }
            let length = next.start_tick - previous.start_tick;
            let duration = segment_sample_offset(
                sample_rate,
                ppq,
                previous.bpm,
                next.bpm,
                previous.curve_to_next,
                length,
                length,
            )?;
            let start_sample = previous.start_sample + duration;
            if !start_sample.is_finite()
                || start_sample <= previous.start_sample
                || start_sample > MAX_EXACT_FLOAT_POSITION as f64
            {
                return Err(TimebaseError::PositionOutOfRange);
            }
            points[index].start_sample = start_sample;
        }
        Ok(())
    }
}

fn segment_sample_offset(
    sample_rate: u32,
    ppq: u32,
    start_bpm: f64,
    end_bpm: f64,
    curve: TempoCurve,
    segment_length: u64,
    offset_ticks: u64,
) -> Result<f64, TimebaseError> {
    if offset_ticks > segment_length {
        return Err(TimebaseError::PositionOutOfRange);
    }
    let scale = f64::from(sample_rate) * 60.0 / f64::from(ppq);
    let delta_bpm = end_bpm - start_bpm;
    let samples = if curve == TempoCurve::Linear && segment_length > 0 && delta_bpm != 0.0 {
        let progress = offset_ticks as f64 / segment_length as f64;
        let log_argument = delta_bpm * progress / start_bpm;
        if log_argument <= -1.0 {
            return Err(TimebaseError::InvalidTempo);
        }
        scale * segment_length as f64 / delta_bpm * log_argument.ln_1p()
    } else {
        scale * offset_ticks as f64 / start_bpm
    };
    if !samples.is_finite() || samples < 0.0 {
        return Err(TimebaseError::PositionOutOfRange);
    }
    Ok(samples)
}

fn sample_offset_to_ticks(
    sample_rate: u32,
    ppq: u32,
    bpm: f64,
    sample_offset: f64,
) -> Result<f64, TimebaseError> {
    let scale = f64::from(sample_rate) * 60.0 / f64::from(ppq);
    let ticks = sample_offset * bpm / scale;
    if !ticks.is_finite() || ticks < 0.0 {
        return Err(TimebaseError::PositionOutOfRange);
    }
    Ok(ticks)
}

fn ticks_from_sample_offset(
    sample_rate: u32,
    ppq: u32,
    start_bpm: f64,
    end_bpm: f64,
    curve: TempoCurve,
    segment_length: u64,
    sample_offset: f64,
) -> Result<f64, TimebaseError> {
    if curve == TempoCurve::Step || segment_length == 0 || start_bpm == end_bpm {
        return sample_offset_to_ticks(sample_rate, ppq, start_bpm, sample_offset);
    }
    let scale = f64::from(sample_rate) * 60.0 / f64::from(ppq);
    let slope = (end_bpm - start_bpm) / segment_length as f64;
    let exponent = slope * sample_offset / scale;
    let ticks = start_bpm * exponent.exp_m1() / slope;
    if !ticks.is_finite() || ticks < 0.0 {
        return Err(TimebaseError::PositionOutOfRange);
    }
    Ok(ticks)
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
    if !rounded.is_finite() || rounded < 0.0 || rounded > MAX_EXACT_FLOAT_POSITION as f64 {
        return Err(TimebaseError::PositionOutOfRange);
    }
    Ok(rounded as u64)
}
