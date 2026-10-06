//! Fixed-layout shared-memory protocol for one supervised CLAP instrument.
//!
//! The protocol has no Rust pointers, `Vec`s, or platform-sized integers in its mapped payload.
//! Slot payload access is protected by release/acquire state transitions. The mapping owner must
//! initialize the region before launching a helper and keep the backing file at its original size
//! until the helper exits.

use std::cell::UnsafeCell;
use std::fs::OpenOptions;
use std::io;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::ptr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::thread;
use tempfile::{NamedTempFile, TempPath};

use memmap2::{MmapMut, MmapOptions};

pub const CLAP_IPC_PROTOCOL_VERSION: u32 = 3;
pub const CLAP_IPC_CHANNELS: usize = 2;
pub const CLAP_IPC_MAX_BLOCK_FRAMES: usize = 1024;
pub const CLAP_IPC_MAX_EVENTS: usize = 1024;
/// Keeps at least one maximum render-graph block of helper output queued ahead of playback.
pub const CLAP_IPC_SLOT_COUNT: usize = 12;
pub const CLAP_IPC_MAGIC: u32 = u32::from_le_bytes(*b"AAIP");

const REGION_INITIALIZING: u32 = 0;
const REGION_READY: u32 = 1;
const REGION_FAULTED: u32 = 2;

const SLOT_FREE: u32 = 0;
const SLOT_WRITING: u32 = 1;
const SLOT_REQUEST_READY: u32 = 2;
const SLOT_PROCESSING: u32 = 3;
const SLOT_RESPONSE_READY: u32 = 4;
const SLOT_RENDERING: u32 = 5;
const SLOT_READING: u32 = 6;
const NO_ACTIVE_SLOT: u32 = u32::MAX;

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClapIpcMidiKind {
    NoteOn = 1,
    NoteOff = 2,
    ControllerChange = 3,
    PitchBend = 4,
}

struct ClapIpcMappingInner {
    mapping: MmapMut,
    path: Option<TempPath>,
}

// SAFETY: the mapping exposes data only through ClapIpcRegion's slot-state protocol. The backing
// file stays open by the mapping and is never resized. `path` is immutable after construction.
unsafe impl Send for ClapIpcMappingInner {}
// SAFETY: all shared payload access is gated by process-shared atomics and slot ownership states.
unsafe impl Sync for ClapIpcMappingInner {}

/// Owns one cross-process mapping and its private temporary backing file.
pub struct ClapIpcMapping {
    inner: Arc<ClapIpcMappingInner>,
}

/// Keeps bounded audio-callback access alive independently of process supervision.
///
/// Its methods are callback-safe; dropping the final handle is not. Retire the render graph and
/// release its handles on a control thread while the supervisor still owns the mapping, so an
/// audio callback can never unmap shared memory or remove its backing file.
#[derive(Clone)]
pub struct ClapIpcAudioPort {
    inner: Arc<ClapIpcMappingInner>,
}

impl ClapIpcAudioPort {
    fn region(&self) -> &ClapIpcRegion {
        // SAFETY: the shared inner owns this exact-size, initialized mapping for this handle's life.
        unsafe { &*self.inner.mapping.as_ptr().cast::<ClapIpcRegion>() }
    }

    /// Submits one bounded MIDI/audio request without waiting or allocating.
    pub fn try_submit(
        &self,
        generation: u64,
        sequence: u64,
        start_sample: u64,
        events: &[ClapIpcMidiEvent],
        frame_count: usize,
    ) -> Result<(), ClapIpcSubmitError> {
        self.region()
            .try_submit(generation, sequence, start_sample, events, frame_count)
    }

    /// Reads one matching response without waiting or allocating; a missing response yields silence.
    pub fn try_read_response(
        &self,
        generation: u64,
        sequence: u64,
        start_sample: u64,
        output: &mut [[f32; CLAP_IPC_CHANNELS]],
    ) -> bool {
        self.region()
            .try_read_response(generation, sequence, start_sample, output)
    }

    pub fn underrun_count(&self) -> u64 {
        self.region().underrun_count()
    }

    pub fn is_faulted(&self) -> bool {
        self.region().is_faulted()
    }

    pub(crate) fn mark_faulted(&self, code: u32) {
        self.region().mark_faulted(code);
    }

    pub fn fault_code(&self) -> u32 {
        self.region().fault_code()
    }

    /// Checks for one completed response without consuming its slot.
    pub(crate) fn has_response(&self, generation: u64, sequence: u64, start_sample: u64) -> bool {
        self.region()
            .has_response(generation, sequence, start_sample)
    }

    /// Frees requests queued for an obsolete transport generation without waiting on the helper.
    pub(crate) fn discard_stale_requests(&self, generation: u64) {
        self.region().discard_stale_requests(generation);
    }

    /// Allocates a fixed-size reader on a control thread for variable callback frame counts.
    pub fn reader(
        &self,
        generation: u64,
        first_sequence: u64,
        first_start_sample: u64,
    ) -> Option<ClapIpcAudioReader> {
        self.region().config.validate().then(|| {
            ClapIpcAudioReader::new(self.clone(), generation, first_sequence, first_start_sample)
        })
    }
}

/// Reads fixed-size helper blocks into variable-size audio callbacks without waiting.
pub struct ClapIpcAudioReader {
    port: ClapIpcAudioPort,
    generation: u64,
    next_sequence: u64,
    next_start_sample: u64,
    block_frames: usize,
    sample_offset: usize,
    block_ready: bool,
    block_expired: bool,
    scratch: Vec<[f32; CLAP_IPC_CHANNELS]>,
}

impl ClapIpcAudioReader {
    fn new(
        port: ClapIpcAudioPort,
        generation: u64,
        first_sequence: u64,
        first_start_sample: u64,
    ) -> Self {
        let block_frames = port.region().config.max_block_frames as usize;
        Self {
            port,
            generation,
            next_sequence: first_sequence,
            next_start_sample: first_start_sample,
            block_frames,
            sample_offset: 0,
            block_ready: false,
            block_expired: false,
            scratch: vec![[0.0; CLAP_IPC_CHANNELS]; block_frames],
        }
    }

    /// Resets the read cursor after a seek or playback generation change.
    pub fn reset(&mut self, generation: u64, first_sequence: u64, first_start_sample: u64) {
        self.generation = generation;
        self.next_sequence = first_sequence;
        self.next_start_sample = first_start_sample;
        self.sample_offset = 0;
        self.block_ready = false;
        self.block_expired = false;
    }

    /// Returns the next sample position expected by this reader.
    pub fn next_sample(&self) -> u64 {
        let offset = if self.sample_offset == self.block_frames {
            self.block_frames
        } else {
            self.sample_offset
        };
        self.next_start_sample.saturating_add(offset as u64)
    }

    /// Returns sequence number for the block containing [`Self::next_sample`].
    pub fn next_sequence(&self) -> u64 {
        self.next_sequence
            .wrapping_add(u64::from(self.sample_offset == self.block_frames))
    }

    /// Copies samples into `output`, filling late or missing helper blocks with silence.
    ///
    /// A missing block expires at its first read deadline. Later completion cannot replace
    /// silence already emitted for that block. Callback work is bounded by output length and the
    /// negotiated block size; this method does not allocate, lock, or perform I/O.
    pub fn read_into(&mut self, output: &mut [[f32; CLAP_IPC_CHANNELS]]) -> bool {
        let mut all_ready = true;
        let mut output_offset = 0;
        while output_offset < output.len() {
            if self.sample_offset == self.block_frames {
                self.next_sequence = self.next_sequence.wrapping_add(1);
                self.next_start_sample = self
                    .next_start_sample
                    .saturating_add(self.block_frames as u64);
                self.sample_offset = 0;
                self.block_ready = false;
                self.block_expired = false;
            }
            if !self.block_ready && !self.block_expired {
                self.block_ready = self.port.try_read_response(
                    self.generation,
                    self.next_sequence,
                    self.next_start_sample,
                    &mut self.scratch,
                );
                self.block_expired = !self.block_ready;
            }
            let copy_frames =
                (output.len() - output_offset).min(self.block_frames - self.sample_offset);
            if self.block_ready {
                output[output_offset..output_offset + copy_frames].copy_from_slice(
                    &self.scratch[self.sample_offset..self.sample_offset + copy_frames],
                );
            } else {
                output[output_offset..output_offset + copy_frames].fill([0.0; CLAP_IPC_CHANNELS]);
                all_ready = false;
            }
            self.sample_offset += copy_frames;
            output_offset += copy_frames;
        }
        all_ready
    }
}

