use aaadaw_core::{DawAction, Project};
use aaadaw_storage::ProjectStore;
use std::error::Error;
use std::fs;
use std::io::Cursor;
use std::path::PathBuf;

const SAMPLE_RATE: u32 = 48_000;
const BEAT_FRAMES: u32 = SAMPLE_RATE / 2;
const NOTE_FREQUENCIES: [f32; 8] = [220.0, 261.63, 329.63, 261.63, 293.66, 349.23, 392.0, 349.23];
const MEDIA_REF: &str = "asset://first-project/melody.wav";

fn main() -> Result<(), Box<dyn Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/first-project");
    fs::create_dir_all(&root)?;

    let wav = melody_wav();
    let wav_path = root.join("melody.wav");
    fs::write(&wav_path, &wav)?;

    let project_path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("first-project.aaadaw"));
    let scratch = tempfile::tempdir()?;
    let scratch_project = scratch.path().join("first-project.aaadaw");

    let mut project = Project::new();
    project.apply(DawAction::CreateTrack {
        index: 0,
        name: "Audio".to_owned(),
    })?;
    project.apply(DawAction::InsertAudioItem {
        track_id: project.tracks()[0].id(),
        media_ref: MEDIA_REF.to_owned(),
        start_sample: 0,
        source_offset_samples: 0,
        length_samples: u64::from(BEAT_FRAMES) * NOTE_FREQUENCIES.len() as u64,
    })?;

    let mut store = ProjectStore::open(&scratch_project)?;
    store.import_audio_asset(MEDIA_REF, "melody.wav", Cursor::new(&wav))?;
    store.save(&project)?;
    store.close()?;
    fs::copy(&scratch_project, &project_path)?;

    println!("Wrote {}", project_path.display());
    println!("Wrote {}", wav_path.display());
    Ok(())
}

fn melody_wav() -> Vec<u8> {
    let frame_count = BEAT_FRAMES as usize * NOTE_FREQUENCIES.len();
    let data_len = (frame_count * 2) as u32;
    let mut wav = Vec::with_capacity(44 + data_len as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_len).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16_u32.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    wav.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes());
    wav.extend_from_slice(&2_u16.to_le_bytes());
    wav.extend_from_slice(&16_u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());

    for frame in 0..frame_count {
        let note_index = frame / BEAT_FRAMES as usize;
        let note_frame = frame % BEAT_FRAMES as usize;
        let frequency = NOTE_FREQUENCIES[note_index];
        let phase = std::f32::consts::TAU * frequency * note_frame as f32 / SAMPLE_RATE as f32;
        let attack = (note_frame as f32 / 480.0).min(1.0);
        let release = ((BEAT_FRAMES as usize - note_frame) as f32 / 2_400.0).min(1.0);
        let envelope = attack.min(release);
        let signal = (phase.sin() * 0.28 + (phase * 2.0).sin() * 0.04) * envelope;
        let pcm = (signal * i16::MAX as f32).round() as i16;
        wav.extend_from_slice(&pcm.to_le_bytes());
    }
    wav
}
