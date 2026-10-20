use aaadaw_core::{DawAction, MidiNoteData, Project};
use aaadaw_storage::ProjectStore;
use std::fs;
use std::path::PathBuf;

fn notes(pitch: u8, velocity: u8) -> Vec<MidiNoteData> {
    vec![
        MidiNoteData {
            pitch,
            channel: 0,
            tick: 120,
            duration: 240,
            velocity,
            off_velocity: 0,
        },
        MidiNoteData {
            pitch: pitch + 2,
            channel: 0,
            tick: 720,
            duration: 240,
            velocity: velocity.saturating_sub(5),
            off_velocity: 0,
        },
    ]
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("expected the output project path")?,
    );
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut project = Project::new();
    for (index, name) in ["Top MIDI track", "Earlier MIDI track", "Lower MIDI track"]
        .into_iter()
        .enumerate()
    {
        project.apply(DawAction::CreateTrack {
            index,
            name: name.to_owned(),
        })?;
    }
    let track_ids = project
        .tracks()
        .iter()
        .map(|track| track.id())
        .collect::<Vec<_>>();
    let items = [
        (track_ids[0], 1_920, 3_840, "Top target", 240, 60, 96),
        (track_ids[1], 0, 2_880, "Earlier donor", 480, 67, 88),
        (track_ids[2], 3_840, 2_880, "Lower donor", 960, 79, 76),
    ];
    let mut item_ids = Vec::new();
    for (track_id, start_tick, length_ticks, name, offset, pitch, velocity) in items {
        project.apply(DawAction::InsertMidiItem {
            track_id,
            start_tick,
            length_ticks,
        })?;
        let item_id = project.midi_items().last().ok_or("MIDI item missing")?.id();
        project.apply(DawAction::SetMidiItemLoopSource {
            item_id,
            enabled: false,
        })?;
        project.apply(DawAction::AddMidiNotes {
            item_id,
            notes: notes(pitch, velocity),
        })?;
        project.apply(DawAction::SetMidiItemName {
            item_id,
            name: name.to_owned(),
        })?;
        project.apply(DawAction::EditMidiItemPlacement {
            item_id,
            start_tick,
            length_ticks,
            source_offset_ticks: offset,
        })?;
        item_ids.push(item_id);
    }

    let middle_id = item_ids[1];
    let mut active_take = project.midi_items()[1].take_snapshots()[0].clone();
    active_take.index = 1;
    active_take.name = "Earlier donor active".to_owned();
    active_take.source_offset_ticks = 720;
    active_take.notes[0].data = MidiNoteData {
        pitch: 74,
        channel: 0,
        tick: 120,
        duration: 240,
        velocity: 80,
        off_velocity: 0,
    };
    active_take.notes[1].data = MidiNoteData {
        pitch: 76,
        channel: 0,
        tick: 720,
        duration: 240,
        velocity: 75,
        off_velocity: 0,
    };
    project.apply(DawAction::PasteMidiItemAsTake {
        item_id: middle_id,
        take: active_take,
    })?;
    project.apply(DawAction::SetMidiItemActiveTake {
        item_id: middle_id,
        take_index: 1,
    })?;

    let mut store = ProjectStore::open(&output)?;
    store.save(&project)?;
    store.close()?;
    println!("Wrote {} from items {:?}", output.display(), item_ids);
    Ok(())
}
