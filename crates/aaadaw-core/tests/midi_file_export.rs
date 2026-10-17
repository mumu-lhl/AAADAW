use aaadaw_core::{
    DawAction, FrameRate, MidiControllerData, MidiExportOptions, MidiFileType, MidiItemPosition,
    MidiMarkerType, MidiNoteData, MidiPitchBendData, Project, TempoCurve, TimeSignature,
    encode_standard_midi, encode_standard_midi_with_conductor_track_name,
};

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
                pitch: 60,
                tick: 0,
                duration: 960,
                velocity: 100,
            }],
        })
        .expect("MIDI note should be inserted");
    project
        .apply(DawAction::SetMidiControllers {
            item_id,
            controllers: vec![MidiControllerData {
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
                pitch: 60,
                tick: 0,
                duration: 120,
                velocity: 100,
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
                pitch: 60,
                tick: 120,
                duration: 120,
                velocity: 100,
            }],
        })
        .unwrap();
    project
        .apply(DawAction::SetMidiControllers {
            item_id,
            controllers: vec![MidiControllerData {
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
        &[0, 1],
        "empty project still has one conductor track"
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

fn native_midi_fixture_project_with_length(item_length_ticks: u64) -> Project {
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
            length_ticks: item_length_ticks,
        })
        .unwrap();
    let item_id = project.midi_items()[0].id();
    project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![MidiNoteData {
                pitch: 60,
                tick: 0,
                duration: 960,
                velocity: 100,
            }],
        })
        .unwrap();
    project
        .apply(DawAction::SetMidiControllers {
            item_id,
            controllers: vec![MidiControllerData {
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
                tick: 360,
                value: 12_288,
            }],
        })
        .unwrap();
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
                pitch: 60,
                tick: 0,
                duration: 240,
                velocity: 100,
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
                pitch: 64,
                tick: 10,
                duration: 1,
                velocity: 80,
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
