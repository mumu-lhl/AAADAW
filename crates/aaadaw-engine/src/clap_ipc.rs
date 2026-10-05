//! Fixed-layout shared-memory protocol for one supervised CLAP instrument.
//!
//! The protocol has no Rust pointers, `Vec`s, or platform-sized integers in its mapped payload.
//! Slot payload access is protected by release/acquire state transitions. The mapping owner must
//! initialize the region before launching a helper and keep the backing file at its original size
//! until the helper exits.

use std::cell::UnsafeCell;
use std::fs::OpenOptions;
use std::io;
use std::path::{Path, PathBuf};
use std::ptr;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use tempfile::{NamedTempFile, TempPath};

use memmap2::{MmapMut, MmapOptions};

pub const CLAP_IPC_PROTOCOL_VERSION: u32 = 1;
pub const CLAP_IPC_CHANNELS: usize = 2;
pub const CLAP_IPC_MAX_BLOCK_FRAMES: usize = 1024;
pub const CLAP_IPC_MAX_EVENTS: usize = 1024;
pub const CLAP_IPC_SLOT_COUNT: usize = 4;
pub const CLAP_IPC_MAGIC: u32 = u32::from_le_bytes(*b"AAIP");

const REGION_INITIALIZING: u32 = 0;
const REGION_READY: u32 = 1;
const REGION_FAULTED: u32 = 2;
const REGION_SHUTDOWN: u32 = 3;

const SLOT_FREE: u32 = 0;
const SLOT_WRITING: u32 = 1;
const SLOT_REQUEST_READY: u32 = 2;
const SLOT_PROCESSING: u32 = 3;
const SLOT_RESPONSE_READY: u32 = 4;

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClapIpcMidiKind {
    NoteOn = 1,
    NoteOff = 2,
    ControllerChange = 3,
    PitchBend = 4,
}

/// Owns one cross-process mapping and its private temporary backing file.
pub struct ClapIpcMapping {
    mapping: MmapMut,
    path: Option<TempPath>,
}

// SAFETY: the mapping exposes data only through ClapIpcRegion's slot-state protocol. The backing
// file stays open by the mapping and is never resized. `path` is immutable after construction.
unsafe impl Send for ClapIpcMapping {}
// SAFETY: see the Send contract above. All shared payload access is gated by process-shared atomics.
unsafe impl Sync for ClapIpcMapping {}

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
            mapping,
            path: Some(temporary.into_temp_path()),
        })
    }

    /// Opens a helper-side view of a mapping created by the host.
    pub fn open(path: &Path) -> io::Result<Self> {
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
            mapping,
            path: None,
        })
    }

    pub const fn mapped_len() -> usize {
        std::mem::size_of::<ClapIpcRegion>()
    }

    pub fn path(&self) -> Option<PathBuf> {
        self.path.as_ref().map(|path| path.to_path_buf())
    }

    pub fn region(&self) -> &ClapIpcRegion {
        // SAFETY: mapping is aligned to the system page size, initialized before use, and has the
        // exact region size. Atomic slot ownership protects payload fields shared with the helper.
        unsafe { &*self.mapping.as_ptr().cast::<ClapIpcRegion>() }
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
            ptr::addr_of_mut!((*region).fault_code).write(AtomicU32::new(0));
            ptr::addr_of_mut!((*region).next_slot).write(AtomicU32::new(0));
            ptr::addr_of_mut!((*region).underruns).write(AtomicU64::new(0));
            for index in 0..CLAP_IPC_SLOT_COUNT {
                let slot = ptr::addr_of_mut!((*region).slots[index]);
                ptr::addr_of_mut!((*slot).state).write(AtomicU32::new(SLOT_FREE));
            }
        }
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
                self.note_id != Self::NO_NOTE_ID
                    && self.note_id <= i32::MAX as u32
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
    fault_code: AtomicU32,
    next_slot: AtomicU32,
    underruns: AtomicU64,
    slots: [ClapIpcSlot; CLAP_IPC_SLOT_COUNT],
}

// SAFETY: non-atomic slot fields are read or written only by the process holding the slot's
// corresponding state. The owner publishes each completed payload with Release and its peer
// acquires that state before touching the payload. Configuration fields are immutable after the
// initial map has been published. Atomics used here are process-shared lock-free integer atomics
// on the supported Linux and Windows x86_64 targets.
unsafe impl Sync for ClapIpcRegion {}

impl ClapIpcRegion {
    pub fn new(config: ClapIpcConfig) -> Option<Box<Self>> {
        config.validate().then(|| {
            Box::new(Self {
                config,
                state: AtomicU32::new(REGION_INITIALIZING),
                fault_code: AtomicU32::new(0),
                next_slot: AtomicU32::new(0),
                underruns: AtomicU64::new(0),
                slots: std::array::from_fn(|_| ClapIpcSlot::new()),
            })
        })
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
        self.state.store(REGION_READY, Ordering::Release);
        true
    }

    pub fn is_ready(&self) -> bool {
        self.state.load(Ordering::Acquire) == REGION_READY
    }

