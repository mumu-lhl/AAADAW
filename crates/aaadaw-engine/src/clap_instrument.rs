//! Minimal in-process CLAP instrument loading and sample-accurate processing.
//!
//! Plugin entries and instances are loaded and owned by the control thread. The audio processor
//! and all buffers are prepared before processing, can move to the render thread, and must be
//! returned stopped before the owner is deactivated.

use crate::{MidiEventKind, ScheduledMidiEvent};
use clack_extensions::audio_ports::{AudioPortInfoBuffer, PluginAudioPorts};
use clack_extensions::note_ports::{NoteDialect, NotePortInfoBuffer, PluginNotePorts};
use clack_host::events::Pckn;
use clack_host::events::event_types::{NoteOffEvent, NoteOnEvent};
use clack_host::events::io::{EventBuffer, InputEvents, OutputEvents, TryPushError};
use clack_host::plugin::features;
use clack_host::prelude::{
    AudioPortBuffer, AudioPortBufferType, AudioPorts, HostInfo, InputAudioBuffers,
    PluginAudioConfiguration, PluginAudioProcessor, PluginEntry, PluginInstance,
};
use std::ffi::CString;
use std::fmt;
use std::path::{Path, PathBuf};

/// A CLAP instrument available in a single plugin entry file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClapInstrumentDescriptor {
    /// Path to the CLAP entry library selected by the user.
    pub entry_path: PathBuf,
    /// Stable CLAP plugin identifier stored in a project.
    pub plugin_id: String,
    /// Display name supplied by the plugin.
    pub name: String,
}

/// A CLAP host setup or processing failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClapInstrumentError {
    message: String,
}

impl ClapInstrumentError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for ClapInstrumentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ClapInstrumentError {}

/// Control-thread ownership required to deactivate and destroy one CLAP instrument.
pub struct ClapInstrumentOwner {
    instance: Option<PluginInstance<()>>,
}

/// A started or stopped instrument processor and its preallocated render buffers.
pub struct ClapInstrumentProcessor {
    processor: PluginAudioProcessor<()>,
    output_ports: AudioPorts,
    left: Vec<f32>,
    right: Vec<f32>,
    event_scratch: Vec<ScheduledMidiEvent>,
    input_events: EventBuffer,
    active_notes: Vec<ActiveNote>,
    active_note_scratch: Vec<ActiveNote>,
    input_note_port: u16,
    max_block_frames: usize,
    max_events: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ActiveNote {
    note_id: u32,
    pitch: u8,
}

/// A processor stopped on the audio thread and ready to return to its control-thread owner.
pub struct StoppedClapInstrumentProcessor {
    processor: clack_host::prelude::StoppedPluginAudioProcessor<()>,
}

/// The fixed output callback used for plugin MIDI/audio events we do not expose yet.
struct DiscardPluginOutputEvents;

impl clack_host::events::io::OutputEventBuffer for DiscardPluginOutputEvents {
    fn try_push(&mut self, _event: &clack_host::events::UnknownEvent) -> Result<(), TryPushError> {
        Ok(())
    }
}

impl ClapInstrumentOwner {
    /// Loads and activates a stereo-output CLAP instrument from a library entry file.
    ///
    /// Loading runs third-party native code in this process. Call this only after the user chooses
    /// a trusted plugin entry; plugins are not crash-isolated. The caller must retain this owner on
    /// its control thread and later pass back the processor returned by [`ClapInstrumentProcessor::stop`].
    ///
    /// # Safety
    ///
    /// The entry at `entry_path` must be a valid, trusted CLAP library. A malformed or malicious
    /// library can crash the process or cause undefined behavior while being loaded or called.
    pub unsafe fn load(
        entry_path: &Path,
        plugin_id: &str,
        sample_rate: u32,
        max_block_frames: usize,
        max_events: usize,
    ) -> Result<(Self, ClapInstrumentProcessor), ClapInstrumentError> {
        // SAFETY: the caller guarantees that the selected file is a trusted CLAP library.
        let entry = unsafe { PluginEntry::load(entry_path) }.map_err(|error| {
            ClapInstrumentError::new(format!("Could not load CLAP entry: {error}"))
        })?;
        Self::load_from_entry(entry, plugin_id, sample_rate, max_block_frames, max_events)
    }

