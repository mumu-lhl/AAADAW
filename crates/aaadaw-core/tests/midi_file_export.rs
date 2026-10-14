use aaadaw_core::{
    DawAction, FrameRate, MidiControllerData, MidiExportOptions, MidiFileType, MidiMarkerType,
    MidiNoteData, MidiPitchBendData, Project, TempoCurve, encode_standard_midi,
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
fn exports_tempo_ramps_as_bounded_step_events() {
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

    assert!(
        tempo_events > 60,
        "the tempo ramp should not become a single jump"
    );
    assert!(
        bytes
            .windows(6)
            .any(|window| window == [0xff, 0x51, 0x03, 0x07, 0xa1, 0x20])
    );
    assert!(
        bytes
            .windows(6)
            .any(|window| window == [0xff, 0x51, 0x03, 0x0f, 0x42, 0x40])
    );
}

#[test]
fn defaults_to_native_multitrack_ppq_without_optional_metadata() {
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
    assert!(!bytes.windows(2).any(|window| window == [0xff, 0x51]));
    assert!(!bytes.windows(2).any(|window| window == [0xff, 0x58]));
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
fn embeds_smpte_offset_and_rejects_non_drop_2997() {
    let mut project = Project::new();
    project
        .apply(DawAction::SetProjectStartTimeOffsetSeconds { seconds: 12.5 })
        .expect("project start offset should be accepted");
    let options = MidiExportOptions {
        embed_smpte_offset: true,
        ..MidiExportOptions::for_project(&project)
    };

    let bytes = encode_standard_midi(&project, options).expect("SMPTE offset should encode");
    assert!(
        bytes
            .windows(8)
            .any(|window| window == [0xff, 0x54, 0x05, 0x60, 0, 12, 15, 0])
    );

    project
        .apply(DawAction::SetFrameRate {
            rate: FrameRate::Fps2997,
        })
        .expect("29.97 non-drop project rate should be accepted");
    assert!(encode_standard_midi(&project, options).is_err());
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