impl ClapIpcMapping {
    /// Creates and initializes a private file-backed mapping before helper startup.
    pub fn create(config: ClapIpcConfig) -> io::Result<Self> {
        if !config.validate() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid CLAP helper mapping configuration",
            ));
        }
        let temporary = NamedTempFile::new()?;
        temporary.as_file().set_len(Self::mapped_len() as u64)?;
        // SAFETY: this process owns the new fixed-size file and will keep it at this length.
        let mut mapping = unsafe {
            MmapOptions::new()
                .len(Self::mapped_len())
                .map_mut(temporary.as_file())?
        };
        // SAFETY: MmapMut is page-aligned. The region has no uninitialized or drop-requiring
        // payload fields. Atomics are explicitly initialized after zeroing the backing bytes.
        unsafe { Self::initialize_mapped_region(&mut mapping, config) };
        Ok(Self {
            inner: Arc::new(ClapIpcMappingInner {
                mapping,
                path: Some(temporary.into_temp_path()),
            }),
        })
    }

    /// Opens a helper-side view of a mapping created by this process.
    ///
    /// # Safety
    ///
    /// `path` must name the same-length mapping initialized by [`Self::create`]. No process may
    /// resize or replace the backing file while this mapping is open. The helper executable only
    /// receives this private path from its trusted AAADAW parent.
    pub unsafe fn open(path: &Path) -> io::Result<Self> {
        let file = OpenOptions::new().read(true).write(true).open(path)?;
        if file.metadata()?.len() != Self::mapped_len() as u64 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "CLAP helper mapping has an invalid length",
            ));
        }
        // SAFETY: the parent owns the fixed-size mapping and does not resize it while this process
        // is alive. Shared payload access is synchronized by atomic slot states.
        let mapping = unsafe { MmapOptions::new().len(Self::mapped_len()).map_mut(&file)? };
        Ok(Self {
            inner: Arc::new(ClapIpcMappingInner {
                mapping,
                path: None,
            }),
        })
    }

    pub const fn mapped_len() -> usize {
        std::mem::size_of::<ClapIpcRegion>()
    }

    /// Returns backing path for launching the trusted helper process.
    ///
    /// # Safety
    ///
    /// Keep the file at its original size and do not replace it until this mapping drops. Only
    /// pass the path to the helper process that participates in the IPC protocol.
    pub unsafe fn path(&self) -> Option<PathBuf> {
        self.inner.path.as_ref().map(|path| path.to_path_buf())
    }

    pub fn region(&self) -> &ClapIpcRegion {
        // SAFETY: the shared inner owns this exact-size, initialized mapping for its life.
        unsafe { &*self.inner.mapping.as_ptr().cast::<ClapIpcRegion>() }
    }

    /// Clones a callback-safe handle without transferring process supervision to the audio graph.
    pub fn audio_port(&self) -> ClapIpcAudioPort {
        ClapIpcAudioPort {
            inner: Arc::clone(&self.inner),
        }
    }

    /// Checks the child's expected limits and publishes its startup handshake.
    pub fn accept_handshake(&self, expected: ClapIpcConfig) -> bool {
        self.region().accept_handshake(expected)
    }

    /// Initializes a zeroed mapping in place without constructing the large region on the stack.
    unsafe fn initialize_mapped_region(mapping: &mut MmapMut, config: ClapIpcConfig) {
        mapping.fill(0);
        let region = mapping.as_mut_ptr().cast::<ClapIpcRegion>();
        // SAFETY: caller provides an aligned mapping with the exact region byte length. Every
        // non-atomic field accepts an all-zero bit pattern; all atomics are initialized below.
        unsafe {
            ptr::addr_of_mut!((*region).config).write(config);
            ptr::addr_of_mut!((*region).state).write(AtomicU32::new(REGION_INITIALIZING));
            ptr::addr_of_mut!((*region).shutdown).write(AtomicBool::new(false));
            ptr::addr_of_mut!((*region).fault_code).write(AtomicU32::new(0));
            ptr::addr_of_mut!((*region).next_slot).write(AtomicU32::new(0));
            ptr::addr_of_mut!((*region).producer_busy).write(AtomicBool::new(false));
            ptr::addr_of_mut!((*region).helper_busy).write(AtomicBool::new(false));
            ptr::addr_of_mut!((*region).active_slot).write(AtomicU32::new(NO_ACTIVE_SLOT));
            ptr::addr_of_mut!((*region).underruns).write(AtomicU64::new(0));
            ptr::addr_of_mut!((*region).helper_heartbeat).write(AtomicU64::new(0));
            ptr::addr_of_mut!((*region).state_save_request).write(AtomicU64::new(0));
            ptr::addr_of_mut!((*region).state_save_complete).write(AtomicU64::new(0));
            ptr::addr_of_mut!((*region).state_save_status).write(AtomicU32::new(0));
            ptr::addr_of_mut!((*region).audio_worker_paused).write(AtomicBool::new(false));
            for index in 0..CLAP_IPC_SLOT_COUNT {
                let slot = ptr::addr_of_mut!((*region).slots[index]);
                ptr::addr_of_mut!((*slot).state).write(AtomicU32::new(SLOT_FREE));
            }
        }
    }
}

/// Runs the bounded request loop used by an isolated CLAP instrument helper.
///
/// Plugin loading and processing happen only in the child process. The caller must invoke this
/// from AAADAW's private helper command, passing the mapping and plugin reference received from
/// its parent process.
///
/// # Safety
///
/// `entry_path` must refer to the trusted plugin selected by the user. Loading a malformed or
/// malicious native plugin can crash the helper or cause undefined behavior there.
pub unsafe fn run_clap_ipc_instrument_helper(
    mapping_path: &Path,
    entry_path: &Path,
    plugin_id: &str,
    expected: ClapIpcConfig,
    state_input_path: &Path,
    state_output_path: &Path,
) -> Result<(), String> {
    // SAFETY: the parent creates and owns this fixed-size mapping for the child lifetime.
    let mapping = unsafe { ClapIpcMapping::open(mapping_path) }
        .map_err(|error| format!("could not open CLAP helper mapping: {error}"))?;
    if mapping.region().config() != expected || !expected.validate() {
        mapping.region().mark_faulted(1);
        return Err("CLAP helper protocol handshake mismatch".to_owned());
    }

    // SAFETY: the parent passes only the trusted entry selected for the track instrument.
    let state = read_helper_state(state_input_path)
        .map_err(|error| format!("could not read CLAP state: {error}"))?;
    let (mut owner, processor) = unsafe {
        crate::ClapInstrumentOwner::load_with_state(
            entry_path,
            plugin_id,
            state.as_deref(),
            expected.sample_rate,
            expected.max_block_frames as usize,
            expected.event_capacity as usize,
        )
    }
    .map_err(|error| {
        let state_restore_failed = error.is_state_restore_error();
        mapping
            .region()
            .mark_faulted(if state_restore_failed { 3 } else { 2 });
        format!("could not activate CLAP instrument: {error}")
    })?;
    if !mapping.accept_handshake(expected) {
        owner.deactivate(processor.stop());
        return Err("CLAP helper protocol handshake mismatch".to_owned());
    }

    let region = mapping.region();
    let worker_result = thread::scope(|scope| {
        let worker = scope.spawn(move || {
            let mut processor = processor;
            let panicked = catch_unwind(AssertUnwindSafe(|| {
                process_helper_requests(region, &mut processor, expected)
            }))
            .is_err();
            if panicked {
                region.mark_faulted(6);
            }
            (processor.stop(), panicked)
        });
        while !region.is_shutdown() && !region.is_faulted() {
            owner.service_main_thread_callback();
            pump_plugin_gui_events();
            if let Some(status) = owner.service_gui_host_callbacks() {
                region.publish_gui_status(status);
            }
            if let Some((sequence, open, parent)) = region.pending_gui_request() {
                let status = owner.set_floating_gui(open, parent);
                region.complete_gui_request(sequence, status);
            }
            if let Some(request) = region.pending_state_save() {
                while !region.is_audio_worker_paused()
                    && !region.is_shutdown()
                    && !region.is_faulted()
                {
                    thread::sleep(std::time::Duration::from_millis(1));
                }
                if region.is_shutdown() || region.is_faulted() {
                    break;
                }
                let saved_state = owner.save_state().map_err(|_| ()).and_then(|state| {
                    write_helper_state(state_output_path, state.as_deref())
                        .map(|()| state)
                        .map_err(|_| ())
                });
                region.complete_state_save(request, saved_state.is_ok());
            }
            thread::sleep(std::time::Duration::from_millis(1));
        }
        worker.join()
    });
    let (stopped_processor, processing_panicked) = match worker_result {
        Ok(result) => result,
        Err(_) => {
            owner
                .try_deactivate_unused()
                .map_err(|error| format!("could not deactivate panicked CLAP helper: {error}"))?;
            return Err("CLAP helper audio worker panicked".to_owned());
        }
    };
    if processing_panicked {
        owner.set_floating_gui(false, 0);
        owner.deactivate(stopped_processor);
        return Err("CLAP helper audio worker panicked".to_owned());
    }

    owner.set_floating_gui(false, 0);
    let saved_state = owner.save_state();
    owner.deactivate(stopped_processor);
    let saved_state =
        saved_state.map_err(|error| format!("could not save CLAP instrument state: {error}"))?;
    write_helper_state(state_output_path, saved_state.as_deref())
        .map_err(|error| format!("could not write CLAP state: {error}"))?;
    Ok(())
}