    fn load_from_entry(
        entry: PluginEntry,
        plugin_id: &str,
        sample_rate: u32,
        max_block_frames: usize,
        max_events: usize,
    ) -> Result<(Self, ClapInstrumentProcessor), ClapInstrumentError> {
        if sample_rate == 0 || max_block_frames == 0 {
            return Err(ClapInstrumentError::new(
                "CLAP sample rate and maximum block size must be positive",
            ));
        }
        let max_block_frames_u32 = u32::try_from(max_block_frames).map_err(|_| {
            ClapInstrumentError::new("CLAP maximum block size exceeds the supported range")
        })?;
        let c_plugin_id = CString::new(plugin_id)
            .map_err(|_| ClapInstrumentError::new("CLAP plugin ID contains a null byte"))?;
        let factory = entry
            .get_plugin_factory()
            .ok_or_else(|| ClapInstrumentError::new("CLAP entry has no plugin factory"))?;
        let descriptor = factory
            .plugin_descriptors()
            .find(|descriptor| {
                descriptor
                    .id()
                    .is_some_and(|id| id.to_bytes() == plugin_id.as_bytes())
            })
            .ok_or_else(|| {
                ClapInstrumentError::new(format!("CLAP plugin {plugin_id:?} was not found"))
            })?;
        if !descriptor
            .features()
            .any(|feature| feature == features::INSTRUMENT)
        {
            return Err(ClapInstrumentError::new(format!(
                "CLAP plugin {plugin_id:?} is not marked as an instrument"
            )));
        }

        let host_info = HostInfo::new(
            "AAADAW",
            "AAADAW",
            "https://github.com/mumu-lhl/AAADAW",
            "0.1.0",
        )
        .map_err(|error| {
            ClapInstrumentError::new(format!("Invalid CLAP host metadata: {error}"))
        })?;
        let mut instance =
            PluginInstance::<()>::new(|_| (), |_| (), &entry, &c_plugin_id, &host_info).map_err(
                |error| {
                    ClapInstrumentError::new(format!("Could not create CLAP instrument: {error}"))
                },
            )?;
        let input_note_port = validate_stereo_synth_ports(&mut instance)?;

        let processor = instance
            .activate(
                |_, _| (),
                PluginAudioConfiguration {
                    sample_rate: f64::from(sample_rate),
                    min_frames_count: 1,
                    max_frames_count: max_block_frames_u32,
                },
            )
            .map_err(|error| {
                ClapInstrumentError::new(format!("Could not activate CLAP instrument: {error}"))
            })?;
        Ok((
            Self {
                instance: Some(instance),
            },
            ClapInstrumentProcessor {
                processor: processor.into(),
                output_ports: AudioPorts::with_capacity(2, 1),
                left: vec![0.0; max_block_frames],
                right: vec![0.0; max_block_frames],
                event_scratch: Vec::with_capacity(max_events),
                input_events: EventBuffer::with_capacity(max_events),
                active_notes: Vec::with_capacity(max_events),
                active_note_scratch: Vec::with_capacity(max_events),
                input_note_port,
                max_block_frames,
                max_events,
            },
        ))
    }