    pub fn fault_code(&self) -> u32 {
        self.fault_code.load(Ordering::Acquire)
    }

    pub fn mark_faulted(&self, code: u32) {
        self.fault_code.store(code, Ordering::Relaxed);
        self.state.store(REGION_FAULTED, Ordering::Release);
    }

    pub fn mark_shutdown(&self) {
        self.state.store(REGION_SHUTDOWN, Ordering::Release);
    }

    pub fn is_shutdown(&self) -> bool {
        self.state.load(Ordering::Acquire) == REGION_SHUTDOWN
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

        let start = self.next_slot.fetch_add(1, Ordering::Relaxed) as usize % CLAP_IPC_SLOT_COUNT;
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
    }

    /// Claims the oldest pending slot. Returns immediately when no request is ready.
    pub fn try_claim_request(&self) -> Option<usize> {
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
        let index = candidate?;
        self.slots[index]
            .state
            .compare_exchange(
                SLOT_REQUEST_READY,
                SLOT_PROCESSING,
                Ordering::Acquire,
                Ordering::Relaxed,
            )
            .ok()
            .map(|_| index)
    }

    /// Processes one claimed request and publishes its response after the closure returns.
    pub fn process_request(
        &self,
        index: usize,
        process: impl for<'a> FnOnce(ClapIpcRequest<'a>) -> bool,
    ) -> bool {
        let Some(slot) = self.slots.get(index) else {
            return false;
        };
        if slot.state.load(Ordering::Acquire) != SLOT_PROCESSING {
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
        slot.state
            .compare_exchange(
                SLOT_PROCESSING,
                next_state,
                Ordering::Release,
                Ordering::Relaxed,
            )
            .is_ok()
            && succeeded
    }

    /// Copies a matching response into preallocated callback output and frees its slot.
    pub fn try_read_response(
        &self,
        generation: u64,
        sequence: u64,
        start_sample: u64,
        output: &mut [[f32; CLAP_IPC_CHANNELS]],
    ) -> bool {
        output.fill([0.0; CLAP_IPC_CHANNELS]);
        for slot in &self.slots {
            if slot.state.load(Ordering::Acquire) != SLOT_RESPONSE_READY {
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
                    || (response_generation == generation && response_sequence < sequence)
                {
                    let _ = slot.state.compare_exchange(
                        SLOT_RESPONSE_READY,
                        SLOT_FREE,
                        Ordering::Release,
                        Ordering::Relaxed,
                    );
                }
                continue;
            }
            if payload.frame_count as usize != output.len() {
                let _ = slot.state.compare_exchange(
                    SLOT_RESPONSE_READY,
                    SLOT_FREE,
                    Ordering::Release,
                    Ordering::Relaxed,
                );
                return false;
            }
            output.copy_from_slice(&payload.audio[..output.len()]);
            if slot
                .state
                .compare_exchange(
                    SLOT_RESPONSE_READY,
                    SLOT_FREE,
                    Ordering::Release,
                    Ordering::Relaxed,
                )
                .is_ok()
            {
                return true;
            }
            output.fill([0.0; CLAP_IPC_CHANNELS]);
            return false;
        }
        self.underruns.fetch_add(1, Ordering::Relaxed);
        false
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClapIpcSubmitError {
    NotReady,
    InvalidFrameCount,
    EventCapacityExceeded,
    InvalidEvent,
    SlotsFull,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn region() -> Box<ClapIpcRegion> {
        let config = ClapIpcConfig::new(48_000, 256, 32).unwrap();
        let region = ClapIpcRegion::new(config).unwrap();
        assert!(region.accept_handshake(config));
        region
    }

    #[test]
    fn versioned_handshake_accepts_matching_limits_and_rejects_mismatch() {
        let config = ClapIpcConfig::new(48_000, 256, 32).unwrap();
        let region = ClapIpcRegion::new(config).unwrap();
        assert!(region.accept_handshake(config));
        assert!(region.is_ready());

        let other = ClapIpcRegion::new(config).unwrap();
        let mut mismatch = config;
        mismatch.protocol_version += 1;
        assert!(!other.accept_handshake(mismatch));
        assert_eq!(other.fault_code(), 1);
        assert!(!other.is_ready());
    }

    #[test]
    fn file_mapping_is_shared_between_host_and_helper_views() {
        let config = ClapIpcConfig::new(48_000, 256, 32).unwrap();
        let host = ClapIpcMapping::create(config).unwrap();
        let helper = ClapIpcMapping::open(&host.path().unwrap()).unwrap();
        assert!(helper.accept_handshake(config));
        assert!(host.region().is_ready());

        host.region().try_submit(1, 7, 512, &[], 4).unwrap();
        let slot = helper.region().try_claim_request().unwrap();
        assert!(helper.region().process_request(slot, |request| {
            request.audio.fill([0.25, -0.25]);
            true
        }));
        let mut output = [[0.0; CLAP_IPC_CHANNELS]; 4];
        assert!(host.region().try_read_response(1, 7, 512, &mut output));
        assert_eq!(output, [[0.25, -0.25]; 4]);
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
        assert!(region.process_request(slot, |request| {
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
}