#[cfg(target_os = "linux")]
fn pump_plugin_gui_events() {
    let context = glib::MainContext::default();
    while context.pending() {
        context.iteration(false);
    }
}

#[cfg(target_os = "windows")]
fn pump_plugin_gui_events() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, MSG, PM_REMOVE, PeekMessageW, TranslateMessage,
    };
    // SAFETY: PeekMessageW removes messages from this helper's current thread only. The message
    // value is initialized by Windows before TranslateMessage/DispatchMessageW inspect it.
    unsafe {
        let mut message = MSG::default();
        while PeekMessageW(&mut message, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
fn pump_plugin_gui_events() {}

pub(crate) fn process_helper_requests(
    region: &ClapIpcRegion,
    processor: &mut crate::ClapInstrumentProcessor,
    expected: ClapIpcConfig,
) {
    let mut events = Vec::with_capacity(expected.event_capacity as usize);
    let mut last_generation = None;
    while !region.is_shutdown() && !region.is_faulted() {
        region.publish_heartbeat();
        if region.pending_state_save().is_some() {
            region.mark_audio_worker_paused(true);
            while region.pending_state_save().is_some()
                && !region.is_shutdown()
                && !region.is_faulted()
            {
                thread::sleep(std::time::Duration::from_millis(1));
            }
            region.mark_audio_worker_paused(false);
            continue;
        }
        if let Some(request) = region.try_claim_request() {
            let _ = request.process(|request| {
                if last_generation.is_some_and(|generation| generation != request.generation)
                    && processor.all_notes_off().is_err()
                {
                    region.mark_faulted(3);
                    return false;
                }
                last_generation = Some(request.generation);
                events.clear();
                for event in request.events {
                    let Some(event_kind) = midi_kind_from_wire(event.kind) else {
                        region.mark_faulted(4);
                        return false;
                    };
                    let note_id = match event.kind {
                        kind if kind == ClapIpcMidiKind::NoteOn as u8
                            || kind == ClapIpcMidiKind::NoteOff as u8 =>
                        {
                            // CLAP permits an unknown note ID. Use pitch as a stable local ID so
                            // matching note-on/off packets retain the instrument's active-note
                            // bookkeeping in this single-note-port helper.
                            Some(
                                aaadaw_core::NoteId::from_value(
                                    if event.note_id == ClapIpcMidiEvent::NO_NOTE_ID {
                                        u64::from(event.pitch) + 1
                                    } else {
                                        u64::from(event.note_id)
                                    },
                                )
                                .expect("wire note ID validation guarantees nonzero IDs"),
                            )
                        }
                        _ => None,
                    };
                    events.push(crate::ScheduledMidiEvent {
                        sample_offset: event.frame_offset as usize,
                        track_id: aaadaw_core::TrackId::from_value(1)
                            .expect("the helper's private synthetic track ID is nonzero"),
                        note_id,
                        pitch: event.pitch,
                        velocity: event.velocity,
                        controller: (event.kind == ClapIpcMidiKind::ControllerChange as u8)
                            .then_some(event.controller),
                        pitch_bend: (event.kind == ClapIpcMidiKind::PitchBend as u8)
                            .then_some(event.pitch_bend),
                        kind: event_kind,
                    });
                }
                if processor.process(&events, request.audio).is_err() {
                    region.mark_faulted(3);
                    return false;
                }
                true
            });
        } else {
            thread::sleep(std::time::Duration::from_millis(1));
        }
    }
}

pub(crate) fn write_helper_state(path: &Path, state: Option<&[u8]>) -> io::Result<()> {
    use std::io::Write;

    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)?;
    file.write_all(b"AAST")?;
    file.write_all(&[u8::from(state.is_some())])?;
    let bytes = state.unwrap_or_default();
    file.write_all(&(bytes.len() as u64).to_le_bytes())?;
    file.write_all(bytes)?;
    file.flush()
}

pub(crate) fn clear_helper_state(path: &Path) -> io::Result<()> {
    use std::io::Write;

    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)?;
    file.write_all(b"AAST")?;
    file.write_all(&[2])?;
    file.write_all(&0_u64.to_le_bytes())?;
    file.flush()
}

pub(crate) fn read_helper_state(path: &Path) -> io::Result<Option<Vec<u8>>> {
    let bytes = std::fs::read(path)?;
    if bytes.len() < 13 || &bytes[..4] != b"AAST" {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "CLAP state file has an invalid header",
        ));
    }
    let state_len = usize::try_from(u64::from_le_bytes(bytes[5..13].try_into().unwrap()))
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "CLAP state is too large"))?;
    let expected_len = 13_usize
        .checked_add(state_len)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "CLAP state is too large"))?;
    if bytes.len() != expected_len {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "CLAP state file has an invalid payload length",
        ));
    }
    match bytes[4] {
        0 if state_len == 0 => Ok(None),
        1 => Ok(Some(bytes[13..].to_vec())),
        2 if state_len == 0 => Err(io::Error::new(
            io::ErrorKind::WouldBlock,
            "CLAP helper has not saved state",
        )),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "CLAP state file has an invalid presence marker",
        )),
    }
}

pub(crate) fn read_saved_helper_state(
    output_path: &Path,
    input_path: &Path,
) -> io::Result<Option<Vec<u8>>> {
    match read_helper_state(output_path) {
        Ok(state) => Ok(state),
        Err(error) if error.kind() == io::ErrorKind::InvalidData => {
            clear_helper_state(output_path)?;
            read_helper_state(input_path)
        }
        Err(error) => Err(error),
    }
}

fn midi_kind_from_wire(kind: u8) -> Option<crate::MidiEventKind> {
    match kind {
        value if value == ClapIpcMidiKind::NoteOn as u8 => Some(crate::MidiEventKind::NoteOn),
        value if value == ClapIpcMidiKind::NoteOff as u8 => Some(crate::MidiEventKind::NoteOff),
        value if value == ClapIpcMidiKind::ControllerChange as u8 => {
            Some(crate::MidiEventKind::ControllerChange)
        }
        value if value == ClapIpcMidiKind::PitchBend as u8 => Some(crate::MidiEventKind::PitchBend),
        _ => None,
    }
}

/// Fixed-capacity MIDI packet entry. Sentinel values encode optional identifiers and data.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ClapIpcMidiEvent {
    pub frame_offset: u32,
    pub note_id: u32,
    pub pitch_bend: u16,
    pub kind: u8,
    pub pitch: u8,
    pub velocity: u8,
    pub controller: u8,
    pub reserved: [u8; 2],
}

impl ClapIpcMidiEvent {
    pub const NO_NOTE_ID: u32 = u32::MAX;
    pub const NO_CONTROLLER: u8 = u8::MAX;
    pub const NO_PITCH_BEND: u16 = u16::MAX;

    /// Encodes one track-scheduled MIDI event into the fixed IPC representation.
    pub fn from_scheduled(event: crate::ScheduledMidiEvent, frame_count: usize) -> Option<Self> {
        let kind = match event.kind {
            crate::MidiEventKind::NoteOn => ClapIpcMidiKind::NoteOn,
            crate::MidiEventKind::NoteOff => ClapIpcMidiKind::NoteOff,
            crate::MidiEventKind::ControllerChange => ClapIpcMidiKind::ControllerChange,
            crate::MidiEventKind::PitchBend => ClapIpcMidiKind::PitchBend,
        };
        let note_id = event
            .note_id
            // Reserve the invalid/zero core ID; the helper uses this as a stable local note ID.
            .and_then(|id| id.value().checked_add(1))
            .and_then(|id| u32::try_from(id).ok())
            .filter(|id| *id <= i32::MAX as u32)
            .unwrap_or(Self::NO_NOTE_ID);
        let controller = event.controller.unwrap_or(Self::NO_CONTROLLER);
        let pitch_bend = event.pitch_bend.unwrap_or(Self::NO_PITCH_BEND);
        let encoded = Self {
            frame_offset: u32::try_from(event.sample_offset).ok()?,
            note_id,
            pitch_bend,
            kind: kind as u8,
            pitch: event.pitch,
            velocity: event.velocity,
            controller,
            reserved: [0; 2],
        };
        encoded.is_valid(frame_count).then_some(encoded)
    }

    fn is_valid(self, frame_count: usize) -> bool {
        if self.frame_offset as usize >= frame_count
            || self.pitch > 127
            || self.velocity > 127
            || self.reserved != [0; 2]
        {
            return false;
        }
        match self.kind {
            kind if kind == ClapIpcMidiKind::NoteOn as u8
                || kind == ClapIpcMidiKind::NoteOff as u8 =>
            {
                (self.note_id == Self::NO_NOTE_ID || self.note_id <= i32::MAX as u32)
                    && self.controller == Self::NO_CONTROLLER
                    && self.pitch_bend == Self::NO_PITCH_BEND
            }
            kind if kind == ClapIpcMidiKind::ControllerChange as u8 => {
                self.note_id == Self::NO_NOTE_ID
                    && self.controller <= 127
                    && self.pitch_bend == Self::NO_PITCH_BEND
            }
            kind if kind == ClapIpcMidiKind::PitchBend as u8 => {
                self.note_id == Self::NO_NOTE_ID
                    && self.controller == Self::NO_CONTROLLER
                    && self.pitch_bend <= 16_383
            }
            _ => false,
        }
    }
}

