use crate::{AudioCaptureControl, AudioCaptureProducer};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{
    BufferSize, FromSample, InputCallbackInfo, Sample, SampleFormat, SizedSample, StreamInstant,
    SupportedBufferSize,
};
use std::error::Error as StdError;
use std::fmt;
use std::time::Duration;

const PREFERRED_CAPTURE_FRAMES: u32 = 512;
const NANOS_PER_SECOND: u128 = 1_000_000_000;

fn is_supported_pcm(format: SampleFormat) -> bool {
    matches!(
        format,
        SampleFormat::I8
            | SampleFormat::I16
            | SampleFormat::I24
            | SampleFormat::I32
            | SampleFormat::I64
            | SampleFormat::U8
            | SampleFormat::U16
            | SampleFormat::U24
            | SampleFormat::U32
            | SampleFormat::U64
            | SampleFormat::F32
            | SampleFormat::F64
    )
}

fn supports_project_input_config(
    channels: u16,
    format: SampleFormat,
    minimum_rate: u32,
    maximum_rate: u32,
    project_rate: u32,
) -> bool {
    channels == 2
        && is_supported_pcm(format)
        && minimum_rate <= project_rate
        && project_rate <= maximum_rate
}

fn capture_buffer_size(supported: &SupportedBufferSize) -> BufferSize {
    match supported {
        SupportedBufferSize::Range { min, max } => {
            BufferSize::Fixed(PREFERRED_CAPTURE_FRAMES.clamp(*min, *max))
        }
        SupportedBufferSize::Unknown => BufferSize::Default,
    }
}

#[derive(Default)]
struct CaptureClock {
    origin: Option<StreamInstant>,
    next_frame: Option<u64>,
}

impl CaptureClock {
    fn first_frame(
        &mut self,
        capture_time: StreamInstant,
        frame_count: usize,
        sample_rate: u32,
    ) -> Option<u64> {
        let origin = *self.origin.get_or_insert(capture_time);
        let elapsed = capture_time.checked_duration_since(origin)?;
        let numerator = elapsed
            .as_nanos()
            .checked_mul(u128::from(sample_rate))?
            .checked_add(NANOS_PER_SECOND / 2)?;
        let first_frame = u64::try_from(numerator / NANOS_PER_SECOND).ok()?;
        let end_frame = first_frame.checked_add(u64::try_from(frame_count).ok()?)?;
        if self.next_frame.is_some_and(|next| first_frame < next) {
            return None;
        }
        self.next_frame = Some(end_frame);
        Some(first_frame)
    }
}

fn capture_interleaved<T>(
    samples: &[T],
    first_frame: u64,
    producer: &mut AudioCaptureProducer,
    control: &AudioCaptureControl,
) where
    T: Copy,
    f32: FromSample<T>,
{
    if samples.len() % 2 != 0 {
        control.fail_if_enabled();
        return;
    }
    if samples.is_empty() {
        control.fail_timing_if_enabled();
        return;
    }
    producer.push_frames_at(
        first_frame,
        samples
            .chunks_exact(2)
            .map(|frame| [f32::from_sample(frame[0]), f32::from_sample(frame[1])]),
    );
}

fn capture_input<T>(
    samples: &[T],
    info: &InputCallbackInfo,
    sample_rate: u32,
    clock: &mut CaptureClock,
    producer: &mut AudioCaptureProducer,
    control: &AudioCaptureControl,
) where
    T: Copy,
    f32: FromSample<T>,
{
    // The stream opens before recording is armed. Start the capture clock at the first accepted
    // callback so setup and recovery-manifest I/O do not become silent pre-roll.
    if !control.is_enabled() {
        return;
    }
    if samples.len() % 2 != 0 {
        control.fail_if_enabled();
        return;
    }
    let frame_count = samples.len() / 2;
    let Some(first_frame) = clock.first_frame(info.timestamp().capture, frame_count, sample_rate)
    else {
        control.fail_timing_if_enabled();
        return;
    };
    capture_interleaved(samples, first_frame, producer, control);
}