    /// Deactivates and destroys the instrument on the calling control thread.
    pub fn deactivate(mut self, processor: StoppedClapInstrumentProcessor) {
        let instance = self
            .instance
            .as_mut()
            .expect("CLAP instrument owner is deactivated once");
        instance.deactivate(processor.processor);
        self.instance.take();
    }
}

impl ClapInstrumentProcessor {
    /// Processes one block of scheduled notes into the caller's interleaved stereo output.
    ///
    /// The caller must route only this processor's track events. Input events are sorted by sample
    /// offset before processing. The block and event limits are fixed at activation; invalid input
    /// fails before calling the plugin.
    pub fn process(
        &mut self,
        events: &[ScheduledMidiEvent],
        output: &mut [[f32; 2]],
    ) -> Result<(), ClapInstrumentError> {
        if output.len() > self.max_block_frames {
            return Err(ClapInstrumentError::new(format!(
                "CLAP block has {} frames; maximum is {}",
                output.len(),
                self.max_block_frames
            )));
        }
        if events.len() > self.max_events {
            return Err(ClapInstrumentError::new(format!(
                "CLAP block has {} note events; preallocated capacity is {}",
                events.len(),
                self.max_events
            )));
        }
        if events
            .iter()
            .any(|event| event.sample_offset >= output.len())
        {
            return Err(ClapInstrumentError::new(
                "CLAP note event offset is outside the current block",
            ));
        }
        if output.is_empty() {
            return Ok(());
        }

        self.event_scratch.clear();
        self.event_scratch.extend_from_slice(events);
        self.event_scratch.sort_unstable_by_key(|event| {
            (
                event.sample_offset,
                event.kind,
                event.track_id.value(),
                event.pitch,
                event.note_id.value(),
            )
        });

        self.active_note_scratch.clear();
        self.active_note_scratch
            .extend_from_slice(&self.active_notes);
        for event in &self.event_scratch {
            let note_id = u32::try_from(event.note_id.value())
                .ok()
                .filter(|id| *id <= i32::MAX as u32)
                .ok_or_else(|| {
                    ClapInstrumentError::new("MIDI note ID exceeds the CLAP note-ID range")
                })?;
            let active_note = ActiveNote {
                note_id,
                pitch: event.pitch,
            };
            match event.kind {
                MidiEventKind::NoteOn => {
                    if !self.active_note_scratch.contains(&active_note) {
                        if self.active_note_scratch.len() == self.max_events {
                            return Err(ClapInstrumentError::new(
                                "active CLAP notes exceed the preallocated note capacity",
                            ));
                        }
                        self.active_note_scratch.push(active_note);
                    }
                }
                MidiEventKind::NoteOff => {
                    if let Some(index) = self
                        .active_note_scratch
                        .iter()
                        .position(|active| *active == active_note)
                    {
                        self.active_note_scratch.swap_remove(index);
                    }
                }
            }
        }

        self.input_events.clear();
        for event in &self.event_scratch {
            let note_id = u32::try_from(event.note_id.value())
                .ok()
                .filter(|id| *id <= i32::MAX as u32)
                .ok_or_else(|| {
                    ClapInstrumentError::new("MIDI note ID exceeds the CLAP note-ID range")
                })?;
            let pckn = Pckn::new(self.input_note_port, 0_u16, u16::from(event.pitch), note_id);
            let sample_offset = u32::try_from(event.sample_offset).map_err(|_| {
                ClapInstrumentError::new("MIDI event offset exceeds the CLAP range")
            })?;
            match event.kind {
                MidiEventKind::NoteOn => self.input_events.push(&NoteOnEvent::new(
                    sample_offset,
                    pckn,
                    f64::from(event.velocity) / 127.0,
                )),
                MidiEventKind::NoteOff => {
                    self.input_events
                        .push(&NoteOffEvent::new(sample_offset, pckn, 0.0))
                }
            }
        }
        output.fill([0.0, 0.0]);
        let result = self.process_prepared_events(output);
        std::mem::swap(&mut self.active_notes, &mut self.active_note_scratch);
        self.active_note_scratch.clear();
        result
    }

    /// Sends note-offs for every currently held note without advancing the project transport.
    pub(crate) fn all_notes_off(&mut self) -> Result<(), ClapInstrumentError> {
        if self.active_notes.is_empty() {
            return Ok(());
        }
        self.input_events.clear();
        for note in &self.active_notes {
            self.input_events.push(&NoteOffEvent::new(
                0,
                Pckn::new(
                    self.input_note_port,
                    0_u16,
                    u16::from(note.pitch),
                    note.note_id,
                ),
                0.0,
            ));
        }
        let mut discarded_audio = [[0.0, 0.0]];
        self.process_prepared_events(&mut discarded_audio)?;
        self.active_notes.clear();
        Ok(())
    }

