use aaadaw_core::{
    DawAction, FrameRate, MidiChannelPressureData, MidiControllerData, MidiExportItemScope,
    MidiExportOptions, MidiExportSelection, MidiExportTimeScope, MidiFileType, MidiItemPosition,
    MidiMarkerType, MidiNoteData, MidiPitchBendData, MidiPolyAftertouchData, MidiProgramChangeData,
    Project, TempoCurve, TimeSignature, encode_standard_midi,
    encode_standard_midi_with_conductor_track_name, encode_standard_midi_with_selection,
    midi_export_has_items_in_scope,
};

#[test]
fn exports_note_controller_and_pitch_bend_channels_as_midi_status_nibbles() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Channels".to_owned(),
        })
        .unwrap();
    let track_id = project.tracks()[0].id();
    project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 0,
            length_ticks: 960,
        })
        .unwrap();
    let item_id = project.midi_items()[0].id();
    project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![MidiNoteData {
                pitch: 60,
                channel: 3,
                tick: 0,
                duration: 240,
                velocity: 100,
                off_velocity: 37,
            }],
        })
        .unwrap();
    project
        .apply(DawAction::SetMidiControllers {
            item_id,
            controllers: vec![MidiControllerData {
                controller: 7,
                channel: 4,
                tick: 0,
                value: 96,
            }],
        })
        .unwrap();
    project
        .apply(DawAction::SetMidiPitchBends {
            item_id,
            pitch_bends: vec![MidiPitchBendData {
                channel: 5,
                tick: 0,
                value: 8_192,
            }],
        })
        .unwrap();

    let bytes = encode_standard_midi(
        &project,
        MidiExportOptions {
            file_type: MidiFileType::Type0,
            ..MidiExportOptions::for_project(&project)
        },
    )
    .unwrap();
    for expected in [
        [0x93, 60, 100],
        [0x83, 60, 37],
        [0xb4, 7, 96],
        [0xe5, 0, 64],
    ] {
        assert!(
            bytes
                .windows(expected.len())
                .any(|window| window == expected),
            "export is missing channel status bytes {expected:?}"
        );
    }
}

#[test]
fn exports_program_change_status_and_zero_based_program_value() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Program".to_owned(),
        })
        .unwrap();
    let track_id = project.tracks()[0].id();
    project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 0,
            length_ticks: 960,
        })
        .unwrap();
    let item_id = project.midi_items()[0].id();
    project
        .apply(DawAction::SetMidiProgramChanges {
            item_id,
            program_changes: vec![MidiProgramChangeData {
                channel: 5,
                tick: 0,
                program: 41,
            }],
        })
        .unwrap();

    let bytes = encode_standard_midi(
        &project,
        MidiExportOptions {
            file_type: MidiFileType::Type0,
            ..MidiExportOptions::for_project(&project)
        },
    )
    .unwrap();
    assert!(
        bytes.windows(2).any(|window| window == [0xc5, 41]),
        "SMF should encode Program Change on channel 6 with its zero-based program byte"
    );
}

#[test]
fn exports_polyphonic_aftertouch_and_channel_pressure_status_bytes() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Aftertouch".to_owned(),
        })
        .unwrap();
    let track_id = project.tracks()[0].id();
    project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 0,
            length_ticks: 960,
        })
        .unwrap();
    let item_id = project.midi_items()[0].id();
    project
        .apply(DawAction::SetMidiPolyAftertouches {
            item_id,
            poly_aftertouches: vec![MidiPolyAftertouchData {
                channel: 2,
                tick: 0,
                note: 60,
                value: 37,
            }],
        })
        .unwrap();
    project
        .apply(DawAction::SetMidiChannelPressures {
            item_id,
            channel_pressures: vec![MidiChannelPressureData {
                channel: 2,
                tick: 0,
                value: 74,
            }],
        })
        .unwrap();

    let bytes = encode_standard_midi(
        &project,
        MidiExportOptions {
            file_type: MidiFileType::Type0,
            ..MidiExportOptions::for_project(&project)
        },
    )
    .unwrap();
    assert!(
        bytes.windows(3).any(|window| window == [0xA2, 60, 37]),
        "Poly Aftertouch should encode its note and pressure on channel 3"
    );
    assert!(
        bytes.windows(2).any(|window| window == [0xD2, 74]),
        "Channel Pressure should encode its one-byte pressure value on channel 3"
    );
}

#[test]
fn exports_tracks_notes_controllers_pitch_bends_tempo_meter_and_markers() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Piano".to_owned(),
        })
        .expect("track should be created");
    let track_id = project.tracks()[0].id();
    project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 480,
            length_ticks: 1920,
        })
        .expect("MIDI item should be inserted");
    let item_id = project.midi_items()[0].id();
    project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![MidiNoteData {
                channel: 0,
                pitch: 60,
                tick: 0,
                duration: 960,
                velocity: 100,
                off_velocity: 0,
            }],
        })
        .expect("MIDI note should be inserted");
    project
        .apply(DawAction::SetMidiControllers {
            item_id,
            controllers: vec![MidiControllerData {
                channel: 0,
                controller: 1,
                tick: 240,
                value: 64,
            }],
        })
        .expect("controller event should be inserted");
    project
        .apply(DawAction::SetMidiPitchBends {
            item_id,
            pitch_bends: vec![MidiPitchBendData {
                channel: 0,
                tick: 360,
                value: 12_288,
            }],
        })
        .expect("pitch bend should be inserted");
    project
        .apply(DawAction::InsertProjectMarker {
            position_ticks: 1440,
            name: "Chorus".to_owned(),
        })
        .expect("marker should be inserted");

    let options = MidiExportOptions {
        embed_tempo_and_time_signature: true,
        export_markers: true,
        ..MidiExportOptions::for_project(&project)
    };
    let bytes =
        encode_standard_midi(&project, options).expect("project should encode as a MIDI file");

    assert_eq!(&bytes[0..4], b"MThd");
    assert_eq!(&bytes[8..10], &[0, 1], "export should use SMF type 1");
    assert_eq!(
        &bytes[10..12],
        &[0, 2],
        "conductor and project tracks are separate"
    );
    assert_eq!(
        &bytes[12..14],
        &[0x03, 0xc0],
        "project PPQ should be preserved"
    );
    assert!(
        bytes
            .windows(6)
            .any(|window| window == [0xff, 0x51, 0x03, 0x07, 0xa1, 0x20])
    );
    assert!(
        bytes
            .windows(7)
            .any(|window| window == [0xff, 0x58, 0x04, 4, 2, 24, 8])
    );
    assert!(bytes.windows(3).any(|window| window == [0x90, 60, 100]));
    assert!(bytes.windows(3).any(|window| window == [0x80, 60, 0]));
    assert!(bytes.windows(3).any(|window| window == [0xb0, 1, 64]));
    assert!(bytes.windows(3).any(|window| window == [0xe0, 0, 96]));
    assert!(
        bytes
            .windows(9)
            .any(|window| window == [0xff, 0x06, 6, b'C', b'h', b'o', b'r', b'u', b's'])
    );
    assert!(
        bytes
            .windows(8)
            .any(|window| { window == [0xff, 0x03, 5, b'P', b'i', b'a', b'n', b'o'] })
    );
}

#[test]
fn midi_export_keeps_fractional_item_position_until_output_ppq_rounding() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Fractional MIDI".to_owned(),
        })
        .unwrap();
    project
        .apply(DawAction::InsertMidiItem {
            track_id: project.tracks()[0].id(),
            start_tick: 0,
            length_ticks: 1_920,
        })
        .unwrap();
    let item_id = project.midi_items()[0].id();
    project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![MidiNoteData {
                channel: 0,
                pitch: 60,
                tick: 0,
                duration: 120,
                velocity: 100,
                off_velocity: 0,
            }],
        })
        .unwrap();
    project
        .apply(DawAction::SetMidiItemPosition {
            item_id,
            position: MidiItemPosition::new(964, 160).unwrap(),
        })
        .unwrap();
    let options = MidiExportOptions {
        file_type: MidiFileType::Type0,
        ppq: 4_800,
        embed_tempo_and_time_signature: false,
        export_markers: false,
        ..MidiExportOptions::for_project(&project)
    };
    let bytes = encode_standard_midi(&project, options).unwrap();
    let event_tick = find_note_on_tick(&bytes, 60, 100);
    assert_eq!(event_tick, 4_824, "964.8 / 960 * 4800 = 4824");
}

fn find_note_on_tick(bytes: &[u8], pitch: u8, velocity: u8) -> u64 {
    assert_eq!(&bytes[14..18], b"MTrk");
    let track_length = u32::from_be_bytes(bytes[18..22].try_into().unwrap()) as usize;
    let track_end = 22 + track_length;
    let mut cursor = 22;
    let mut absolute_tick = 0_u64;
    while cursor < track_end {
        let mut delta = 0_u64;
        loop {
            let byte = bytes[cursor];
            cursor += 1;
            delta = (delta << 7) | u64::from(byte & 0x7f);
            if byte & 0x80 == 0 {
                break;
            }
        }
        absolute_tick += delta;
        let status = bytes[cursor];
        cursor += 1;
        if status == 0xff {
            cursor += 1;
            let mut length = 0_usize;
            loop {
                let byte = bytes[cursor];
                cursor += 1;
                length = (length << 7) | usize::from(byte & 0x7f);
                if byte & 0x80 == 0 {
                    break;
                }
            }
            cursor += length;
        } else if status == 0xf0 || status == 0xf7 {
            let mut length = 0_usize;
            loop {
                let byte = bytes[cursor];
                cursor += 1;
                length = (length << 7) | usize::from(byte & 0x7f);
                if byte & 0x80 == 0 {
                    break;
                }
            }
            cursor += length;
        } else {
            let data_length = match status & 0xf0 {
                0xc0 | 0xd0 => 1,
                _ => 2,
            };
            let data = &bytes[cursor..cursor + data_length];
            cursor += data_length;
            if status & 0xf0 == 0x90 && data == [pitch, velocity] {
                return absolute_tick;
            }
        }
    }
    panic!("MIDI note-on event was not found");
}