fn build_input_stream<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    sample_rate: u32,
    mut producer: AudioCaptureProducer,
    control: AudioCaptureControl,
) -> Result<cpal::Stream, cpal::Error>
where
    T: Copy + SizedSample + Sample,
    f32: FromSample<T>,
{
    let error_control = control.clone();
    let mut clock = CaptureClock::default();
    device.build_input_stream(
        config,
        move |samples: &[T], info: &InputCallbackInfo| {
            capture_input(
                samples,
                info,
                sample_rate,
                &mut clock,
                &mut producer,
                &control,
            )
        },
        move |_error| error_control.fail(),
        Some(Duration::from_secs(2)),
    )
}

/// Errors encountered while opening the default Windows stereo capture device.
#[derive(Debug)]
pub enum WasapiInputError {
    DeviceUnavailable,
    Cpal(cpal::Error),
    NoStereoConfig { sample_rate: u32 },
}

impl fmt::Display for WasapiInputError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DeviceUnavailable => {
                formatter.write_str("Windows has no default recording device")
            }
            Self::Cpal(error) => write!(formatter, "WASAPI input error: {error}"),
            Self::NoStereoConfig { sample_rate } => write!(
                formatter,
                "default WASAPI input has no stereo PCM configuration supporting {sample_rate} Hz"
            ),
        }
    }
}

impl StdError for WasapiInputError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Cpal(error) => Some(error),
            Self::DeviceUnavailable | Self::NoStereoConfig { .. } => None,
        }
    }
}

impl From<cpal::Error> for WasapiInputError {
    fn from(error: cpal::Error) -> Self {
        Self::Cpal(error)
    }
}

/// A CPAL stream capturing stereo audio from the default WASAPI input device.
pub struct WasapiAudioInput {
    stream: Option<cpal::Stream>,
    sample_rate: u32,
}

impl WasapiAudioInput {
    /// Opens the default shared-mode stereo input at the project's sample rate.
    pub fn open(
        producer: AudioCaptureProducer,
        control: AudioCaptureControl,
        project_sample_rate: u32,
    ) -> Result<Self, WasapiInputError> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or(WasapiInputError::DeviceUnavailable)?;
        let (selected, buffer_size) = device
            .supported_input_configs()?
            .filter(|range| {
                supports_project_input_config(
                    range.channels(),
                    range.sample_format(),
                    range.min_sample_rate(),
                    range.max_sample_rate(),
                    project_sample_rate,
                )
            })
            .map(|range| {
                let buffer_size = capture_buffer_size(range.buffer_size());
                (range, buffer_size)
            })
            .min_by_key(|(range, _)| u8::from(range.sample_format() != SampleFormat::F32))
            .ok_or(WasapiInputError::NoStereoConfig {
                sample_rate: project_sample_rate,
            })?;
        let sample_format = selected.sample_format();
        let mut config = selected.with_sample_rate(project_sample_rate).config();
        config.buffer_size = buffer_size;
        let stream = match sample_format {
            SampleFormat::F32 => {
                build_input_stream::<f32>(&device, config, project_sample_rate, producer, control)
            }
            SampleFormat::F64 => {
                build_input_stream::<f64>(&device, config, project_sample_rate, producer, control)
            }
            SampleFormat::I8 => {
                build_input_stream::<i8>(&device, config, project_sample_rate, producer, control)
            }
            SampleFormat::I16 => {
                build_input_stream::<i16>(&device, config, project_sample_rate, producer, control)
            }
            SampleFormat::I24 => build_input_stream::<cpal::I24>(
                &device,
                config,
                project_sample_rate,
                producer,
                control,
            ),
            SampleFormat::I32 => {
                build_input_stream::<i32>(&device, config, project_sample_rate, producer, control)
            }
            SampleFormat::I64 => {
                build_input_stream::<i64>(&device, config, project_sample_rate, producer, control)
            }
            SampleFormat::U8 => {
                build_input_stream::<u8>(&device, config, project_sample_rate, producer, control)
            }
            SampleFormat::U16 => {
                build_input_stream::<u16>(&device, config, project_sample_rate, producer, control)
            }
            SampleFormat::U24 => build_input_stream::<cpal::U24>(
                &device,
                config,
                project_sample_rate,
                producer,
                control,
            ),
            SampleFormat::U32 => {
                build_input_stream::<u32>(&device, config, project_sample_rate, producer, control)
            }
            SampleFormat::U64 => {
                build_input_stream::<u64>(&device, config, project_sample_rate, producer, control)
            }
            _unsupported => {
                return Err(WasapiInputError::NoStereoConfig {
                    sample_rate: project_sample_rate,
                });
            }
        }?;
        stream.play()?;
        Ok(Self {
            stream: Some(stream),
            sample_rate: project_sample_rate,
        })
    }

    /// Returns the project/device sample rate.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Pauses and releases the default input stream.
    pub fn shutdown(&mut self) {
        if let Some(stream) = self.stream.take() {
            let _ = stream.pause();
            drop(stream);
        }
    }
}