    fn process_prepared_events(
        &mut self,
        output: &mut [[f32; 2]],
    ) -> Result<(), ClapInstrumentError> {
        self.left[..output.len()].fill(0.0);
        self.right[..output.len()].fill(0.0);

        let input_events = InputEvents::from_buffer(&self.input_events);
        let mut plugin_output_events = DiscardPluginOutputEvents;
        let mut output_events = OutputEvents::from_buffer(&mut plugin_output_events);
        let mut audio_outputs = self.output_ports.with_output_buffers([AudioPortBuffer {
            latency: 0,
            channels: AudioPortBufferType::f32_output_only(
                [
                    self.left[..output.len()].as_mut(),
                    self.right[..output.len()].as_mut(),
                ]
                .into_iter(),
            ),
        }]);
        let audio_processor = self
            .processor
            .ensure_processing_started()
            .map_err(|error| {
                ClapInstrumentError::new(format!("Could not start CLAP processing: {error}"))
            })?;
        audio_processor
            .process(
                &InputAudioBuffers::empty(),
                &mut audio_outputs,
                &input_events,
                &mut output_events,
                None,
                None,
            )
            .map_err(|error| {
                ClapInstrumentError::new(format!("CLAP instrument processing failed: {error}"))
            })?;

        for (frame, (left, right)) in output
            .iter_mut()
            .zip(self.left.iter().copied().zip(self.right.iter().copied()))
        {
            *frame = [left, right];
        }
        Ok(())
    }

    /// Stops processing on the audio thread and returns the processor for control-thread teardown.
    pub fn stop(self) -> StoppedClapInstrumentProcessor {
        self.stop_with_status().0
    }

    pub(crate) fn stop_with_status(mut self) -> (StoppedClapInstrumentProcessor, bool) {
        let released = self.all_notes_off().is_ok();
        let stopped = StoppedClapInstrumentProcessor {
            processor: self.processor.into_stopped(),
        };
        (stopped, released)
    }

    pub(crate) fn max_block_frames(&self) -> usize {
        self.max_block_frames
    }

