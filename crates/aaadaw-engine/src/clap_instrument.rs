//! Minimal in-process CLAP instrument loading and sample-accurate processing.
//!
//! Plugin entries and instances are loaded and owned by the control thread. The audio processor
//! and all buffers are prepared before processing, can move to the render thread, and must be
//! returned stopped before the owner is deactivated.

use crate::{MidiEventKind, ScheduledMidiEvent};
use clack_extensions::audio_ports::{AudioPortInfoBuffer, PluginAudioPorts};
#[cfg(any(target_os = "linux", target_os = "windows"))]
use clack_extensions::gui::{GuiApiType, GuiConfiguration};
use clack_extensions::gui::{GuiSize, HostGui, HostGuiImpl, PluginGui};
use clack_extensions::note_ports::{NoteDialect, NotePortInfoBuffer, PluginNotePorts};
use clack_extensions::params::{ParamInfoBuffer, ParamInfoFlags, PluginParams};
#[cfg(target_os = "linux")]
use clack_extensions::posix_fd::{FdFlags, HostPosixFd, HostPosixFdImpl, PluginPosixFd};
use clack_extensions::state::PluginState;
use clack_extensions::tail::{PluginTail, TailLength};
use clack_host::events::Pckn;
use clack_host::events::event_types::{MidiEvent, NoteOffEvent, NoteOnEvent};
use clack_host::events::event_types::{
    ParamGestureBeginEvent, ParamGestureEndEvent, ParamValueEvent,
};
use clack_host::events::io::{EventBuffer, InputEvents, OutputEvents, TryPushError};
#[cfg(target_os = "linux")]
use clack_host::host::MainThreadHandler;
use clack_host::plugin::features;
use clack_host::prelude::{
    AudioPortBuffer, AudioPortBufferType, AudioPorts, HostError, HostExtensions, HostHandlers,
    HostInfo, InputAudioBuffers, InputChannel, PluginAudioConfiguration, PluginAudioProcessor,
    PluginEntry, PluginInstance, SharedHandler,
};
use rtrb::{Consumer, Producer, RingBuffer};
use std::borrow::Cow;
use std::ffi::CString;
use std::fmt;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Arc;
#[cfg(target_os = "linux")]
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
#[cfg(target_os = "linux")]
use std::{collections::HashMap, os::fd::RawFd};

/// Parameter metadata exposed by a CLAP effect.
#[derive(Clone, Debug, PartialEq)]
pub struct ClapParameterInfo {
    pub id: u32,
    pub name: String,
    pub min_value: f64,
    pub max_value: f64,
    pub default_value: f64,
    pub value: f64,
    pub display_value: String,
    pub stepped: bool,
    pub read_only: bool,
}

/// A bounded command sent from the control thread to an active CLAP processor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ClapParameterCommand {
    Begin { id: u32 },
    Set { id: u32, value: f64 },
    End { id: u32 },
    DisarmAutomation { parameter_id: u32 },
}

/// A parameter value captured at an absolute project sample while an FX lane is write-armed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClapParameterAutomationEvent {
    pub parameter_id: u32,
    pub sample: u64,
    pub value: f64,
}

/// Control-thread endpoint for bounded parameter automation capture from one effect.
pub struct ClapParameterAutomationReceiver {
    consumer: Consumer<CapturedParameterValue>,
    overflowed: Arc<AtomicBool>,
    playback_overflowed: Arc<AtomicBool>,
    capture_callback_active: Arc<AtomicBool>,
    write_disarm_acknowledged: Arc<AtomicBool>,
}

#[derive(Clone, Copy)]
struct CapturedParameterValue {
    parameter_id: u32,
    sample: u64,
    value: f64,
}

#[derive(Clone, Copy)]
struct ScheduledAutomationValue {
    offset: u32,
    parameter_id: u32,
    value: f64,
}

impl ClapParameterAutomationReceiver {
    /// Drains captured values without waiting for the audio thread.
    pub fn drain_into(&mut self, output: &mut Vec<ClapParameterAutomationEvent>) {
        while let Ok(event) = self.consumer.pop() {
            output.push(ClapParameterAutomationEvent {
                parameter_id: event.parameter_id,
                sample: event.sample,
                value: event.value,
            });
        }
    }

    /// Returns and clears the overflow flag set when the bounded capture queue filled.
    pub fn take_overflowed(&mut self) -> bool {
        self.overflowed.swap(false, Ordering::AcqRel)
    }

    /// Returns and clears the flag set when a block exceeded the bounded playback event budget.
    pub fn take_playback_overflowed(&mut self) -> bool {
        self.playback_overflowed.swap(false, Ordering::AcqRel)
    }

    /// Returns whether the audio thread is currently consuming parameter commands and capture.
    pub fn capture_callback_active(&self) -> bool {
        self.capture_callback_active.load(Ordering::SeqCst)
    }

    /// Returns whether the ordered write-disarm command has reached the audio thread.
    pub fn write_disarm_acknowledged(&self) -> bool {
        self.write_disarm_acknowledged.load(Ordering::SeqCst)
    }
}

/// Control-thread endpoint for one effect's parameter event queue.
pub struct ClapParameterSender {
    commands: Producer<ClapParameterCommand>,
    write_armed_parameter: Arc<AtomicU64>,
    write_disarm_acknowledged: Arc<AtomicBool>,
}

impl ClapParameterSender {
    pub fn try_send(&mut self, command: ClapParameterCommand) -> Result<(), ClapParameterCommand> {
        // Keep one slot available for the matching End while a parameter gesture is active.
        let reserved_slots = usize::from(!matches!(
            command,
            ClapParameterCommand::End { .. } | ClapParameterCommand::DisarmAutomation { .. }
        ));
        if self.commands.slots() <= reserved_slots {
            return Err(command);
        }
        let valid = match command {
            ClapParameterCommand::Begin { id } | ClapParameterCommand::End { id } => {
                clack_host::prelude::ClapId::from_raw(id).is_some()
            }
            ClapParameterCommand::Set { id, value } => {
                clack_host::prelude::ClapId::from_raw(id).is_some() && value.is_finite()
            }
            ClapParameterCommand::DisarmAutomation { parameter_id } => {
                clack_host::prelude::ClapId::from_raw(parameter_id).is_some()
            }
        };
        if !valid {
            return Err(command);
        }
        self.commands.push(command).map_err(|error| match error {
            rtrb::PushError::Full(command) => command,
        })
    }

    /// Selects the single parameter captured by this effect, or disarms capture.
    pub fn set_write_armed_parameter(&self, parameter_id: Option<u32>) {
        if parameter_id.is_some() {
            self.write_disarm_acknowledged
                .store(false, Ordering::SeqCst);
        }
        self.write_armed_parameter
            .store(parameter_id.map_or(u64::MAX, u64::from), Ordering::SeqCst);
    }

    /// Queues an ordered capture stop after all parameter edits already sent to this processor.
    pub fn try_disarm_write(&mut self, parameter_id: u32) -> bool {
        self.try_send(ClapParameterCommand::DisarmAutomation { parameter_id })
            .is_ok()
    }
}

static NEXT_CLAP_INSTANCE_ID: AtomicU64 = AtomicU64::new(1);
const GUI_EVENT_SHOW: u32 = 1;
const GUI_EVENT_HIDE: u32 = 2;
const GUI_EVENT_CLOSED: u32 = 4;
const GUI_EVENT_DESTROYED: u32 = 8;

struct IsolatedInstrumentHost;

#[derive(Default)]
struct IsolatedInstrumentHostShared {
    gui_events: AtomicU32,
    callback_requested: AtomicBool,
    #[cfg(target_os = "linux")]
    posix_fd_reactor: Arc<PosixFdReactor>,
}

#[cfg(target_os = "linux")]
#[derive(Default)]
struct PosixFdReactor {
    registrations: Mutex<HashMap<RawFd, (FdFlags, glib::SourceId)>>,
    ready_events: Arc<Mutex<Vec<(RawFd, FdFlags)>>>,
}

#[cfg(target_os = "linux")]
struct IsolatedInstrumentHostMainThread {
    posix_fd_reactor: Arc<PosixFdReactor>,
}

#[cfg(target_os = "linux")]
impl Drop for PosixFdReactor {
    fn drop(&mut self) {
        if let Ok(registrations) = self.registrations.get_mut() {
            for (_, (_, source)) in registrations.drain() {
                source.remove();
            }
        }
    }
}

impl<'a> SharedHandler<'a> for IsolatedInstrumentHostShared {
    fn request_restart(&self) {}
    fn request_process(&self) {}
    fn request_callback(&self) {
        self.callback_requested.store(true, Ordering::Release);
    }
}

impl HostGuiImpl for IsolatedInstrumentHostShared {
    fn resize_hints_changed(&self) {}

    fn request_resize(&self, _new_size: GuiSize) -> Result<(), HostError> {
        Err(HostError::Message(
            "Floating instrument editors cannot be resized by the host",
        ))
    }

    fn request_show(&self) -> Result<(), HostError> {
        self.gui_events.fetch_or(GUI_EVENT_SHOW, Ordering::Release);
        Ok(())
    }

    fn request_hide(&self) -> Result<(), HostError> {
        self.gui_events.fetch_or(GUI_EVENT_HIDE, Ordering::Release);
        Ok(())
    }

    fn closed(&self, was_destroyed: bool) {
        let event = GUI_EVENT_CLOSED
            | if was_destroyed {
                GUI_EVENT_DESTROYED
            } else {
                0
            };
        self.gui_events.fetch_or(event, Ordering::Release);
    }
}

#[cfg(target_os = "linux")]
impl MainThreadHandler<'_> for IsolatedInstrumentHostMainThread {}

#[cfg(target_os = "linux")]
impl HostPosixFdImpl for IsolatedInstrumentHostMainThread {
    fn register_fd(&self, fd: RawFd, flags: FdFlags) -> Result<(), HostError> {
        if fd < 0 || flags.is_empty() {
            return Err(HostError::Message("Invalid CLAP file descriptor watch"));
        }
        let mut registrations = self
            .posix_fd_reactor
            .registrations
            .lock()
            .map_err(|_| HostError::Message("CLAP file descriptor registry is unavailable"))?;
        if registrations.contains_key(&fd) {
            return Err(HostError::Message(
                "CLAP file descriptor is already registered",
            ));
        }
        let source = make_posix_fd_source(&self.posix_fd_reactor, fd, flags);
        registrations.insert(fd, (flags, source));
        Ok(())
    }

    fn modify_fd(&self, fd: RawFd, flags: FdFlags) -> Result<(), HostError> {
        if flags.is_empty() {
            return self.unregister_fd(fd);
        }
        let mut registrations = self
            .posix_fd_reactor
            .registrations
            .lock()
            .map_err(|_| HostError::Message("CLAP file descriptor registry is unavailable"))?;
        if !registrations.contains_key(&fd) {
            return Err(HostError::Message("CLAP file descriptor is not registered"));
        }
        let source = make_posix_fd_source(&self.posix_fd_reactor, fd, flags);
        if let Some((_, old_source)) = registrations.insert(fd, (flags, source)) {
            old_source.remove();
        }
        Ok(())
    }

    fn unregister_fd(&self, fd: RawFd) -> Result<(), HostError> {
        let Some((_, source)) = self
            .posix_fd_reactor
            .registrations
            .lock()
            .map_err(|_| HostError::Message("CLAP file descriptor registry is unavailable"))?
            .remove(&fd)
        else {
            return Err(HostError::Message("CLAP file descriptor is not registered"));
        };
        source.remove();
        if let Ok(mut ready_events) = self.posix_fd_reactor.ready_events.lock() {
            ready_events.retain(|(event_fd, _)| *event_fd != fd);
        }
        Ok(())
    }
}

#[cfg(target_os = "linux")]
fn make_posix_fd_source(reactor: &PosixFdReactor, fd: RawFd, flags: FdFlags) -> glib::SourceId {
    use glib::ControlFlow;

    let condition = io_condition_for_fd_flags(flags);
    let ready_events = reactor.ready_events.clone();
    glib::source::unix_fd_add_local(fd, condition, move |ready_fd, ready_condition| {
        let ready_flags = fd_flags_for_io_condition(ready_condition);
        if !ready_flags.is_empty()
            && let Ok(mut events) = ready_events.lock()
        {
            events.push((ready_fd, ready_flags));
        }
        ControlFlow::Continue
    })
}

#[cfg(target_os = "linux")]
fn io_condition_for_fd_flags(flags: FdFlags) -> glib::IOCondition {
    let mut condition = glib::IOCondition::empty();
    if flags.contains(FdFlags::READ) {
        condition |= glib::IOCondition::IN | glib::IOCondition::PRI;
    }
    if flags.contains(FdFlags::WRITE) {
        condition |= glib::IOCondition::OUT;
    }
    if flags.contains(FdFlags::ERROR) {
        condition |= glib::IOCondition::ERR | glib::IOCondition::HUP | glib::IOCondition::NVAL;
    }
    condition
}

#[cfg(target_os = "linux")]
fn fd_flags_for_io_condition(condition: glib::IOCondition) -> FdFlags {
    let mut flags = FdFlags::empty();
    if condition.intersects(glib::IOCondition::IN | glib::IOCondition::PRI) {
        flags |= FdFlags::READ;
    }
    if condition.contains(glib::IOCondition::OUT) {
        flags |= FdFlags::WRITE;
    }
    if condition
        .intersects(glib::IOCondition::ERR | glib::IOCondition::HUP | glib::IOCondition::NVAL)
    {
        flags |= FdFlags::ERROR;
    }
    flags
}

impl HostHandlers for IsolatedInstrumentHost {
    type Shared<'a> = IsolatedInstrumentHostShared;
    #[cfg(target_os = "linux")]
    type MainThread<'a> = IsolatedInstrumentHostMainThread;
    #[cfg(not(target_os = "linux"))]
    type MainThread<'a> = ();
    type AudioProcessor<'a> = ();

    fn declare_extensions(builder: &mut HostExtensions<Self>, _shared: &Self::Shared<'_>) {
        builder.register::<HostGui>();
        #[cfg(target_os = "linux")]
        builder.register::<HostPosixFd>();
    }
}

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