impl Drop for WasapiAudioInput {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CaptureClock, capture_buffer_size, capture_interleaved, supports_project_input_config,
    };
    use crate::audio_capture_stream;
    use cpal::{BufferSize, SampleFormat, StreamInstant, SupportedBufferSize};

    #[test]
    fn input_config_requires_stereo_pcm_at_the_project_rate() {
        assert!(supports_project_input_config(
            2,
            SampleFormat::F32,
            44_100,
            96_000,
            48_000
        ));
        assert!(supports_project_input_config(
            2,
            SampleFormat::I16,
            44_100,
            48_000,
            44_100
        ));
        assert!(!supports_project_input_config(
            1,
            SampleFormat::F32,
            44_100,
            96_000,
            48_000
        ));
        assert!(!supports_project_input_config(
            2,
            SampleFormat::F32,
            48_001,
            96_000,
            48_000
        ));
        assert!(!supports_project_input_config(
            2,
            SampleFormat::DsdU8,
            44_100,
            96_000,
            48_000
        ));
    }

    #[test]
    fn input_buffer_size_uses_a_supported_low_latency_period() {
        assert_eq!(
            capture_buffer_size(&SupportedBufferSize::Range {
                min: 128,
                max: 1_024
            }),
            BufferSize::Fixed(512)
        );
        assert_eq!(
            capture_buffer_size(&SupportedBufferSize::Range {
                min: 768,
                max: 1_024
            }),
            BufferSize::Fixed(768)
        );
        assert_eq!(
            capture_buffer_size(&SupportedBufferSize::Unknown),
            BufferSize::Default
        );
    }

    #[test]
    fn capture_clock_maps_monotonic_times_to_frames_and_rejects_regressions() {
        let mut clock = CaptureClock::default();
        let origin = StreamInstant::from_secs_f64(10.0);
        assert_eq!(clock.first_frame(origin, 480, 48_000), Some(0));
        assert_eq!(
            clock.first_frame(
                origin
                    .checked_add(std::time::Duration::from_millis(10))
                    .unwrap(),
                480,
                48_000
            ),
            Some(480)
        );
        assert_eq!(
            clock.first_frame(
                origin
                    .checked_add(std::time::Duration::from_millis(9))
                    .unwrap(),
                480,
                48_000
            ),
            None
        );
    }

    #[test]
    fn input_conversion_preserves_stereo_order_in_the_bounded_capture_queue() {
        let (mut producer, mut consumer, control) = audio_capture_stream(4);
        control.start();
        capture_interleaved(
            &[16_384_i16, -16_384, i16::MAX, i16::MIN],
            100,
            &mut producer,
            &control,
        );

        let mut output = [[0.0_f32; 2]; 2];
        let block = consumer.pop_timed_frames(&mut output).unwrap();
        assert_eq!((block.first_frame, block.frame_count), (100, 2));
        assert_eq!(output[0], [0.5, -0.5]);
        assert!(output[1][0] > 0.99 && output[1][1] < -0.99);
    }

    #[test]
    fn invalid_input_blocks_fail_an_armed_take() {
        let (mut producer, _consumer, control) = audio_capture_stream(4);
        control.start();
        capture_interleaved(&[0.0_f32; 3], 0, &mut producer, &control);
        assert!(control.has_failed());
    }
}