const _: () = assert!(std::mem::size_of::<ClapIpcMidiEvent>() == 16);

/// Versioned, fixed-size parameters negotiated before the helper loads a plugin.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClapIpcConfig {
    pub magic: u32,
    pub protocol_version: u32,
    pub sample_rate: u32,
    pub channels: u32,
    pub max_block_frames: u32,
    pub event_capacity: u32,
    pub slot_count: u32,
    pub reserved: u32,
}

impl ClapIpcConfig {
    pub fn new(sample_rate: u32, max_block_frames: usize, event_capacity: usize) -> Option<Self> {
        if sample_rate == 0
            || max_block_frames == 0
            || max_block_frames > CLAP_IPC_MAX_BLOCK_FRAMES
            || event_capacity == 0
            || event_capacity > CLAP_IPC_MAX_EVENTS
        {
            return None;
        }
        Some(Self {
            magic: CLAP_IPC_MAGIC,
            protocol_version: CLAP_IPC_PROTOCOL_VERSION,
            sample_rate,
            channels: CLAP_IPC_CHANNELS as u32,
            max_block_frames: max_block_frames as u32,
            event_capacity: event_capacity as u32,
            slot_count: CLAP_IPC_SLOT_COUNT as u32,
            reserved: 0,
        })
    }

    pub fn validate(&self) -> bool {
        self.magic == CLAP_IPC_MAGIC
            && self.protocol_version == CLAP_IPC_PROTOCOL_VERSION
            && self.sample_rate > 0
            && self.channels == CLAP_IPC_CHANNELS as u32
            && (1..=CLAP_IPC_MAX_BLOCK_FRAMES as u32).contains(&self.max_block_frames)
            && (1..=CLAP_IPC_MAX_EVENTS as u32).contains(&self.event_capacity)
            && self.slot_count == CLAP_IPC_SLOT_COUNT as u32
            && self.reserved == 0
    }
}

#[repr(C, align(64))]
struct ClapIpcSlot {
    state: AtomicU32,
    payload: UnsafeCell<ClapIpcSlotPayload>,
}

#[repr(C)]
struct ClapIpcSlotPayload {
    frame_count: u32,
    event_count: u32,
    reserved: u32,
    generation: u64,
    sequence: u64,
    start_sample: u64,
    events: [ClapIpcMidiEvent; CLAP_IPC_MAX_EVENTS],
    audio: [[f32; CLAP_IPC_CHANNELS]; CLAP_IPC_MAX_BLOCK_FRAMES],
}

impl ClapIpcSlot {
    fn new() -> Self {
        Self {
            state: AtomicU32::new(SLOT_FREE),
            payload: UnsafeCell::new(ClapIpcSlotPayload {
                frame_count: 0,
                event_count: 0,
                reserved: 0,
                generation: 0,
                sequence: 0,
                start_sample: 0,
                events: [ClapIpcMidiEvent::default(); CLAP_IPC_MAX_EVENTS],
                audio: [[0.0; CLAP_IPC_CHANNELS]; CLAP_IPC_MAX_BLOCK_FRAMES],
            }),
        }
    }
}

/// Shared, fixed-layout region for one helper process.
#[repr(C, align(64))]
pub struct ClapIpcRegion {
    config: ClapIpcConfig,
    state: AtomicU32,
    shutdown: AtomicBool,
    fault_code: AtomicU32,
    next_slot: AtomicU32,
    producer_busy: AtomicBool,
    helper_busy: AtomicBool,
    active_slot: AtomicU32,
    underruns: AtomicU64,
    helper_heartbeat: AtomicU64,
    state_save_request: AtomicU64,
    state_save_complete: AtomicU64,
    state_save_status: AtomicU32,
    audio_worker_paused: AtomicBool,
    gui_request: AtomicU64,
    gui_complete: AtomicU64,
    gui_command: AtomicU32,
    gui_parent: AtomicU64,
    gui_status: AtomicU32,
    slots: [ClapIpcSlot; CLAP_IPC_SLOT_COUNT],
}

// SAFETY: producer, helper, and response-reader entry points acquire process-shared ownership
// atomics before touching payloads. One active producer and one active helper are enforced by
// `producer_busy` and `helper_busy`; each response reader must claim SLOT_READING. The owner
// publishes payloads with Release and peers acquire them before access. Configuration stays
// immutable after mapping initialization. Supported Linux/Windows x86_64 targets provide lock-free
// integer atomics for these fields.
unsafe impl Sync for ClapIpcRegion {}

impl ClapIpcRegion {
    pub fn new(config: ClapIpcConfig) -> Option<Box<Self>> {
        if !config.validate() {
            return None;
        }

        // Keep the large fixed slot array on the heap from the first write. Constructing `Self`
        // as a temporary before `Box::new` can exceed the small default Windows test-thread stack.
        let mut region = Box::<Self>::new_uninit();
        let region_ptr = region.as_mut_ptr();
        // SAFETY: every field is initialized exactly once before `assume_init`; the slot array is
        // written element-by-element so no full-size array temporary is formed on the stack.
        unsafe {
            ptr::addr_of_mut!((*region_ptr).config).write(config);
            ptr::addr_of_mut!((*region_ptr).state).write(AtomicU32::new(REGION_INITIALIZING));
            ptr::addr_of_mut!((*region_ptr).shutdown).write(AtomicBool::new(false));
            ptr::addr_of_mut!((*region_ptr).fault_code).write(AtomicU32::new(0));
            ptr::addr_of_mut!((*region_ptr).next_slot).write(AtomicU32::new(0));
            ptr::addr_of_mut!((*region_ptr).producer_busy).write(AtomicBool::new(false));
            ptr::addr_of_mut!((*region_ptr).helper_busy).write(AtomicBool::new(false));
            ptr::addr_of_mut!((*region_ptr).active_slot).write(AtomicU32::new(NO_ACTIVE_SLOT));
            ptr::addr_of_mut!((*region_ptr).underruns).write(AtomicU64::new(0));
            ptr::addr_of_mut!((*region_ptr).helper_heartbeat).write(AtomicU64::new(0));
            ptr::addr_of_mut!((*region_ptr).state_save_request).write(AtomicU64::new(0));
            ptr::addr_of_mut!((*region_ptr).state_save_complete).write(AtomicU64::new(0));
            ptr::addr_of_mut!((*region_ptr).state_save_status).write(AtomicU32::new(0));
            ptr::addr_of_mut!((*region_ptr).audio_worker_paused).write(AtomicBool::new(false));
            ptr::addr_of_mut!((*region_ptr).gui_request).write(AtomicU64::new(0));
            ptr::addr_of_mut!((*region_ptr).gui_complete).write(AtomicU64::new(0));
            ptr::addr_of_mut!((*region_ptr).gui_command).write(AtomicU32::new(0));
            ptr::addr_of_mut!((*region_ptr).gui_parent).write(AtomicU64::new(0));
            ptr::addr_of_mut!((*region_ptr).gui_status).write(AtomicU32::new(0));

            let slots = ptr::addr_of_mut!((*region_ptr).slots).cast::<ClapIpcSlot>();
            for index in 0..CLAP_IPC_SLOT_COUNT {
                slots.add(index).write(ClapIpcSlot::new());
            }

            Some(region.assume_init())
        }
    }

    pub fn config(&self) -> ClapIpcConfig {
        self.config
    }

    /// Validates the mapped handshake values and publishes readiness to the host.
    pub fn accept_handshake(&self, expected: ClapIpcConfig) -> bool {
        if self.config != expected || !self.config.validate() {
            self.fault_code.store(1, Ordering::Relaxed);
            self.state.store(REGION_FAULTED, Ordering::Release);
            return false;
        }
        self.fault_code.store(0, Ordering::Relaxed);
        self.state.store(REGION_READY, Ordering::Release);
        true
    }

    pub(crate) fn begin_startup(&self) {
        self.state.store(REGION_INITIALIZING, Ordering::Release);
        self.shutdown.store(false, Ordering::Release);
        self.next_slot.store(0, Ordering::Relaxed);
        self.producer_busy.store(false, Ordering::Relaxed);
        self.helper_busy.store(false, Ordering::Relaxed);
        self.active_slot.store(NO_ACTIVE_SLOT, Ordering::Relaxed);
        self.state_save_request.store(0, Ordering::Relaxed);
        self.state_save_complete.store(0, Ordering::Relaxed);
        self.state_save_status.store(0, Ordering::Relaxed);
        self.audio_worker_paused.store(false, Ordering::Relaxed);
        self.gui_request.store(0, Ordering::Relaxed);
        self.gui_complete.store(0, Ordering::Relaxed);
        self.gui_command.store(0, Ordering::Relaxed);
        self.gui_parent.store(0, Ordering::Relaxed);
        self.gui_status.store(0, Ordering::Relaxed);
        for slot in &self.slots {
            slot.state.store(SLOT_FREE, Ordering::Release);
        }
    }