/// A CLAP plugin exposed by one plugin entry file, for discovery and selection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClapPluginDescriptor {
    /// Path to the CLAP entry library or bundle selected by the user.
    pub entry_path: PathBuf,
    /// Stable CLAP plugin identifier stored in a project.
    pub plugin_id: String,
    /// Display name supplied by the plugin.
    pub name: String,
    /// Optional vendor name supplied by the plugin.
    pub vendor: Option<String>,
    /// CLAP feature tags supplied by the plugin.
    pub features: Vec<String>,
}

impl ClapPluginDescriptor {
    /// Returns whether the plugin advertises itself as an instrument.
    pub fn is_instrument(&self) -> bool {
        self.features.iter().any(|feature| feature == "instrument")
    }

    /// Returns whether the plugin advertises itself as an audio effect.
    pub fn is_audio_effect(&self) -> bool {
        self.features
            .iter()
            .any(|feature| feature == "audio-effect")
    }
}

/// A CLAP host setup or processing failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClapInstrumentError {
    message: Cow<'static, str>,
    state_restore: bool,
}

impl ClapInstrumentError {
    pub(crate) fn new(message: impl Into<Cow<'static, str>>) -> Self {
        Self {
            message: message.into(),
            state_restore: false,
        }
    }

    pub(crate) fn state_restore(message: impl Into<Cow<'static, str>>) -> Self {
        Self {
            message: message.into(),
            state_restore: true,
        }
    }

    /// Returns whether this error came from applying a project's saved state.
    pub fn is_state_restore_error(&self) -> bool {
        self.state_restore
    }
}

impl fmt::Display for ClapInstrumentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.message.as_ref())
    }
}

impl std::error::Error for ClapInstrumentError {}

/// Control-thread ownership required to deactivate and destroy one CLAP instrument.
pub struct ClapInstrumentOwner {
    instance: Option<PluginInstance<IsolatedInstrumentHost>>,
    instance_id: u64,
    gui_created: bool,
    gui_visible: bool,
    gui_status: u32,
}

/// A started or stopped instrument processor and its preallocated render buffers.
pub struct ClapInstrumentProcessor {
    instance_id: u64,
    processor: PluginAudioProcessor<IsolatedInstrumentHost>,
    output_ports: AudioPorts,
    left: Vec<f32>,
    right: Vec<f32>,
    event_scratch: Vec<ScheduledMidiEvent>,
    input_events: EventBuffer,
    active_notes: Vec<ActiveNote>,
    active_note_scratch: Vec<ActiveNote>,
    input_note_port: u16,
    input_midi_port: Option<u16>,
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
    instance_id: u64,
    processor: clack_host::prelude::StoppedPluginAudioProcessor<IsolatedInstrumentHost>,
}

/// Control-thread ownership required to deactivate and destroy one CLAP audio effect.
pub struct ClapEffectOwner {
    instance: Option<PluginInstance<()>>,
    instance_id: u64,
}

/// A stereo CLAP effect processor with preallocated input and output buffers.
pub struct ClapEffectProcessor {
    instance_id: u64,
    processor: PluginAudioProcessor<()>,
    input_ports: AudioPorts,
    output_ports: AudioPorts,
    input_left: Vec<f32>,
    input_right: Vec<f32>,
    output_left: Vec<f32>,
    output_right: Vec<f32>,
    input_events: EventBuffer,
    parameter_sender: Option<ClapParameterSender>,
    parameter_commands: Consumer<ClapParameterCommand>,
    automation_capture: Producer<CapturedParameterValue>,
    automation_capture_overflowed: Arc<AtomicBool>,
    write_armed_parameter: Arc<AtomicU64>,
    write_disarm_acknowledged: Arc<AtomicBool>,
    automation_capture_receiver: Option<ClapParameterAutomationReceiver>,
    capture_callback_active: Arc<AtomicBool>,
    parameter_automation: Vec<aaadaw_core::FxParameterAutomationLane>,
    automation_event_scratch: Vec<ScheduledAutomationValue>,
    automation_playback_overflowed: Arc<AtomicBool>,
    active_parameter_gestures: Vec<u32>,
    max_block_frames: usize,
}

/// A stereo CLAP effect stopped on the audio thread and ready for control-thread deactivation.
pub struct StoppedClapEffectProcessor {
    instance_id: u64,
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
    /// Opens or closes this instrument's floating native editor on the plugin main thread.
    /// Status values are shared with the helper protocol: 0 closed, 1 visible, 2 unsupported,
    /// and 3 failed.
    pub(crate) fn set_floating_gui(&mut self, open: bool, parent: u64) -> u32 {
        let status = self.apply_floating_gui(open, parent);
        self.gui_status = status;
        self.gui_visible = status == 1;
        status
    }

    fn apply_floating_gui(&mut self, open: bool, parent: u64) -> u32 {
        let _ = parent;
        let Some(instance) = self.instance.as_mut() else {
            return 3;
        };
        let plugin = instance.plugin_handle();
        let Some(gui) = plugin.get_extension::<PluginGui>() else {
            return 2;
        };
        if !open {
            if self.gui_created {
                let _ = gui.hide(&plugin);
                gui.destroy(&plugin);
                self.gui_created = false;
            }
            return 0;
        }
        if self.gui_created {
            return if gui.show(&plugin).is_ok() { 1 } else { 3 };
        }
        #[cfg(target_os = "windows")]
        let native_api = GuiApiType::WIN32;
        #[cfg(target_os = "linux")]
        let native_api = GuiApiType::X11;
        #[cfg(not(any(target_os = "windows", target_os = "linux")))]
        return 2;

        #[cfg(any(target_os = "linux", target_os = "windows"))]
        {
            let preferred = gui.get_preferred_api(&plugin);
            let configuration = preferred
                .filter(|configuration| {
                    configuration.is_floating
                        && configuration.api_type == native_api
                        && gui.is_api_supported(&plugin, *configuration)
                })
                .unwrap_or(GuiConfiguration {
                    api_type: native_api,
                    is_floating: true,
                });
            if !gui.is_api_supported(&plugin, configuration) {
                return 2;
            }
            if gui.create(&plugin, configuration).is_err() {
                return 3;
            }
            self.gui_created = true;
            if parent != 0 {
                #[cfg(target_os = "linux")]
                let parent_window = clack_extensions::gui::Window::from_x11_handle(parent as _);
                #[cfg(target_os = "windows")]
                // SAFETY: the DAW main window remains alive until its helper processes are stopped.
                let parent_window = unsafe {
                    clack_extensions::gui::Window::from_win32_hwnd(parent as usize as *mut _)
                };
                // SAFETY: `parent_window` refers to the live DAW main window, retained until helper shutdown.
                let _ = unsafe { gui.set_transient(&plugin, parent_window) };
            }
            if let Ok(title) = CString::new("AAADAW Instrument") {
                gui.suggest_title(&plugin, &title);
            }
            if gui.show(&plugin).is_err() {
                gui.destroy(&plugin);
                self.gui_created = false;
                return 3;
            }
            1
        }
    }

    pub(crate) fn service_gui_host_callbacks(&mut self) -> Option<u32> {
        let instance = self.instance.as_mut()?;
        let events =
            instance.access_shared_handler(|shared| shared.gui_events.swap(0, Ordering::AcqRel));
        if events == 0 {
            return None;
        }
        let plugin = instance.plugin_handle();
        if let Some(gui) = plugin.get_extension::<PluginGui>() {
            if events & GUI_EVENT_DESTROYED != 0 && self.gui_created {
                gui.destroy(&plugin);
                self.gui_created = false;
                self.gui_visible = false;
                self.gui_status = 0;
            } else if events & GUI_EVENT_CLOSED != 0 {
                self.gui_visible = false;
                self.gui_status = 0;
            }
            if events & GUI_EVENT_SHOW != 0 && self.gui_created {
                self.gui_visible = gui.show(&plugin).is_ok();
                self.gui_status = if self.gui_visible { 1 } else { 3 };
            }
            if events & GUI_EVENT_HIDE != 0 && self.gui_created {
                self.gui_visible = gui.hide(&plugin).is_err();
                self.gui_status = if self.gui_visible { 3 } else { 0 };
            }
        }
        Some(self.gui_status)
    }

    /// Services callbacks requested by the plugin on its main thread.
    /// Runs a callback requested by the plugin on the CLAP main thread.
    pub fn service_main_thread_callback(&mut self) {
        if let Some(instance) = self.instance.as_mut() {
            let callback_requested = instance.access_shared_handler(|shared| {
                shared.callback_requested.swap(false, Ordering::AcqRel)
            });
            if callback_requested {
                instance.call_on_main_thread_callback();
            }
        }
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn service_posix_fd_callbacks(&mut self) {
        let Some(instance) = self.instance.as_mut() else {
            return;
        };
        let ready_events = instance.access_shared_handler(|shared| {
            let Ok(mut ready_events) = shared.posix_fd_reactor.ready_events.lock() else {
                return Vec::new();
            };
            let events = std::mem::take(&mut *ready_events);
            let Ok(registrations) = shared.posix_fd_reactor.registrations.lock() else {
                return Vec::new();
            };
            events
                .into_iter()
                .filter_map(|(fd, flags)| {
                    registrations
                        .get(&fd)
                        .map(|(registered, _)| (fd, flags & *registered))
                })
                .filter(|(_, flags)| !flags.is_empty())
                .collect::<Vec<_>>()
        });
        if ready_events.is_empty() {
            return;
        }
        let plugin = instance.plugin_handle();
        if let Some(posix_fd) = plugin.get_extension::<PluginPosixFd>() {
            for (fd, flags) in ready_events {
                posix_fd.on_fd(&plugin, fd, flags);
            }
        }
    }

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

    /// Loads a CLAP instrument and restores opaque state before activation when supplied.
    ///
    /// # Safety
    ///
    /// The entry at `entry_path` must be a valid, trusted CLAP library.
    pub unsafe fn load_with_state(
        entry_path: &Path,
        plugin_id: &str,
        state: Option<&[u8]>,
        sample_rate: u32,
        max_block_frames: usize,
        max_events: usize,
    ) -> Result<(Self, ClapInstrumentProcessor), ClapInstrumentError> {
        // SAFETY: the caller guarantees that the selected file is a trusted CLAP library.
        let entry = unsafe { PluginEntry::load(entry_path) }.map_err(|error| {
            ClapInstrumentError::new(format!("Could not load CLAP entry: {error}"))
        })?;
        Self::load_from_entry_with_state(
            entry,
            plugin_id,
            state,
            sample_rate,
            max_block_frames,
            max_events,
        )
    }

    fn load_from_entry(
        entry: PluginEntry,
        plugin_id: &str,
        sample_rate: u32,
        max_block_frames: usize,
        max_events: usize,
    ) -> Result<(Self, ClapInstrumentProcessor), ClapInstrumentError> {
        Self::load_from_entry_with_state(
            entry,
            plugin_id,
            None,
            sample_rate,
            max_block_frames,
            max_events,
        )
    }

    fn load_from_entry_with_state(
        entry: PluginEntry,
        plugin_id: &str,
        state: Option<&[u8]>,
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
        let mut instance = PluginInstance::<IsolatedInstrumentHost>::new(
            |_| IsolatedInstrumentHostShared::default(),
            #[cfg(target_os = "linux")]
            |shared| IsolatedInstrumentHostMainThread {
                posix_fd_reactor: shared.posix_fd_reactor.clone(),
            },
            #[cfg(not(target_os = "linux"))]
            |_| (),
            &entry,
            &c_plugin_id,
            &host_info,
        )
        .map_err(|error| {
            ClapInstrumentError::new(format!("Could not create CLAP instrument: {error}"))
        })?;
        restore_plugin_state(&mut instance, state)?;
        let (input_note_port, input_midi_port) = validate_stereo_synth_ports(&mut instance)?;

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
        let instance_id = NEXT_CLAP_INSTANCE_ID.fetch_add(1, Ordering::Relaxed);
        Ok((
            Self {
                instance: Some(instance),
                instance_id,
                gui_created: false,
                gui_visible: false,
                gui_status: 0,
            },
            ClapInstrumentProcessor {
                instance_id,
                processor: processor.into(),
                output_ports: AudioPorts::with_capacity(2, 1),
                left: vec![0.0; max_block_frames],
                right: vec![0.0; max_block_frames],
                event_scratch: Vec::with_capacity(max_events),
                // A reset block sends three all-channel CCs and a centered pitch bend,
                // in addition to every preallocated active-note release.
                input_events: EventBuffer::with_capacity(max_events.saturating_add(64)),
                active_notes: Vec::with_capacity(max_events),
                active_note_scratch: Vec::with_capacity(max_events),
                input_note_port,
                input_midi_port,
                max_block_frames,
                max_events,
            },
        ))
    }

    /// Deactivates and destroys the instrument on the calling control thread.
    pub fn deactivate(mut self, processor: StoppedClapInstrumentProcessor) {
        assert_eq!(
            self.instance_id, processor.instance_id,
            "CLAP instrument processor must return to its matching owner"
        );
        self.set_floating_gui(false, 0);
        let instance = self
            .instance
            .as_mut()
            .expect("CLAP instrument owner is deactivated once");
        instance.deactivate(processor.processor);
        self.instance.take();
    }

    /// Returns the identity paired with this owner's activated processor.
    pub fn instance_id(&self) -> u64 {
        self.instance_id
    }

    /// Saves opaque plugin state on the owner thread. Returns `None` when the plugin has no state
    /// extension.
    pub fn save_state(&mut self) -> Result<Option<Vec<u8>>, ClapInstrumentError> {
        save_plugin_state(&mut self.instance)
    }

    /// Deactivates an instrument whose graph was discarded before audio processing began.
    ///
    /// The processor handle must already have been dropped. Use [`Self::deactivate`] when the
    /// graph returned a stopped processor after realtime processing.
    pub fn try_deactivate_unused(&mut self) -> Result<(), ClapInstrumentError> {
        self.set_floating_gui(false, 0);
        let instance = self
            .instance
            .as_mut()
            .expect("CLAP instrument owner is deactivated once");
        instance.try_deactivate().map_err(|error| {
            ClapInstrumentError::new(format!("Could not deactivate CLAP instrument: {error}"))
        })?;
        self.instance.take();
        Ok(())
    }
}

impl ClapEffectOwner {
    /// Loads and activates a stereo-in/stereo-out CLAP audio effect.
    ///
    /// Loading executes third-party native code in this process. Call only for a plugin the user
    /// selected and trusts; plugins are not crash-isolated.
    ///
    /// # Safety
    ///
    /// `entry_path` must be a valid, trusted CLAP library.
    pub unsafe fn load(
        entry_path: &Path,
        plugin_id: &str,
        sample_rate: u32,
        max_block_frames: usize,
    ) -> Result<(Self, ClapEffectProcessor), ClapInstrumentError> {
        // SAFETY: the caller guarantees the selected library is valid and trusted.
        let entry = unsafe { PluginEntry::load(entry_path) }.map_err(|error| {
            ClapInstrumentError::new(format!("Could not load CLAP entry: {error}"))
        })?;
        Self::load_from_entry(entry, plugin_id, sample_rate, max_block_frames)
    }