    pub(crate) fn max_events(&self) -> usize {
        self.max_events
    }
}

/// Lists instrument descriptors exposed by one explicitly selected CLAP entry file.
///
/// Loading a CLAP entry executes its native initialization code. Call this from a worker after the
/// user selects a trusted plugin library. Process isolation is not provided.
///
/// # Safety
///
/// `entry_path` must point to a valid, trusted CLAP library.
pub unsafe fn inspect_clap_instrument_entry(
    entry_path: &Path,
) -> Result<Vec<ClapInstrumentDescriptor>, ClapInstrumentError> {
    // SAFETY: upheld by this function's caller.
    let entry = unsafe { PluginEntry::load(entry_path) }
        .map_err(|error| ClapInstrumentError::new(format!("Could not load CLAP entry: {error}")))?;
    let factory = entry
        .get_plugin_factory()
        .ok_or_else(|| ClapInstrumentError::new("CLAP entry has no plugin factory"))?;
    let mut descriptors = Vec::new();
    for descriptor in factory.plugin_descriptors() {
        if !descriptor
            .features()
            .any(|feature| feature == features::INSTRUMENT)
        {
            continue;
        }
        let (Some(plugin_id), Some(name)) = (descriptor.id(), descriptor.name()) else {
            continue;
        };
        descriptors.push(ClapInstrumentDescriptor {
            entry_path: entry_path.to_owned(),
            plugin_id: plugin_id.to_string_lossy().into_owned(),
            name: name.to_string_lossy().into_owned(),
        });
    }
    Ok(descriptors)
}

fn validate_stereo_synth_ports(
    instance: &mut PluginInstance<()>,
) -> Result<u16, ClapInstrumentError> {
    let plugin = instance.plugin_handle();
    let ports = plugin
        .get_extension::<PluginAudioPorts>()
        .ok_or_else(|| ClapInstrumentError::new("CLAP instrument does not expose audio ports"))?;
    let input_count = ports.count(&plugin, true);
    let output_count = ports.count(&plugin, false);
    if input_count != 0 || output_count != 1 {
        return Err(ClapInstrumentError::new(format!(
            "CLAP instrument needs zero audio inputs and one output bus; found {input_count} inputs and {output_count} outputs"
        )));
    }
    let mut buffer = AudioPortInfoBuffer::new();
    let output = ports
        .get(&plugin, 0, false, &mut buffer)
        .ok_or_else(|| ClapInstrumentError::new("CLAP instrument output bus could not be read"))?;
    if output.channel_count != 2 {
        return Err(ClapInstrumentError::new(format!(
            "CLAP instrument output needs two channels; found {}",
            output.channel_count
        )));
    }
    if !output
        .flags
        .contains(clack_extensions::audio_ports::AudioPortFlags::IS_MAIN)
    {
        return Err(ClapInstrumentError::new(
            "CLAP instrument does not expose its stereo output as the main bus",
        ));
    }
    let note_ports = plugin
        .get_extension::<PluginNotePorts>()
        .ok_or_else(|| ClapInstrumentError::new("CLAP instrument does not expose note ports"))?;
    let mut note_buffer = NotePortInfoBuffer::new();
    let mut input_note_port = None;
    for index in 0..note_ports.count(&plugin, true) {
        let Some(note_port) = note_ports.get(&plugin, index, true, &mut note_buffer) else {
            continue;
        };
        if note_port.supported_dialects.supports(NoteDialect::Clap) {
            input_note_port = Some(u16::try_from(index).map_err(|_| {
                ClapInstrumentError::new("CLAP instrument has too many note input ports")
            })?);
            break;
        }
    }
    input_note_port.ok_or_else(|| {
        ClapInstrumentError::new("CLAP instrument has no note input port supporting CLAP events")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AudioItemStream, AudioRenderGraph, TrackInstrumentProcessor, pcm_stream};
    use aaadaw_core::{DawAction, MidiNoteData, NoteId, Project, TrackId};
    use clack_extensions::audio_ports::{
        AudioPortFlags, AudioPortInfo, AudioPortInfoWriter, AudioPortType, PluginAudioPortsImpl,
    };
    use clack_extensions::note_ports::{
        NoteDialects, NotePortInfo, NotePortInfoWriter, PluginNotePortsImpl,
    };
    use clack_plugin::entry::{DefaultPluginFactory, SinglePluginEntry};
    use clack_plugin::events::spaces::CoreEventSpace;
    use clack_plugin::plugin::PluginMainThread;
    use clack_plugin::plugin::{PluginDescriptor, features as plugin_features};
    use clack_plugin::prelude::{
        Audio, HostAudioProcessorHandle, HostMainThreadHandle, HostSharedHandle, Plugin,
        PluginAudioProcessor as ClackPluginAudioProcessor, PluginError, PluginExtensions, Process,
        ProcessStatus,
    };
    use clack_plugin::utils::ClapId;

    const PLUGIN_ID: &str = "org.aaadaw.test.synth";
    const MONO_PLUGIN_ID: &str = "org.aaadaw.test.mono-synth";
    const EFFECT_PLUGIN_ID: &str = "org.aaadaw.test.effect";

    fn test_project(note_duration: u64) -> (Project, TrackId, NoteId) {
        let mut project = Project::new();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "MIDI".to_owned(),
            })
            .expect("track creation");
        let track_id = project.tracks()[0].id();
        project
            .apply(DawAction::InsertMidiItem {
                track_id,
                start_tick: 0,
                length_ticks: 960,
            })
            .expect("MIDI item insertion");
        let item_id = project.midi_items()[0].id();
        project
            .apply(DawAction::AddMidiNotes {
                item_id,
                notes: vec![MidiNoteData {
                    pitch: 64,
                    tick: 0,
                    duration: note_duration,
                    velocity: 127,
                }],
            })
            .expect("MIDI note insertion");
        let note_id = project.midi_items()[0].notes()[0].id();
        (project, track_id, note_id)
    }

    fn test_ids() -> (TrackId, NoteId) {
        let (_, track_id, note_id) = test_project(120);
        (track_id, note_id)
    }

    struct TestInstrument<const IS_INSTRUMENT: bool, const CHANNEL_COUNT: u32>;

    struct TestInstrumentMainThread<const CHANNEL_COUNT: u32>;

    struct TestInstrumentAudioProcessor {
        active_pitch: Option<u8>,
    }

    impl<const CHANNEL_COUNT: u32> PluginMainThread<'_, ()>
        for TestInstrumentMainThread<CHANNEL_COUNT>
    {
    }

