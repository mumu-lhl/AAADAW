use crate::{AudioCaptureControl, AudioCaptureProducer};
use pipewire as pw;
use pw::spa::buffer::meta::{MetaHeader, MetaHeaderFlags};
use pw::spa::pod::{Pod, Property, Value};
use pw::spa::utils::{Id, SpaTypes};
use std::error::Error as StdError;
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

enum ThreadCommand {
    Shutdown,
}

struct CaptureData {
    producer: AudioCaptureProducer,
    control: AudioCaptureControl,
    first_pts: Option<i64>,
}

const NANOSECONDS_PER_SECOND: i128 = 1_000_000_000;

fn pipewire_pts_to_frame(pts: i64, origin_pts: i64, sample_rate: u32) -> Option<u64> {
    let delta_ns = i128::from(pts).checked_sub(i128::from(origin_pts))?;
    if delta_ns < 0 || sample_rate == 0 {
        return None;
    }
    let scaled = delta_ns.checked_mul(i128::from(sample_rate))?;
    let rounded = scaled.checked_add(NANOSECONDS_PER_SECOND / 2)? / NANOSECONDS_PER_SECOND;
    u64::try_from(rounded).ok()
}

/// Errors encountered while connecting a PipeWire default stereo input.
#[derive(Debug)]
pub enum PipeWireInputError {
    Thread(String),
    SampleRateMismatch { project: u32, device: u32 },
    StartupTimedOut,
}

impl fmt::Display for PipeWireInputError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Thread(error) => write!(formatter, "PipeWire input setup failed: {error}"),
            Self::SampleRateMismatch { project, device } => write!(
                formatter,
                "project sample rate {project} Hz does not match PipeWire input rate {device} Hz"
            ),
            Self::StartupTimedOut => {
                formatter.write_str("PipeWire did not connect to a default stereo input")
            }
        }
    }
}

impl StdError for PipeWireInputError {}

/// A PipeWire stream connected to the system's default stereo audio input.
pub struct PipeWireAudioInput {
    shutdown: Sender<ThreadCommand>,
    thread: Option<JoinHandle<()>>,
    sample_rate: u32,
}