    /// Loads a CLAP effect and restores opaque state before activation when supplied.
    ///
    /// # Safety
    ///
    /// `entry_path` must be a valid, trusted CLAP library.
    pub unsafe fn load_with_state(
        entry_path: &Path,
        plugin_id: &str,
        state: Option<&[u8]>,
        parameter_values: &[(u32, f64)],
        sample_rate: u32,
        max_block_frames: usize,
    ) -> Result<(Self, ClapEffectProcessor), ClapInstrumentError> {
        // SAFETY: the caller guarantees the selected entry is valid and trusted.
        let entry = unsafe { PluginEntry::load(entry_path) }.map_err(|error| {
            ClapInstrumentError::new(format!("Could not load CLAP entry: {error}"))
        })?;
        Self::load_from_entry_with_state(
            entry,
            plugin_id,
            state,
            parameter_values,
            sample_rate,
            max_block_frames,
        )
    }

    fn load_from_entry(
        entry: PluginEntry,
        plugin_id: &str,
        sample_rate: u32,
        max_block_frames: usize,
    ) -> Result<(Self, ClapEffectProcessor), ClapInstrumentError> {
        Self::load_from_entry_with_state(entry, plugin_id, None, &[], sample_rate, max_block_frames)
    }

    fn load_from_entry_with_state(
        entry: PluginEntry,
        plugin_id: &str,
        state: Option<&[u8]>,
        parameter_values: &[(u32, f64)],
        sample_rate: u32,
        max_block_frames: usize,
    ) -> Result<(Self, ClapEffectProcessor), ClapInstrumentError> {
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
            .any(|feature| feature == features::AUDIO_EFFECT)
        {
            return Err(ClapInstrumentError::new(format!(
                "CLAP plugin {plugin_id:?} is not marked as an audio effect"
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
                |error| ClapInstrumentError::new(format!("Could not create CLAP effect: {error}")),
            )?;
        restore_plugin_state(&mut instance, state)?;
        if state.is_none() {
            restore_parameter_values(&mut instance, parameter_values);
        }
        validate_stereo_effect_ports(&mut instance)?;
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
                ClapInstrumentError::new(format!("Could not activate CLAP effect: {error}"))
            })?;

        let instance_id = NEXT_CLAP_INSTANCE_ID.fetch_add(1, Ordering::Relaxed);
        let (parameter_tx, parameter_commands) = RingBuffer::new(128);
        let (automation_capture, automation_capture_consumer) = RingBuffer::new(1024);
        let automation_capture_overflowed = Arc::new(AtomicBool::new(false));
        let automation_playback_overflowed = Arc::new(AtomicBool::new(false));
        let capture_callback_active = Arc::new(AtomicBool::new(false));
        let write_armed_parameter = Arc::new(AtomicU64::new(u64::MAX));
        let write_disarm_acknowledged = Arc::new(AtomicBool::new(false));

        Ok((
            Self {
                instance: Some(instance),
                instance_id,
            },
            ClapEffectProcessor {
                instance_id,
                processor: processor.into(),
                input_ports: AudioPorts::with_capacity(2, 1),
                output_ports: AudioPorts::with_capacity(2, 1),
                input_left: vec![0.0; max_block_frames],
                input_right: vec![0.0; max_block_frames],
                output_left: vec![0.0; max_block_frames],
                output_right: vec![0.0; max_block_frames],
                input_events: EventBuffer::with_capacity(128),
                parameter_sender: Some(ClapParameterSender {
                    commands: parameter_tx,
                    write_armed_parameter: Arc::clone(&write_armed_parameter),
                    write_disarm_acknowledged: Arc::clone(&write_disarm_acknowledged),
                }),
                parameter_commands,
                automation_capture,
                automation_capture_overflowed: Arc::clone(&automation_capture_overflowed),
                write_armed_parameter,
                write_disarm_acknowledged: Arc::clone(&write_disarm_acknowledged),
                automation_capture_receiver: Some(ClapParameterAutomationReceiver {
                    consumer: automation_capture_consumer,
                    overflowed: Arc::clone(&automation_capture_overflowed),
                    playback_overflowed: Arc::clone(&automation_playback_overflowed),
                    capture_callback_active: Arc::clone(&capture_callback_active),
                    write_disarm_acknowledged: Arc::clone(&write_disarm_acknowledged),
                }),
                capture_callback_active,
                parameter_automation: Vec::new(),
                automation_event_scratch: Vec::with_capacity(64),
                automation_playback_overflowed,
                active_parameter_gestures: Vec::with_capacity(16),
                max_block_frames,
            },
        ))
    }

    /// Deactivates and destroys the effect on the calling control thread.
    pub fn deactivate(mut self, processor: StoppedClapEffectProcessor) {
        assert_eq!(
            self.instance_id, processor.instance_id,
            "CLAP effect processor must return to its matching owner"
        );
        let instance = self
            .instance
            .as_mut()
            .expect("CLAP effect owner is deactivated once");
        instance.deactivate(processor.processor);
        self.instance.take();
    }

    /// Returns the identity paired with this owner's activated processor.
    pub fn instance_id(&self) -> u64 {
        self.instance_id
    }

    /// Saves opaque plugin state on the owner thread. Returns `None` when the plugin has no state
    /// extension.
    pub fn save_state(&mut self) -> Result<Option<Vec<u8>>, ClapInstrumentError> {
        save_plugin_state(&mut self.instance)
    }

    /// Lists visible parameter metadata and values on the CLAP main thread.
    pub fn parameters(&mut self) -> Vec<ClapParameterInfo> {
        let Some(instance) = self.instance.as_mut() else {
            return Vec::new();
        };
        let handle = instance.plugin_handle();
        let Some(params) = handle.get_extension::<PluginParams>() else {
            return Vec::new();
        };
        let count = params.count(&handle);
        let mut result = Vec::with_capacity(count as usize);
        let mut buffer = ParamInfoBuffer::new();
        for index in 0..count {
            let Some(info) = params.get_info(&handle, index, &mut buffer) else {
                continue;
            };
            if info.flags.contains(ParamInfoFlags::IS_HIDDEN)
                || !info.flags.contains(ParamInfoFlags::IS_AUTOMATABLE)
            {
                continue;
            }
            let id = info.id;
            let name = String::from_utf8_lossy(info.name).into_owned();
            let (min_value, max_value, stepped, read_only, default_value) = (
                info.min_value,
                info.max_value,
                info.flags.contains(ParamInfoFlags::IS_STEPPED),
                info.flags.contains(ParamInfoFlags::IS_READONLY),
                info.default_value,
            );
            let value = params.get_value(&handle, id).unwrap_or(default_value);
            if !min_value.is_finite()
                || !max_value.is_finite()
                || max_value < min_value
                || !default_value.is_finite()
                || !value.is_finite()
            {
                continue;
            }
            let mut display_buffer = [0_u8; 64];
            let display_value = params
                .value_to_text(&handle, id, value, &mut display_buffer)
                .map(|display| String::from_utf8_lossy(display).into_owned())
                .unwrap_or_else(|_| format!("{value:.3}"));
            result.push(ClapParameterInfo {
                id: id.get(),
                name,
                min_value,
                max_value,
                default_value,
                value,
                display_value,
                stepped,
                read_only,
            });
        }
        result
    }

    /// Deactivates an effect whose graph was discarded before audio processing began.
    ///
    /// The processor handle must already have been dropped. Use [`Self::deactivate`] when the
    /// graph returned a stopped processor after realtime processing.
    pub fn try_deactivate_unused(&mut self) -> Result<(), ClapInstrumentError> {
        let instance = self
            .instance
            .as_mut()
            .expect("CLAP effect owner is deactivated once");
        instance.try_deactivate().map_err(|error| {
            ClapInstrumentError::new(format!("Could not deactivate CLAP effect: {error}"))
        })?;
        self.instance.take();
        Ok(())
    }
}

impl ClapEffectProcessor {
    /// Returns the CLAP-declared tail length for this effect, or zero when it has no tail extension.
    ///
    /// This must be called on the control thread before the processor is moved into the realtime
    /// graph.
    pub fn tail_length_samples(&mut self) -> Option<u32> {
        let plugin = self.processor.plugin_handle();
        match plugin
            .get_extension::<PluginTail>()
            .map(|tail| tail.get(&plugin))
            .unwrap_or_default()
        {
            TailLength::Finite(frames) => Some(frames),
            TailLength::Infinite => None,
        }
    }

    /// Installs an owned automation schedule before the processor enters the realtime graph.
    pub fn set_parameter_automation(&mut self, lanes: &[aaadaw_core::FxParameterAutomationLane]) {
        self.parameter_automation.clear();
        self.parameter_automation.extend_from_slice(lanes);
    }

    /// Takes the producer for this processor's fixed-capacity parameter queue.
    pub fn take_parameter_sender(&mut self) -> ClapParameterSender {
        self.parameter_sender
            .take()
            .expect("parameter sender is taken once before processor installation")
    }

    /// Takes the bounded capture receiver for control-thread polling.
    pub fn take_automation_capture_receiver(&mut self) -> ClapParameterAutomationReceiver {
        self.automation_capture_receiver
            .take()
            .expect("automation capture receiver is taken once before processor installation")
    }

    /// Runs one interleaved stereo block through the effect.
    pub fn process(
        &mut self,
        audio: &mut [[f32; 2]],
        start_sample: u64,
        advances_timeline: bool,
    ) -> Result<(), ClapInstrumentError> {
        if audio.len() > self.max_block_frames {
            return Err(ClapInstrumentError::new(
                "CLAP effect block exceeds its prepared frame capacity",
            ));
        }
        if audio.is_empty() {
            return Ok(());
        }
        for (frame, (left, right)) in audio.iter().zip(
            self.input_left[..audio.len()]
                .iter_mut()
                .zip(self.input_right[..audio.len()].iter_mut()),
        ) {
            *left = frame[0];
            *right = frame[1];
        }
        self.output_left[..audio.len()].fill(0.0);
        self.output_right[..audio.len()].fill(0.0);
        self.input_events.clear();
        self.capture_callback_active.store(true, Ordering::SeqCst);
        let mut write_armed_parameter = self.write_armed_parameter.load(Ordering::SeqCst);
        // Bound host parameter event work per callback. This reserves half the fixed event
        // buffer for scheduled automation and prevents a saturated UI queue from growing work.
        for _ in 0..64 {
            let Ok(command) = self.parameter_commands.pop() else {
                break;
            };
            match command {
                ClapParameterCommand::Begin { id } => {
                    if let Some(param_id) = clack_host::prelude::ClapId::from_raw(id)
                        && self.active_parameter_gestures.len()
                            < self.active_parameter_gestures.capacity()
                    {
                        self.active_parameter_gestures.push(id);
                        self.input_events
                            .push(&ParamGestureBeginEvent::new(0, param_id));
                    }
                }
                ClapParameterCommand::Set { id, value } => {
                    if let Some(param_id) = clack_host::prelude::ClapId::from_raw(id) {
                        self.input_events.push(&ParamValueEvent::new(
                            0,
                            param_id,
                            Pckn::match_all(),
                            value,
                        ));
                        if advances_timeline && write_armed_parameter == u64::from(id) {
                            let sample = start_sample;
                            if self
                                .automation_capture
                                .push(CapturedParameterValue {
                                    parameter_id: id,
                                    sample,
                                    value,
                                })
                                .is_err()
                            {
                                self.automation_capture_overflowed
                                    .store(true, Ordering::Release);
                            }
                        }
                    }
                }
                ClapParameterCommand::End { id } => {
                    if let Some(param_id) = clack_host::prelude::ClapId::from_raw(id)
                        && let Some(index) = self
                            .active_parameter_gestures
                            .iter()
                            .position(|active| *active == id)
                    {
                        self.active_parameter_gestures.swap_remove(index);
                        self.input_events
                            .push(&ParamGestureEndEvent::new(0, param_id));
                    }
                }
                ClapParameterCommand::DisarmAutomation { parameter_id } => {
                    if write_armed_parameter == u64::from(parameter_id) {
                        write_armed_parameter = u64::MAX;
                        self.write_armed_parameter.store(u64::MAX, Ordering::SeqCst);
                        self.write_disarm_acknowledged.store(true, Ordering::SeqCst);
                    }
                }
            }
        }
        self.capture_callback_active.store(false, Ordering::SeqCst);
        self.schedule_parameter_automation(start_sample, audio.len(), write_armed_parameter);

        let input_events = InputEvents::from_buffer(&self.input_events);
        let mut plugin_output_events = DiscardPluginOutputEvents;
        let mut output_events = OutputEvents::from_buffer(&mut plugin_output_events);
        let input_audio = self.input_ports.with_input_buffers([AudioPortBuffer {
            latency: 0,
            channels: AudioPortBufferType::f32_input_only(
                [
                    InputChannel::variable(&mut self.input_left[..audio.len()]),
                    InputChannel::variable(&mut self.input_right[..audio.len()]),
                ]
                .into_iter(),
            ),
        }]);
        let mut output_audio = self.output_ports.with_output_buffers([AudioPortBuffer {
            latency: 0,
            channels: AudioPortBufferType::f32_output_only(
                [
                    self.output_left[..audio.len()].as_mut(),
                    self.output_right[..audio.len()].as_mut(),
                ]
                .into_iter(),
            ),
        }]);
        let audio_processor = self
            .processor
            .ensure_processing_started()
            .map_err(|_| ClapInstrumentError::new("Could not start CLAP effect processing"))?;
        audio_processor
            .process(
                &input_audio,
                &mut output_audio,
                &input_events,
                &mut output_events,
                None,
                None,
            )
            .map_err(|_| ClapInstrumentError::new("CLAP effect processing failed"))?;

        let frame_count = audio.len();
        for (frame, (left, right)) in audio.iter_mut().zip(
            self.output_left[..frame_count]
                .iter()
                .copied()
                .zip(self.output_right[..frame_count].iter().copied()),
        ) {
            *frame = [left, right];
        }
        Ok(())
    }