    impl<const IS_INSTRUMENT: bool, const CHANNEL_COUNT: u32> Plugin
        for TestInstrument<IS_INSTRUMENT, CHANNEL_COUNT>
    {
        type AudioProcessor<'a> = TestInstrumentAudioProcessor;
        type Shared<'a> = ();
        type MainThread<'a> = TestInstrumentMainThread<CHANNEL_COUNT>;

        fn declare_extensions(
            builder: &mut PluginExtensions<Self>,
            _shared: Option<&Self::Shared<'_>>,
        ) {
            builder.register::<clack_extensions::audio_ports::PluginAudioPorts>();
            builder.register::<clack_extensions::note_ports::PluginNotePorts>();
        }
    }

    impl<const IS_INSTRUMENT: bool, const CHANNEL_COUNT: u32> DefaultPluginFactory
        for TestInstrument<IS_INSTRUMENT, CHANNEL_COUNT>
    {
        fn get_descriptor() -> PluginDescriptor {
            let id = if IS_INSTRUMENT {
                if CHANNEL_COUNT == 2 {
                    PLUGIN_ID
                } else {
                    MONO_PLUGIN_ID
                }
            } else {
                EFFECT_PLUGIN_ID
            };
            let descriptor = PluginDescriptor::new(id, "AAADAW Test Synth");
            if IS_INSTRUMENT {
                descriptor
                    .with_features([plugin_features::INSTRUMENT, plugin_features::SYNTHESIZER])
            } else {
                descriptor.with_features([plugin_features::AUDIO_EFFECT])
            }
        }

        fn new_shared(_host: HostSharedHandle<'_>) -> Result<Self::Shared<'_>, PluginError> {
            Ok(())
        }

        fn new_main_thread<'a>(
            _host: HostMainThreadHandle<'a>,
            _shared: &'a Self::Shared<'a>,
        ) -> Result<Self::MainThread<'a>, PluginError> {
            Ok(TestInstrumentMainThread)
        }
    }

    impl<const CHANNEL_COUNT: u32> PluginAudioPortsImpl for TestInstrumentMainThread<CHANNEL_COUNT> {
        fn count(&self, is_input: bool) -> u32 {
            u32::from(!is_input)
        }

        fn get(&self, index: u32, is_input: bool, writer: &mut AudioPortInfoWriter) {
            if index == 0 && !is_input {
                writer.set(&AudioPortInfo {
                    id: ClapId::new(0),
                    name: b"Stereo Out",
                    channel_count: CHANNEL_COUNT,
                    flags: AudioPortFlags::IS_MAIN,
                    port_type: (CHANNEL_COUNT == 2).then_some(AudioPortType::STEREO),
                    in_place_pair: None,
                });
            }
        }
    }

    impl<const CHANNEL_COUNT: u32> PluginNotePortsImpl for TestInstrumentMainThread<CHANNEL_COUNT> {
        fn count(&self, is_input: bool) -> u32 {
            u32::from(is_input)
        }

        fn get(&self, index: u32, is_input: bool, writer: &mut NotePortInfoWriter<'_>) {
            if index == 0 && is_input {
                writer.set(&NotePortInfo {
                    id: ClapId::new(1),
                    name: b"MIDI In",
                    supported_dialects: NoteDialects::CLAP,
                    preferred_dialect: Some(NoteDialect::Clap),
                });
            }
        }
    }

    impl<'a, const CHANNEL_COUNT: u32>
        ClackPluginAudioProcessor<'a, (), TestInstrumentMainThread<CHANNEL_COUNT>>
        for TestInstrumentAudioProcessor
    {
        fn activate(
            _host: HostAudioProcessorHandle<'a>,
            _main_thread: &TestInstrumentMainThread<CHANNEL_COUNT>,
            _shared: &'a (),
            _audio_config: clack_plugin::prelude::PluginAudioConfiguration,
        ) -> Result<Self, PluginError> {
            Ok(Self { active_pitch: None })
        }

        fn process(
            &mut self,
            _process: Process,
            mut audio: Audio,
            events: clack_plugin::process::Events,
        ) -> Result<ProcessStatus, PluginError> {
            let frames = audio.frames_count() as usize;
            let mut input_events = events.input.iter().peekable();
            let mut output = audio.output_port(0).expect("stereo output port");
            let mut channels = output
                .channels()
                .expect("valid output buffer")
                .into_f32()
                .expect("host uses f32 samples")
                .into_iter();
            let left = channels.next().expect("left output channel");
            let right = channels.next().expect("right output channel");

            for frame in 0..frames {
                while input_events
                    .peek()
                    .is_some_and(|event| event.header().time() as usize <= frame)
                {
                    match input_events.next().expect("peeked event").as_core_event() {
                        Some(CoreEventSpace::NoteOn(note)) => {
                            self.active_pitch =
                                Some(note.key().as_specific().copied().unwrap_or(0) as u8)
                        }
                        Some(CoreEventSpace::NoteOff(_)) => self.active_pitch = None,
                        _ => {}
                    }
                }
                let level = f32::from(self.active_pitch.unwrap_or(0)) / 127.0;
                left[frame] = level;
                right[frame] = level * 0.5;
            }
            Ok(ProcessStatus::Continue)
        }
    }

    fn test_plugin_entry<const IS_INSTRUMENT: bool, const CHANNEL_COUNT: u32>() -> PluginEntry {
        PluginEntry::load_from_clack::<
            SinglePluginEntry<TestInstrument<IS_INSTRUMENT, CHANNEL_COUNT>>,
        >(c"test")
        .expect("static test plugin entry")
    }

    #[test]
    fn processor_delivers_note_events_at_their_sample_offsets_and_stops_on_control_thread() {
        let entry = test_plugin_entry::<true, 2>();
        let (owner, processor) =
            ClapInstrumentOwner::load_from_entry(entry, PLUGIN_ID, 48_000, 16, 4)
                .expect("test synth should load");
        let (track_id, note_id) = test_ids();
        let events = [
            ScheduledMidiEvent {
                sample_offset: 7,
                track_id,
                note_id,
                pitch: 64,
                velocity: 127,
                kind: MidiEventKind::NoteOff,
            },
            ScheduledMidiEvent {
                sample_offset: 3,
                track_id,
                note_id,
                pitch: 64,
                velocity: 127,
                kind: MidiEventKind::NoteOn,
            },
            ScheduledMidiEvent {
                sample_offset: 3,
                track_id,
                note_id,
                pitch: 64,
                velocity: 0,
                kind: MidiEventKind::NoteOff,
            },
        ];
        let mut output = [[0.0; 2]; 10];

        let (stopped, rendered) = std::thread::spawn(move || {
            let mut processor = processor;
            processor
                .process(&events, &mut output)
                .expect("process block");
            (processor.stop(), output)
        })
        .join()
        .expect("audio processor thread");

        let level = 64.0 / 127.0;
        for (index, frame) in rendered.iter().enumerate() {
            let expected = if (3..7).contains(&index) { level } else { 0.0 };
            assert!((frame[0] - expected).abs() < 0.0001);
            assert!((frame[1] - expected * 0.5).abs() < 0.0001);
        }
        owner.deactivate(stopped);
    }

    #[test]
    fn processor_rejects_events_outside_the_preallocated_block() {
        let entry = test_plugin_entry::<true, 2>();
        let (owner, mut processor) =
            ClapInstrumentOwner::load_from_entry(entry, PLUGIN_ID, 48_000, 4, 1)
                .expect("test synth should load");
        let event = ScheduledMidiEvent {
            sample_offset: 4,
            track_id: test_ids().0,
            note_id: test_ids().1,
            pitch: 60,
            velocity: 100,
            kind: MidiEventKind::NoteOn,
        };
        let mut output = [[1.0; 2]; 4];

        let error = processor
            .process(&[event], &mut output)
            .expect_err("event at block end is outside the half-open block");

        assert!(error.to_string().contains("outside the current block"));
        assert_eq!(output, [[1.0; 2]; 4]);
        owner.deactivate(processor.stop());
    }

    #[test]
    fn loader_rejects_non_instruments_and_non_stereo_output() {
        let non_instrument_error = ClapInstrumentOwner::load_from_entry(
            test_plugin_entry::<false, 2>(),
            EFFECT_PLUGIN_ID,
            48_000,
            16,
            4,
        )
        .err()
        .expect("audio effect must not load as an instrument");
        assert!(
            non_instrument_error
                .to_string()
                .contains("not marked as an instrument")
        );

        let mono_error = ClapInstrumentOwner::load_from_entry(
            test_plugin_entry::<true, 1>(),
            MONO_PLUGIN_ID,
            48_000,
            16,
            4,
        )
        .err()
        .expect("mono instrument must be rejected");
        assert!(mono_error.to_string().contains("needs two channels"));
    }

    #[test]
    fn render_graph_mixes_sample_accurate_stereo_instrument_output_with_pcm() {
        let (mut project, track_id, _) = test_project(1);
        project
            .apply(DawAction::InsertAudioItem {
                track_id,
                media_ref: "asset://test".to_owned(),
                start_sample: 0,
                source_offset_samples: 0,
                length_samples: 32,
            })
            .expect("test audio item should be valid");
        let item_id = project.audio_items()[0].id();
        let (owner, processor) = ClapInstrumentOwner::load_from_entry(
            test_plugin_entry::<true, 2>(),
            PLUGIN_ID,
            48_000,
            32,
            2,
        )
        .expect("test synth should load");
        let (mut producer, consumer) = pcm_stream(32).expect("stream capacity is valid");
        assert_eq!(producer.push_samples(&[0.1; 32]), 32);
        let mut instruments = vec![TrackInstrumentProcessor::new(track_id, processor)];
        let mut graph = AudioRenderGraph::new_for_audio_items_with_instruments(
            &project,
            vec![AudioItemStream::new(item_id, consumer)],
            &mut instruments,
            32,
        )
        .expect("streams and instrument should build a graph");
        assert!(instruments.is_empty());
        graph.transport_mut().start();

        let mut output = [[0.0; 2]; 32];
        let stats = graph
            .render_into(&mut output)
            .expect("instrument graph should render");

        assert_eq!(stats.midi_event_count, 2);
        assert_eq!(stats.underrun_samples, 0);
        let midi_level = 64.0 / 127.0;
        let pcm_level = 0.1 * std::f32::consts::FRAC_1_SQRT_2;
        for (frame_index, frame) in output.iter().enumerate() {
            let instrument_level = if frame_index < 25 { midi_level } else { 0.0 };
            assert!((frame[0] - pcm_level - instrument_level).abs() < 0.0001);
            assert!((frame[1] - pcm_level - instrument_level * 0.5).abs() < 0.0001);
        }
        let stopped = graph.stop_instruments();
        assert_eq!(stopped, 0);
        let mut retired = graph.take_stopped_instruments();
        assert_eq!(retired.len(), 1);
        let (_, processor) = retired.pop().expect("stopped route exists").into_parts();
        owner.deactivate(processor);
        drop(producer);
    }

    #[test]
    fn transport_stop_releases_held_notes_and_retirement_returns_processor_to_owner() {
        let (project, track_id, _) = test_project(120);
        let (owner, processor) = ClapInstrumentOwner::load_from_entry(
            test_plugin_entry::<true, 2>(),
            PLUGIN_ID,
            48_000,
            32,
            2,
        )
        .expect("test synth should load");
        let mut instruments = vec![TrackInstrumentProcessor::new(track_id, processor)];
        let mut graph = AudioRenderGraph::new_for_audio_items_with_instruments(
            &project,
            Vec::new(),
            &mut instruments,
            32,
        )
        .expect("test graph should build");
        graph.transport_mut().start();

        let mut output = [[0.0; 2]; 8];
        graph
            .render_into(&mut output)
            .expect("first note block should render");
        assert!(output[0][0] > 0.0);
        assert_eq!(graph.release_midi_notes(), 0);
        graph.transport_mut().stop();
        graph
            .render_into(&mut output)
            .expect("stopped block should render silence");
        assert_eq!(output, [[0.0; 2]; 8]);

        graph.transport_mut().start();
        graph
            .render_into(&mut output)
            .expect("playback can resume after note release");
        assert_eq!(output, [[0.0; 2]; 8]);

        assert_eq!(graph.stop_instruments(), 0);
        let mut retired = graph.take_stopped_instruments();
        let (_, stopped) = retired
            .pop()
            .expect("retired processor exists")
            .into_parts();
        owner.deactivate(stopped);
    }
}