#[test]
fn midi_export_repeats_loop_source_events_to_fill_the_visible_item() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Looped MIDI".to_owned(),
        })
        .unwrap();
    project
        .apply(DawAction::InsertMidiItem {
            track_id: project.tracks()[0].id(),
            start_tick: 0,
            length_ticks: 960,
        })
        .unwrap();
    let item_id = project.midi_items()[0].id();
    project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![MidiNoteData {
                channel: 0,
                pitch: 60,
                tick: 120,
                duration: 120,
                velocity: 100,
                off_velocity: 0,
            }],
        })
        .unwrap();
    project
        .apply(DawAction::SetMidiControllers {
            item_id,
            controllers: vec![MidiControllerData {
                channel: 0,
                controller: 1,
                tick: 240,
                value: 64,
            }],
        })
        .unwrap();
    project
        .apply(DawAction::SetMidiPitchBends {
            item_id,
            pitch_bends: vec![MidiPitchBendData {
                channel: 0,
                tick: 360,
                value: 12_288,
            }],
        })
        .unwrap();
    project
        .apply(DawAction::EditMidiItemPlacement {
            item_id,
            start_tick: 0,
            length_ticks: 3_000,
            source_offset_ticks: 720,
        })
        .unwrap();

    let bytes = encode_standard_midi(&project, MidiExportOptions::for_project(&project))
        .expect("looped MIDI should export");
    assert_eq!(
        bytes
            .windows(3)
            .filter(|event| *event == [0x90, 60, 100])
            .count(),
        3
    );
    assert_eq!(
        bytes
            .windows(3)
            .filter(|event| *event == [0x80, 60, 0])
            .count(),
        3
    );
    assert_eq!(
        bytes
            .windows(3)
            .filter(|event| *event == [0xb0, 1, 64])
            .count(),
        3
    );
    assert_eq!(
        bytes
            .windows(3)
            .filter(|event| *event == [0xe0, 0, 96])
            .count(),
        3
    );
}

#[test]
fn exports_linear_tempo_ramps_on_the_native_eighth_note_grid() {
    let mut project = Project::new();
    project
        .apply(DawAction::SetTempo {
            start_tick: 3840,
            bpm: 60.0,
        })
        .expect("tempo point should be inserted");
    project
        .apply(DawAction::SetTempoCurve {
            start_tick: 0,
            curve: TempoCurve::Linear,
        })
        .expect("tempo ramp should be selected");

    let options = MidiExportOptions {
        embed_tempo_and_time_signature: true,
        ..MidiExportOptions::for_project(&project)
    };
    let bytes = encode_standard_midi(&project, options).expect("tempo ramp should encode");
    let tempo_events = bytes
        .windows(3)
        .filter(|window| *window == [0xff, 0x51, 0x03])
        .count();

    assert_eq!(
        tempo_events, 33,
        "a four-quarter-note ramp uses 1/8-note steps"
    );
    assert!(
        bytes
            .windows(b"REAPER gradual tempo change start 120.000000".len())
            .any(|window| window == b"REAPER gradual tempo change start 120.000000")
    );
    assert!(
        bytes
            .windows(b"REAPER gradual tempo change interpolation end".len())
            .any(|window| window == b"REAPER gradual tempo change interpolation end")
    );
    assert!(
        bytes
            .windows(6)
            .any(|window| window == [0xff, 0x51, 0x03, 0x0f, 0x42, 0x40])
    );
}

#[test]
fn default_export_always_includes_the_native_start_tempo_and_time_signature() {
    let project = Project::new();
    let options = MidiExportOptions::for_project(&project);
    let bytes = encode_standard_midi(&project, options).expect("empty project should export");

    assert_eq!(&bytes[8..10], &[0, 1], "REAPER defaults to SMF type 1");
    assert_eq!(
        &bytes[10..12],
        &[0, 2],
        "empty project has the conductor and REAPER's empty MIDI track"
    );
    assert_eq!(&bytes[12..14], &[0x03, 0xc0], "REAPER default PPQN is 960");
    assert!(
        bytes
            .windows(6)
            .any(|window| { window == [0xff, 0x51, 0x03, 0x07, 0xa1, 0x20] })
    );
    assert!(
        bytes
            .windows(7)
            .any(|window| window == [0xff, 0x58, 0x04, 4, 2, 24, 8])
    );
}

#[test]
fn reaper_782_empty_project_export_matches_native_two_track_header() {
    let project = Project::new();
    let actual = encode_standard_midi_with_conductor_track_name(
        &project,
        MidiExportOptions::for_project(&project),
        "midi-export-empty-project-all",
    )
    .unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-empty-project-all.mid"
    );

    assert_eq!(actual, native);
}

#[test]
fn reaper_782_export_skips_midi_items_on_muted_tracks() {
    let project = native_midi_muted_scope_fixture(true, false);
    let options = MidiExportOptions::for_project(&project);
    assert!(
        !midi_export_has_items_in_scope(&project, options, &MidiExportSelection::default())
            .unwrap()
    );
    let actual =
        encode_standard_midi_with_conductor_track_name(&project, options, "midi_export").unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-track-muted-all.mid"
    );

    assert_eq!(actual, native);
}

#[test]
fn reaper_782_export_keeps_midi_items_on_unmuted_tracks_inside_muted_folders() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Muted Folder".to_owned(),
        })
        .unwrap();
    project
        .apply(DawAction::CreateTrack {
            index: 1,
            name: "MIDI Child".to_owned(),
        })
        .unwrap();
    let folder_id = project.tracks()[0].id();
    let child_id = project.tracks()[1].id();
    project
        .apply(DawAction::SetTrackFolder {
            track_id: folder_id,
            enabled: true,
        })
        .unwrap();
    project
        .apply(DawAction::SetTrackParent {
            track_id: child_id,
            parent: Some(folder_id),
        })
        .unwrap();
    project
        .apply(DawAction::SetTrackMute {
            track_id: folder_id,
            muted: true,
        })
        .unwrap();
    project
        .apply(DawAction::InsertMidiItem {
            track_id: child_id,
            start_tick: 0,
            length_ticks: 960,
        })
        .unwrap();
    let item_id = project.midi_items()[0].id();
    project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![MidiNoteData {
                channel: 0,
                pitch: 60,
                tick: 0,
                duration: 240,
                velocity: 100,
                off_velocity: 0,
            }],
        })
        .unwrap();

    let options = MidiExportOptions::for_project(&project);
    assert!(
        midi_export_has_items_in_scope(&project, options, &MidiExportSelection::default()).unwrap()
    );
    let actual =
        encode_standard_midi_with_conductor_track_name(&project, options, "midi_export").unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-folder-muted-all.mid"
    );

    assert_eq!(actual, native);
}

#[test]
fn reaper_782_selected_track_time_scope_without_overlap_exports_empty_header() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Outside time selection".to_owned(),
        })
        .unwrap();
    let track_id = project.tracks()[0].id();
    project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 0,
            length_ticks: 960,
        })
        .unwrap();
    let item_id = project.midi_items()[0].id();
    project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![MidiNoteData {
                channel: 0,
                pitch: 60,
                tick: 0,
                duration: 240,
                velocity: 100,
                off_velocity: 0,
            }],
        })
        .unwrap();

    let options = MidiExportOptions {
        item_scope: MidiExportItemScope::SelectedTracksOnly,
        time_scope: MidiExportTimeScope::TimeSelectionOnly,
        ..MidiExportOptions::for_project(&project)
    };
    let selection = MidiExportSelection {
        selected_track_ids: vec![track_id],
        selected_item_ids: Vec::new(),
        time_selection_ticks: Some((1_920, 3_840)),
    };
    assert!(!midi_export_has_items_in_scope(&project, options, &selection).unwrap());
    let actual =
        encode_standard_midi_with_selection(&project, options, "midi_export", &selection).unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-scope-selected-tracks-time-empty.mid"
    );

    assert_eq!(actual, native);
}

#[test]
fn reaper_782_all_time_scope_without_overlap_exports_empty_header() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Unselected track".to_owned(),
        })
        .unwrap();
    let track_id = project.tracks()[0].id();
    project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 0,
            length_ticks: 1_920,
        })
        .unwrap();
    let item_id = project.midi_items()[0].id();
    project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![MidiNoteData {
                channel: 0,
                pitch: 60,
                tick: 0,
                duration: 240,
                velocity: 100,
                off_velocity: 0,
            }],
        })
        .unwrap();

    let options = MidiExportOptions {
        time_scope: MidiExportTimeScope::TimeSelectionOnly,
        ..MidiExportOptions::for_project(&project)
    };
    let selection = MidiExportSelection {
        selected_track_ids: Vec::new(),
        selected_item_ids: Vec::new(),
        time_selection_ticks: Some((3_840, 5_760)),
    };
    assert!(!midi_export_has_items_in_scope(&project, options, &selection).unwrap());
    let actual =
        encode_standard_midi_with_selection(&project, options, "midi_export", &selection).unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-scope-all-time-empty.mid"
    );

    assert_eq!(actual, native);
}

#[test]
fn reaper_782_export_skips_muted_midi_items() {
    let project = native_midi_muted_scope_fixture(false, true);
    let options = MidiExportOptions::for_project(&project);
    assert!(
        !midi_export_has_items_in_scope(&project, options, &MidiExportSelection::default())
            .unwrap()
    );
    let actual =
        encode_standard_midi_with_conductor_track_name(&project, options, "item-muted-midi-export")
            .unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-item-muted-all.mid"
    );

    assert_eq!(actual, native);
}

#[test]
fn reaper_782_type_one_export_matches_native_track_and_event_bytes() {
    let project = native_midi_fixture_project();
    let actual = encode_standard_midi_with_conductor_track_name(
        &project,
        MidiExportOptions::for_project(&project),
        "aaadaw-midi-export-native-20261015",
    )
    .unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-content.mid"
    );

    assert_eq!(
        actual, native,
        "type-1 tracks and all event ticks/bytes match REAPER 7.82"
    );
}