    pub fn is_ready(&self) -> bool {
        self.state.load(Ordering::Acquire) == REGION_READY && !self.shutdown.load(Ordering::Acquire)
    }

    /// Requests a GUI transition without touching the audio request slots. `open` is encoded as
    /// 1 and `close` as 2; only one unacknowledged command can be in flight per helper.
    pub fn request_gui(&self, open: bool, parent: u64) -> Option<u64> {
        let completed = self.gui_complete.load(Ordering::Acquire);
        let requested = self.gui_request.load(Ordering::Acquire);
        if requested != completed {
            return None;
        }
        let sequence = requested.checked_add(1)?;
        self.gui_command
            .store(if open { 1 } else { 2 }, Ordering::Relaxed);
        self.gui_parent.store(parent, Ordering::Relaxed);
        self.gui_request.store(sequence, Ordering::Release);
        Some(sequence)
    }

    pub(crate) fn pending_gui_request(&self) -> Option<(u64, bool, u64)> {
        let requested = self.gui_request.load(Ordering::Acquire);
        let completed = self.gui_complete.load(Ordering::Acquire);
        (requested != completed).then(|| {
            let command = self.gui_command.load(Ordering::Acquire);
            let parent = self.gui_parent.load(Ordering::Relaxed);
            (requested, command == 1, parent)
        })
    }

    pub(crate) fn complete_gui_request(&self, sequence: u64, status: u32) {
        self.gui_status.store(status, Ordering::Relaxed);
        self.gui_complete.store(sequence, Ordering::Release);
    }

    /// Returns whether the helper acknowledged this GUI command sequence.
    pub fn gui_request_completed(&self, sequence: u64) -> bool {
        self.gui_complete.load(Ordering::Acquire) >= sequence
    }

    pub(crate) fn publish_gui_status(&self, status: u32) {
        self.gui_status.store(status, Ordering::Release);
    }

    /// Returns the editor lifecycle status: 0 closed, 1 visible, 2 unsupported, 3 failed.
    pub fn gui_status(&self) -> u32 {
        self.gui_status.load(Ordering::Acquire)
    }

    pub fn is_faulted(&self) -> bool {
        self.state.load(Ordering::Acquire) == REGION_FAULTED
    }

    /// Returns the monotonically increasing child heartbeat observed by its parent.
    pub fn helper_heartbeat(&self) -> u64 {
        self.helper_heartbeat.load(Ordering::Acquire)
    }

    pub fn request_state_save(&self) -> Option<u64> {
        let completed = self.state_save_complete.load(Ordering::Acquire);
        let requested = self.state_save_request.load(Ordering::Acquire);
        if requested != completed {
            return None;
        }
        let next = requested.checked_add(1)?;
        self.state_save_status.store(0, Ordering::Relaxed);
        self.state_save_request
            .compare_exchange(requested, next, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| next)
    }

    pub fn pending_state_save(&self) -> Option<u64> {
        let requested = self.state_save_request.load(Ordering::Acquire);
        (requested != self.state_save_complete.load(Ordering::Acquire)).then_some(requested)
    }

    pub fn mark_audio_worker_paused(&self, paused: bool) {
        self.audio_worker_paused.store(paused, Ordering::Release);
    }

    pub fn is_audio_worker_paused(&self) -> bool {
        self.audio_worker_paused.load(Ordering::Acquire)
    }

    pub fn complete_state_save(&self, request: u64, success: bool) {
        self.state_save_status
            .store(if success { 1 } else { 2 }, Ordering::Relaxed);
        self.state_save_complete.store(request, Ordering::Release);
    }

    pub fn state_save_succeeded(&self) -> bool {
        self.state_save_status.load(Ordering::Acquire) == 1
    }

    pub(crate) fn state_save_is_complete(&self, request: u64) -> bool {
        self.state_save_complete.load(Ordering::Acquire) == request
    }

    fn publish_heartbeat(&self) {
        self.helper_heartbeat.fetch_add(1, Ordering::Release);
    }

    /// Reclaims child-owned slots after the supervisor has confirmed that the helper exited.
    pub(crate) fn recover_after_helper_exit(&self) {
        for slot in &self.slots {
            let state = slot.state.load(Ordering::Acquire);
            if state == SLOT_PROCESSING || state == SLOT_RENDERING {
                let _ = slot.state.compare_exchange(
                    state,
                    SLOT_FREE,
                    Ordering::AcqRel,
                    Ordering::Relaxed,
                );
            }
        }
        self.active_slot.store(NO_ACTIVE_SLOT, Ordering::Release);
        self.helper_busy.store(false, Ordering::Release);
    }

    pub fn fault_code(&self) -> u32 {
        self.fault_code.load(Ordering::Acquire)
    }

    pub fn mark_faulted(&self, code: u32) {
        self.fault_code.store(code, Ordering::Relaxed);
        self.state.store(REGION_FAULTED, Ordering::Release);
    }

    pub fn mark_shutdown(&self) {
        self.shutdown.store(true, Ordering::Release);
    }

    pub fn is_shutdown(&self) -> bool {
        self.shutdown.load(Ordering::Acquire)
    }

    /// Submits one bounded MIDI block without waiting or allocating.
    pub fn try_submit(
        &self,
        generation: u64,
        sequence: u64,
        start_sample: u64,
        events: &[ClapIpcMidiEvent],
        frame_count: usize,
    ) -> Result<(), ClapIpcSubmitError> {
        if !self.is_ready() {
            return Err(ClapIpcSubmitError::NotReady);
        }
        if frame_count == 0 || frame_count > self.config.max_block_frames as usize {
            return Err(ClapIpcSubmitError::InvalidFrameCount);
        }
        if events.len() > self.config.event_capacity as usize {
            return Err(ClapIpcSubmitError::EventCapacityExceeded);
        }
        if events.iter().any(|event| !event.is_valid(frame_count)) {
            return Err(ClapIpcSubmitError::InvalidEvent);
        }
        if self
            .producer_busy
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            return Err(ClapIpcSubmitError::ProducerBusy);
        }

