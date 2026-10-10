use std::fmt;

/// Default project sample rate in samples per second.
pub const DEFAULT_SAMPLE_RATE: u32 = 48_000;
/// Default musical resolution in ticks per quarter note.
pub const DEFAULT_PPQ: u32 = 960;
/// Default project tempo in quarter notes per minute.
pub const DEFAULT_TEMPO_BPM: f64 = 120.0;
// Tempo segment integration uses f64 positions; reject integers beyond its exact range.
const MAX_EXACT_FLOAT_POSITION: u64 = 1 << 53;

/// Track-level gain policy inherited from the project at construction.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PanMode {
    /// Historical AAADAW equal-power mono and linear stereo balance behavior.
    LegacyMonoStereo,
    /// REAPER factory 0 dB linear stereo balance, also applied to mono sources.
    #[default]
    ZeroDbBalance,
}

/// Immutable construction settings for a project.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProjectSettings {
    sample_rate: u32,
    ppq: u32,
    initial_tempo_bpm: f64,
    pan_mode: PanMode,
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
            pan_mode: PanMode::default(),
        })
    }

    /// Selects the gain policy without changing timebase settings.
    pub fn with_pan_mode(mut self, mode: PanMode) -> Self {
        self.pan_mode = mode;
        self
    }

    /// Returns the project gain policy.
    pub fn pan_mode(self) -> PanMode {
        self.pan_mode
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
            pan_mode: PanMode::default(),
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
    /// Meter-map points are unordered or duplicated.
    InvalidMeterMap,
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
            Self::InvalidMeterMap => {
                formatter.write_str("meter map points must be ordered and have unique positions")
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

    pub(crate) fn replace_points(
        &mut self,
        points: &[(u64, TimeSignature)],
    ) -> Result<(), TimebaseError> {
        if points.first().map(|point| point.0) != Some(0) {
            return Err(TimebaseError::CannotRemoveInitialMeter);
        }
        if points.windows(2).any(|pair| pair[0].0 >= pair[1].0) {
            return Err(TimebaseError::InvalidMeterMap);
        }
        let mut replacement = points
            .iter()
            .map(|(start_tick, signature)| MeterPoint {
                start_tick: *start_tick,
                signature: *signature,
                start_measure: 0,
            })
            .collect::<Vec<_>>();
        for point in &replacement {
            point.signature.ticks_per_measure(self.ppq)?;
        }
        Self::recalculate_measures(&mut replacement, self.ppq)?;
        self.points = replacement;
        Ok(())
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
            if !offset.is_multiple_of(ticks_per_measure) {
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
    /// Interpolate linearly in log-BPM space, producing an exponential BPM ramp.
    Logarithmic,
    /// Use a cubic Bézier ramp with horizontal endpoint tangents.
    Bézier,
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
            pan_mode: PanMode::default(),
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
        tempo_at_progress(point.bpm, next.bpm, point.curve_to_next, progress)
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

    pub(crate) fn sample_at_tick_position(&self, tick: f64) -> Result<f64, TimebaseError> {
        validate_fractional_position(tick)?;
        let index = self
            .points
            .partition_point(|point| point.start_tick as f64 <= tick)
            - 1;
        let point = self.points[index];
        let offset = tick - point.start_tick as f64;
        let integral = if let Some(next) = self.points.get(index + 1) {
            let length = next.start_tick - point.start_tick;
            segment_integral_progress(
                point.bpm,
                next.bpm,
                point.curve_to_next,
                length,
                offset / length as f64,
            )?
        } else {
            offset / point.bpm
        };
        let sample = point.start_sample
            + f64::from(self.sample_rate) * 60.0 / f64::from(self.ppq) * integral;
        validate_fractional_position(sample)?;
        Ok(sample)
    }

    pub(crate) fn tick_at_sample_position(&self, sample: f64) -> Result<f64, TimebaseError> {
        validate_fractional_position(sample)?;
        let index = self
            .points
            .partition_point(|point| point.start_sample <= sample)
            - 1;
        let point = self.points[index];
        let offset = if let Some(next) = self.points.get(index + 1) {
            let length = next.start_tick - point.start_tick;
            ticks_from_sample_offset(
                self.sample_rate,
                self.ppq,
                point.bpm,
                next.bpm,
                point.curve_to_next,
                length,
                sample - point.start_sample,
            )?
            .min(length as f64)
        } else {
            sample_offset_to_ticks(
                self.sample_rate,
                self.ppq,
                point.bpm,
                sample - point.start_sample,
            )?
        };
        let tick = point.start_tick as f64 + offset;
        validate_fractional_position(tick)?;
        Ok(tick)
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
    let samples =
        scale * segment_integral_ticks(start_bpm, end_bpm, curve, segment_length, offset_ticks)?;
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
    let target = sample_offset / scale;
    let ticks = match curve {
        TempoCurve::Linear => {
            let slope = (end_bpm - start_bpm) / segment_length as f64;
            let exponent = slope * target;
            start_bpm * exponent.exp_m1() / slope
        }
        TempoCurve::Logarithmic => {
            let log_ratio = end_bpm.ln() - start_bpm.ln();
            if log_ratio.abs() < 1.0e-12 {
                target * start_bpm
            } else {
                let argument = target * log_ratio * start_bpm / segment_length as f64;
                if argument >= 1.0 {
                    return Err(TimebaseError::PositionOutOfRange);
                }
                -(segment_length as f64) * (-argument).ln_1p() / log_ratio
            }
        }
        TempoCurve::Bézier => {
            let mut low = 0.0;
            let mut high = 1.0;
            for _ in 0..56 {
                let middle = (low + high) * 0.5;
                let elapsed =
                    segment_integral_progress(start_bpm, end_bpm, curve, segment_length, middle)?;
                if elapsed < target {
                    low = middle;
                } else {
                    high = middle;
                }
            }
            (low + high) * 0.5 * segment_length as f64
        }
        TempoCurve::Step => unreachable!("step tempo ramps are handled above"),
    };
    if !ticks.is_finite() || ticks < 0.0 {
        return Err(TimebaseError::PositionOutOfRange);
    }
    Ok(ticks)
}

fn tempo_at_progress(start_bpm: f64, end_bpm: f64, curve: TempoCurve, progress: f64) -> f64 {
    match curve {
        TempoCurve::Step => start_bpm,
        TempoCurve::Linear => start_bpm + (end_bpm - start_bpm) * progress,
        TempoCurve::Logarithmic => {
            (start_bpm.ln() + (end_bpm.ln() - start_bpm.ln()) * progress).exp()
        }
        TempoCurve::Bézier => {
            let smooth_progress = progress * progress * (3.0 - 2.0 * progress);
            start_bpm + (end_bpm - start_bpm) * smooth_progress
        }
    }
}

fn segment_integral_ticks(
    start_bpm: f64,
    end_bpm: f64,
    curve: TempoCurve,
    segment_length: u64,
    offset_ticks: u64,
) -> Result<f64, TimebaseError> {
    if offset_ticks > segment_length {
        return Err(TimebaseError::PositionOutOfRange);
    }
    if offset_ticks == 0 {
        return Ok(0.0);
    }
    segment_integral_progress(
        start_bpm,
        end_bpm,
        curve,
        segment_length,
        offset_ticks as f64 / segment_length as f64,
    )
}

fn segment_integral_progress(
    start_bpm: f64,
    end_bpm: f64,
    curve: TempoCurve,
    segment_length: u64,
    progress: f64,
) -> Result<f64, TimebaseError> {
    if curve == TempoCurve::Step || segment_length == 0 || start_bpm == end_bpm {
        return Ok(segment_length as f64 * progress / start_bpm);
    }
    let integral = match curve {
        TempoCurve::Linear => {
            let log_argument = (end_bpm - start_bpm) * progress / start_bpm;
            if log_argument <= -1.0 {
                return Err(TimebaseError::InvalidTempo);
            }
            segment_length as f64 * (log_argument.ln_1p()) / (end_bpm - start_bpm)
        }
        TempoCurve::Logarithmic => {
            let log_ratio = end_bpm.ln() - start_bpm.ln();
            if log_ratio.abs() < 1.0e-12 {
                segment_length as f64 * progress / start_bpm
            } else {
                segment_length as f64 * (-(-log_ratio * progress).exp_m1())
                    / (start_bpm * log_ratio)
            }
        }
        TempoCurve::Bézier => {
            let f = |u: f64| 1.0 / tempo_at_progress(start_bpm, end_bpm, curve, u);
            adaptive_simpson(f, 0.0, progress, 1.0e-13, 20) * segment_length as f64
        }
        TempoCurve::Step => unreachable!("step ramps are handled above"),
    };
    if !integral.is_finite() || integral < 0.0 {
        return Err(TimebaseError::PositionOutOfRange);
    }
    Ok(integral)
}

fn adaptive_simpson(
    f: impl Fn(f64) -> f64 + Copy,
    start: f64,
    end: f64,
    tolerance: f64,
    depth: u8,
) -> f64 {
    let middle = (start + end) * 0.5;
    let start_value = f(start);
    let middle_value = f(middle);
    let end_value = f(end);
    let whole = (end - start) * (start_value + 4.0 * middle_value + end_value) / 6.0;
    adaptive_simpson_refine(
        f,
        start,
        end,
        start_value,
        middle_value,
        end_value,
        whole,
        tolerance,
        depth,
    )
}

#[allow(clippy::too_many_arguments)]
fn adaptive_simpson_refine(
    f: impl Fn(f64) -> f64 + Copy,
    start: f64,
    end: f64,
    start_value: f64,
    middle_value: f64,
    end_value: f64,
    whole: f64,
    tolerance: f64,
    depth: u8,
) -> f64 {
    let middle = (start + end) * 0.5;
    let left_middle = (start + middle) * 0.5;
    let right_middle = (middle + end) * 0.5;
    let left_value = f(left_middle);
    let right_value = f(right_middle);
    let left = (middle - start) * (start_value + 4.0 * left_value + middle_value) / 6.0;
    let right = (end - middle) * (middle_value + 4.0 * right_value + end_value) / 6.0;
    let delta = left + right - whole;
    if depth == 0 || delta.abs() <= 15.0 * tolerance {
        return left + right + delta / 15.0;
    }
    adaptive_simpson_refine(
        f,
        start,
        middle,
        start_value,
        left_value,
        middle_value,
        left,
        tolerance * 0.5,
        depth - 1,
    ) + adaptive_simpson_refine(
        f,
        middle,
        end,
        middle_value,
        right_value,
        end_value,
        right,
        tolerance * 0.5,
        depth - 1,
    )
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

fn validate_fractional_position(position: f64) -> Result<(), TimebaseError> {
    if !position.is_finite() || !(0.0..=MAX_EXACT_FLOAT_POSITION as f64).contains(&position) {
        return Err(TimebaseError::PositionOutOfRange);
    }
    Ok(())
}

fn rounded_position(position: f64) -> Result<u64, TimebaseError> {
    let rounded = position.round();
    if !rounded.is_finite() || rounded < 0.0 || rounded > MAX_EXACT_FLOAT_POSITION as f64 {
        return Err(TimebaseError::PositionOutOfRange);
    }
    Ok(rounded as u64)
}