    fn schedule_parameter_automation(
        &mut self,
        start_sample: u64,
        frame_count: usize,
        write_armed_parameter: u64,
    ) {
        let block_end = start_sample.saturating_add(frame_count as u64);
        let scratch = &mut self.automation_event_scratch;
        scratch.clear();
        let playback_overflowed = &self.automation_playback_overflowed;
        'lanes: for lane in &self.parameter_automation {
            if write_armed_parameter == u64::from(lane.parameter_id()) {
                continue;
            }
            let points = lane.points();
            let next = points.partition_point(|point| point.sample() <= start_sample);
            let held_point = next
                .checked_sub(1)
                .and_then(|index| points.get(index))
                .or_else(|| points.first());
            if let Some(point) = held_point {
                if scratch.len() == scratch.capacity() {
                    playback_overflowed.store(true, Ordering::Release);
                    break 'lanes;
                }
                push_automation_value(
                    scratch,
                    playback_overflowed,
                    ScheduledAutomationValue {
                        offset: 0,
                        parameter_id: lane.parameter_id(),
                        value: point.value(),
                    },
                );
            }
            for point in points[next..]
                .iter()
                .take_while(|point| point.sample() < block_end)
            {
                if scratch.len() == scratch.capacity() {
                    playback_overflowed.store(true, Ordering::Release);
                    break 'lanes;
                }
                push_automation_value(
                    scratch,
                    playback_overflowed,
                    ScheduledAutomationValue {
                        offset: point.sample().saturating_sub(start_sample) as u32,
                        parameter_id: lane.parameter_id(),
                        value: point.value(),
                    },
                );
            }
        }
        scratch.sort_unstable_by_key(|event| event.offset);
        for event in scratch.iter() {
            if let Some(param_id) = clack_host::prelude::ClapId::from_raw(event.parameter_id) {
                self.input_events.push(&ParamValueEvent::new(
                    event.offset,
                    param_id,
                    Pckn::match_all(),
                    event.value,
                ));
            }
        }
    }

    pub(crate) fn instance_id(&self) -> u64 {
        self.instance_id
    }

    /// Stops processing on the audio thread and returns a handle for control-thread teardown.
    pub fn stop(self) -> StoppedClapEffectProcessor {
        StoppedClapEffectProcessor {
            instance_id: self.instance_id,
            processor: self.processor.into_stopped(),
        }
    }