#[test]
fn reaper_782_type_one_non_ascii_track_name_uses_ascii_replacement() {
    let track_name = format!("轨{}", "A".repeat(125));
    assert_eq!(track_name.len(), 128, "fixture should cross the byte limit");
    let project = native_midi_fixture_project_with_track_name(1920, &track_name);
    let actual = encode_standard_midi_with_conductor_track_name(
        &project,
        MidiExportOptions::for_project(&project),
        "aaadaw-midi-long-utf8-track",
    )
    .unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-track-name-unicode.mid"
    );

    assert_eq!(
        actual, native,
        "REAPER replaces each non-ASCII track-name character with one underscore"
    );
}

#[test]
fn reaper_782_type_one_128_byte_ascii_track_name_uses_multibyte_vlq_length() {
    let track_name = "A".repeat(128);
    let project = native_midi_fixture_project_with_track_name(1920, &track_name);
    let actual = encode_standard_midi_with_conductor_track_name(
        &project,
        MidiExportOptions::for_project(&project),
        "aaadaw-midi-long-ascii-track",
    )
    .unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-track-name-long-ascii.mid"
    );

    assert_eq!(
        actual, native,
        "REAPER preserves 128 ASCII bytes and encodes the event length as VLQ 81 00"
    );
}

#[test]
fn reaper_782_type_one_127_byte_ascii_track_name_uses_single_byte_vlq_length() {
    let track_name = "A".repeat(127);
    let project = native_midi_fixture_project_with_track_name(1920, &track_name);
    let actual = encode_standard_midi_with_conductor_track_name(
        &project,
        MidiExportOptions::for_project(&project),
        "aaadaw-midi-127-ascii-track",
    )
    .unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-track-name-127-ascii.mid"
    );

    assert_eq!(
        actual, native,
        "REAPER encodes a 127-byte text-event length as the one-byte VLQ 7F"
    );
}

#[test]
fn reaper_782_midi_export_all_items_matches_native_consolidation() {
    let (project, selection) = native_midi_consolidation_fixture_project();
    let actual = encode_standard_midi_with_selection(
        &project,
        MidiExportOptions::for_project(&project),
        "aaadaw-consolidate-all",
        &selection,
    )
    .unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-consolidation-all.mid"
    );

    assert_eq!(actual, native, "All exports all project MIDI items");
}

#[test]
fn reaper_782_midi_export_selected_tracks_matches_native_consolidation() {
    let (project, selection) = native_midi_consolidation_fixture_project();
    let options = MidiExportOptions {
        item_scope: MidiExportItemScope::SelectedTracksOnly,
        ..MidiExportOptions::for_project(&project)
    };
    let actual = encode_standard_midi_with_selection(
        &project,
        options,
        "aaadaw-consolidate-selected-tracks",
        &selection,
    )
    .unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-consolidation-selected-tracks.mid"
    );

    assert_eq!(actual, native, "selected-track mode exports Track B only");
}

#[test]
fn reaper_782_midi_export_selected_items_matches_native_consolidation() {
    let (project, selection) = native_midi_consolidation_fixture_project();
    let options = MidiExportOptions {
        item_scope: MidiExportItemScope::SelectedItemsOnly,
        ..MidiExportOptions::for_project(&project)
    };
    let actual = encode_standard_midi_with_selection(
        &project,
        options,
        "aaadaw-consolidate-selected-items",
        &selection,
    )
    .unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-consolidation-selected-items.mid"
    );

    assert_eq!(
        actual, native,
        "selected-item mode exports the selected C4 only"
    );
}

#[test]
fn reaper_782_midi_export_time_selection_matches_native_consolidation() {
    let (project, selection) = native_midi_consolidation_fixture_project();
    let options = MidiExportOptions {
        time_scope: MidiExportTimeScope::TimeSelectionOnly,
        ..MidiExportOptions::for_project(&project)
    };
    let actual = encode_standard_midi_with_selection(
        &project,
        options,
        "aaadaw-consolidate-time-selection",
        &selection,
    )
    .unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-consolidation-time-selection.mid"
    );

    assert_eq!(
        actual, native,
        "time-selection mode exports overlapping items and rebases their project ticks"
    );
}

#[test]
fn reaper_782_midi_export_selected_track_and_time_selection_match_native() {
    let (project, selection, _) = native_midi_scope_intersection_fixture_project();
    let options = MidiExportOptions {
        item_scope: MidiExportItemScope::SelectedTracksOnly,
        time_scope: MidiExportTimeScope::TimeSelectionOnly,
        ..MidiExportOptions::for_project(&project)
    };
    let actual = encode_standard_midi_with_selection(
        &project,
        options,
        "midi-export-scope-selected-tracks-time",
        &selection,
    )
    .unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-scope-selected-tracks-time.mid"
    );

    assert_eq!(actual, native);
}

#[test]
fn reaper_782_type_zero_midi_export_selected_track_and_time_selection_match_native() {
    let (project, selection, _) = native_midi_scope_intersection_fixture_project();
    let options = MidiExportOptions {
        file_type: MidiFileType::Type0,
        item_scope: MidiExportItemScope::SelectedTracksOnly,
        time_scope: MidiExportTimeScope::TimeSelectionOnly,
        ..MidiExportOptions::for_project(&project)
    };
    let actual = encode_standard_midi_with_selection(
        &project,
        options,
        "menu-export-midi-native-scope-intersections",
        &selection,
    )
    .unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-scope-selected-tracks-time-type0.mid"
    );

    assert_eq!(
        actual, native,
        "Type 0 selected-track + time-selection output matches REAPER's merged event track"
    );
}

#[test]
fn reaper_782_type_zero_midi_export_all_items_and_time_selection_match_native() {
    let (project, selection, _) = native_midi_scope_intersection_fixture_project();
    let options = MidiExportOptions {
        file_type: MidiFileType::Type0,
        time_scope: MidiExportTimeScope::TimeSelectionOnly,
        ..MidiExportOptions::for_project(&project)
    };
    let actual =
        encode_standard_midi_with_selection(&project, options, "type0-all-time", &selection)
            .unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-scope-all-time-type0.mid"
    );

    assert_eq!(
        actual, native,
        "Type 0 all-items + time-selection output matches REAPER's merged event track"
    );
}

#[test]
fn reaper_782_midi_export_selected_item_outside_time_selection_matches_native_empty_track() {
    let (project, selection, _) = native_midi_scope_intersection_fixture_project();
    let options = MidiExportOptions {
        item_scope: MidiExportItemScope::SelectedItemsOnly,
        time_scope: MidiExportTimeScope::TimeSelectionOnly,
        ..MidiExportOptions::for_project(&project)
    };
    assert!(!midi_export_has_items_in_scope(&project, options, &selection).unwrap());
    let actual = encode_standard_midi_with_selection(
        &project,
        options,
        "midi-export-scope-selected-items-outside-time",
        &selection,
    )
    .unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-scope-selected-items-outside-time.mid"
    );

    assert_eq!(actual, native);
}

#[test]
fn reaper_782_midi_export_selected_item_inside_time_selection_matches_native() {
    let (project, mut selection, in_range_item_id) =
        native_midi_scope_intersection_fixture_project();
    selection.selected_item_ids = vec![in_range_item_id];
    let options = MidiExportOptions {
        item_scope: MidiExportItemScope::SelectedItemsOnly,
        time_scope: MidiExportTimeScope::TimeSelectionOnly,
        ..MidiExportOptions::for_project(&project)
    };
    assert!(midi_export_has_items_in_scope(&project, options, &selection).unwrap());
    let actual = encode_standard_midi_with_selection(
        &project,
        options,
        "midi-export-scope-selected-items-inside-time",
        &selection,
    )
    .unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-scope-selected-items-inside-time.mid"
    );

    assert_eq!(actual, native);
}

#[test]
fn reaper_782_type_zero_midi_export_selected_item_and_time_selection_match_native() {
    let (project, mut selection, in_range_item_id) =
        native_midi_scope_intersection_fixture_project();
    selection.selected_item_ids = vec![in_range_item_id];
    let options = MidiExportOptions {
        file_type: MidiFileType::Type0,
        item_scope: MidiExportItemScope::SelectedItemsOnly,
        time_scope: MidiExportTimeScope::TimeSelectionOnly,
        ..MidiExportOptions::for_project(&project)
    };
    let actual = encode_standard_midi_with_selection(
        &project,
        options,
        "type0-selected-items-time",
        &selection,
    )
    .unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-scope-selected-items-time-type0.mid"
    );

    assert_eq!(
        actual, native,
        "Type 0 selected-item + time-selection output matches REAPER's merged event track"
    );
}

#[test]
fn reaper_782_type_zero_selected_item_time_selection_keeps_start_tempo_and_meter() {
    let (project, mut selection, in_range_item_id) =
        native_midi_scope_intersection_fixture_project();
    selection.selected_item_ids = vec![in_range_item_id];
    let options = MidiExportOptions {
        file_type: MidiFileType::Type0,
        item_scope: MidiExportItemScope::SelectedItemsOnly,
        time_scope: MidiExportTimeScope::TimeSelectionOnly,
        embed_tempo_and_time_signature: true,
        ..MidiExportOptions::for_project(&project)
    };
    let actual = encode_standard_midi_with_selection(
        &project,
        options,
        "type0-selected-items-time-tempo",
        &selection,
    )
    .unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-scope-selected-items-time-type0-tempo.mid"
    );

    assert_eq!(
        actual, native,
        "Type 0 time-selection export retains REAPER's start tempo and meter events"
    );
}

#[test]
fn reaper_782_type_zero_selected_item_time_selection_offsets_smpte_to_range_start() {
    let (project, mut selection, in_range_item_id) =
        native_midi_scope_intersection_fixture_project();
    selection.selected_item_ids = vec![in_range_item_id];
    let options = MidiExportOptions {
        file_type: MidiFileType::Type0,
        item_scope: MidiExportItemScope::SelectedItemsOnly,
        time_scope: MidiExportTimeScope::TimeSelectionOnly,
        embed_smpte_offset: true,
        ..MidiExportOptions::for_project(&project)
    };
    let actual = encode_standard_midi_with_selection(
        &project,
        options,
        "type0-selected-items-time-smpte",
        &selection,
    )
    .unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-scope-selected-items-time-type0-smpte.mid"
    );

    assert_eq!(
        actual, native,
        "Type 0 time-selection export encodes the selection start in its SMPTE offset"
    );
}