        let result = (|| {
            let start =
                self.next_slot.fetch_add(1, Ordering::Relaxed) as usize % CLAP_IPC_SLOT_COUNT;
            for offset in 0..CLAP_IPC_SLOT_COUNT {
                let slot = &self.slots[(start + offset) % CLAP_IPC_SLOT_COUNT];
                if slot
                    .state
                    .compare_exchange(
                        SLOT_FREE,
                        SLOT_WRITING,
                        Ordering::Acquire,
                        Ordering::Relaxed,
                    )
                    .is_err()
                {
                    continue;
                }
                // SAFETY: successful FREE -> WRITING transition grants this process exclusive slot
                // ownership until it publishes REQUEST_READY below.
                let payload = unsafe { &mut *slot.payload.get() };
                payload.frame_count = frame_count as u32;
                payload.event_count = events.len() as u32;
                payload.generation = generation;
                payload.sequence = sequence;
                payload.start_sample = start_sample;
                payload.events[..events.len()].copy_from_slice(events);
                payload.audio[..frame_count].fill([0.0; CLAP_IPC_CHANNELS]);
                slot.state.store(SLOT_REQUEST_READY, Ordering::Release);
                return Ok(());
            }
            Err(ClapIpcSubmitError::SlotsFull)
        })();
        self.producer_busy.store(false, Ordering::Release);
        result
    }

    /// Claims the oldest pending slot. Returns immediately when no request is ready.
    pub fn try_claim_request(&self) -> Option<ClapIpcRequestSlot<'_>> {
        if self
            .helper_busy
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            return None;
        }
        let mut candidate = None;
        let mut oldest = u64::MAX;
        for (index, slot) in self.slots.iter().enumerate() {
            if slot.state.load(Ordering::Acquire) == SLOT_REQUEST_READY {
                // SAFETY: REQUEST_READY means the producer published the immutable sequence.
                let payload = unsafe { &*slot.payload.get() };
                if payload.sequence >= oldest {
                    continue;
                }
                oldest = payload.sequence;
                candidate = Some(index);
            }
        }
        let Some(index) = candidate else {
            self.helper_busy.store(false, Ordering::Release);
            return None;
        };
        if self.slots[index]
            .state
            .compare_exchange(
                SLOT_REQUEST_READY,
                SLOT_PROCESSING,
                Ordering::Acquire,
                Ordering::Relaxed,
            )
            .is_err()
        {
            self.helper_busy.store(false, Ordering::Release);
            return None;
        }
        self.active_slot.store(index as u32, Ordering::Release);
        Some(ClapIpcRequestSlot {
            region: self,
            index,
        })
    }

    /// Processes one claimed request and publishes its response after the closure returns.
    fn process_claimed_request(
        &self,
        index: usize,
        process: impl for<'a> FnOnce(ClapIpcRequest<'a>) -> bool,
    ) -> bool {
        let Some(slot) = self.slots.get(index) else {
            return false;
        };
        if self.active_slot.load(Ordering::Acquire) != index as u32
            || slot
                .state
                .compare_exchange(
                    SLOT_PROCESSING,
                    SLOT_RENDERING,
                    Ordering::Acquire,
                    Ordering::Relaxed,
                )
                .is_err()
        {
            return false;
        }
        let succeeded = {
            // SAFETY: REQUEST_READY -> PROCESSING grants the helper exclusive slot ownership.
            let payload = unsafe { &mut *slot.payload.get() };
            if payload.frame_count == 0
                || payload.frame_count as usize > CLAP_IPC_MAX_BLOCK_FRAMES
                || payload.event_count as usize > self.config.event_capacity as usize
                || payload.events[..payload.event_count as usize]
                    .iter()
                    .any(|event| !event.is_valid(payload.frame_count as usize))
            {
                false
            } else {
                process(ClapIpcRequest {
                    generation: payload.generation,
                    sequence: payload.sequence,
                    start_sample: payload.start_sample,
                    events: &payload.events[..payload.event_count as usize],
                    audio: &mut payload.audio[..payload.frame_count as usize],
                })
            }
        };
        let next_state = if succeeded {
            SLOT_RESPONSE_READY
        } else {
            SLOT_FREE
        };
        let transitioned = slot
            .state
            .compare_exchange(
                SLOT_RENDERING,
                next_state,
                Ordering::Release,
                Ordering::Relaxed,
            )
            .is_ok();
        transitioned && succeeded
    }

    /// Copies a matching bounded response into callback output and frees its slot.
    ///
    /// A request larger than the negotiated block limit returns `false` without touching output.
    pub fn try_read_response(
        &self,
        generation: u64,
        sequence: u64,
        start_sample: u64,
        output: &mut [[f32; CLAP_IPC_CHANNELS]],
    ) -> bool {
        if output.len() > self.config.max_block_frames as usize {
            return false;
        }
        output.fill([0.0; CLAP_IPC_CHANNELS]);
        for slot in &self.slots {
            if slot
                .state
                .compare_exchange(
                    SLOT_RESPONSE_READY,
                    SLOT_READING,
                    Ordering::Acquire,
                    Ordering::Relaxed,
                )
                .is_err()
            {
                continue;
            }
            // SAFETY: RESPONSE_READY makes payload immutable until this reader frees the slot.
            let payload = unsafe { &*slot.payload.get() };
            let response_generation = payload.generation;
            let response_sequence = payload.sequence;
            let response_start = payload.start_sample;
            if response_generation != generation
                || response_sequence != sequence
                || response_start != start_sample
            {
                if response_generation < generation
                    || (response_generation == generation
                        && (response_sequence < sequence
                            || (response_sequence == sequence && response_start != start_sample)))
                {
                    slot.state.store(SLOT_FREE, Ordering::Release);
                } else {
                    slot.state.store(SLOT_RESPONSE_READY, Ordering::Release);
                }
                continue;
            }
            if payload.frame_count as usize != output.len() {
                slot.state.store(SLOT_FREE, Ordering::Release);
                return false;
            }
            output.copy_from_slice(&payload.audio[..output.len()]);
            slot.state.store(SLOT_FREE, Ordering::Release);
            return true;
        }
        self.underruns.fetch_add(1, Ordering::Relaxed);
        false
    }

    fn has_response(&self, generation: u64, sequence: u64, start_sample: u64) -> bool {
        self.slots.iter().any(|slot| {
            if slot
                .state
                .compare_exchange(
                    SLOT_RESPONSE_READY,
                    SLOT_READING,
                    Ordering::Acquire,
                    Ordering::Relaxed,
                )
                .is_err()
            {
                return false;
            }
            // SAFETY: RESPONSE_READY -> READING grants exclusive payload access even if the audio
            // callback consumes a different response concurrently.
            let payload = unsafe { &*slot.payload.get() };
            let matches = payload.generation == generation
                && payload.sequence == sequence
                && payload.start_sample == start_sample;
            slot.state.store(SLOT_RESPONSE_READY, Ordering::Release);
            matches
        })
    }

    fn discard_stale_requests(&self, generation: u64) {
        for slot in &self.slots {
            if slot
                .state
                .compare_exchange(
                    SLOT_REQUEST_READY,
                    SLOT_READING,
                    Ordering::Acquire,
                    Ordering::Relaxed,
                )
                .is_err()
            {
                continue;
            }
            // SAFETY: REQUEST_READY -> READING prevents the helper from claiming this payload.
            let request_generation = unsafe { (&*slot.payload.get()).generation };
            slot.state.store(
                if request_generation == generation {
                    SLOT_REQUEST_READY
                } else {
                    SLOT_FREE
                },
                Ordering::Release,
            );
        }
    }

    pub fn underrun_count(&self) -> u64 {
        self.underruns.load(Ordering::Relaxed)
    }
}

/// Claimed request data. The caller must finish the slot before releasing its ownership.
pub struct ClapIpcRequest<'a> {
    pub generation: u64,
    pub sequence: u64,
    pub start_sample: u64,
    pub events: &'a [ClapIpcMidiEvent],
    pub audio: &'a mut [[f32; CLAP_IPC_CHANNELS]],
}

/// Exclusive claim for one helper request. Dropping an unused claim frees its slot.
pub struct ClapIpcRequestSlot<'a> {
    region: &'a ClapIpcRegion,
    index: usize,
}

impl ClapIpcRequestSlot<'_> {
    /// Processes the claimed request and publishes its output if processing succeeds.
    pub fn process(self, process: impl for<'a> FnOnce(ClapIpcRequest<'a>) -> bool) -> bool {
        self.region.process_claimed_request(self.index, process)
    }
}