impl PipeWireAudioInput {
    /// Opens the default PipeWire input using the project's sample rate.
    pub fn open(
        producer: AudioCaptureProducer,
        control: AudioCaptureControl,
        project_sample_rate: u32,
    ) -> Result<Self, PipeWireInputError> {
        let (shutdown, shutdown_receiver) = mpsc::channel();
        let (setup_sender, setup_receiver) = mpsc::sync_channel(1);
        let control_on_error = control.clone();
        let thread = thread::Builder::new()
            .name("aaadaw-pipewire-capture".to_owned())
            .spawn(move || {
                pipewire_thread(
                    producer,
                    control,
                    project_sample_rate,
                    shutdown_receiver,
                    setup_sender,
                )
            })
            .map_err(|error| PipeWireInputError::Thread(error.to_string()))?;
        let sample_rate = match setup_receiver.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(sample_rate)) => sample_rate,
            Ok(Err(error)) => {
                let _ = shutdown.send(ThreadCommand::Shutdown);
                let _ = thread.join();
                return Err(PipeWireInputError::Thread(error));
            }
            Err(_) => {
                control_on_error.fail();
                let _ = shutdown.send(ThreadCommand::Shutdown);
                let _ = thread.join();
                return Err(PipeWireInputError::StartupTimedOut);
            }
        };
        if sample_rate != project_sample_rate {
            let _ = shutdown.send(ThreadCommand::Shutdown);
            let _ = thread.join();
            return Err(PipeWireInputError::SampleRateMismatch {
                project: project_sample_rate,
                device: sample_rate,
            });
        }
        Ok(Self {
            shutdown,
            thread: Some(thread),
            sample_rate,
        })
    }

    /// Returns the negotiated device sample rate.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Stops the PipeWire stream and joins its non-realtime control thread.
    pub fn shutdown(&mut self) {
        let _ = self.shutdown.send(ThreadCommand::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for PipeWireAudioInput {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn pipewire_thread(
    producer: AudioCaptureProducer,
    control: AudioCaptureControl,
    sample_rate: u32,
    shutdown: Receiver<ThreadCommand>,
    setup: mpsc::SyncSender<Result<u32, String>>,
) {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(pw::init);
    if let Err(error) = run_pipewire_input(producer, control, sample_rate, shutdown, &setup) {
        let _ = setup.send(Err(error));
    }
}

fn run_pipewire_input(
    producer: AudioCaptureProducer,
    control: AudioCaptureControl,
    sample_rate: u32,
    shutdown: Receiver<ThreadCommand>,
    setup: &mpsc::SyncSender<Result<u32, String>>,
) -> Result<(), String> {
    let mainloop = pw::main_loop::MainLoopRc::new(None).map_err(|error| error.to_string())?;
    let context =
        pw::context::ContextRc::new(&mainloop, None).map_err(|error| error.to_string())?;
    let core = context
        .connect_rc(None)
        .map_err(|error| error.to_string())?;
    let stream = pw::stream::StreamBox::new(
        &core,
        "AAADAW Capture",
        pw::properties::properties! {
            *pw::keys::MEDIA_TYPE => "Audio",
            *pw::keys::MEDIA_ROLE => "Music",
            *pw::keys::MEDIA_CATEGORY => "Capture",
            *pw::keys::AUDIO_CHANNELS => "2",
        },
    )
    .map_err(|error| error.to_string())?;
    let stream_error = Arc::new(AtomicBool::new(false));
    let callback_error = Arc::clone(&stream_error);
    let listener = stream
        .add_local_listener_with_user_data(CaptureData {
            producer,
            control,
            first_pts: None,
        })
        .state_changed(move |_, data, _, state| {
            if matches!(state, pw::stream::StreamState::Error(_)) {
                callback_error.store(true, Ordering::Release);
                data.control.fail();
            }
        })
        .process(|stream, data| {
            let Some(mut buffer) = stream.dequeue_buffer() else {
                data.control.fail_if_enabled();
                return;
            };
            let header = buffer
                .find_meta::<MetaHeader>()
                .map(|header| (header.pts(), header.flags()));
            let Some((pts, flags)) = header else {
                data.control.fail_timing_if_enabled();
                return;
            };
            if flags.contains(MetaHeaderFlags::CORRUPTED)
                || (flags.contains(MetaHeaderFlags::DISCONT) && data.first_pts.is_some())
            {
                data.control.fail_timing_if_enabled();
                return;
            }
            let datas = buffer.datas_mut();
            let [input] = datas else {
                data.control.fail();
                return;
            };
            let chunk = input.chunk();
            let offset = chunk.offset() as usize;
            let size = chunk.size() as usize;
            let stride = chunk.stride();
            if stride != 8 || size % 8 != 0 {
                data.control.fail_timing_if_enabled();
                return;
            }
            let frame_count = size / 8;
            if frame_count == 0 {
                data.control.fail_timing_if_enabled();
                return;
            }
            let origin_pts = *data.first_pts.get_or_insert(pts);
            let Some(first_frame) = pipewire_pts_to_frame(pts, origin_pts, sample_rate) else {
                data.control.fail_timing_if_enabled();
                return;
            };
            if flags.contains(MetaHeaderFlags::GAP) {
                data.producer
                    .push_frames_at(first_frame, std::iter::repeat_n([0.0, 0.0], frame_count));
            } else {
                let Some(bytes) = input.data() else {
                    data.control.fail();
                    return;
                };
                let Some(end) = offset.checked_add(size) else {
                    data.control.fail_timing_if_enabled();
                    return;
                };
                if end > bytes.len() {
                    data.control.fail_timing_if_enabled();
                    return;
                }
                data.producer.push_frames_at(
                    first_frame,
                    bytes[offset..end].chunks_exact(8).map(|frame| {
                        [
                            f32::from_le_bytes([frame[0], frame[1], frame[2], frame[3]]),
                            f32::from_le_bytes([frame[4], frame[5], frame[6], frame[7]]),
                        ]
                    }),
                );
            }
        })
        .register()
        .map_err(|error| error.to_string())?;

    let mut audio_info = pw::spa::param::audio::AudioInfoRaw::new();
    audio_info.set_format(pw::spa::param::audio::AudioFormat::F32LE);
    audio_info.set_rate(sample_rate);
    audio_info.set_channels(2);
    let mut position = [0; pw::spa::param::audio::MAX_CHANNELS];
    position[0] = pw::spa::sys::SPA_AUDIO_CHANNEL_FL;
    position[1] = pw::spa::sys::SPA_AUDIO_CHANNEL_FR;
    audio_info.set_position(position);
    let values: Vec<u8> = pw::spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &pw::spa::pod::Value::Object(pw::spa::pod::Object {
            type_: pw::spa::sys::SPA_TYPE_OBJECT_Format,
            id: pw::spa::sys::SPA_PARAM_EnumFormat,
            properties: audio_info.into(),
        }),
    )
    .map_err(|error| error.to_string())?
    .0
    .into_inner();
    let audio_param = Pod::from_bytes(&values).ok_or_else(|| "invalid audio format".to_owned())?;
    let metadata_values = serialize_header_metadata()?;
    let metadata = Pod::from_bytes(&metadata_values)
        .ok_or_else(|| "invalid PipeWire header metadata parameter".to_owned())?;
    let mut params = [audio_param, metadata];
    stream
        .connect(
            pw::spa::utils::Direction::Input,
            None,
            pw::stream::StreamFlags::AUTOCONNECT
                | pw::stream::StreamFlags::MAP_BUFFERS
                | pw::stream::StreamFlags::RT_PROCESS
                | pw::stream::StreamFlags::NO_CONVERT,
            &mut params,
        )
        .map_err(|error| error.to_string())?;
    if setup.send(Ok(sample_rate)).is_err() {
        return Err("input setup receiver was dropped".to_owned());
    }

    loop {
        match shutdown.try_recv() {
            Ok(ThreadCommand::Shutdown) | Err(TryRecvError::Disconnected) => break,
            Err(TryRecvError::Empty) => {}
        }
        if stream_error.load(Ordering::Acquire) {
            break;
        }
        mainloop
            .loop_()
            .iterate(pw::loop_::Timeout::Finite(Duration::from_millis(20)));
    }
    drop(listener);
    drop(stream);
    drop(core);
    drop(context);
    drop(mainloop);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::pipewire_pts_to_frame;

    #[test]
    fn pipewire_pts_deltas_map_to_frames_at_common_rates() {
        for sample_rate in [44_100, 48_000] {
            let expected_frames = u64::from(sample_rate) * 2 + 17;
            let delta_ns = (u128::from(expected_frames) * 1_000_000_000
                + u128::from(sample_rate) / 2)
                / u128::from(sample_rate);
            assert_eq!(
                pipewire_pts_to_frame(9_000_000_000 + delta_ns as i64, 9_000_000_000, sample_rate,),
                Some(expected_frames)
            );
            let gap_ns =
                (256_u128 * 1_000_000_000 + u128::from(sample_rate) / 2) / u128::from(sample_rate);
            assert_eq!(
                pipewire_pts_to_frame(12_000_000_000 + gap_ns as i64, 12_000_000_000, sample_rate),
                Some(256)
            );
        }
    }

    #[test]
    fn pipewire_pts_conversion_rejects_regressions_and_invalid_rates() {
        assert_eq!(pipewire_pts_to_frame(99, 100, 48_000), None);
        assert_eq!(pipewire_pts_to_frame(100, 100, 0), None);
        assert_eq!(pipewire_pts_to_frame(i64::MAX, i64::MIN, u32::MAX), None);
    }
}

fn serialize_header_metadata() -> Result<Vec<u8>, String> {
    let meta = pw::spa::pod::Value::Object(pw::spa::pod::Object {
        type_: SpaTypes::ObjectParamMeta.as_raw(),
        id: pw::spa::param::ParamType::Meta.as_raw(),
        properties: vec![
            Property::new(
                pw::spa::sys::SPA_PARAM_META_type,
                Value::Id(Id(pw::spa::sys::SPA_META_Header)),
            ),
            Property::new(
                pw::spa::sys::SPA_PARAM_META_size,
                Value::Int(std::mem::size_of::<MetaHeader>() as i32),
            ),
        ],
    });
    pw::spa::pod::serialize::PodSerializer::serialize(std::io::Cursor::new(Vec::new()), &meta)
        .map(|(cursor, _)| cursor.into_inner())
        .map_err(|error| error.to_string())
}