#[test]
fn reaper_782_type_one_selected_item_time_selection_offsets_smpte_to_range_start() {
    let (project, mut selection, in_range_item_id) =
        native_midi_scope_intersection_fixture_project();
    selection.selected_item_ids = vec![in_range_item_id];
    let options = MidiExportOptions {
        item_scope: MidiExportItemScope::SelectedItemsOnly,
        time_scope: MidiExportTimeScope::TimeSelectionOnly,
        embed_smpte_offset: true,
        ..MidiExportOptions::for_project(&project)
    };
    let actual = encode_standard_midi_with_selection(
        &project,
        options,
        "type1-selected-items-time-smpte",
        &selection,
    )
    .unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-scope-selected-items-time-type1-smpte.mid"
    );

    assert_eq!(
        actual, native,
        "Type 1 time-selection export encodes the selection start in its SMPTE offset"
    );
}

#[test]
fn reaper_782_midi_export_time_selection_clips_notes_at_selection_start() {
    let project = native_midi_time_selection_boundary_fixture_project();
    let options = MidiExportOptions {
        time_scope: MidiExportTimeScope::TimeSelectionOnly,
        ..MidiExportOptions::for_project(&project)
    };
    let selection = MidiExportSelection {
        time_selection_ticks: Some((1920, 3840)),
        ..MidiExportSelection::default()
    };
    let actual = encode_standard_midi_with_selection(
        &project,
        options,
        "aaadaw-time-selection-boundaries",
        &selection,
    )
    .unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-time-selection-boundaries.mid"
    );

    assert_eq!(
        actual, native,
        "overlapping notes are clipped at selection start and events at/after its end are omitted"
    );
}

#[test]
fn midi_export_time_selection_requires_a_nonempty_project_selection() {
    let project = Project::new();
    let options = MidiExportOptions {
        time_scope: MidiExportTimeScope::TimeSelectionOnly,
        ..MidiExportOptions::for_project(&project)
    };

    assert_eq!(
        encode_standard_midi_with_selection(
            &project,
            options,
            "midi_export",
            &MidiExportSelection::default(),
        ),
        Err("time-selection export requires an active time selection".to_owned())
    );
}

#[test]
fn reaper_782_type_one_marker_export_matches_native_bytes() {
    let project = native_midi_marker_fixture_project();
    let mut options = MidiExportOptions::for_project(&project);
    options.export_markers = true;
    let actual = encode_standard_midi_with_conductor_track_name(
        &project,
        options,
        "midi-markers-native-20261016",
    )
    .unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-markers.mid"
    );

    assert_eq!(
        actual, native,
        "Type 1 marker metadata and MIDI tracks match REAPER 7.82"
    );
}

#[test]
fn reaper_782_type_one_cue_export_matches_native_bytes() {
    let project = native_midi_marker_fixture_project();
    let options = MidiExportOptions {
        export_markers: true,
        marker_type: MidiMarkerType::Cues,
        ..MidiExportOptions::for_project(&project)
    };
    let actual = encode_standard_midi_with_conductor_track_name(
        &project,
        options,
        "midi-cues-native-20261016",
    )
    .unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-cues.mid"
    );

    assert_eq!(
        actual, native,
        "Type 1 cue metadata and MIDI tracks match REAPER 7.82"
    );
}

#[test]
fn reaper_782_type_one_hash_marker_filter_matches_native_bytes() {
    let project = native_midi_marker_filter_fixture_project();
    let options = MidiExportOptions {
        export_markers: true,
        only_markers_starting_with_hash: true,
        ..MidiExportOptions::for_project(&project)
    };
    let actual = encode_standard_midi_with_conductor_track_name(
        &project,
        options,
        "midi-markers-filtered-native-20261016",
    )
    .unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-marker-filtered.mid"
    );

    assert_eq!(
        actual, native,
        "Type 1 hash filtering and hidden-marker exclusion match REAPER 7.82"
    );
}

#[test]
fn reaper_782_type_one_hidden_hash_marker_export_matches_native_bytes() {
    let project = native_midi_marker_filter_fixture_project();
    let options = MidiExportOptions {
        export_markers: true,
        export_hidden_markers: true,
        only_markers_starting_with_hash: true,
        ..MidiExportOptions::for_project(&project)
    };
    let actual = encode_standard_midi_with_conductor_track_name(
        &project,
        options,
        "midi-markers-hidden-native-20261016",
    )
    .unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-marker-hidden.mid"
    );

    assert_eq!(
        actual, native,
        "Type 1 hidden hash-marker inclusion matches REAPER 7.82"
    );
}

#[test]
fn reaper_782_type_one_hidden_marker_ruler_is_excluded_by_default() {
    let project = native_midi_marker_ruler_hidden_fixture_project();
    let options = MidiExportOptions {
        export_markers: true,
        ..MidiExportOptions::for_project(&project)
    };
    let actual = encode_standard_midi_with_conductor_track_name(
        &project,
        options,
        "midi-marker-ruler-hidden-excluded-native-20261016",
    )
    .unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-marker-ruler-hidden-excluded.mid"
    );

    assert_eq!(
        actual, native,
        "Type 1 export excludes markers hidden by the ruler by default"
    );
}

#[test]
fn reaper_782_type_one_hidden_marker_ruler_opt_in_matches_native_bytes() {
    let project = native_midi_marker_ruler_hidden_fixture_project();
    let options = MidiExportOptions {
        export_markers: true,
        export_hidden_markers: true,
        ..MidiExportOptions::for_project(&project)
    };
    let actual = encode_standard_midi_with_conductor_track_name(
        &project,
        options,
        "midi-marker-ruler-hidden-included-native-20261016",
    )
    .unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-marker-ruler-hidden-included.mid"
    );

    assert_eq!(
        actual, native,
        "Type 1 hidden-marker opt-in includes markers hidden by the ruler"
    );
}

#[test]
fn reaper_782_type_one_smpte_offset_matches_native_bytes() {
    let project = native_midi_fixture_project();
    let mut options = MidiExportOptions::for_project(&project);
    options.embed_smpte_offset = true;
    let actual = encode_standard_midi_with_conductor_track_name(
        &project,
        options,
        "midi-smpte-native-20261016",
    )
    .unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-smpte.mid"
    );

    assert_eq!(
        actual, native,
        "Type 1 SMPTE offset metadata and MIDI tracks match REAPER 7.82"
    );
}

#[test]
fn reaper_782_type_one_nonzero_smpte_offset_matches_native_bytes() {
    let mut project = native_midi_fixture_project();
    project
        .apply(DawAction::SetProjectStartTimeOffsetSeconds { seconds: 10.5 })
        .unwrap();
    let options = MidiExportOptions {
        embed_smpte_offset: true,
        ..MidiExportOptions::for_project(&project)
    };
    let actual = encode_standard_midi_with_conductor_track_name(
        &project,
        options,
        "midi-smpte-offset-native-20261016",
    )
    .unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-smpte-nonzero-offset.mid"
    );

    assert_eq!(
        actual, native,
        "Type 1 nonzero SMPTE offset metadata and MIDI tracks match REAPER 7.82"
    );
}

#[test]
fn reaper_782_type_zero_nonzero_smpte_offset_and_time_selection_match_native_bytes() {
    let mut project = native_midi_fixture_project();
    project
        .apply(DawAction::SetProjectStartTimeOffsetSeconds { seconds: 10.5 })
        .unwrap();
    let options = MidiExportOptions {
        file_type: MidiFileType::Type0,
        time_scope: MidiExportTimeScope::TimeSelectionOnly,
        embed_smpte_offset: true,
        ..MidiExportOptions::for_project(&project)
    };
    let selection = MidiExportSelection {
        time_selection_ticks: Some((960, 2880)),
        ..MidiExportSelection::default()
    };
    let actual = encode_standard_midi_with_selection(
        &project,
        options,
        "midi-type0-selection-offset",
        &selection,
    )
    .unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-smpte-selection-offset-type0.mid"
    );

    assert_eq!(
        actual, native,
        "Type 0 adds the 0.5-second range start to the configured 10.5-second SMPTE offset"
    );
}

#[test]
fn reaper_782_type_one_nonzero_smpte_offset_and_time_selection_match_native_bytes() {
    let mut project = native_midi_fixture_project();
    project
        .apply(DawAction::SetProjectStartTimeOffsetSeconds { seconds: 10.5 })
        .unwrap();
    let options = MidiExportOptions {
        time_scope: MidiExportTimeScope::TimeSelectionOnly,
        embed_smpte_offset: true,
        ..MidiExportOptions::for_project(&project)
    };
    let selection = MidiExportSelection {
        time_selection_ticks: Some((960, 2880)),
        ..MidiExportSelection::default()
    };
    let actual = encode_standard_midi_with_selection(
        &project,
        options,
        "midi-type1-selection-offset",
        &selection,
    )
    .unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-smpte-selection-offset-type1.mid"
    );

    assert_eq!(
        actual, native,
        "Type 1 adds the 0.5-second range start to the configured 10.5-second SMPTE offset"
    );
}

#[test]
fn reaper_782_type_one_24fps_nonzero_smpte_offset_matches_native_bytes() {
    let mut project = native_midi_fixture_project();
    project
        .apply(DawAction::SetProjectStartTimeOffsetSeconds { seconds: 10.5 })
        .unwrap();
    project
        .apply(DawAction::SetFrameRate {
            rate: FrameRate::Fps24,
        })
        .unwrap();
    let options = MidiExportOptions {
        embed_smpte_offset: true,
        ..MidiExportOptions::for_project(&project)
    };
    let actual =
        encode_standard_midi_with_conductor_track_name(&project, options, "project-24fps").unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-smpte-24fps-nonzero-offset.mid"
    );

    assert_eq!(
        actual, native,
        "24 fps non-drop SMPTE offset metadata and MIDI tracks match REAPER 7.82"
    );
}

