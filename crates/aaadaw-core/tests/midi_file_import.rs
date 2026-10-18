use aaadaw_core::{
    ActionError, DawAction, MidiFileEventKind, MidiFileFormat, Project, decode_standard_midi,
};

fn smf(format: u16, division: u16, tracks: &[&[u8]]) -> Vec<u8> {
    let mut bytes = b"MThd\0\0\0\x06".to_vec();
    bytes.extend_from_slice(&format.to_be_bytes());
    bytes.extend_from_slice(&(tracks.len() as u16).to_be_bytes());
    bytes.extend_from_slice(&division.to_be_bytes());
    for track in tracks {
        bytes.extend_from_slice(b"MTrk");
        bytes.extend_from_slice(&(track.len() as u32).to_be_bytes());
        bytes.extend_from_slice(track);
    }
    bytes
}

#[test]
fn decodes_type_zero_running_status_and_preserves_sysex_and_meta() {
    let track = [
        0x00, 0x90, 60, 100, // Note On
        0x00, 64, 90, // Running-status Note On
        0x83, 0x60, 0x80, 60, 23, // Note Off at tick 480
        0x00, 64, 45, // Running-status Note Off
        0x00, 0xF0, 0x03, 0x01, 0x02, 0x03, // SysEx
        0x00, 0xFF, 0x01, 0x03, b'X', b'Y', b'Z', // Text meta event
        0x00, 0xFF, 0x2F, 0x00, // End of Track
    ];
    let decoded = decode_standard_midi(&smf(0, 960, &[&track])).unwrap();

    assert_eq!(decoded.format, MidiFileFormat::Type0);
    assert_eq!(decoded.ticks_per_quarter_note, 960);
    assert_eq!(decoded.tracks.len(), 1);
    assert_eq!(decoded.tracks[0].end_tick, 480);
    assert_eq!(decoded.tracks[0].events.len(), 7);
    assert_eq!(
        decoded.tracks[0].events[1].kind,
        MidiFileEventKind::Channel {
            status: 0x90,
            data: vec![64, 90]
        }
    );
    assert_eq!(
        decoded.tracks[0].events[3].kind,
        MidiFileEventKind::Channel {
            status: 0x80,
            data: vec![64, 45]
        }
    );
    assert_eq!(
        decoded.tracks[0].events[4].kind,
        MidiFileEventKind::SystemExclusive {
            status: 0xF0,
            data: vec![1, 2, 3]
        }
    );
    assert_eq!(
        decoded.tracks[0].events[5].kind,
        MidiFileEventKind::Meta {
            kind: 0x01,
            data: b"XYZ".to_vec()
        }
    );
    assert_eq!(decoded.tracks[0].events[6].tick, 480);
}

#[test]
fn decodes_each_type_one_track_with_independent_absolute_time() {
    let conductor = [
        0x00, 0xFF, 0x03, 0x04, b'M', b'a', b'i', b'n', // Track name
        0x00, 0xB0, 7, 90, // Channel event on the first musical track
        0x64, 0xFF, 0x2F, 0x00, // End at tick 100
    ];
    let instrument = [
        0x00, 0xC2, 0x05, // Program Change, channel 2
        0x05, 0xE2, 0x00, 0x40, // Pitch bend at tick 5
        0x00, 0xD2, 0x7F, // Channel pressure
        0x00, 0xFF, 0x2F, 0x00,
    ];
    let decoded = decode_standard_midi(&smf(1, 480, &[&conductor, &instrument])).unwrap();

    assert_eq!(decoded.format, MidiFileFormat::Type1);
    assert_eq!(decoded.tracks.len(), 2);
    assert_eq!(decoded.tracks[0].end_tick, 100);
    assert_eq!(decoded.tracks[1].end_tick, 5);
    assert_eq!(
        decoded.tracks[0].events[0].kind,
        MidiFileEventKind::Meta {
            kind: 0x03,
            data: b"Main".to_vec()
        }
    );
    assert_eq!(
        decoded.tracks[1].events[0].kind,
        MidiFileEventKind::Channel {
            status: 0xC2,
            data: vec![5]
        }
    );
    assert_eq!(
        decoded.tracks[1].events[1].kind,
        MidiFileEventKind::Channel {
            status: 0xE2,
            data: vec![0, 64]
        }
    );
    let project_import = decoded.to_project_import(480).unwrap();
    assert_eq!(project_import.tracks.len(), 2);
    assert_eq!(project_import.tracks[0].name, "Main");
    assert_eq!(project_import.tracks[0].controllers[0].value, 90);
    assert_eq!(project_import.tracks[1].name, "MIDI 2");
}