    pub(crate) fn max_block_frames(&self) -> usize {
        self.max_block_frames
    }
}

fn push_automation_value(
    scratch: &mut Vec<ScheduledAutomationValue>,
    overflowed: &AtomicBool,
    event: ScheduledAutomationValue,
) {
    if scratch.len() < scratch.capacity() {
        scratch.push(event);
    } else {
        overflowed.store(true, Ordering::Release);
    }
}

impl ClapInstrumentProcessor {
    /// Returns the CLAP-declared tail length for this instrument, or zero when it has no tail
    /// extension. Call on the control thread before moving the processor into the render graph.
    pub fn tail_length_samples(&mut self) -> Option<u32> {
        let plugin = self.processor.plugin_handle();
        match plugin
            .get_extension::<PluginTail>()
            .map(|tail| tail.get(&plugin))
            .unwrap_or_default()
        {
            TailLength::Finite(frames) => Some(frames),
            TailLength::Infinite => None,
        }
    }

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
            return Err(ClapInstrumentError::new(
                "CLAP instrument block exceeds its prepared frame capacity",
            ));
        }
        if events.len() > self.max_events {
            return Err(ClapInstrumentError::new(
                "CLAP instrument event block exceeds its prepared event capacity",
            ));
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
                event.sort_priority(),
                event.track_id.value(),
                event.pitch,
                event.note_id.map_or(0, aaadaw_core::NoteId::value),
                event.controller.unwrap_or(0),
            )
        });

        self.active_note_scratch.clear();
        self.active_note_scratch
            .extend_from_slice(&self.active_notes);
        for event in &self.event_scratch {
            if matches!(
                event.kind,
                MidiEventKind::ControllerChange | MidiEventKind::PitchBend
            ) {
                continue;
            }
            let note_id = u32::try_from(
                event
                    .note_id
                    .expect("note events carry a note identifier")
                    .value(),
            )
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
                MidiEventKind::ControllerChange => {
                    unreachable!("controller events were skipped while updating active note state")
                }
                MidiEventKind::PitchBend => {
                    unreachable!("pitch-bend events were skipped while updating active note state")
                }
            }
        }

        self.input_events.clear();
        for event in &self.event_scratch {
            let sample_offset = u32::try_from(event.sample_offset).map_err(|_| {
                ClapInstrumentError::new("MIDI event offset exceeds the CLAP range")
            })?;
            if matches!(
                event.kind,
                MidiEventKind::ControllerChange | MidiEventKind::PitchBend
            ) {
                let port = self.input_midi_port.ok_or_else(|| {
                    ClapInstrumentError::new(
                        "CLAP instrument does not support MIDI 1.0 control events",
                    )
                })?;
                let midi_bytes = match event.kind {
                    MidiEventKind::ControllerChange => [
                        0xb0,
                        event.controller.expect("controller event number"),
                        event.velocity,
                    ],
                    MidiEventKind::PitchBend => {
                        pitch_bend_midi_bytes(event.pitch_bend.expect("pitch-bend event value"))
                    }
                    MidiEventKind::NoteOff | MidiEventKind::NoteOn => {
                        unreachable!("only control events are handled in this branch")
                    }
                };
                self.input_events
                    .push(&MidiEvent::new(sample_offset, port, midi_bytes));
                continue;
            }
            let note_id = u32::try_from(
                event
                    .note_id
                    .expect("note events carry a note identifier")
                    .value(),
            )
            .ok()
            .filter(|id| *id <= i32::MAX as u32)
            .ok_or_else(|| {
                ClapInstrumentError::new("MIDI note ID exceeds the CLAP note-ID range")
            })?;
            let pckn = Pckn::new(self.input_note_port, 0_u16, u16::from(event.pitch), note_id);
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
                MidiEventKind::ControllerChange => {
                    unreachable!("controller events were emitted as raw MIDI events above")
                }
                MidiEventKind::PitchBend => {
                    unreachable!("pitch-bend events were emitted as raw MIDI events above")
                }
            }
        }
        output.fill([0.0, 0.0]);
        let result = self.process_prepared_events(output);
        std::mem::swap(&mut self.active_notes, &mut self.active_note_scratch);
        self.active_note_scratch.clear();
        result
    }

    /// Releases every held note and resets the plugin's MIDI note state.
    pub(crate) fn all_notes_off(&mut self) -> Result<(), ClapInstrumentError> {
        if self.active_notes.is_empty() {
            self.input_events.clear();
            if let Some(port) = self.input_midi_port {
                self.push_midi_controller_all_channels(port, 64);
                self.push_midi_controller_all_channels(port, 123);
                self.push_midi_controller_all_channels(port, 120);
                self.push_midi_pitch_bend_center_all_channels(port);
            } else {
                return Ok(());
            }
            let mut discarded_audio = [[0.0, 0.0]];
            self.process_prepared_events(&mut discarded_audio)?;
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
        if let Some(port) = self.input_midi_port {
            self.push_midi_controller_all_channels(port, 64);
            self.push_midi_controller_all_channels(port, 123);
            self.push_midi_controller_all_channels(port, 120);
            self.push_midi_pitch_bend_center_all_channels(port);
        }
        let mut discarded_audio = [[0.0, 0.0]];
        self.process_prepared_events(&mut discarded_audio)?;
        self.active_notes.clear();
        Ok(())
    }

    fn push_midi_controller_all_channels(&mut self, port: u16, controller: u8) {
        for channel in 0..16 {
            let status = 0xb0 | channel;
            self.input_events
                .push(&MidiEvent::new(0, port, [status, controller, 0]));
        }
    }

    fn push_midi_pitch_bend_center_all_channels(&mut self, port: u16) {
        for channel in 0..16 {
            let status = 0xe0 | channel;
            self.input_events
                .push(&MidiEvent::new(0, port, [status, 0, 64]));
        }
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
            .map_err(|_| ClapInstrumentError::new("Could not start CLAP instrument processing"))?;
        audio_processor
            .process(
                &InputAudioBuffers::empty(),
                &mut audio_outputs,
                &input_events,
                &mut output_events,
                None,
                None,
            )
            .map_err(|_| ClapInstrumentError::new("CLAP instrument processing failed"))?;

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

    pub(crate) fn instance_id(&self) -> u64 {
        self.instance_id
    }

    pub(crate) fn stop_with_status(mut self) -> (StoppedClapInstrumentProcessor, bool) {
        let released = self.all_notes_off().is_ok();
        let stopped = StoppedClapInstrumentProcessor {
            instance_id: self.instance_id,
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

    pub(crate) fn supports_midi_controllers(&self) -> bool {
        self.input_midi_port.is_some()
    }
}

fn pitch_bend_midi_bytes(value: u16) -> [u8; 3] {
    [0xe0, (value & 0x7f) as u8, (value >> 7) as u8]
}

/// Lists plugin descriptors exposed by one CLAP entry file.
///
/// Loading a CLAP entry executes its native initialization code. Call this from a worker after the
/// user selects a trusted plugin library. Process isolation is not provided.
///
/// # Safety
///
/// `entry_path` must point to a valid, trusted CLAP library.
pub unsafe fn inspect_clap_plugin_entry(
    entry_path: &Path,
) -> Result<Vec<ClapPluginDescriptor>, ClapInstrumentError> {
    // SAFETY: upheld by this function's caller.
    let entry = unsafe { PluginEntry::load(entry_path) }
        .map_err(|error| ClapInstrumentError::new(format!("Could not load CLAP entry: {error}")))?;
    describe_plugin_entry(&entry, entry_path)
}

fn describe_plugin_entry(
    entry: &PluginEntry,
    entry_path: &Path,
) -> Result<Vec<ClapPluginDescriptor>, ClapInstrumentError> {
    let factory = entry
        .get_plugin_factory()
        .ok_or_else(|| ClapInstrumentError::new("CLAP entry has no plugin factory"))?;
    let mut descriptors = Vec::new();
    for descriptor in factory.plugin_descriptors() {
        let (Some(plugin_id), Some(name)) = (descriptor.id(), descriptor.name()) else {
            continue;
        };
        descriptors.push(ClapPluginDescriptor {
            entry_path: entry_path.to_owned(),
            plugin_id: plugin_id.to_string_lossy().into_owned(),
            name: name.to_string_lossy().into_owned(),
            vendor: descriptor
                .vendor()
                .map(|vendor| vendor.to_string_lossy().into_owned()),
            features: descriptor
                .features()
                .map(|feature| feature.to_string_lossy().into_owned())
                .collect(),
        });
    }
    Ok(descriptors)
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
    let plugins = unsafe { inspect_clap_plugin_entry(entry_path) }?;
    Ok(plugins
        .into_iter()
        .filter(ClapPluginDescriptor::is_instrument)
        .map(|plugin| ClapInstrumentDescriptor {
            entry_path: plugin.entry_path,
            plugin_id: plugin.plugin_id,
            name: plugin.name,
        })
        .collect())
}

fn restore_plugin_state<H: HostHandlers>(
    instance: &mut PluginInstance<H>,
    state: Option<&[u8]>,
) -> Result<(), ClapInstrumentError> {
    let Some(state) = state else {
        return Ok(());
    };
    let plugin = instance.plugin_handle();
    let state_extension = plugin.get_extension::<PluginState>().ok_or_else(|| {
        ClapInstrumentError::state_restore("CLAP plugin does not implement the state extension")
    })?;
    state_extension
        .load(&plugin, &mut Cursor::new(state))
        .map_err(|error| {
            ClapInstrumentError::state_restore(format!("Could not restore CLAP state: {error}"))
        })
}

pub(crate) fn restore_parameter_values(instance: &mut PluginInstance<()>, values: &[(u32, f64)]) {
    if values.is_empty() {
        return;
    }
    let Some(mut handle) = instance.inactive_plugin_handle() else {
        return;
    };
    let Some(params) = handle.get_extension::<PluginParams>() else {
        return;
    };
    let mut events = EventBuffer::with_capacity(values.len());
    let mut info_buffer = ParamInfoBuffer::new();
    for (id, value) in values {
        let Some(id) = clack_host::prelude::ClapId::from_raw(*id) else {
            continue;
        };
        if !value.is_finite() {
            continue;
        }
        let mut range = None;
        for index in 0..params.count(&handle) {
            if let Some(info) = params.get_info(&handle, index, &mut info_buffer)
                && info.id == id
            {
                range = Some((
                    info.min_value,
                    info.max_value,
                    info.flags.contains(ParamInfoFlags::IS_READONLY),
                ));
                break;
            }
        }
        let Some((min_value, max_value, read_only)) = range else {
            continue;
        };
        if !min_value.is_finite() || !max_value.is_finite() || max_value < min_value || read_only {
            continue;
        }
        events.push(&ParamValueEvent::new(
            0,
            id,
            Pckn::match_all(),
            value.clamp(min_value, max_value),
        ));
    }
    if events.is_empty() {
        return;
    }
    let input = InputEvents::from_buffer(&events);
    let mut output = EventBuffer::with_capacity(8);
    let mut output_events = output.as_output();
    params.flush(&mut handle, &input, &mut output_events);
}

fn save_plugin_state<H: HostHandlers>(
    instance: &mut Option<PluginInstance<H>>,
) -> Result<Option<Vec<u8>>, ClapInstrumentError> {
    let instance = instance
        .as_mut()
        .ok_or_else(|| ClapInstrumentError::new("CLAP plugin instance was already destroyed"))?;
    let plugin = instance.plugin_handle();
    let Some(state_extension) = plugin.get_extension::<PluginState>() else {
        return Ok(None);
    };
    let mut state = Vec::new();
    state_extension
        .save(&plugin, &mut state)
        .map_err(|error| ClapInstrumentError::new(format!("Could not save CLAP state: {error}")))?;
    Ok(Some(state))
}

fn validate_stereo_synth_ports<H: HostHandlers>(
    instance: &mut PluginInstance<H>,
) -> Result<(u16, Option<u16>), ClapInstrumentError> {
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
    let mut input_midi_port = None;
    for index in 0..note_ports.count(&plugin, true) {
        let Some(note_port) = note_ports.get(&plugin, index, true, &mut note_buffer) else {
            continue;
        };
        if note_port.supported_dialects.supports(NoteDialect::Clap) {
            input_note_port = Some(u16::try_from(index).map_err(|_| {
                ClapInstrumentError::new("CLAP instrument has too many note input ports")
            })?);
        }
        if note_port.supported_dialects.supports(NoteDialect::Midi) && input_midi_port.is_none() {
            input_midi_port = Some(u16::try_from(index).map_err(|_| {
                ClapInstrumentError::new("CLAP instrument has too many note input ports")
            })?);
        }
    }
    let input_note_port = input_note_port.ok_or_else(|| {
        ClapInstrumentError::new("CLAP instrument has no note input port supporting CLAP events")
    })?;
    Ok((input_note_port, input_midi_port))
}

fn validate_stereo_effect_ports(
    instance: &mut PluginInstance<()>,
) -> Result<(), ClapInstrumentError> {
    let plugin = instance.plugin_handle();
    let ports = plugin
        .get_extension::<PluginAudioPorts>()
        .ok_or_else(|| ClapInstrumentError::new("CLAP effect does not expose audio ports"))?;
    let input_count = ports.count(&plugin, true);
    let output_count = ports.count(&plugin, false);
    if input_count != 1 || output_count != 1 {
        return Err(ClapInstrumentError::new(format!(
            "CLAP effect needs one input and one output bus; found {input_count} inputs and {output_count} outputs"
        )));
    }
    let mut buffer = AudioPortInfoBuffer::new();
    let input = ports
        .get(&plugin, 0, true, &mut buffer)
        .ok_or_else(|| ClapInstrumentError::new("CLAP effect input bus could not be read"))?;
    if input.channel_count != 2
        || !input
            .flags
            .contains(clack_extensions::audio_ports::AudioPortFlags::IS_MAIN)
    {
        return Err(ClapInstrumentError::new(
            "CLAP effect needs a stereo main input bus",
        ));
    }
    let output = ports
        .get(&plugin, 0, false, &mut buffer)
        .ok_or_else(|| ClapInstrumentError::new("CLAP effect output bus could not be read"))?;
    if output.channel_count != 2
        || !output
            .flags
            .contains(clack_extensions::audio_ports::AudioPortFlags::IS_MAIN)
    {
        return Err(ClapInstrumentError::new(
            "CLAP effect needs a stereo main output bus",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pitch_bend_values_encode_as_little_endian_14_bit_midi_data() {
        assert_eq!(pitch_bend_midi_bytes(0), [0xe0, 0, 0]);
        assert_eq!(pitch_bend_midi_bytes(8192), [0xe0, 0, 64]);
        assert_eq!(pitch_bend_midi_bytes(16_383), [0xe0, 127, 127]);
    }
    use crate::{
        AudioItemStream, AudioRenderGraph, TrackFxProcessor, TrackInstrumentProcessor,
        audio_monitor_stream, pcm_stream,
    };
    use aaadaw_core::{
        DawAction, MidiNoteData, NoteId, Project, TrackFxPlugin, TrackId, TrackInstrument,
    };
    use clack_common::stream::{InputStream, OutputStream};
    use clack_extensions::audio_ports::{
        AudioPortFlags, AudioPortInfo, AudioPortInfoWriter, AudioPortType, PluginAudioPortsImpl,
    };
    use clack_extensions::note_ports::{
        NoteDialects, NotePortInfo, NotePortInfoWriter, PluginNotePortsImpl,
    };
    use clack_extensions::params::{
        ParamDisplayWriter, ParamInfo, ParamInfoFlags, ParamInfoWriter, PluginAudioProcessorParams,
        PluginMainThreadParams,
    };
    use clack_extensions::state::PluginStateImpl;
    use clack_plugin::entry::{DefaultPluginFactory, SinglePluginEntry};
    use clack_plugin::events::spaces::CoreEventSpace;
    use clack_plugin::plugin::{PluginDescriptor, features as plugin_features};
    use clack_plugin::plugin::{PluginMainThread, PluginShared};
    use clack_plugin::prelude::{
        Audio, HostAudioProcessorHandle, HostMainThreadHandle, HostSharedHandle, Plugin,
        PluginAudioProcessor as ClackPluginAudioProcessor, PluginError, PluginExtensions, Process,
        ProcessStatus,
    };

    use clack_plugin::process::audio::ChannelPair;
    use clack_plugin::utils::ClapId;
    use std::fmt::Write as FmtWrite;
    use std::io::{Read, Write};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU8, AtomicU32, AtomicUsize};

    #[test]
    fn static_processor_errors_borrow_their_message() {
        let error = ClapInstrumentError::new("CLAP processing failed");

        assert!(matches!(
            error.message,
            Cow::Borrowed("CLAP processing failed")
        ));
    }

    const PLUGIN_ID: &str = "org.aaadaw.test.synth";
    const MONO_PLUGIN_ID: &str = "org.aaadaw.test.mono-synth";
    const EFFECT_PLUGIN_ID: &str = "org.aaadaw.test.effect";
    const STATELESS_EFFECT_PLUGIN_ID: &str = "org.aaadaw.test.stateless-effect";

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
        modulation: f32,
        expression: f32,
        sustain: bool,
        released_while_sustained: bool,
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
                    supported_dialects: NoteDialects::CLAP | NoteDialects::MIDI,
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
            Ok(Self {
                active_pitch: None,
                modulation: 1.0,
                expression: 1.0,
                sustain: false,
                released_while_sustained: false,
            })
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
                        Some(CoreEventSpace::NoteOff(_)) => {
                            if self.sustain {
                                self.released_while_sustained = true;
                            } else {
                                self.active_pitch = None;
                            }
                        }
                        Some(CoreEventSpace::Midi(event)) => {
                            let [status, controller, value] = event.data();
                            if status & 0xf0 == 0xb0 {
                                match controller {
                                    1 => self.modulation = f32::from(value) / 127.0,
                                    11 => self.expression = f32::from(value) / 127.0,
                                    64 => {
                                        let sustain = value >= 64;
                                        if self.sustain && !sustain && self.released_while_sustained
                                        {
                                            self.active_pitch = None;
                                            self.released_while_sustained = false;
                                        }
                                        self.sustain = sustain;
                                    }
                                    123 => self.modulation = 0.5 + f32::from(status & 0x0f) / 100.0,
                                    120 => {
                                        self.modulation = 0.25 + f32::from(status & 0x0f) / 100.0
                                    }
                                    _ => {}
                                }
                            }
                        }
                        _ => {}
                    }
                }
                let level = f32::from(self.active_pitch.unwrap_or(0)) / 127.0
                    * self.modulation
                    * self.expression;
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

    struct TestEffect;
    struct TestStatelessEffect;
    struct TestEffectState {
        value: AtomicU8,
        event_count: AtomicUsize,
        event_times: [AtomicU32; 128],
    }

    impl TestEffectState {
        fn new() -> Self {
            Self {
                value: AtomicU8::new(0),
                event_count: AtomicUsize::new(0),
                event_times: std::array::from_fn(|_| AtomicU32::new(0)),
            }
        }
    }

    struct TestEffectShared(Arc<TestEffectState>);
    struct TestEffectMainThread(Arc<TestEffectState>);
    struct TestEffectAudioProcessor(Arc<TestEffectState>);

    impl PluginShared<'_> for TestEffectShared {}

    impl PluginMainThread<'_, TestEffectShared> for TestEffectMainThread {}

    impl PluginMainThreadParams for TestEffectMainThread {
        fn count(&self) -> u32 {
            1
        }

        fn get_info(&self, index: u32, writer: &mut ParamInfoWriter) {
            if index == 0 {
                writer.set(&ParamInfo {
                    id: ClapId::new(1),
                    flags: ParamInfoFlags::IS_AUTOMATABLE,
                    cookie: clack_plugin::utils::Cookie::empty(),
                    name: b"Amount",
                    module: b"",
                    min_value: 0.0,
                    max_value: 255.0,
                    default_value: 0.0,
                });
            }
        }

        fn get_value(&self, param_id: ClapId) -> Option<f64> {
            (param_id.get() == 1).then(|| f64::from(self.0.value.load(Ordering::Relaxed)))
        }

        fn value_to_text(
            &self,
            _param_id: ClapId,
            value: f64,
            writer: &mut ParamDisplayWriter,
        ) -> std::fmt::Result {
            write!(writer, "{value:.0}")
        }

        fn text_to_value(&self, param_id: ClapId, text: &std::ffi::CStr) -> Option<f64> {
            (param_id.get() == 1)
                .then(|| text.to_str().ok()?.parse::<f64>().ok())
                .flatten()
        }

        fn flush(
            &self,
            input: &clack_plugin::events::io::InputEvents,
            _output: &mut clack_plugin::events::io::OutputEvents,
        ) {
            for event in input {
                if let Some(CoreEventSpace::ParamValue(value)) = event.as_core_event()
                    && value.param_id().is_some_and(|id| id.get() == 1)
                {
                    self.0
                        .value
                        .store(value.value().clamp(0.0, 255.0) as u8, Ordering::Relaxed);
                }
            }
        }
    }

    impl PluginAudioProcessorParams for TestEffectAudioProcessor {
        fn flush(
            &mut self,
            _input: &clack_plugin::events::io::InputEvents,
            _output: &mut clack_plugin::events::io::OutputEvents,
        ) {
        }
    }

    impl PluginStateImpl for TestEffectMainThread {
        fn save(&self, output: &mut OutputStream) -> Result<(), PluginError> {
            let event_count = self.0.event_count.load(Ordering::Relaxed).min(128);
            output.write_all(&[self.0.value.load(Ordering::Relaxed), event_count as u8])?;
            for event_time in self.0.event_times.iter().take(event_count) {
                output.write_all(&event_time.load(Ordering::Relaxed).to_le_bytes())?;
            }
            Ok(())
        }

        fn load(&self, input: &mut InputStream) -> Result<(), PluginError> {
            let mut state = [0];
            input.read_exact(&mut state)?;
            self.0.value.store(state[0], Ordering::Relaxed);
            self.0.event_count.store(0, Ordering::Relaxed);
            Ok(())
        }
    }

    impl Plugin for TestEffect {
        type AudioProcessor<'a> = TestEffectAudioProcessor;
        type Shared<'a> = TestEffectShared;
        type MainThread<'a> = TestEffectMainThread;

        fn declare_extensions(
            builder: &mut PluginExtensions<Self>,
            _shared: Option<&Self::Shared<'_>>,
        ) {
            builder.register::<clack_extensions::audio_ports::PluginAudioPorts>();
            builder.register::<clack_extensions::params::PluginParams>();
            builder.register::<clack_extensions::state::PluginState>();
        }
    }

    impl Plugin for TestStatelessEffect {
        type AudioProcessor<'a> = TestEffectAudioProcessor;
        type Shared<'a> = TestEffectShared;
        type MainThread<'a> = TestEffectMainThread;

        fn declare_extensions(
            builder: &mut PluginExtensions<Self>,
            _shared: Option<&Self::Shared<'_>>,
        ) {
            builder.register::<clack_extensions::audio_ports::PluginAudioPorts>();
            builder.register::<clack_extensions::params::PluginParams>();
        }
    }

    impl DefaultPluginFactory for TestStatelessEffect {
        fn get_descriptor() -> PluginDescriptor {
            PluginDescriptor::new(STATELESS_EFFECT_PLUGIN_ID, "AAADAW Stateless Test Effect")
                .with_features([plugin_features::AUDIO_EFFECT])
        }

        fn new_shared(_host: HostSharedHandle<'_>) -> Result<Self::Shared<'_>, PluginError> {
            Ok(TestEffectShared(Arc::new(TestEffectState::new())))
        }

        fn new_main_thread<'a>(
            _host: HostMainThreadHandle<'a>,
            shared: &'a Self::Shared<'a>,
        ) -> Result<Self::MainThread<'a>, PluginError> {
            Ok(TestEffectMainThread(Arc::clone(&shared.0)))
        }
    }

    impl DefaultPluginFactory for TestEffect {
        fn get_descriptor() -> PluginDescriptor {
            PluginDescriptor::new(EFFECT_PLUGIN_ID, "AAADAW Test Effect")
                .with_features([plugin_features::AUDIO_EFFECT])
        }

        fn new_shared(_host: HostSharedHandle<'_>) -> Result<Self::Shared<'_>, PluginError> {
            Ok(TestEffectShared(Arc::new(TestEffectState::new())))
        }

        fn new_main_thread<'a>(
            _host: HostMainThreadHandle<'a>,
            shared: &'a Self::Shared<'a>,
        ) -> Result<Self::MainThread<'a>, PluginError> {
            Ok(TestEffectMainThread(Arc::clone(&shared.0)))
        }
    }

    impl PluginAudioPortsImpl for TestEffectMainThread {
        fn count(&self, _is_input: bool) -> u32 {
            1
        }

        fn get(&self, index: u32, is_input: bool, writer: &mut AudioPortInfoWriter) {
            if index == 0 {
                writer.set(&AudioPortInfo {
                    id: ClapId::new(u32::from(!is_input)),
                    name: if is_input {
                        b"Stereo In"
                    } else {
                        b"Stereo Out"
                    },
                    channel_count: 2,
                    flags: AudioPortFlags::IS_MAIN,
                    port_type: Some(AudioPortType::STEREO),
                    in_place_pair: None,
                });
            }
        }
    }

    impl<'a> ClackPluginAudioProcessor<'a, TestEffectShared, TestEffectMainThread>
        for TestEffectAudioProcessor
    {
        fn activate(
            _host: HostAudioProcessorHandle<'a>,
            _main_thread: &TestEffectMainThread,
            shared: &'a TestEffectShared,
            _audio_config: clack_plugin::prelude::PluginAudioConfiguration,
        ) -> Result<Self, PluginError> {
            Ok(Self(Arc::clone(&shared.0)))
        }

        fn process(
            &mut self,
            _process: Process,
            mut audio: Audio,
            events: clack_plugin::process::Events,
        ) -> Result<ProcessStatus, PluginError> {
            self.0.value.fetch_add(1, Ordering::Relaxed);
            self.0.event_count.store(0, Ordering::Relaxed);
            for event in events.input {
                if let Some(CoreEventSpace::ParamValue(value)) = event.as_core_event()
                    && value.param_id().is_some_and(|id| id.get() == 1)
                {
                    self.0
                        .value
                        .store(value.value().clamp(0.0, 255.0) as u8, Ordering::Relaxed);
                    let event_index = self.0.event_count.fetch_add(1, Ordering::Relaxed);
                    if let Some(event_time) = self.0.event_times.get(event_index) {
                        event_time.store(event.header().time(), Ordering::Relaxed);
                    }
                }
            }
            for mut port in &mut audio {
                let channels = port
                    .channels()
                    .expect("valid stereo effect buffers")
                    .into_f32()
                    .expect("host supplies f32 buffers");
                for channel in channels {
                    match channel {
                        ChannelPair::InputOutput(input, output) => {
                            for (input, output) in input.iter().zip(output) {
                                *output = input * 0.5;
                            }
                        }
                        ChannelPair::InPlace(buffer) => {
                            for sample in buffer {
                                *sample *= 0.5;
                            }
                        }
                        ChannelPair::InputOnly(_) => {}
                        ChannelPair::OutputOnly(output) => output.fill(0.0),
                    }
                }
            }
            Ok(ProcessStatus::Continue)
        }
    }

    fn test_effect_entry() -> PluginEntry {
        PluginEntry::load_from_clack::<SinglePluginEntry<TestEffect>>(c"test")
            .expect("static test effect entry")
    }

    fn test_stateless_effect_entry() -> PluginEntry {
        PluginEntry::load_from_clack::<SinglePluginEntry<TestStatelessEffect>>(c"stateless-test")
            .expect("static stateless test plugin entry")
    }

    #[test]
    fn plugin_inspection_includes_instruments_and_effects() {
        let instrument = test_plugin_entry::<true, 2>();
        let effects = test_plugin_entry::<false, 2>();
        let instrument = describe_plugin_entry(&instrument, Path::new("synth.clap")).unwrap();
        let effects = describe_plugin_entry(&effects, Path::new("effect.clap")).unwrap();

        assert_eq!(instrument.len(), 1);
        assert!(instrument[0].is_instrument());
        assert!(!instrument[0].is_audio_effect());
        assert_eq!(instrument[0].name, "AAADAW Test Synth");
        assert_eq!(effects.len(), 1);
        assert!(effects[0].is_audio_effect());
        assert!(!effects[0].is_instrument());
    }

    #[test]
    fn effect_processor_runs_stereo_audio_and_stops_on_control_thread() {
        let (owner, processor) =
            ClapEffectOwner::load_from_entry(test_effect_entry(), EFFECT_PLUGIN_ID, 48_000, 16)
                .expect("test effect should load");
        let mut input = [[0.8, -0.4], [0.2, -0.1]];

        let (stopped, output) = std::thread::spawn(move || {
            let mut processor = processor;
            processor
                .process(&mut input, 0, true)
                .expect("effect should process");
            (processor.stop(), input)
        })
        .join()
        .expect("effect processor thread");

        assert_eq!(output, [[0.4, -0.2], [0.1, -0.05]]);
        owner.deactivate(stopped);
    }

    #[test]
    fn effect_state_is_restored_before_activation_and_saved_from_its_owner() {
        let state = [0x27];
        let (mut owner, processor) = ClapEffectOwner::load_from_entry_with_state(
            test_effect_entry(),
            EFFECT_PLUGIN_ID,
            Some(&state),
            &[(1, 64.0)],
            48_000,
            16,
        )
        .expect("test effect state should restore");
        assert_eq!(owner.parameters()[0].value, 39.0);
        let mut processor = processor;
        let mut audio = [[0.8, -0.4], [0.2, -0.1]];
        processor
            .process(&mut audio, 0, false)
            .expect("test effect should process and mutate state");
        drop(processor);
        assert_eq!(owner.save_state().unwrap(), Some(vec![0x28, 0]));
        owner
            .try_deactivate_unused()
            .expect("unused test effect should deactivate");
    }

    #[test]
    fn effect_parameters_are_enumerated_and_queue_updates_through_processing() {
        let (mut owner, mut processor) =
            ClapEffectOwner::load_from_entry(test_effect_entry(), EFFECT_PLUGIN_ID, 48_000, 16)
                .expect("test effect should load");
        let parameters = owner.parameters();
        assert_eq!(parameters.len(), 1);
        assert_eq!(parameters[0].id, 1);
        assert_eq!(parameters[0].name, "Amount");
        assert_eq!(parameters[0].min_value, 0.0);
        assert_eq!(parameters[0].max_value, 255.0);
        assert_eq!(parameters[0].value, 0.0);

        let mut sender = processor.take_parameter_sender();
        sender
            .try_send(ClapParameterCommand::Begin { id: 1 })
            .unwrap();
        sender
            .try_send(ClapParameterCommand::Set { id: 1, value: 42.0 })
            .unwrap();
        sender
            .try_send(ClapParameterCommand::End { id: 1 })
            .unwrap();
        processor.process(&mut [[0.0, 0.0]; 4], 0, false).unwrap();
        assert_eq!(owner.parameters()[0].value, 42.0);
        assert_eq!(owner.save_state().unwrap(), Some(vec![42, 1, 0, 0, 0, 0]));
        owner.deactivate(processor.stop());
    }

    #[test]
    fn write_armed_parameter_capture_uses_the_advancing_block_sample() {
        let (owner, mut processor) =
            ClapEffectOwner::load_from_entry(test_effect_entry(), EFFECT_PLUGIN_ID, 48_000, 16)
                .expect("test effect should load");
        let mut sender = processor.take_parameter_sender();
        let mut capture = processor.take_automation_capture_receiver();
        sender.set_write_armed_parameter(Some(1));
        sender
            .try_send(ClapParameterCommand::Set { id: 1, value: 42.0 })
            .unwrap();
        processor
            .process(&mut [[0.0, 0.0]; 4], 12_345, true)
            .unwrap();
        let mut captured = Vec::new();
        capture.drain_into(&mut captured);
        assert_eq!(
            captured,
            vec![ClapParameterAutomationEvent {
                parameter_id: 1,
                sample: 12_345,
                value: 42.0,
            }]
        );

        sender
            .try_send(ClapParameterCommand::Set { id: 1, value: 84.0 })
            .unwrap();
        processor
            .process(&mut [[0.0, 0.0]; 4], 12_349, false)
            .unwrap();
        capture.drain_into(&mut captured);
        assert_eq!(captured.len(), 1, "stopped processing must not be captured");
        owner.deactivate(processor.stop());
    }

    #[test]
    fn parameter_capture_overflow_is_reported_without_blocking_processing() {
        let (owner, mut processor) =
            ClapEffectOwner::load_from_entry(test_effect_entry(), EFFECT_PLUGIN_ID, 48_000, 16)
                .expect("test effect should load");
        let mut sender = processor.take_parameter_sender();
        let mut capture = processor.take_automation_capture_receiver();
        sender.set_write_armed_parameter(Some(1));
        for sample in 0..1_025 {
            sender
                .try_send(ClapParameterCommand::Set {
                    id: 1,
                    value: sample as f64,
                })
                .unwrap();
            processor.process(&mut [[0.0, 0.0]], sample, true).unwrap();
        }
        assert!(capture.take_overflowed());
        let mut captured = Vec::new();
        capture.drain_into(&mut captured);
        assert_eq!(captured.len(), 1_024);
        owner.deactivate(processor.stop());
    }

    #[test]
    fn parameter_automation_schedules_block_offsets_and_holds_between_points() {
        let (mut owner, mut processor) =
            ClapEffectOwner::load_from_entry(test_effect_entry(), EFFECT_PLUGIN_ID, 48_000, 16)
                .expect("test effect should load");
        let lane = aaadaw_core::FxParameterAutomationLane::new(
            1,
            vec![
                aaadaw_core::FxParameterAutomationPoint::new(100, 11.0).unwrap(),
                aaadaw_core::FxParameterAutomationPoint::new(102, 99.0).unwrap(),
            ],
        )
        .unwrap();
        processor.set_parameter_automation(&[lane]);
        let mut sender = processor.take_parameter_sender();

        processor.process(&mut [[0.0, 0.0]; 4], 90, true).unwrap();
        assert_eq!(
            owner.parameters()[0].value,
            11.0,
            "the first endpoint value must hold before the first point"
        );
        processor.process(&mut [[0.0, 0.0]; 4], 100, true).unwrap();
        assert_eq!(owner.parameters()[0].value, 99.0);
        assert_eq!(
            owner.save_state().unwrap(),
            Some(vec![99, 2, 0, 0, 0, 0, 2, 0, 0, 0]),
            "the plugin must receive automation offsets zero and two within the block"
        );
        sender.set_write_armed_parameter(Some(1));
        sender
            .try_send(ClapParameterCommand::Set { id: 1, value: 42.0 })
            .unwrap();
        processor.process(&mut [[0.0, 0.0]; 4], 104, true).unwrap();
        assert_eq!(owner.parameters()[0].value, 42.0);
        processor.process(&mut [[0.0, 0.0]; 4], 108, true).unwrap();
        assert_eq!(owner.parameters()[0].value, 43.0);
        sender.set_write_armed_parameter(None);
        processor.process(&mut [[0.0, 0.0]; 4], 112, true).unwrap();
        assert_eq!(owner.parameters()[0].value, 99.0);
        owner.deactivate(processor.stop());
    }

    #[test]
    fn write_disarm_ack_waits_until_backlogged_parameter_commands_are_captured() {
        let (owner, mut processor) =
            ClapEffectOwner::load_from_entry(test_effect_entry(), EFFECT_PLUGIN_ID, 48_000, 16)
                .expect("test effect should load");
        let mut sender = processor.take_parameter_sender();
        let mut capture = processor.take_automation_capture_receiver();
        sender.set_write_armed_parameter(Some(1));
        for value in 0..65 {
            sender
                .try_send(ClapParameterCommand::Set {
                    id: 1,
                    value: value as f64,
                })
                .unwrap();
        }
        assert!(sender.try_disarm_write(1));
        sender
            .try_send(ClapParameterCommand::Set {
                id: 1,
                value: 200.0,
            })
            .unwrap();

        processor.process(&mut [[0.0, 0.0]], 100, true).unwrap();
        assert!(!capture.write_disarm_acknowledged());
        let mut captured = Vec::new();
        capture.drain_into(&mut captured);
        assert_eq!(captured.len(), 64);

        processor.process(&mut [[0.0, 0.0]], 101, true).unwrap();
        assert!(capture.write_disarm_acknowledged());
        capture.drain_into(&mut captured);
        assert_eq!(captured.len(), 65);
        assert_eq!(captured.last().unwrap().sample, 101);
        owner.deactivate(processor.stop());
    }

    #[test]
    fn parameter_queue_keeps_room_for_a_gesture_end_event() {
        let (producer, mut consumer) = RingBuffer::new(4);
        let mut sender = ClapParameterSender {
            commands: producer,
            write_armed_parameter: Arc::new(AtomicU64::new(u64::MAX)),
            write_disarm_acknowledged: Arc::new(AtomicBool::new(false)),
        };
        sender
            .try_send(ClapParameterCommand::Begin { id: 1 })
            .unwrap();
        sender
            .try_send(ClapParameterCommand::Set { id: 1, value: 1.0 })
            .unwrap();
        sender
            .try_send(ClapParameterCommand::Set { id: 1, value: 2.0 })
            .unwrap();
        assert!(
            sender
                .try_send(ClapParameterCommand::Set { id: 1, value: 3.0 })
                .is_err()
        );
        sender
            .try_send(ClapParameterCommand::End { id: 1 })
            .unwrap();

        let commands = std::iter::from_fn(|| consumer.pop().ok()).collect::<Vec<_>>();
        assert_eq!(commands.last(), Some(&ClapParameterCommand::End { id: 1 }));
        assert_eq!(commands.len(), 4);
    }

    #[test]
    fn parameter_automation_playback_caps_block_events_and_reports_overflow() {
        let (owner, mut processor) =
            ClapEffectOwner::load_from_entry(test_effect_entry(), EFFECT_PLUGIN_ID, 48_000, 16)
                .expect("test effect should load");
        let lanes = (1..=65)
            .map(|parameter_id| {
                aaadaw_core::FxParameterAutomationLane::new(
                    parameter_id,
                    vec![aaadaw_core::FxParameterAutomationPoint::new(100, 1.0).unwrap()],
                )
                .unwrap()
            })
            .collect::<Vec<_>>();
        processor.set_parameter_automation(&lanes);
        let mut capture = processor.take_automation_capture_receiver();

        processor.process(&mut [[0.0, 0.0]; 4], 100, true).unwrap();

        assert!(capture.take_playback_overflowed());
        owner.deactivate(processor.stop());
    }

    #[test]
    fn stored_host_parameters_restore_for_effects_without_state_extension() {
        let (mut owner, processor) = ClapEffectOwner::load_from_entry_with_state(
            test_stateless_effect_entry(),
            STATELESS_EFFECT_PLUGIN_ID,
            None,
            &[(1, 64.0)],
            48_000,
            16,
        )
        .expect("stateless effect should load with stored host parameters");

        assert_eq!(owner.parameters()[0].value, 64.0);
        assert_eq!(owner.save_state().unwrap(), None);
        owner.deactivate(processor.stop());
    }

    #[test]
    fn restore_errors_are_distinguished_from_plugin_activation_errors() {
        let state_error = ClapEffectOwner::load_from_entry_with_state(
            test_effect_entry(),
            EFFECT_PLUGIN_ID,
            Some(&[]),
            &[],
            48_000,
            16,
        )
        .err()
        .expect("empty test state should fail restoration");
        assert!(state_error.is_state_restore_error());

        let plugin_error = ClapEffectOwner::load_from_entry(
            test_effect_entry(),
            "org.aaadaw.missing-effect",
            48_000,
            16,
        )
        .err()
        .expect("missing test effect should fail activation");
        assert!(!plugin_error.is_state_restore_error());
    }

    #[test]
    fn unused_effect_owner_can_deactivate_after_its_processor_handle_is_dropped() {
        let (mut owner, processor) =
            ClapEffectOwner::load_from_entry(test_effect_entry(), EFFECT_PLUGIN_ID, 48_000, 16)
                .expect("test effect should load");
        drop(processor);

        owner
            .try_deactivate_unused()
            .expect("an unprocessed effect can be deactivated after its graph is discarded");
    }

    #[test]
    fn effect_loader_rejects_instruments_and_wrong_buffer_sizes() {
        let instrument_error =
            ClapEffectOwner::load_from_entry(test_plugin_entry::<true, 2>(), PLUGIN_ID, 48_000, 16)
                .err()
                .expect("an instrument cannot load as an audio effect");
        assert!(
            instrument_error
                .to_string()
                .contains("not marked as an audio effect")
        );

        let (owner, mut processor) =
            ClapEffectOwner::load_from_entry(test_effect_entry(), EFFECT_PLUGIN_ID, 48_000, 1)
                .expect("test effect should load");
        let mut input = [[1.0, -1.0]; 2];
        assert!(
            processor
                .process(&mut input, 0, true)
                .expect_err("oversized block should fail")
                .to_string()
                .contains("prepared frame capacity")
        );
        assert_eq!(input, [[1.0, -1.0]; 2]);
        owner.deactivate(processor.stop());
    }

    #[test]
    fn render_graph_processes_enabled_track_effect_slots_in_chain_order() {
        let mut project = Project::new();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Guitar".to_owned(),
            })
            .expect("track creation");
        let track_id = project.tracks()[0].id();
        project
            .apply(DawAction::SetTrackRecordArm {
                track_id,
                armed: true,
            })
            .expect("track should be armed for input monitoring");
        project
            .apply(DawAction::SetTrackVolume {
                track_id,
                volume_db: 12.0,
            })
            .expect("track gain should be valid");
        project
            .apply(DawAction::SetTrackFxChain {
                track_id,
                plugins: vec![
                    TrackFxPlugin::new(EFFECT_PLUGIN_ID, "first.clap").unwrap(),
                    TrackFxPlugin::new("org.example.bypassed", "bypassed.clap")
                        .unwrap()
                        .with_enabled(false),
                    TrackFxPlugin::new(EFFECT_PLUGIN_ID, "last.clap").unwrap(),
                ],
            })
            .expect("FX chain should be valid");
        project
            .apply(DawAction::InsertAudioItem {
                track_id,
                media_ref: "asset://guitar".to_owned(),
                start_sample: 0,
                source_offset_samples: 0,
                length_samples: 4,
            })
            .expect("audio item should be valid");
        let item_id = project.audio_items()[0].id();
        let (mut producer, consumer) = pcm_stream(4).unwrap();
        assert_eq!(producer.push_samples(&[1.2, -1.2, 0.2, -0.1]), 4);
        let (first_owner, first_processor) =
            ClapEffectOwner::load_from_entry(test_effect_entry(), EFFECT_PLUGIN_ID, 48_000, 4)
                .unwrap();
        let (last_owner, last_processor) =
            ClapEffectOwner::load_from_entry(test_effect_entry(), EFFECT_PLUGIN_ID, 48_000, 4)
                .unwrap();
        let mut effects = vec![
            TrackFxProcessor::new(track_id, 0, EFFECT_PLUGIN_ID, first_processor),
            TrackFxProcessor::new(track_id, 2, EFFECT_PLUGIN_ID, last_processor),
        ];
        let mut graph = AudioRenderGraph::new_for_audio_items(
            &project,
            vec![AudioItemStream::new(item_id, consumer)],
            4,
        )
        .expect("audio graph should compile before effect activation");
        graph
            .install_fx_processors(&project, &mut effects)
            .expect("enabled effect processors should match their chain slots");
        assert!(effects.is_empty());
        let meter = graph.track_mix_controller();
        let (mut monitor_producer, monitor_consumer, monitor_gate) = audio_monitor_stream(4);
        graph.install_input_monitor(monitor_consumer, monitor_gate);
        let monitor = graph
            .input_monitor_controller()
            .expect("graph should expose monitor controls");
        assert!(monitor.set_track_enabled(track_id, true));
        assert!(monitor_producer.push_frame([0.2, -0.2]));
        let mut stopped_output = [[0.0; 2]; 1];
        graph
            .render_into(&mut stopped_output)
            .expect("FX chain should process the stopped transport monitor path");
        let track_gain = 10.0_f32.powf(12.0 / 20.0);
        assert!((stopped_output[0][0] - 0.05 * track_gain).abs() < 1.0e-6);
        assert!((stopped_output[0][1] + 0.05 * track_gain).abs() < 1.0e-6);
        let peak = meter
            .take_track_peak(track_id)
            .expect("track meter should be available");
        assert!((peak[0] - 0.05 * track_gain).abs() < 1.0e-6);
        assert!((peak[1] - 0.05 * track_gain).abs() < 1.0e-6);
        assert_eq!(graph.transport_mut().position_samples(), 0);
        graph.transport_mut().start();
        let (retired, output, stats) = std::thread::spawn(move || {
            let mut output = [[0.0; 2]; 4];
            let stats = graph.render_into(&mut output).unwrap();
            graph.stop_fx_processors();
            (graph.take_stopped_fx_processors(), output, stats)
        })
        .join()
        .expect("graph processing thread");

        let ceiling = 10.0_f32.powf(-1.0 / 20.0);
        assert_eq!(stats.master_guarded_samples, 4);
        assert_eq!(stats.master_non_finite_samples, 0);
        assert!((output[0][0] - ceiling).abs() < 1.0e-6);
        assert!((output[0][1] - ceiling).abs() < 1.0e-6);
        assert!((output[1][0] + ceiling).abs() < 1.0e-6);
        assert!((output[1][1] + ceiling).abs() < 1.0e-6);
        assert!((output[2][0] - 0.05 * 10.0_f32.powf(12.0 / 20.0)).abs() < 1.0e-6);
        assert_eq!(output[2][0], output[2][1]);
        assert!((output[3][0] + 0.025 * 10.0_f32.powf(12.0 / 20.0)).abs() < 1.0e-6);
        assert_eq!(output[3][0], output[3][1]);
        assert_eq!(retired.len(), 2);
        let mut first_stopped = None;
        let mut last_stopped = None;
        for processor in retired {
            let instance_id = processor.instance_id();
            let (stopped_track, chain_index, plugin_id, stopped) = processor.into_parts();
            assert_eq!(stopped_track, track_id);
            assert_eq!(plugin_id, EFFECT_PLUGIN_ID);
            match chain_index {
                0 => {
                    assert_eq!(instance_id, first_owner.instance_id());
                    first_stopped = Some(stopped);
                }
                2 => {
                    assert_eq!(instance_id, last_owner.instance_id());
                    last_stopped = Some(stopped);
                }
                _ => panic!("unexpected FX slot {chain_index}"),
            }
        }
        first_owner.deactivate(first_stopped.expect("first effect was retired"));
        last_owner.deactivate(last_stopped.expect("last effect was retired"));
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
                note_id: Some(note_id),
                pitch: 64,
                velocity: 127,
                controller: None,
                pitch_bend: None,
                kind: MidiEventKind::NoteOff,
            },
            ScheduledMidiEvent {
                sample_offset: 3,
                track_id,
                note_id: Some(note_id),
                pitch: 64,
                velocity: 127,
                controller: None,
                pitch_bend: None,
                kind: MidiEventKind::NoteOn,
            },
            ScheduledMidiEvent {
                sample_offset: 3,
                track_id,
                note_id: Some(note_id),
                pitch: 64,
                velocity: 0,
                controller: None,
                pitch_bend: None,
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
    fn processor_delivers_midi_controller_events_at_their_sample_offsets() {
        let entry = test_plugin_entry::<true, 2>();
        let (owner, mut processor) =
            ClapInstrumentOwner::load_from_entry(entry, PLUGIN_ID, 48_000, 16, 10)
                .expect("test synth should load");
        let (track_id, note_id) = test_ids();
        let events = [
            ScheduledMidiEvent {
                sample_offset: 0,
                track_id,
                note_id: Some(note_id),
                pitch: 64,
                velocity: 127,
                controller: None,
                pitch_bend: None,
                kind: MidiEventKind::NoteOn,
            },
            ScheduledMidiEvent {
                sample_offset: 3,
                track_id,
                note_id: None,
                pitch: 64,
                velocity: 127,
                controller: Some(64),
                pitch_bend: None,
                kind: MidiEventKind::ControllerChange,
            },
            ScheduledMidiEvent {
                sample_offset: 8,
                track_id,
                note_id: None,
                pitch: 64,
                velocity: 32,
                controller: Some(11),
                pitch_bend: None,
                kind: MidiEventKind::ControllerChange,
            },
            ScheduledMidiEvent {
                sample_offset: 4,
                track_id,
                note_id: None,
                pitch: 64,
                velocity: 64,
                controller: Some(1),
                pitch_bend: None,
                kind: MidiEventKind::ControllerChange,
            },
            ScheduledMidiEvent {
                sample_offset: 5,
                track_id,
                note_id: Some(note_id),
                pitch: 64,
                velocity: 0,
                controller: None,
                pitch_bend: None,
                kind: MidiEventKind::NoteOff,
            },
            ScheduledMidiEvent {
                sample_offset: 6,
                track_id,
                note_id: None,
                pitch: 64,
                velocity: 0,
                controller: Some(123),
                pitch_bend: None,
                kind: MidiEventKind::ControllerChange,
            },
            ScheduledMidiEvent {
                sample_offset: 7,
                track_id,
                note_id: None,
                pitch: 64,
                velocity: 0,
                controller: Some(120),
                pitch_bend: None,
                kind: MidiEventKind::ControllerChange,
            },
            ScheduledMidiEvent {
                sample_offset: 10,
                track_id,
                note_id: None,
                pitch: 64,
                velocity: 0,
                controller: Some(64),
                pitch_bend: None,
                kind: MidiEventKind::ControllerChange,
            },
            ScheduledMidiEvent {
                sample_offset: 9,
                track_id,
                note_id: None,
                pitch: 0,
                velocity: 0,
                controller: None,
                pitch_bend: Some(16_383),
                kind: MidiEventKind::PitchBend,
            },
        ];
        let mut output = [[0.0; 2]; 12];
        processor
            .process(&events, &mut output)
            .expect("controller events should process");

        let level = 64.0 / 127.0;
        for (index, frame) in output.iter().enumerate() {
            let expected = match index {
                0..4 => level,
                4..6 => level * (64.0 / 127.0),
                6 => level * 0.5,
                7 => level * 0.25,
                8..10 => level * 0.25 * (32.0 / 127.0),
                _ => 0.0,
            };
            assert!((frame[0] - expected).abs() < 0.0001);
            assert!((frame[1] - expected * 0.5).abs() < 0.0001);
        }
        owner.deactivate(processor.stop());
    }

    #[test]
    fn controller_reset_covers_all_midi_channels() {
        let (owner, mut processor) = ClapInstrumentOwner::load_from_entry(
            test_plugin_entry::<true, 2>(),
            PLUGIN_ID,
            48_000,
            16,
            1,
        )
        .expect("test synth should load");
        let (track_id, note_id) = test_ids();
        processor
            .all_notes_off()
            .expect("controller-capable plugin should accept reset events");

        let event = ScheduledMidiEvent {
            sample_offset: 0,
            track_id,
            note_id: Some(note_id),
            pitch: 127,
            velocity: 127,
            controller: None,
            pitch_bend: None,
            kind: MidiEventKind::NoteOn,
        };
        let mut output = [[0.0; 2]; 1];
        processor
            .process(&[event], &mut output)
            .expect("test note should render after reset");

        assert!((output[0][0] - 0.4).abs() < 0.0001);
        owner.deactivate(processor.stop());
    }

    #[test]
    fn note_only_plugin_receives_host_tracked_note_off_on_reset() {
        let (owner, mut processor) = ClapInstrumentOwner::load_from_entry(
            test_plugin_entry::<true, 2>(),
            PLUGIN_ID,
            48_000,
            16,
            1,
        )
        .expect("test synth should load");
        processor.input_midi_port = None;
        let (track_id, note_id) = test_ids();
        let event = ScheduledMidiEvent {
            sample_offset: 0,
            track_id,
            note_id: Some(note_id),
            pitch: 64,
            velocity: 127,
            controller: None,
            pitch_bend: None,
            kind: MidiEventKind::NoteOn,
        };
        let mut output = [[0.0; 2]; 1];
        processor
            .process(&[event], &mut output)
            .expect("test note should start");
        assert!(output[0][0] > 0.0);

        processor
            .all_notes_off()
            .expect("tracked note-off should be delivered without MIDI controllers");
        processor
            .process(&[], &mut output)
            .expect("silence should render after tracked note-off");
        assert_eq!(output, [[0.0; 2]; 1]);
        owner.deactivate(processor.stop());
    }

    #[test]
    fn unused_instrument_owner_can_deactivate_after_its_processor_handle_is_dropped() {
        let (mut owner, processor) = ClapInstrumentOwner::load_from_entry(
            test_plugin_entry::<true, 2>(),
            PLUGIN_ID,
            48_000,
            16,
            1,
        )
        .expect("test synth should load");
        drop(processor);

        owner
            .try_deactivate_unused()
            .expect("unprocessed synth should deactivate when its prepared graph is discarded");
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
            note_id: Some(test_ids().1),
            pitch: 60,
            velocity: 100,
            controller: None,
            pitch_bend: None,
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
            .apply(DawAction::SetTrackVolume {
                track_id,
                volume_db: 12.0,
            })
            .expect("track gain should be valid");
        project
            .apply(DawAction::SetTrackInstrument {
                track_id,
                instrument: TrackInstrument::new(PLUGIN_ID, "synth.clap"),
            })
            .expect("instrument assignment should be valid");
        project
            .apply(DawAction::SetTrackFxChain {
                track_id,
                plugins: vec![TrackFxPlugin::new(EFFECT_PLUGIN_ID, "effect.clap").unwrap()],
            })
            .expect("track effect should be valid");
        project
            .apply(DawAction::CreateBusTrack {
                index: 1,
                name: "Instrument Bus".to_owned(),
            })
            .expect("bus track should be valid");
        let bus_id = project.tracks()[1].id();
        project
            .apply(DawAction::SetTrackOutput {
                track_id,
                output_track: Some(bus_id),
            })
            .expect("instrument track should route through the bus");
        project
            .apply(DawAction::SetTrackFxChain {
                track_id: bus_id,
                plugins: vec![TrackFxPlugin::new(EFFECT_PLUGIN_ID, "bus-effect.clap").unwrap()],
            })
            .expect("bus FX chain should be valid");
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
        let (instrument_owner, processor) = ClapInstrumentOwner::load_from_entry(
            test_plugin_entry::<true, 2>(),
            PLUGIN_ID,
            48_000,
            32,
            2,
        )
        .expect("test synth should load");
        let instrument_instance_id = instrument_owner.instance_id();
        let (effect_owner, effect_processor) =
            ClapEffectOwner::load_from_entry(test_effect_entry(), EFFECT_PLUGIN_ID, 48_000, 32)
                .expect("test effect should load");
        let effect_instance_id = effect_owner.instance_id();
        let (bus_effect_owner, bus_effect_processor) =
            ClapEffectOwner::load_from_entry(test_effect_entry(), EFFECT_PLUGIN_ID, 48_000, 32)
                .expect("bus effect should load");
        let bus_effect_instance_id = bus_effect_owner.instance_id();
        let (mut producer, consumer) = pcm_stream(32).expect("stream capacity is valid");
        assert_eq!(producer.push_samples(&[0.1; 32]), 32);
        let mut instruments = vec![TrackInstrumentProcessor::new(track_id, processor)];
        let mut effects = vec![
            TrackFxProcessor::new(track_id, 0, EFFECT_PLUGIN_ID, effect_processor),
            TrackFxProcessor::new(bus_id, 0, EFFECT_PLUGIN_ID, bus_effect_processor),
        ];
        let mut graph = AudioRenderGraph::new_for_audio_items(
            &project,
            vec![AudioItemStream::new(item_id, consumer)],
            32,
        )
        .expect("audio streams should build a graph");
        graph
            .install_instrument_processors(&project, &mut instruments)
            .expect("assigned track instrument should install");
        graph
            .install_fx_processors(&project, &mut effects)
            .expect("assigned track effect should install");
        assert!(instruments.is_empty());
        assert!(effects.is_empty());
        graph.transport_mut().start();

        let (mut retired_instruments, retired_effects, output, stats) =
            std::thread::spawn(move || {
                let mut output = [[0.0; 2]; 32];
                let stats = graph
                    .render_into(&mut output)
                    .expect("instrument graph should render");
                graph.stop_instruments();
                graph.stop_fx_processors();
                (
                    graph.take_stopped_instruments(),
                    graph.take_stopped_fx_processors(),
                    output,
                    stats,
                )
            })
            .join()
            .expect("render graph thread");

        assert_eq!(stats.midi_event_count, 2);
        assert_eq!(stats.underrun_samples, 0);
        assert_eq!(stats.master_guarded_samples, 0);
        let midi_level = 64.0 / 127.0;
        let pcm_level = 0.1;
        let track_gain = 10.0_f32.powf(12.0 / 20.0);
        let ceiling = 10.0_f32.powf(-1.0 / 20.0);
        for (frame_index, frame) in output.iter().enumerate() {
            let instrument_level = if frame_index < 25 { midi_level } else { 0.0 };
            let expected_left =
                ((pcm_level + instrument_level) * 0.25 * track_gain).clamp(-ceiling, ceiling);
            let expected_right =
                ((pcm_level + instrument_level * 0.5) * 0.25 * track_gain).clamp(-ceiling, ceiling);
            assert!((frame[0] - expected_left).abs() < 0.0001);
            assert!((frame[1] - expected_right).abs() < 0.0001);
        }
        assert_eq!(retired_instruments.len(), 1);
        assert_eq!(retired_effects.len(), 2);
        let stopped_instrument = retired_instruments.pop().expect("instrument route exists");
        assert_eq!(stopped_instrument.instance_id(), instrument_instance_id);
        let (_, processor) = stopped_instrument.into_parts();
        instrument_owner.deactivate(processor);
        let mut effect_owner = Some(effect_owner);
        let mut bus_effect_owner = Some(bus_effect_owner);
        for stopped_effect in retired_effects {
            let instance_id = stopped_effect.instance_id();
            let (_, _, _, processor) = stopped_effect.into_parts();
            if instance_id == effect_instance_id {
                effect_owner
                    .take()
                    .expect("source effect owner should be deactivated once")
                    .deactivate(processor);
            } else {
                assert_eq!(instance_id, bus_effect_instance_id);
                bus_effect_owner
                    .take()
                    .expect("bus effect owner should be deactivated once")
                    .deactivate(processor);
            }
        }
        drop(producer);
    }

    #[test]
    fn stopped_render_graph_auditions_one_note_and_releases_it_without_moving_transport() {
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
        graph
            .install_instrument_processors(&project, &mut instruments)
            .expect("test instrument should install");
        let preview = graph.midi_preview_controller();
        assert!(preview.note_on(track_id, 60, 96));

        let mut output = [[0.0; 2]; 8];
        let on = graph
            .render_into(&mut output)
            .expect("stopped graph should process preview MIDI");
        assert_eq!(on.midi_event_count, 1);
        assert!(output[0][0] > 0.0);
        assert_eq!(graph.transport_mut().position_samples(), 0);

        assert!(preview.note_on(track_id, 72, 96));
        let change = graph
            .render_into(&mut output)
            .expect("dragging to another key should replace the preview pitch");
        assert_eq!(change.midi_event_count, 2);
        assert!(output[0][0] > 0.0);
        preview.release();
        let off = graph
            .render_into(&mut output)
            .expect("released preview should render the instrument tail");
        assert_eq!(off.midi_event_count, 1);
        assert_eq!(output, [[0.0; 2]; 8]);
        assert_eq!(graph.transport_mut().position_samples(), 0);

        assert_eq!(graph.stop_instruments(), 0);
        let stopped = graph.take_stopped_instruments().pop().unwrap();
        let (_, processor) = stopped.into_parts();
        owner.deactivate(processor);
    }

    #[test]
    fn isolated_shared_memory_route_renders_a_real_test_clap_instrument() {
        let (project, track_id, _) = test_project(120);
        let config = crate::ClapIpcConfig::new(48_000, 1024, 8).unwrap();
        let mapping = crate::ClapIpcMapping::create(config).unwrap();
        assert!(mapping.region().accept_handshake(config));
        let (owner, processor) = ClapInstrumentOwner::load_from_entry(
            test_plugin_entry::<true, 2>(),
            PLUGIN_ID,
            48_000,
            1024,
            8,
        )
        .expect("test instrument should load");
        let region = mapping.region();
        std::thread::scope(|scope| {
            let worker = scope.spawn(move || {
                let mut processor = processor;
                crate::clap_ipc::process_helper_requests(region, &mut processor, config);
                processor.stop()
            });

            let (_, pcm) = crate::pcm_stream(1024).unwrap();
            let mut graph = crate::AudioRenderGraph::new(&project, vec![pcm], 1024).unwrap();
            let mut routes = vec![crate::TrackIsolatedInstrument::new(
                track_id,
                owner.instance_id(),
                mapping.audio_port(),
                config,
            )];
            graph
                .install_isolated_instrument_ports(&project, &mut routes)
                .expect("shared-memory instrument route should install");
            graph.transport_mut().start();
            let mut output = [[0.0_f32; 2]; 128];
            graph
                .render_into(&mut output)
                .expect("isolated instrument block should render");
            assert!(output.iter().any(|sample| sample[0].abs() > 0.001));

            mapping.region().mark_shutdown();
            let stopped = worker.join().expect("helper audio worker should stop");
            owner.deactivate(stopped);
        });
    }

    #[test]
    fn isolated_instrument_route_accepts_stopped_midi_preview_requests() {
        let (project, track_id, _) = test_project(120);
        let config = crate::ClapIpcConfig::new(48_000, 1024, 8).unwrap();
        let mapping = crate::ClapIpcMapping::create(config).unwrap();
        assert!(mapping.region().accept_handshake(config));
        let (owner, processor) = ClapInstrumentOwner::load_from_entry(
            test_plugin_entry::<true, 2>(),
            PLUGIN_ID,
            48_000,
            1024,
            config.event_capacity as usize,
        )
        .expect("test instrument should load");
        let region = mapping.region();
        std::thread::scope(|scope| {
            let worker = scope.spawn(move || {
                let mut processor = processor;
                crate::clap_ipc::process_helper_requests(region, &mut processor, config);
                processor.stop()
            });

            let (_, pcm) = crate::pcm_stream(1024).unwrap();
            let mut graph = crate::AudioRenderGraph::new(&project, vec![pcm], 1024).unwrap();
            let mut routes = vec![crate::TrackIsolatedInstrument::new(
                track_id,
                owner.instance_id(),
                mapping.audio_port(),
                config,
            )];
            graph
                .install_isolated_instrument_ports(&project, &mut routes)
                .expect("shared-memory instrument route should install");
            let preview = graph.midi_preview_controller();
            assert!(preview.note_on(track_id, 60, 96));
            let mut output = [[0.0_f32; 2]; 128];
            let mut rendered = false;
            let mut preview_event_count = 0;
            for _ in 0..100 {
                let stats = graph
                    .render_into(&mut output)
                    .expect("stopped isolated route should render preview blocks");
                preview_event_count = preview_event_count.max(stats.midi_event_count);
                if output.iter().any(|sample| sample[0].abs() > 0.001) {
                    rendered = true;
                    break;
                }
                std::thread::yield_now();
            }
            assert_eq!(graph.transport_mut().position_samples(), 0);
            preview.release();
            for _ in 0..8 {
                graph
                    .render_into(&mut output)
                    .expect("preview release should continue nonblocking helper rendering");
                std::thread::yield_now();
            }

            mapping.region().mark_shutdown();
            let stopped = worker.join().expect("helper audio worker should stop");
            owner.deactivate(stopped);
            assert!(
                rendered,
                "isolated helper should render the requested preview note; events {preview_event_count}, heartbeat {}, fault code {}, pending {}, queued {}, audio {}, output {}",
                mapping.region().helper_heartbeat(),
                mapping.region().fault_code(),
                graph.instruments[0].isolated_preview_pending,
                graph.instruments[0].isolated_next_sequence,
                graph.instruments[0].audio[0][0],
                output[0][0],
            );
        });
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
        assert!(graph.transport_mut().is_playing());
        assert_eq!(graph.transport_mut().position_samples(), 8);
        graph
            .render_into(&mut output)
            .expect("Panic can silence voices while playback remains active");
        assert_eq!(output, [[0.0; 2]; 8]);
        assert!(graph.transport_mut().is_playing());
        assert_eq!(graph.transport_mut().position_samples(), 16);
        graph.transport_mut().stop();
        graph
            .render_into(&mut output)
            .expect("stopped block should render silence");
        assert_eq!(output, [[0.0; 2]; 8]);

        graph.transport_mut().start();
        graph
            .render_into(&mut output)
            .expect("playback can resume after note release");
        assert!(output[0][0] > 0.0, "sustained notes resume after restart");

        assert_eq!(graph.stop_instruments(), 0);
        let mut retired = graph.take_stopped_instruments();
        let (_, stopped) = retired
            .pop()
            .expect("retired processor exists")
            .into_parts();
        owner.deactivate(stopped);
    }
    #[test]
    fn send_taps_bracket_fx_and_source_fader_without_reprocessing() {
        use aaadaw_core::{AudioSendParameters, AudioSendTap};
        for (tap, expected) in [
            (AudioSendTap::PreFx, [0.125, 0.125]),
            (AudioSendTap::PreFader, [0.0625, 0.0625]),
            (AudioSendTap::PostFader, [0.0, 0.015625]),
        ] {
            let mut project = Project::new();
            for (index, name) in ["Source", "Receiver"].into_iter().enumerate() {
                project
                    .apply(DawAction::CreateTrack {
                        index,
                        name: name.into(),
                    })
                    .unwrap();
            }
            let source = project.tracks()[0].id();
            let receiver = project.tracks()[1].id();
            project
                .apply(DawAction::SetTrackMainSend {
                    track_id: source,
                    enabled: false,
                })
                .unwrap();
            project
                .apply(DawAction::SetTrackVolume {
                    track_id: source,
                    volume_db: 20.0 * 0.25_f32.log10(),
                })
                .unwrap();
            project
                .apply(DawAction::SetTrackPan {
                    track_id: source,
                    pan: 1.0,
                })
                .unwrap();
            project
                .apply(DawAction::SetTrackFxChain {
                    track_id: source,
                    plugins: vec![TrackFxPlugin::new(EFFECT_PLUGIN_ID, "test.clap").unwrap()],
                })
                .unwrap();
            project
                .apply(DawAction::CreateAudioSend {
                    track_id: source,
                    destination: receiver,
                    parameters: AudioSendParameters {
                        tap,
                        ..Default::default()
                    },
                })
                .unwrap();
            let (mut producer, consumer) = pcm_stream(4).unwrap();
            producer.push_samples(&[0.125; 4]);
            let (_, silent) = pcm_stream(4).unwrap();
            let (owner, processor) =
                ClapEffectOwner::load_from_entry(test_effect_entry(), EFFECT_PLUGIN_ID, 48_000, 4)
                    .unwrap();
            let mut graph = AudioRenderGraph::new(&project, vec![consumer, silent], 4).unwrap();
            graph
                .install_fx_processors(
                    &project,
                    &mut vec![TrackFxProcessor::new(
                        source,
                        0,
                        EFFECT_PLUGIN_ID,
                        processor,
                    )],
                )
                .unwrap();
            graph.transport_mut().start();
            let mut output = [[0.0; 2]; 4];
            graph.render_into(&mut output).unwrap();
            for frame in output {
                for channel in 0..2 {
                    assert!(
                        (frame[channel] - expected[channel]).abs() < 1e-6,
                        "{tap:?} {frame:?}"
                    );
                }
            }
            graph.stop_fx_processors();
            let (_, _, _, stopped) = graph
                .take_stopped_fx_processors()
                .pop()
                .unwrap()
                .into_parts();
            owner.deactivate(stopped);
        }
    }
}