#[test]
fn reaper_782_type_one_23976fps_nonzero_smpte_offset_matches_native_bytes() {
    let mut project = native_midi_fixture_project();
    project
        .apply(DawAction::SetProjectStartTimeOffsetSeconds { seconds: 10.5 })
        .unwrap();
    project
        .apply(DawAction::SetFrameRate {
            rate: FrameRate::Fps23976,
        })
        .unwrap();
    let options = MidiExportOptions {
        embed_smpte_offset: true,
        ..MidiExportOptions::for_project(&project)
    };
    let actual =
        encode_standard_midi_with_conductor_track_name(&project, options, "project-23976fps")
            .unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-smpte-23976-nonzero-offset.mid"
    );

    assert_eq!(
        actual, native,
        "23.976 fps non-drop SMPTE offset metadata and MIDI tracks match REAPER 7.82"
    );
}

#[test]
fn reaper_782_type_one_25fps_nonzero_smpte_offset_matches_native_bytes() {
    let mut project = native_midi_fixture_project();
    project
        .apply(DawAction::SetProjectStartTimeOffsetSeconds { seconds: 10.5 })
        .unwrap();
    project
        .apply(DawAction::SetFrameRate {
            rate: FrameRate::Fps25,
        })
        .unwrap();
    let options = MidiExportOptions {
        embed_smpte_offset: true,
        ..MidiExportOptions::for_project(&project)
    };
    let actual =
        encode_standard_midi_with_conductor_track_name(&project, options, "project-25fps").unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-smpte-25fps-nonzero-offset.mid"
    );

    assert_eq!(
        actual, native,
        "25 fps non-drop SMPTE offset metadata and MIDI tracks match REAPER 7.82"
    );
}

#[test]
fn reaper_782_type_one_2997drop_nonzero_smpte_offset_matches_native_bytes() {
    let mut project = native_midi_fixture_project();
    project
        .apply(DawAction::SetProjectStartTimeOffsetSeconds { seconds: 10.5 })
        .unwrap();
    project
        .apply(DawAction::SetFrameRate {
            rate: FrameRate::Fps2997Drop,
        })
        .unwrap();
    let options = MidiExportOptions {
        embed_smpte_offset: true,
        ..MidiExportOptions::for_project(&project)
    };
    let actual =
        encode_standard_midi_with_conductor_track_name(&project, options, "project-2997drop")
            .unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-smpte-2997drop-nonzero-offset.mid"
    );

    assert_eq!(
        actual, native,
        "29.97 fps drop-frame SMPTE offset metadata and MIDI tracks match REAPER 7.82"
    );
}

#[test]
fn reaper_782_type_one_2997drop_first_minute_boundary_matches_native_bytes() {
    let mut project = native_midi_fixture_project();
    project
        .apply(DawAction::SetProjectStartTimeOffsetSeconds { seconds: 60.06 })
        .unwrap();
    project
        .apply(DawAction::SetFrameRate {
            rate: FrameRate::Fps2997Drop,
        })
        .unwrap();
    let options = MidiExportOptions {
        embed_smpte_offset: true,
        ..MidiExportOptions::for_project(&project)
    };
    let actual =
        encode_standard_midi_with_conductor_track_name(&project, options, "project-2997drop")
            .unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-smpte-2997drop-minute-boundary.mid"
    );

    assert_eq!(
        actual, native,
        "29.97 fps drop-frame minute-boundary offset and MIDI tracks match REAPER 7.82"
    );
}

#[test]
fn reaper_782_type_one_2997drop_hour_boundary_matches_native_bytes() {
    let mut project = native_midi_fixture_project();
    project
        .apply(DawAction::SetProjectStartTimeOffsetSeconds { seconds: 3600.0 })
        .unwrap();
    project
        .apply(DawAction::SetFrameRate {
            rate: FrameRate::Fps2997Drop,
        })
        .unwrap();
    let options = MidiExportOptions {
        embed_smpte_offset: true,
        ..MidiExportOptions::for_project(&project)
    };
    let actual = encode_standard_midi_with_conductor_track_name(
        &project,
        options,
        "aaadaw-smpte-2997drop-hour-boundary",
    )
    .unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-smpte-2997drop-hour-boundary.mid"
    );

    assert_eq!(
        actual, native,
        "29.97 fps drop-frame hour-boundary offset and MIDI tracks match REAPER 7.82"
    );
}

#[test]
fn reaper_782_type_one_2997nd_nonzero_smpte_offset_matches_native_bytes() {
    let mut project = native_midi_fixture_project();
    project
        .apply(DawAction::SetProjectStartTimeOffsetSeconds { seconds: 10.5 })
        .unwrap();
    project
        .apply(DawAction::SetFrameRate {
            rate: FrameRate::Fps2997,
        })
        .unwrap();
    let options = MidiExportOptions {
        embed_smpte_offset: true,
        ..MidiExportOptions::for_project(&project)
    };
    let actual =
        encode_standard_midi_with_conductor_track_name(&project, options, "project-2997nd")
            .unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-smpte-2997nd-nonzero-offset.mid"
    );

    assert_eq!(
        actual, native,
        "29.97 fps non-drop SMPTE offset metadata and MIDI tracks match REAPER 7.82"
    );
}

#[test]
fn reaper_782_type_zero_export_matches_native_event_bytes() {
    let project = native_midi_fixture_project();
    let mut options = MidiExportOptions::for_project(&project);
    options.file_type = MidiFileType::Type0;
    let actual = encode_standard_midi_with_conductor_track_name(
        &project,
        options,
        "midi-type0-native-20261016",
    )
    .unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-type0.mid"
    );

    assert_eq!(
        actual, native,
        "type-0 event ticks and bytes match REAPER 7.82 with metadata options off"
    );
}

#[test]
fn reaper_782_type_zero_hash_marker_filter_matches_native_bytes() {
    let project = native_midi_marker_fixture_project();
    let options = MidiExportOptions {
        file_type: MidiFileType::Type0,
        export_markers: true,
        marker_type: MidiMarkerType::Markers,
        only_markers_starting_with_hash: true,
        ..MidiExportOptions::for_project(&project)
    };
    let actual = encode_standard_midi(&project, options).unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-type0-markers-filtered.mid"
    );

    assert_eq!(
        actual, native,
        "Type 0 filtered marker events match REAPER 7.82"
    );
}

#[test]
fn reaper_782_type_zero_hash_cue_filter_matches_native_bytes() {
    let project = native_midi_marker_fixture_project();
    let options = MidiExportOptions {
        file_type: MidiFileType::Type0,
        export_markers: true,
        marker_type: MidiMarkerType::Cues,
        only_markers_starting_with_hash: true,
        ..MidiExportOptions::for_project(&project)
    };
    let actual = encode_standard_midi(&project, options).unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-type0-cues-filtered.mid"
    );

    assert_eq!(
        actual, native,
        "Type 0 filtered cue events match REAPER 7.82"
    );
}

#[test]
fn reaper_782_type_zero_hidden_marker_is_excluded_by_default() {
    let project = native_midi_marker_filter_fixture_project();
    let options = MidiExportOptions {
        file_type: MidiFileType::Type0,
        export_markers: true,
        ..MidiExportOptions::for_project(&project)
    };
    let actual = encode_standard_midi(&project, options).unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-type0-hidden-excluded.mid"
    );

    assert_eq!(
        actual, native,
        "Type 0 export excludes individually hidden markers by default"
    );
}

#[test]
fn reaper_782_type_zero_hidden_marker_opt_in_matches_native_bytes() {
    let project = native_midi_marker_filter_fixture_project();
    let options = MidiExportOptions {
        file_type: MidiFileType::Type0,
        export_markers: true,
        export_hidden_markers: true,
        ..MidiExportOptions::for_project(&project)
    };
    let actual = encode_standard_midi(&project, options).unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-type0-hidden-included.mid"
    );

    assert_eq!(
        actual, native,
        "Type 0 hidden-marker opt-in matches REAPER 7.82 output"
    );
}

#[test]
fn reaper_782_type_zero_time_selection_hidden_marker_opt_in_matches_native_bytes() {
    let project = native_midi_marker_ruler_and_individually_hidden_fixture_project();
    let options = MidiExportOptions {
        file_type: MidiFileType::Type0,
        time_scope: MidiExportTimeScope::TimeSelectionOnly,
        export_markers: true,
        export_hidden_markers: true,
        ..MidiExportOptions::for_project(&project)
    };
    let selection = MidiExportSelection {
        time_selection_ticks: Some((960, 1920)),
        ..MidiExportSelection::default()
    };
    let actual = encode_standard_midi_with_selection(
        &project,
        options,
        "type0-marker-time-selection",
        &selection,
    )
    .unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-type0-marker-time.mid"
    );

    assert_eq!(
        actual, native,
        "Type 0 time-selection export includes an individually hidden marker at the range end"
    );
}

#[test]
fn reaper_782_type_zero_time_selection_hidden_marker_inside_range_matches_native_bytes() {
    let project = native_midi_marker_ruler_and_individually_hidden_fixture_project();
    let options = MidiExportOptions {
        file_type: MidiFileType::Type0,
        time_scope: MidiExportTimeScope::TimeSelectionOnly,
        export_markers: true,
        export_hidden_markers: true,
        ..MidiExportOptions::for_project(&project)
    };
    let selection = MidiExportSelection {
        time_selection_ticks: Some((960, 2400)),
        ..MidiExportSelection::default()
    };
    let actual = encode_standard_midi_with_selection(
        &project,
        options,
        "type0-marker-time-selection-interior",
        &selection,
    )
    .unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-type0-marker-time.mid"
    );

    assert_eq!(
        actual, native,
        "Type 0 time-selection export includes an individually hidden marker inside the range"
    );
}

#[test]
fn reaper_782_type_zero_time_selection_hidden_ruler_checkbox_off_matches_native_bytes() {
    let project = native_midi_marker_ruler_and_individually_hidden_fixture_project();
    let options = MidiExportOptions {
        file_type: MidiFileType::Type0,
        time_scope: MidiExportTimeScope::TimeSelectionOnly,
        export_markers: true,
        ..MidiExportOptions::for_project(&project)
    };
    let selection = MidiExportSelection {
        time_selection_ticks: Some((960, 1920)),
        ..MidiExportSelection::default()
    };
    let actual = encode_standard_midi_with_selection(
        &project,
        options,
        "type0-marker-time-selection-hidden-off",
        &selection,
    )
    .unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-type0-marker-time-nohidden.mid"
    );

    assert_eq!(
        actual, native,
        "Type 0 time-selection export excludes markers when the hidden-marker option is off"
    );
}