#[test]
fn rejects_unsupported_time_bases_and_truncated_or_invalid_events() {
    assert!(decode_standard_midi(&smf(2, 480, &[&[0x00, 0xFF, 0x2F, 0x00]])).is_err());
    assert!(decode_standard_midi(&smf(0, 0xE728, &[&[0x00, 0xFF, 0x2F, 0x00]])).is_err());
    assert!(decode_standard_midi(&smf(0, 480, &[&[0x80, 0x80, 0x80, 0x80, 0x00]])).is_err());
    assert!(decode_standard_midi(&smf(0, 480, &[&[0x00, 0x90, 60]])).is_err());
    assert!(decode_standard_midi(&smf(0, 480, &[&[0x00, 0xF0, 0x03, 0x01]])).is_err());
    assert!(decode_standard_midi(&smf(0, 480, &[&[0x00, 0xFF, 0x2F, 0x01, 0x00]])).is_err());
}

#[test]
fn rejects_data_after_end_of_track_and_running_status_without_a_channel_status() {
    assert!(
        decode_standard_midi(&smf(
            0,
            480,
            &[&[0x00, 0xFF, 0x2F, 0x00, 0x00, 0x90, 60, 100]],
        ))
        .is_err()
    );
    assert!(decode_standard_midi(&smf(0, 480, &[&[0x00, 60, 100]])).is_err());
}

#[test]
fn pairs_retriggered_same_pitch_notes_fifo_and_rejects_unbalanced_note_events() {
    let track = [
        0x00, 0x90, 60, 80, // First Note On at tick 0
        0x0F, 60, 90, // Retrigger at tick 15
        0x05, 0x80, 60, 10, // First Note Off at tick 20
        0x05, 60, 20, // Second Note Off at tick 25
        0x00, 0xFF, 0x2F, 0x00,
    ];
    let decoded = decode_standard_midi(&smf(0, 480, &[&track])).unwrap();
    let notes = decoded
        .to_project_import(480)
        .unwrap()
        .tracks
        .remove(0)
        .notes;
    assert_eq!(notes.len(), 2);
    assert_eq!(notes[0].velocity, 80);
    assert_eq!(notes[0].duration, 20);
    assert_eq!(notes[0].off_velocity, 10);
    assert_eq!(notes[1].velocity, 90);
    assert_eq!(notes[1].duration, 10);
    assert_eq!(notes[1].off_velocity, 20);

    let unmatched_note_off = smf(0, 480, &[&[0x00, 0x80, 60, 0, 0, 0xFF, 0x2F, 0]]);
    assert!(
        decode_standard_midi(&unmatched_note_off)
            .unwrap()
            .to_project_import(480)
            .is_err()
    );
    let unmatched_note_on = smf(0, 480, &[&[0x00, 0x90, 60, 100, 0, 0xFF, 0x2F, 0]]);
    assert!(
        decode_standard_midi(&unmatched_note_on)
            .unwrap()
            .to_project_import(480)
            .is_err()
    );
}