impl Drop for ClapIpcRequestSlot<'_> {
    fn drop(&mut self) {
        if self
            .region
            .active_slot
            .compare_exchange(
                self.index as u32,
                NO_ACTIVE_SLOT,
                Ordering::AcqRel,
                Ordering::Relaxed,
            )
            .is_ok()
        {
            let slot = &self.region.slots[self.index];
            let _ = slot.state.compare_exchange(
                SLOT_PROCESSING,
                SLOT_FREE,
                Ordering::Release,
                Ordering::Relaxed,
            );
            let _ = slot.state.compare_exchange(
                SLOT_RENDERING,
                SLOT_FREE,
                Ordering::Release,
                Ordering::Relaxed,
            );
        }
        self.region.helper_busy.store(false, Ordering::Release);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClapIpcSubmitError {
    NotReady,
    InvalidFrameCount,
    EventCapacityExceeded,
    InvalidEvent,
    ProducerBusy,
    SlotsFull,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn region_initialization_fits_in_a_one_megabyte_thread_stack() {
        let thread = thread::Builder::new()
            .stack_size(1024 * 1024)
            .spawn(|| {
                let config = ClapIpcConfig::new(48_000, 256, 32).unwrap();
                assert!(ClapIpcRegion::new(config).is_some());
            })
            .unwrap();
        thread.join().unwrap();
    }

    fn region() -> Box<ClapIpcRegion> {
        let config = ClapIpcConfig::new(48_000, 256, 32).unwrap();
        let region = ClapIpcRegion::new(config).unwrap();
        assert!(region.accept_handshake(config));
        region
    }

    #[test]
    fn versioned_handshake_accepts_matching_limits_and_rejects_mismatch() {
        let config = ClapIpcConfig::new(48_000, 256, 32).unwrap();
        let first_region = ClapIpcRegion::new(config).unwrap();
        assert!(first_region.accept_handshake(config));
        assert!(first_region.is_ready());

        let other = ClapIpcRegion::new(config).unwrap();
        let mut mismatch = config;
        mismatch.protocol_version += 1;
        assert!(!other.accept_handshake(mismatch));
        assert_eq!(other.fault_code(), 1);
        assert!(other.is_faulted());
        assert!(!other.is_ready());

        let retry_region = region();
        retry_region.try_submit(1, 3, 16, &[], 8).unwrap();
        let request = retry_region.try_claim_request().unwrap();
        assert!(request.process(|request| {
            request.audio.fill([0.25, -0.25]);
            true
        }));
        assert!(retry_region.has_response(1, 3, 16));
        assert!(retry_region.request_state_save().is_some());
        retry_region.mark_audio_worker_paused(true);
        retry_region.mark_faulted(5);
        retry_region.mark_shutdown();
        assert!(retry_region.is_faulted());
        assert_eq!(retry_region.fault_code(), 5);
        assert!(!retry_region.is_ready());
        retry_region.begin_startup();
        assert!(!retry_region.is_faulted());
        assert!(!retry_region.is_shutdown());
        assert!(!retry_region.has_response(1, 3, 16));
        assert_eq!(retry_region.pending_state_save(), None);
        assert!(!retry_region.is_audio_worker_paused());
        assert!(retry_region.accept_handshake(config));
        assert!(retry_region.is_ready());
        assert_eq!(retry_region.fault_code(), 0);
    }

    #[test]
    fn helper_heartbeat_increments_for_parent_side_stall_detection() {
        let region = region();
        assert_eq!(region.helper_heartbeat(), 0);
        region.publish_heartbeat();
        assert_eq!(region.helper_heartbeat(), 1);
        region.publish_heartbeat();
        assert_eq!(region.helper_heartbeat(), 2);
    }

    #[test]
    fn gui_requests_are_bounded_and_report_lifecycle_status() {
        let region = region();
        let open = region.request_gui(true, 42).unwrap();
        assert_eq!(open, 1);
        assert_eq!(region.request_gui(false, 42), None);
        assert_eq!(region.pending_gui_request(), Some((open, true, 42)));
        region.complete_gui_request(open, 1);
        assert_eq!(region.gui_status(), 1);

        let close = region.request_gui(false, 42).unwrap();
        assert_eq!(close, 2);
        assert_eq!(region.pending_gui_request(), Some((close, false, 42)));
        region.complete_gui_request(close, 0);
        assert_eq!(region.gui_status(), 0);
    }

    #[test]
    fn helper_rejects_a_mismatched_version_before_loading_a_plugin() {
        let config = ClapIpcConfig::new(48_000, 16, 8).unwrap();
        let mapping = ClapIpcMapping::create(config).unwrap();
        // SAFETY: this test retains the private mapping at its original size for the helper call.
        let path = unsafe { mapping.path() }.unwrap();
        let mut mismatched = config;
        mismatched.protocol_version += 1;

        // SAFETY: the mismatch is rejected before the plugin path can be loaded.
        let result = unsafe {
            run_clap_ipc_instrument_helper(
                &path,
                Path::new("plugin-must-not-be-loaded.clap"),
                "test.plugin",
                mismatched,
                Path::new("unused-input-state"),
                Path::new("unused-output-state"),
            )
        };
        assert!(result.is_err());
        assert!(mapping.region().is_faulted());
        assert_eq!(mapping.region().fault_code(), 1);
    }

    #[test]
    fn helper_state_file_roundtrips_optional_opaque_state() {
        let path = tempfile::NamedTempFile::new().unwrap().into_temp_path();
        write_helper_state(&path, None).unwrap();
        assert_eq!(read_helper_state(&path).unwrap(), None);

        let state = [0, 1, 255, 42];
        write_helper_state(&path, Some(&state)).unwrap();
        assert_eq!(read_helper_state(&path).unwrap(), Some(state.to_vec()));

        clear_helper_state(&path).unwrap();
        assert_eq!(
            read_helper_state(&path).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
    }

    #[test]
    fn interrupted_helper_state_write_keeps_last_saved_state() {
        let input = tempfile::NamedTempFile::new().unwrap().into_temp_path();
        let output = tempfile::NamedTempFile::new().unwrap().into_temp_path();
        let prior_state = [4, 8, 15, 16, 23, 42];
        write_helper_state(&input, Some(&prior_state)).unwrap();
        std::fs::write(&output, b"AAST\x01\x20").unwrap();

        assert_eq!(
            read_saved_helper_state(&output, &input).unwrap(),
            Some(prior_state.to_vec())
        );
        assert_eq!(
            read_helper_state(&output).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
    }

    #[test]
    fn dead_helper_recovery_reclaims_only_child_owned_slots() {
        let region = region();
        region.try_submit(1, 7, 512, &[], 4).unwrap();
        let request = region.try_claim_request().unwrap();
        std::mem::forget(request);

        // SAFETY: this test models a supervisor that has confirmed the helper process exited.
        region.recover_after_helper_exit();
        assert!(region.try_claim_request().is_none());
        region.try_submit(2, 8, 516, &[], 4).unwrap();
        assert!(region.try_claim_request().is_some());
    }

    #[test]
    fn oversized_response_output_is_rejected_without_unbounded_clear() {
        let region = region();
        let mut output = vec![[0.25, -0.25]; CLAP_IPC_MAX_BLOCK_FRAMES + 1];
        assert!(!region.try_read_response(1, 1, 0, &mut output));
        assert!(output.iter().all(|frame| *frame == [0.25, -0.25]));
    }

    #[test]
    fn file_mapping_is_shared_between_host_and_helper_views() {
        let config = ClapIpcConfig::new(48_000, 256, 32).unwrap();
        let host = ClapIpcMapping::create(config).unwrap();
        // SAFETY: the parent created this file, keeps its size fixed, and shares it only with the
        // helper view below.
        let path = unsafe { host.path() }.unwrap();
        // SAFETY: `path` names the fixed-size mapping initialized above; the host retains it.
        let helper = unsafe { ClapIpcMapping::open(&path) }.unwrap();
        assert!(helper.accept_handshake(config));
        assert!(host.region().is_ready());

        host.region().try_submit(1, 7, 512, &[], 4).unwrap();
        let slot = helper.region().try_claim_request().unwrap();
        assert!(slot.process(|request| {
            request.audio.fill([0.25, -0.25]);
            true
        }));
        let mut output = [[0.0; CLAP_IPC_CHANNELS]; 4];
        assert!(host.region().try_read_response(1, 7, 512, &mut output));
        assert_eq!(output, [[0.25, -0.25]; 4]);
    }

    #[test]
    fn callback_region_handle_keeps_mapping_alive_after_supervisor_release() {
        let config = ClapIpcConfig::new(48_000, 16, 8).unwrap();
        let mapping = ClapIpcMapping::create(config).unwrap();
        assert!(mapping.region().accept_handshake(config));
        // SAFETY: this test retains the callback handle at the mapping's original size.
        let path = unsafe { mapping.path() }.unwrap();
        let port = mapping.audio_port();
        drop(mapping);

        assert!(path.exists());
        assert_eq!(port.try_submit(1, 1, 0, &[], 8), Ok(()));
        drop(port);
        assert!(!path.exists());
    }

    #[test]
    fn concurrent_consumers_cannot_claim_or_read_the_same_slot() {
        use std::sync::{Arc, Barrier};
        use std::thread;

        let config = ClapIpcConfig::new(48_000, 8, 8).unwrap();
        let region = ClapIpcRegion::new(config).unwrap();
        region.accept_handshake(config);
        region.try_submit(3, 11, 64, &[], 8).unwrap();
        let region = Arc::<ClapIpcRegion>::from(region);
        let claim_barrier = Arc::new(Barrier::new(3));
        let claimers = (0..2)
            .map(|_| {
                let region = Arc::clone(&region);
                let barrier = Arc::clone(&claim_barrier);
                thread::spawn(move || {
                    barrier.wait();
                    let claim = region.try_claim_request();
                    let won = claim.is_some();
                    barrier.wait();
                    won
                })
            })
            .collect::<Vec<_>>();
        claim_barrier.wait();
        claim_barrier.wait();
        let claims = claimers
            .into_iter()
            .map(|join| join.join().unwrap())
            .filter(|won| *won)
            .collect::<Vec<_>>();
        assert_eq!(claims.len(), 1);
        assert!(!region.helper_busy.load(Ordering::Acquire));
        assert_eq!(region.slots[0].state.load(Ordering::Acquire), SLOT_FREE);
        region.try_submit(3, 11, 64, &[], 8).unwrap();
        let slot = region.try_claim_request().unwrap();
        assert!(slot.process(|request| {
            request.audio.fill([0.5, -0.5]);
            true
        }));

        let read_barrier = Arc::new(Barrier::new(3));
        let readers = (0..2)
            .map(|_| {
                let region = Arc::clone(&region);
                let barrier = Arc::clone(&read_barrier);
                thread::spawn(move || {
                    let mut output = [[0.0; CLAP_IPC_CHANNELS]; 8];
                    barrier.wait();
                    (region.try_read_response(3, 11, 64, &mut output), output)
                })
            })
            .collect::<Vec<_>>();
        read_barrier.wait();
        let responses = readers
            .into_iter()
            .map(|join| join.join().unwrap())
            .filter(|(read, _)| *read)
            .collect::<Vec<_>>();
        assert_eq!(responses.len(), 1);
        assert_eq!(responses[0].1, [[0.5, -0.5]; 8]);
    }

    #[test]
    fn shared_slots_transfer_bounded_midi_and_stereo_audio() {
        let region = region();
        let event = ClapIpcMidiEvent {
            frame_offset: 7,
            note_id: 55,
            pitch_bend: ClapIpcMidiEvent::NO_PITCH_BEND,
            kind: ClapIpcMidiKind::NoteOn as u8,
            pitch: 60,
            velocity: 100,
            controller: ClapIpcMidiEvent::NO_CONTROLLER,
            ..ClapIpcMidiEvent::default()
        };
        region.try_submit(4, 9, 256, &[event], 16).unwrap();
        let slot = region.try_claim_request().unwrap();
        assert!(slot.process(|request| {
            assert_eq!(request.generation, 4);
            assert_eq!(request.sequence, 9);
            assert_eq!(request.start_sample, 256);
            assert_eq!(request.events, &[event]);
            for (frame, audio) in request.audio.iter_mut().enumerate() {
                *audio = [frame as f32, -(frame as f32)];
            }
            true
        }));

        let mut output = [[1.0, 1.0]; 16];
        assert!(region.try_read_response(4, 9, 256, &mut output));
        assert_eq!(output[0], [0.0, -0.0]);
        assert_eq!(output[15], [15.0, -15.0]);
        assert!(!region.try_read_response(4, 9, 256, &mut output));
        assert!(output.iter().all(|frame| *frame == [0.0, 0.0]));
        assert_eq!(region.underrun_count(), 1);
    }

    #[test]
    fn obsolete_transport_generation_requests_are_reclaimed_for_seek_recovery() {
        let region = region();
        region.try_submit(4, 0, 128, &[], 8).unwrap();
        region.try_submit(4, 1, 136, &[], 8).unwrap();

        region.discard_stale_requests(5);

        region.try_submit(5, 0, 256, &[], 8).unwrap();
        let request = region.try_claim_request().unwrap();
        assert!(request.process(|request| {
            assert_eq!(request.generation, 5);
            assert_eq!(request.sequence, 0);
            assert_eq!(request.start_sample, 256);
            true
        }));
        assert!(region.has_response(5, 0, 256));
    }

    #[test]
    fn audio_reader_preserves_helper_block_across_callback_sizes() {
        let config = ClapIpcConfig::new(48_000, 4, 8).unwrap();
        let ipc_mapping = ClapIpcMapping::create(config).unwrap();
        ipc_mapping.region().accept_handshake(config);
        ipc_mapping.region().try_submit(3, 5, 64, &[], 4).unwrap();
        let request = ipc_mapping.region().try_claim_request().unwrap();
        assert!(request.process(|request| {
            request
                .audio
                .copy_from_slice(&[[0.1, -0.1], [0.2, -0.2], [0.3, -0.3], [0.4, -0.4]]);
            true
        }));

        let port = ipc_mapping.audio_port();
        let mut reader = port.reader(3, 5, 64).unwrap();
        let mut first = [[0.0; 2]; 1];
        let mut second = [[0.0; 2]; 3];
        assert!(reader.read_into(&mut first));
        assert!(reader.read_into(&mut second));
        assert_eq!(first, [[0.1, -0.1]]);
        assert_eq!(second, [[0.2, -0.2], [0.3, -0.3], [0.4, -0.4]]);
        assert_eq!(reader.next_sequence(), 6);
        assert_eq!(reader.next_sample(), 68);
    }

    #[test]
    fn audio_reader_expires_late_block_and_continues_at_next_position() {
        let config = ClapIpcConfig::new(48_000, 4, 8).unwrap();
        let mapping = ClapIpcMapping::create(config).unwrap();
        mapping.region().accept_handshake(config);
        let port = mapping.audio_port();
        let mut reader = port.reader(1, 0, 0).unwrap();
        mapping.region().try_submit(1, 0, 0, &[], 4).unwrap();

        let mut first = [[1.0; 2]; 2];
        assert!(!reader.read_into(&mut first));
        assert_eq!(first, [[0.0; 2]; 2]);

        let late_request = mapping.region().try_claim_request().unwrap();
        assert!(late_request.process(|request| {
            request.audio.fill([0.75, -0.75]);
            true
        }));
        mapping.region().try_submit(1, 1, 4, &[], 4).unwrap();
        let on_time_request = mapping.region().try_claim_request().unwrap();
        assert!(on_time_request.process(|request| {
            request.audio.fill([0.25, -0.25]);
            true
        }));

        let mut next = [[1.0; 2]; 4];
        assert!(!reader.read_into(&mut next));
        assert_eq!(next, [[0.0; 2], [0.0; 2], [0.25, -0.25], [0.25, -0.25]]);
        assert_eq!(reader.next_sequence(), 1);
        assert_eq!(reader.next_sample(), 6);
    }

    #[test]
    fn oversized_blocks_and_full_slot_set_fail_without_waiting() {
        let config = ClapIpcConfig::new(48_000, 2, 1).unwrap();
        let region = ClapIpcRegion::new(config).unwrap();
        region.accept_handshake(config);
        let event = [ClapIpcMidiEvent::default(); 2];
        assert_eq!(
            region.try_submit(1, 1, 0, &[], 3),
            Err(ClapIpcSubmitError::InvalidFrameCount)
        );
        assert_eq!(
            region.try_submit(1, 1, 0, &event, 1),
            Err(ClapIpcSubmitError::EventCapacityExceeded)
        );
        for sequence in 0..CLAP_IPC_SLOT_COUNT as u64 {
            region
                .try_submit(1, sequence, sequence * 2, &[], 1)
                .unwrap();
        }
        assert_eq!(
            region.try_submit(1, 5, 10, &[], 1),
            Err(ClapIpcSubmitError::SlotsFull)
        );
    }

    #[test]
    fn midi_events_accept_unknown_clap_note_id_but_reject_invalid_offsets() {
        let mut event = ClapIpcMidiEvent {
            frame_offset: 3,
            note_id: ClapIpcMidiEvent::NO_NOTE_ID,
            pitch_bend: ClapIpcMidiEvent::NO_PITCH_BEND,
            kind: ClapIpcMidiKind::NoteOn as u8,
            pitch: 64,
            velocity: 110,
            controller: ClapIpcMidiEvent::NO_CONTROLLER,
            reserved: [0; 2],
        };
        assert!(event.is_valid(4));
        event.frame_offset = 4;
        assert!(!event.is_valid(4));
    }

    #[test]
    fn scheduled_midi_events_encode_to_valid_ipc_packets() {
        let track_id = aaadaw_core::TrackId::from_value(1).unwrap();
        let note = crate::ScheduledMidiEvent {
            sample_offset: 3,
            track_id,
            note_id: Some(aaadaw_core::NoteId::from_value(11).unwrap()),
            pitch: 64,
            velocity: 100,
            controller: None,
            pitch_bend: None,
            kind: crate::MidiEventKind::NoteOn,
        };
        let encoded = ClapIpcMidiEvent::from_scheduled(note, 8).unwrap();
        assert_eq!(encoded.frame_offset, 3);
        assert_eq!(encoded.note_id, 12);
        assert_eq!(encoded.kind, ClapIpcMidiKind::NoteOn as u8);
        assert!(encoded.is_valid(8));

        let controller = crate::ScheduledMidiEvent {
            sample_offset: 0,
            track_id,
            note_id: None,
            pitch: 0,
            velocity: 0,
            controller: Some(64),
            pitch_bend: None,
            kind: crate::MidiEventKind::ControllerChange,
        };
        let encoded = ClapIpcMidiEvent::from_scheduled(controller, 8).unwrap();
        assert_eq!(encoded.controller, 64);
        assert_eq!(encoded.note_id, ClapIpcMidiEvent::NO_NOTE_ID);

        let mut invalid_offset = note;
        invalid_offset.sample_offset = 8;
        assert!(ClapIpcMidiEvent::from_scheduled(invalid_offset, 8).is_none());
    }

    #[test]
    fn future_response_stays_available_until_its_audio_position() {
        let region = region();
        region.try_submit(2, 5, 128, &[], 4).unwrap();
        let slot = region.try_claim_request().unwrap();
        assert!(slot.process(|request| {
            request.audio.fill([0.75, -0.75]);
            true
        }));

        let mut output = [[0.0; CLAP_IPC_CHANNELS]; 4];
        assert!(!region.try_read_response(2, 4, 96, &mut output));
        assert_eq!(output, [[0.0, 0.0]; 4]);
        assert!(region.try_read_response(2, 5, 128, &mut output));
        assert_eq!(output, [[0.75, -0.75]; 4]);
    }

    #[test]
    fn mismatched_sample_for_matching_sequence_reclaims_corrupt_response() {
        let region = region();
        region.try_submit(2, 5, 128, &[], 4).unwrap();
        let slot = region.try_claim_request().unwrap();
        assert!(slot.process(|request| {
            request.audio.fill([0.75, -0.75]);
            true
        }));

        let mut output = [[1.0; CLAP_IPC_CHANNELS]; 4];
        assert!(!region.try_read_response(2, 5, 124, &mut output));
        assert_eq!(output, [[0.0, 0.0]; 4]);
        assert!(!region.try_read_response(2, 5, 128, &mut output));
        assert_eq!(output, [[0.0, 0.0]; 4]);
    }
}