#[test]
fn reaper_782_type_one_time_selection_includes_pre_range_markers_and_hidden_endpoint() {
    let project = native_midi_marker_ruler_and_individually_hidden_fixture_project();
    let options = MidiExportOptions {
        time_scope: MidiExportTimeScope::TimeSelectionOnly,
        export_markers: true,
        export_hidden_markers: true,
        ..MidiExportOptions::for_project(&project)
    };
    let selection = MidiExportSelection {
        time_selection_ticks: Some((960, 1920)),
        ..MidiExportSelection::default()
    };
    let actual = encode_standard_midi_with_selection(
        &project,
        options,
        "marker-type1-endpoint-hidden-on",
        &selection,
    )
    .unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-type1-marker-time-endpoint.mid"
    );

    assert_eq!(
        actual, native,
        "Type 1 time-selection export rebases pre-range markers and includes the hidden endpoint"
    );
}

#[test]
fn reaper_782_type_one_time_selection_includes_pre_range_and_interior_hidden_markers() {
    let project = native_midi_marker_ruler_and_individually_hidden_fixture_project();
    let options = MidiExportOptions {
        time_scope: MidiExportTimeScope::TimeSelectionOnly,
        export_markers: true,
        export_hidden_markers: true,
        ..MidiExportOptions::for_project(&project)
    };
    let selection = MidiExportSelection {
        time_selection_ticks: Some((960, 2400)),
        ..MidiExportSelection::default()
    };
    let actual = encode_standard_midi_with_selection(
        &project,
        options,
        "marker-type1-interior-hidden-on",
        &selection,
    )
    .unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-type1-marker-time-interior.mid"
    );

    assert_eq!(
        actual, native,
        "Type 1 time-selection export preserves pre-range markers and includes hidden markers inside the range"
    );
}

#[test]
fn reaper_782_type_zero_hidden_cue_is_excluded_by_default() {
    let project = native_midi_marker_filter_fixture_project();
    let options = MidiExportOptions {
        file_type: MidiFileType::Type0,
        export_markers: true,
        marker_type: MidiMarkerType::Cues,
        ..MidiExportOptions::for_project(&project)
    };
    let actual = encode_standard_midi(&project, options).unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-type0-hidden-cues-excluded.mid"
    );

    assert_eq!(
        actual, native,
        "Type 0 export excludes individually hidden cues by default"
    );
}

#[test]
fn reaper_782_type_zero_hidden_cue_opt_in_matches_native_bytes() {
    let project = native_midi_marker_filter_fixture_project();
    let options = MidiExportOptions {
        file_type: MidiFileType::Type0,
        export_markers: true,
        marker_type: MidiMarkerType::Cues,
        export_hidden_markers: true,
        ..MidiExportOptions::for_project(&project)
    };
    let actual = encode_standard_midi(&project, options).unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-type0-hidden-cues-included.mid"
    );

    assert_eq!(
        actual, native,
        "Type 0 hidden-cue opt-in matches REAPER 7.82 output"
    );
}

#[test]
fn reaper_782_type_zero_hidden_marker_ruler_is_excluded_by_default() {
    let project = native_midi_marker_ruler_hidden_fixture_project();
    let options = MidiExportOptions {
        file_type: MidiFileType::Type0,
        export_markers: true,
        ..MidiExportOptions::for_project(&project)
    };
    let actual = encode_standard_midi(&project, options).unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-type0-marker-ruler-hidden-excluded.mid"
    );

    assert_eq!(
        actual, native,
        "Type 0 export excludes markers hidden by the ruler by default"
    );
}

#[test]
fn reaper_782_type_zero_hidden_marker_ruler_opt_in_matches_native_bytes() {
    let project = native_midi_marker_ruler_hidden_fixture_project();
    let options = MidiExportOptions {
        file_type: MidiFileType::Type0,
        export_markers: true,
        export_hidden_markers: true,
        ..MidiExportOptions::for_project(&project)
    };
    let actual = encode_standard_midi(&project, options).unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-type0-marker-ruler-hidden-included.mid"
    );

    assert_eq!(
        actual, native,
        "Type 0 hidden-marker opt-in includes markers hidden by the ruler"
    );
}

#[test]
fn reaper_782_type_zero_hidden_cue_ruler_is_excluded_by_default() {
    let project = native_midi_marker_ruler_hidden_fixture_project();
    let options = MidiExportOptions {
        file_type: MidiFileType::Type0,
        export_markers: true,
        marker_type: MidiMarkerType::Cues,
        ..MidiExportOptions::for_project(&project)
    };
    let actual = encode_standard_midi(&project, options).unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-type0-cue-ruler-hidden-excluded.mid"
    );

    assert_eq!(
        actual, native,
        "Type 0 cue export excludes cues hidden by the ruler by default"
    );
}

#[test]
fn reaper_782_type_zero_hidden_cue_ruler_opt_in_matches_native_bytes() {
    let project = native_midi_marker_ruler_hidden_fixture_project();
    let options = MidiExportOptions {
        file_type: MidiFileType::Type0,
        export_markers: true,
        marker_type: MidiMarkerType::Cues,
        export_hidden_markers: true,
        ..MidiExportOptions::for_project(&project)
    };
    let actual = encode_standard_midi(&project, options).unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-type0-cue-ruler-hidden-included.mid"
    );

    assert_eq!(
        actual, native,
        "Type 0 hidden-cue opt-in includes cues hidden by the ruler"
    );
}

#[test]
fn reaper_782_type_zero_hidden_ruler_opt_in_still_excludes_individually_hidden_markers() {
    let project = native_midi_marker_ruler_and_individually_hidden_fixture_project();
    let options = MidiExportOptions {
        file_type: MidiFileType::Type0,
        export_markers: true,
        export_hidden_markers: true,
        ..MidiExportOptions::for_project(&project)
    };
    let actual = encode_standard_midi(&project, options).unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-type0-marker-ruler-individual-hidden-markers-included.mid"
    );

    assert_eq!(
        actual, native,
        "Type 0 ruler opt-in includes ruler-hidden markers but still omits an individually hidden marker"
    );
}

#[test]
fn reaper_782_type_zero_hidden_ruler_cue_opt_in_still_excludes_individually_hidden_cues() {
    let project = native_midi_marker_ruler_and_individually_hidden_fixture_project();
    let options = MidiExportOptions {
        file_type: MidiFileType::Type0,
        export_markers: true,
        marker_type: MidiMarkerType::Cues,
        export_hidden_markers: true,
        ..MidiExportOptions::for_project(&project)
    };
    let actual = encode_standard_midi(&project, options).unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-type0-marker-ruler-individual-hidden-cues-included.mid"
    );

    assert_eq!(
        actual, native,
        "Type 0 cue ruler opt-in includes ruler-hidden cues but still omits an individually hidden cue"
    );
}

#[test]
fn reaper_782_type_zero_hidden_ruler_hash_marker_filter_matches_native_bytes() {
    let project = native_midi_marker_ruler_and_individually_hidden_fixture_project();
    let options = MidiExportOptions {
        file_type: MidiFileType::Type0,
        export_markers: true,
        export_hidden_markers: true,
        only_markers_starting_with_hash: true,
        ..MidiExportOptions::for_project(&project)
    };
    let actual = encode_standard_midi(&project, options).unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-type0-marker-ruler-individual-hidden-filtered-markers.mid"
    );

    assert_eq!(
        actual, native,
        "Type 0 hidden-ruler marker export applies the hash filter and omits the individually hidden marker"
    );
}

#[test]
fn reaper_782_type_zero_hidden_ruler_hash_cue_filter_matches_native_bytes() {
    let project = native_midi_marker_ruler_and_individually_hidden_fixture_project();
    let options = MidiExportOptions {
        file_type: MidiFileType::Type0,
        export_markers: true,
        marker_type: MidiMarkerType::Cues,
        export_hidden_markers: true,
        only_markers_starting_with_hash: true,
        ..MidiExportOptions::for_project(&project)
    };
    let actual = encode_standard_midi(&project, options).unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-type0-marker-ruler-individual-hidden-filtered-cues.mid"
    );

    assert_eq!(
        actual, native,
        "Type 0 hidden-ruler cue export applies the hash filter and omits the individually hidden cue"
    );
}

#[test]
fn reaper_782_type_one_tempo_and_meter_changes_match_native_bytes() {
    let mut project = native_midi_fixture_project();
    project
        .apply(DawAction::SetTempo {
            start_tick: 3840,
            bpm: 90.0,
        })
        .unwrap();
    project
        .apply(DawAction::SetTimeSignature {
            start_tick: 3840,
            signature: TimeSignature::new(3, 4).unwrap(),
        })
        .unwrap();
    let mut options = MidiExportOptions::for_project(&project);
    options.embed_tempo_and_time_signature = true;
    let actual = encode_standard_midi_with_conductor_track_name(
        &project,
        options,
        "midi-tempo-meter-native-20261016",
    )
    .unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-tempo-meter.mid"
    );

    assert_eq!(
        actual, native,
        "Type 1 tempo/meter events and MIDI tracks match REAPER 7.82"
    );
}

#[test]
fn reaper_782_type_one_time_selection_preserves_tempo_and_meter_changes() {
    let project = native_midi_tempo_selection_fixture_project(false);
    let options = MidiExportOptions {
        time_scope: MidiExportTimeScope::TimeSelectionOnly,
        embed_tempo_and_time_signature: true,
        ..MidiExportOptions::for_project(&project)
    };
    let selection = MidiExportSelection {
        time_selection_ticks: Some((960, 4_560)),
        ..MidiExportSelection::default()
    };
    let actual =
        encode_standard_midi_with_selection(&project, options, "tempo-selection-type1", &selection)
            .unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-tempo-selection-type1.mid"
    );

    assert_eq!(
        actual, native,
        "Type 1 keeps tempo/meter changes in the time selection even beyond MIDI content"
    );
}