#[test]
fn converts_supported_channel_messages_and_groups_import_as_one_undoable_action() {
    let track = [
        0x00, 0xFF, 0x03, 0x05, b'P', b'i', b'a', b'n', b'o', // Track name
        0x00, 0x90, 60, 100, // Note On
        0x00, 0xB2, 7, 96, // CC on channel 2
        0x00, 0xC2, 5, // Program Change
        0x00, 0xA2, 60, 70, // Poly Aftertouch
        0x00, 0xD2, 80, // Channel Pressure
        0x00, 0xE2, 0x00, 0x40, // Pitch bend center
        0x83, 0x60, 0x80, 60, 27, // Note Off at tick 480
        0x00, 0xF0, 0x02, 0x01, 0x02, // Unsupported SysEx, retained by parser
        0x00, 0xFF, 0x01, 0x01, b'x', // Unsupported text metadata
        0x00, 0xFF, 0x2F, 0x00,
    ];
    let parsed = decode_standard_midi(&smf(0, 480, &[&track])).unwrap();
    let converted = parsed.to_project_import(960).unwrap();
    assert_eq!(converted.unsupported_event_count, 2);
    assert_eq!(converted.tracks.len(), 1);
    assert_eq!(converted.tracks[0].name, "Piano");
    assert_eq!(converted.tracks[0].length_ticks, 960);
    assert_eq!(converted.tracks[0].notes[0].duration, 960);
    assert_eq!(converted.tracks[0].notes[0].off_velocity, 27);
    assert_eq!(converted.tracks[0].controllers[0].channel, 2);
    assert_eq!(converted.tracks[0].pitch_bends[0].value, 8192);
    assert_eq!(converted.tracks[0].program_changes[0].program, 5);
    assert_eq!(converted.tracks[0].poly_aftertouches[0].value, 70);
    assert_eq!(converted.tracks[0].channel_pressures[0].value, 80);

    let mut project = Project::new();
    project
        .apply(DawAction::BatchTransaction {
            tx_id: 77,
            actions: converted
                .tracks
                .into_iter()
                .map(|track| DawAction::ImportMidiTrack {
                    start_tick: 1920,
                    track,
                })
                .collect(),
        })
        .unwrap();
    assert_eq!(project.tracks().len(), 1);
    assert_eq!(project.tracks()[0].name(), "Piano");
    assert_eq!(project.midi_items().len(), 1);
    let item = &project.midi_items()[0];
    assert_eq!(item.start_tick(), 1920);
    assert_eq!(item.length_ticks(), 960);
    assert!(!item.is_loop_source());
    assert_eq!(item.notes().len(), 1);
    assert_eq!(item.controllers().len(), 1);
    assert_eq!(item.pitch_bends().len(), 1);
    assert_eq!(item.program_changes().len(), 1);
    assert_eq!(item.poly_aftertouches().len(), 1);
    assert_eq!(item.channel_pressures().len(), 1);

    let snapshot = project.snapshot();
    assert_eq!(
        Project::from_snapshot(snapshot.clone()).unwrap().snapshot(),
        snapshot
    );

    assert!(project.undo().unwrap());
    assert!(project.tracks().is_empty());
    assert!(project.midi_items().is_empty());
    assert!(project.redo().unwrap());
    assert_eq!(project.midi_items()[0].notes().len(), 1);
}

#[test]
fn a_failed_import_batch_rolls_back_tracks_and_identifier_allocation() {
    let mut project = Project::new();
    let error = project
        .apply(DawAction::BatchTransaction {
            tx_id: 88,
            actions: vec![DawAction::ImportMidiTrack {
                start_tick: 0,
                track: aaadaw_core::MidiImportedTrack {
                    name: "Invalid".to_owned(),
                    length_ticks: 1,
                    notes: vec![aaadaw_core::MidiNoteData {
                        pitch: 200,
                        channel: 0,
                        tick: 0,
                        duration: 1,
                        velocity: 100,
                        off_velocity: 0,
                    }],
                    controllers: Vec::new(),
                    pitch_bends: Vec::new(),
                    program_changes: Vec::new(),
                    poly_aftertouches: Vec::new(),
                    channel_pressures: Vec::new(),
                },
            }],
        })
        .unwrap_err();
    assert_eq!(error, ActionError::InvalidMidiNote);
    assert!(project.tracks().is_empty());
    assert!(project.midi_items().is_empty());
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "First committed track".to_owned(),
        })
        .unwrap();
    assert_eq!(project.tracks()[0].id().value(), 0);
}