#[test]
fn reaper_782_type_zero_time_selection_stops_tempo_map_after_last_midi_event() {
    let project = native_midi_tempo_selection_fixture_project(false);
    let options = MidiExportOptions {
        file_type: MidiFileType::Type0,
        time_scope: MidiExportTimeScope::TimeSelectionOnly,
        embed_tempo_and_time_signature: true,
        ..MidiExportOptions::for_project(&project)
    };
    let selection = MidiExportSelection {
        time_selection_ticks: Some((960, 4_560)),
        ..MidiExportSelection::default()
    };
    let actual =
        encode_standard_midi_with_selection(&project, options, "tempo-selection-type0", &selection)
            .unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-tempo-selection-type0.mid"
    );

    assert_eq!(
        actual, native,
        "Type 0 omits later tempo/meter changes beyond the last MIDI event"
    );
}

#[test]
fn reaper_782_type_zero_time_selection_keeps_tempo_map_before_later_midi_events() {
    let project = native_midi_tempo_selection_fixture_project(true);
    let options = MidiExportOptions {
        file_type: MidiFileType::Type0,
        time_scope: MidiExportTimeScope::TimeSelectionOnly,
        embed_tempo_and_time_signature: true,
        ..MidiExportOptions::for_project(&project)
    };
    let selection = MidiExportSelection {
        time_selection_ticks: Some((960, 4_560)),
        ..MidiExportSelection::default()
    };
    let actual = encode_standard_midi_with_selection(
        &project,
        options,
        "tempo-selection-post-event-type0",
        &selection,
    )
    .unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-tempo-selection-post-event-type0.mid"
    );

    assert_eq!(
        actual, native,
        "Type 0 retains tempo/meter changes through the last MIDI event"
    );
}

#[test]
fn reaper_782_type_one_linear_tempo_ramp_matches_native_bytes() {
    let mut project = native_midi_fixture_project_with_length(960);
    project
        .apply(DawAction::SetTempo {
            start_tick: 7680,
            bpm: 60.0,
        })
        .unwrap();
    project
        .apply(DawAction::SetTempoCurve {
            start_tick: 0,
            curve: TempoCurve::Linear,
        })
        .unwrap();
    let mut options = MidiExportOptions::for_project(&project);
    options.embed_tempo_and_time_signature = true;
    let actual = encode_standard_midi_with_conductor_track_name(
        &project,
        options,
        "midi-tempo-ramp-native-20261016",
    )
    .unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-tempo-ramp.mid"
    );

    assert_eq!(
        actual, native,
        "Type 1 linear tempo-ramp events and tracks match REAPER 7.82"
    );
}

#[test]
fn reaper_782_type_zero_linear_tempo_ramp_matches_native_bytes() {
    let mut project = native_midi_fixture_project_with_length(960);
    project
        .apply(DawAction::SetTempo {
            start_tick: 7_680,
            bpm: 60.0,
        })
        .unwrap();
    project
        .apply(DawAction::SetTempoCurve {
            start_tick: 0,
            curve: TempoCurve::Linear,
        })
        .unwrap();
    let options = MidiExportOptions {
        file_type: MidiFileType::Type0,
        embed_tempo_and_time_signature: true,
        ..MidiExportOptions::for_project(&project)
    };
    let actual = encode_standard_midi(&project, options).unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-tempo-ramp-type0.mid"
    );

    assert_eq!(
        actual, native,
        "Type 0 ramp events extend to the one-second horizon and match REAPER 7.82"
    );
}

#[test]
fn reaper_782_type_zero_linear_tempo_ramp_keeps_terminal_midi_grid_tick() {
    let mut project = native_midi_fixture_with_note_duration(1_320, 1_320);
    project
        .apply(DawAction::SetTempo {
            start_tick: 7_680,
            bpm: 60.0,
        })
        .unwrap();
    project
        .apply(DawAction::SetTempoCurve {
            start_tick: 0,
            curve: TempoCurve::Linear,
        })
        .unwrap();
    let options = MidiExportOptions {
        file_type: MidiFileType::Type0,
        embed_tempo_and_time_signature: true,
        ..MidiExportOptions::for_project(&project)
    };
    let actual = encode_standard_midi(&project, options).unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-tempo-ramp-type0-terminal-grid.mid"
    );

    assert_eq!(
        actual, native,
        "REAPER keeps the tempo sample at the last MIDI event tick"
    );
}

#[test]
fn reaper_782_type_zero_linear_tempo_ramp_stops_at_exact_whole_second_boundary() {
    let mut project = native_midi_fixture_with_note_duration(1_350, 1_350);
    project
        .apply(DawAction::SetTempo {
            start_tick: 7_680,
            bpm: 60.0,
        })
        .unwrap();
    project
        .apply(DawAction::SetTempoCurve {
            start_tick: 0,
            curve: TempoCurve::Linear,
        })
        .unwrap();
    let options = MidiExportOptions {
        file_type: MidiFileType::Type0,
        embed_tempo_and_time_signature: true,
        ..MidiExportOptions::for_project(&project)
    };
    let actual = encode_standard_midi(&project, options).unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-tempo-ramp-type0-exact-one-second.mid"
    );

    assert_eq!(
        actual, native,
        "REAPER stops ramp samples before a MIDI event at exactly one second"
    );
}

#[test]
fn reaper_782_type_zero_linear_tempo_ramp_stops_before_next_whole_second() {
    let mut project = native_midi_fixture_with_note_duration(1_620, 1_620);
    project
        .apply(DawAction::SetTempo {
            start_tick: 7_680,
            bpm: 60.0,
        })
        .unwrap();
    project
        .apply(DawAction::SetTempoCurve {
            start_tick: 0,
            curve: TempoCurve::Linear,
        })
        .unwrap();
    let options = MidiExportOptions {
        file_type: MidiFileType::Type0,
        embed_tempo_and_time_signature: true,
        ..MidiExportOptions::for_project(&project)
    };
    let actual = encode_standard_midi(&project, options).unwrap();
    let native = include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/menu-export-midi-native-tempo-ramp-type0-later-content.mid"
    );

    assert_eq!(
        actual, native,
        "REAPER keeps ramp samples before the next whole elapsed second"
    );
}

fn native_midi_fixture_project() -> Project {
    native_midi_fixture_project_with_length(1920)
}

fn native_midi_marker_fixture_project() -> Project {
    let mut project = native_midi_fixture_project();
    project
        .apply(DawAction::InsertProjectMarker {
            position_ticks: 0,
            name: "#Intro".to_owned(),
        })
        .unwrap();
    project
        .apply(DawAction::InsertProjectMarker {
            position_ticks: 960,
            name: "Verse".to_owned(),
        })
        .unwrap();
    project
}

fn native_midi_marker_filter_fixture_project() -> Project {
    let mut project = native_midi_fixture_project();
    project
        .apply(DawAction::InsertProjectMarker {
            position_ticks: 0,
            name: "#Intro".to_owned(),
        })
        .unwrap();
    project
        .apply(DawAction::InsertProjectMarker {
            position_ticks: 960,
            name: "Verse".to_owned(),
        })
        .unwrap();
    project
        .apply(DawAction::InsertProjectMarker {
            position_ticks: 1920,
            name: "#Hidden".to_owned(),
        })
        .unwrap();
    let hidden_marker_id = project.markers()[2].id();
    project
        .apply(DawAction::SetProjectMarkerVisibility {
            marker_id: hidden_marker_id,
            visible: false,
        })
        .unwrap();
    project
}

fn native_midi_marker_ruler_hidden_fixture_project() -> Project {
    let mut project = native_midi_marker_fixture_project();
    project
        .apply(DawAction::SetProjectMarkersVisibleOnRuler { visible: false })
        .unwrap();
    project
}

fn native_midi_marker_ruler_and_individually_hidden_fixture_project() -> Project {
    let mut project = native_midi_marker_filter_fixture_project();
    project
        .apply(DawAction::SetProjectMarkersVisibleOnRuler { visible: false })
        .unwrap();
    project
}

fn native_midi_consolidation_fixture_project() -> (Project, MidiExportSelection) {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Track A".to_owned(),
        })
        .unwrap();
    project
        .apply(DawAction::CreateTrack {
            index: 1,
            name: "Track B".to_owned(),
        })
        .unwrap();
    let track_a = project.tracks()[0].id();
    let track_b = project.tracks()[1].id();
    let item_a_selected = add_native_consolidation_item(&mut project, track_a, 480, 60);
    add_native_consolidation_item(&mut project, track_a, 2400, 61);
    add_native_consolidation_item(&mut project, track_b, 4320, 62);

    let selection = MidiExportSelection {
        selected_track_ids: vec![track_b],
        selected_item_ids: vec![item_a_selected],
        time_selection_ticks: Some((1920, 3840)),
    };
    (project, selection)
}

fn native_midi_scope_intersection_fixture_project()
-> (Project, MidiExportSelection, aaadaw_core::ItemId) {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Track A".to_owned(),
        })
        .unwrap();
    project
        .apply(DawAction::CreateTrack {
            index: 1,
            name: "Track B".to_owned(),
        })
        .unwrap();
    let track_a = project.tracks()[0].id();
    let track_b = project.tracks()[1].id();
    let selected_outside_item = add_native_consolidation_item(&mut project, track_a, 480, 60);
    let in_range_item = add_native_consolidation_item(&mut project, track_a, 2400, 61);
    add_native_consolidation_item(&mut project, track_b, 4320, 62);
    let selection = MidiExportSelection {
        selected_track_ids: vec![track_a],
        selected_item_ids: vec![selected_outside_item],
        time_selection_ticks: Some((1920, 3840)),
    };
    (project, selection, in_range_item)
}

fn native_midi_muted_scope_fixture(track_muted: bool, item_muted: bool) -> Project {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: if track_muted {
                "Muted Track".to_owned()
            } else {
                "Unmuted Track".to_owned()
            },
        })
        .unwrap();
    let track_id = project.tracks()[0].id();
    project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 0,
            length_ticks: 960,
        })
        .unwrap();
    let item_id = project.midi_items()[0].id();
    project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![MidiNoteData {
                channel: 0,
                pitch: 60,
                tick: 0,
                duration: 240,
                velocity: 100,
                off_velocity: 0,
            }],
        })
        .unwrap();
    if item_muted {
        project
            .apply(DawAction::SetMidiItemMute {
                item_id,
                muted: true,
            })
            .unwrap();
    }
    if track_muted {
        project
            .apply(DawAction::SetTrackMute {
                track_id,
                muted: true,
            })
            .unwrap();
    }
    project
}

fn add_native_consolidation_item(
    project: &mut Project,
    track_id: aaadaw_core::TrackId,
    start_tick: u64,
    pitch: u8,
) -> aaadaw_core::ItemId {
    project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick,
            length_ticks: 960,
        })
        .unwrap();
    let item_id = project.midi_items().last().unwrap().id();
    project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![MidiNoteData {
                channel: 0,
                pitch,
                tick: 0,
                duration: 240,
                velocity: 100,
                off_velocity: 0,
            }],
        })
        .unwrap();
    item_id
}

fn native_midi_time_selection_boundary_fixture_project() -> Project {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Wide".to_owned(),
        })
        .unwrap();
    let track_id = project.tracks()[0].id();
    project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 480,
            length_ticks: 4800,
        })
        .unwrap();
    let item_id = project.midi_items()[0].id();
    project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![
                MidiNoteData {
                    channel: 0,
                    pitch: 60,
                    tick: 0,
                    duration: 960,
                    velocity: 100,
                    off_velocity: 0,
                },
                MidiNoteData {
                    channel: 0,
                    pitch: 61,
                    tick: 960,
                    duration: 1920,
                    velocity: 100,
                    off_velocity: 0,
                },
                MidiNoteData {
                    channel: 0,
                    pitch: 62,
                    tick: 3000,
                    duration: 1080,
                    velocity: 100,
                    off_velocity: 0,
                },
                MidiNoteData {
                    channel: 0,
                    pitch: 63,
                    tick: 3360,
                    duration: 480,
                    velocity: 100,
                    off_velocity: 0,
                },
            ],
        })
        .unwrap();
    project
}

fn native_midi_fixture_project_with_length(item_length_ticks: u64) -> Project {
    native_midi_fixture_with_note_duration(item_length_ticks, 960)
}

fn native_midi_fixture_with_note_duration(
    item_length_ticks: u64,
    note_duration_ticks: u64,
) -> Project {
    native_midi_fixture_project_with_track_name_and_note_duration(
        item_length_ticks,
        "Piano",
        note_duration_ticks,
    )
}

fn native_midi_fixture_project_with_track_name(
    item_length_ticks: u64,
    track_name: &str,
) -> Project {
    native_midi_fixture_project_with_track_name_and_note_duration(
        item_length_ticks,
        track_name,
        960,
    )
}

fn native_midi_fixture_project_with_track_name_and_note_duration(
    item_length_ticks: u64,
    track_name: &str,
    note_duration_ticks: u64,
) -> Project {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: track_name.to_owned(),
        })
        .unwrap();
    let track_id = project.tracks()[0].id();
    project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 480,
            length_ticks: item_length_ticks,
        })
        .unwrap();
    let item_id = project.midi_items()[0].id();
    project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![MidiNoteData {
                channel: 0,
                pitch: 60,
                tick: 0,
                duration: note_duration_ticks,
                velocity: 100,
                off_velocity: 0,
            }],
        })
        .unwrap();
    project
        .apply(DawAction::SetMidiControllers {
            item_id,
            controllers: vec![MidiControllerData {
                channel: 0,
                controller: 1,
                tick: 240,
                value: 64,
            }],
        })
        .unwrap();
    project
        .apply(DawAction::SetMidiPitchBends {
            item_id,
            pitch_bends: vec![MidiPitchBendData {
                channel: 0,
                tick: 360,
                value: 12_288,
            }],
        })
        .unwrap();
    project
}

fn native_midi_tempo_selection_fixture_project(include_post_tempo_note: bool) -> Project {
    let mut project = native_midi_fixture_project();
    project
        .apply(DawAction::SetTempo {
            start_tick: 3_840,
            bpm: 90.0,
        })
        .unwrap();
    project
        .apply(DawAction::SetTimeSignature {
            start_tick: 3_840,
            signature: TimeSignature::new(3, 4).unwrap(),
        })
        .unwrap();
    if include_post_tempo_note {
        let track_id = project.tracks()[0].id();
        project
            .apply(DawAction::InsertMidiItem {
                track_id,
                start_tick: 4_020,
                length_ticks: 360,
            })
            .unwrap();
        let item_id = project.midi_items()[1].id();
        project
            .apply(DawAction::AddMidiNotes {
                item_id,
                notes: vec![MidiNoteData {
                    channel: 0,
                    pitch: 62,
                    tick: 0,
                    duration: 360,
                    velocity: 100,
                    off_velocity: 0,
                }],
            })
            .unwrap();
    }
    project
}

#[test]
fn type_zero_merges_tracks_scales_ppq_and_exports_filtered_cues() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Piano".to_owned(),
        })
        .unwrap();
    let track_id = project.tracks()[0].id();
    project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 480,
            length_ticks: 1920,
        })
        .unwrap();
    let item_id = project.midi_items()[0].id();
    project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![MidiNoteData {
                channel: 0,
                pitch: 60,
                tick: 0,
                duration: 240,
                velocity: 100,
                off_velocity: 0,
            }],
        })
        .unwrap();
    project
        .apply(DawAction::InsertProjectMarker {
            position_ticks: 1440,
            name: "#Intro".to_owned(),
        })
        .unwrap();
    project
        .apply(DawAction::InsertProjectMarker {
            position_ticks: 1920,
            name: "Verse".to_owned(),
        })
        .unwrap();
    let options = MidiExportOptions {
        file_type: MidiFileType::Type0,
        ppq: 480,
        export_markers: true,
        marker_type: MidiMarkerType::Cues,
        only_markers_starting_with_hash: true,
        ..MidiExportOptions::for_project(&project)
    };

    let bytes = encode_standard_midi(&project, options).expect("type zero export should encode");

    assert_eq!(&bytes[8..10], &[0, 0]);
    assert_eq!(&bytes[10..12], &[0, 1]);
    assert_eq!(&bytes[12..14], &[0x01, 0xe0]);
    assert!(
        bytes
            .windows(5)
            .any(|window| window == [0x81, 0x70, 0x90, 60, 100])
    );
    assert!(
        bytes
            .windows(9)
            .any(|window| window == [0xff, 0x07, 6, b'#', b'I', b'n', b't', b'r', b'o'])
    );
    assert!(!bytes.windows(5).any(|window| window == b"Verse"));
}

#[test]
fn ppq_rounding_never_turns_a_note_into_a_zero_length_event() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Short notes".to_owned(),
        })
        .unwrap();
    let track_id = project.tracks()[0].id();
    project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 0,
            length_ticks: 960,
        })
        .unwrap();
    let item_id = project.midi_items()[0].id();
    project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![MidiNoteData {
                channel: 0,
                pitch: 64,
                tick: 10,
                duration: 1,
                velocity: 80,
                off_velocity: 0,
            }],
        })
        .unwrap();
    let options = MidiExportOptions {
        ppq: 1,
        ..MidiExportOptions::for_project(&project)
    };

    let bytes = encode_standard_midi(&project, options).expect("short note should encode");

    assert!(
        bytes
            .windows(7)
            .any(|window| { window == [0x90, 64, 80, 1, 0x80, 64, 0] })
    );
}

#[test]
fn embeds_smpte_offset_for_2997_non_drop_with_native_30fps_rate_code() {
    let mut project = Project::new();
    project
        .apply(DawAction::SetProjectStartTimeOffsetSeconds { seconds: 12.5 })
        .expect("project start offset should be accepted");
    project
        .apply(DawAction::SetFrameRate {
            rate: FrameRate::Fps2997,
        })
        .expect("29.97 non-drop project rate should be accepted");
    let options = MidiExportOptions {
        embed_smpte_offset: true,
        ..MidiExportOptions::for_project(&project)
    };

    let bytes = encode_standard_midi(&project, options).expect("SMPTE offset should encode");
    assert!(
        bytes
            .windows(8)
            .any(|window| window == [0xff, 0x54, 0x05, 0x60, 0, 12, 14, 0])
    );
}

#[test]
fn hidden_markers_are_excluded_unless_hidden_marker_export_is_enabled() {
    let mut project = Project::new();
    project
        .apply(DawAction::InsertProjectMarker {
            position_ticks: 960,
            name: "Visible marker".to_owned(),
        })
        .unwrap();
    project
        .apply(DawAction::InsertProjectMarker {
            position_ticks: 1_920,
            name: "Hidden marker".to_owned(),
        })
        .unwrap();
    let hidden_id = project.markers()[1].id();
    project
        .apply(DawAction::SetProjectMarkerVisibility {
            marker_id: hidden_id,
            visible: false,
        })
        .unwrap();

    let visible_only = encode_standard_midi(
        &project,
        MidiExportOptions {
            export_markers: true,
            ..MidiExportOptions::for_project(&project)
        },
    )
    .unwrap();
    assert!(
        visible_only
            .windows(14)
            .any(|bytes| bytes == b"Visible marker")
    );
    assert!(
        !visible_only
            .windows(13)
            .any(|bytes| bytes == b"Hidden marker")
    );

    let include_hidden = encode_standard_midi(
        &project,
        MidiExportOptions {
            export_markers: true,
            export_hidden_markers: true,
            ..MidiExportOptions::for_project(&project)
        },
    )
    .unwrap();
    assert!(
        include_hidden
            .windows(14)
            .any(|bytes| bytes == b"Visible marker")
    );
    assert!(
        include_hidden
            .windows(13)
            .any(|bytes| bytes == b"Hidden marker")
    );

    project
        .apply(DawAction::SetProjectMarkersVisibleOnRuler { visible: false })
        .unwrap();
    let no_markers = encode_standard_midi(
        &project,
        MidiExportOptions {
            export_markers: true,
            ..MidiExportOptions::for_project(&project)
        },
    )
    .unwrap();
    assert!(
        !no_markers
            .windows(14)
            .any(|bytes| bytes == b"Visible marker")
    );
}
