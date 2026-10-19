use aaadaw_core::{
    DawAction, MidiChannelPressureData, MidiControllerData, MidiEventOrderData,
    MidiEventOrderTarget, MidiItemTakeSnapshot, MidiNoteData, MidiNoteSnapshot,
    MidiOpaqueEventData, MidiOpaqueEventKind, MidiPitchBendData, MidiPolyAftertouchData,
    MidiProgramChangeData, Project,
};
use aaadaw_storage::ProjectStore;

#[test]
fn midi_item_takes_round_trip_all_event_kinds_and_active_index() {
    let path = tempfile::NamedTempFile::new().unwrap();
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Take persistence".into(),
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

    let mut snapshot = project.snapshot();
    let item = &mut snapshot.midi_items[0];
    item.active_take_index = 1;
    item.name = "Take B".into();
    item.notes = vec![MidiNoteSnapshot {
        id: 10,
        data: MidiNoteData {
            pitch: 64,
            channel: 2,
            tick: 24,
            duration: 120,
            velocity: 95,
            off_velocity: 17,
        },
    }];
    item.controllers = vec![MidiControllerData {
        controller: 7,
        channel: 2,
        tick: 48,
        value: 100,
    }];
    item.pitch_bends = vec![MidiPitchBendData {
        channel: 2,
        tick: 72,
        value: 9_001,
    }];
    item.program_changes = vec![MidiProgramChangeData {
        channel: 2,
        tick: 96,
        program: 9,
    }];
    item.poly_aftertouches = vec![MidiPolyAftertouchData {
        channel: 2,
        tick: 120,
        note: 64,
        value: 71,
    }];
    item.channel_pressures = vec![MidiChannelPressureData {
        channel: 2,
        tick: 144,
        value: 53,
    }];
    item.opaque_events = vec![MidiOpaqueEventData {
        tick: 168,
        order: 3,
        kind: MidiOpaqueEventKind::Meta {
            kind: 0x01,
            data: b"take B".to_vec(),
        },
    }];
    item.event_order = vec![MidiEventOrderData {
        target: MidiEventOrderTarget::Note(item.notes[0].data),
        order: 4,
        note_off_order: Some(5),
    }];
    item.other_takes = vec![MidiItemTakeSnapshot {
        index: 0,
        name: "Take A".into(),
        source_offset_ticks: 12,
        loop_source: true,
        source_loop_length_ticks: 960,
        notes: vec![MidiNoteSnapshot {
            id: 11,
            data: MidiNoteData {
                pitch: 60,
                channel: 1,
                tick: 12,
                duration: 240,
                velocity: 80,
                off_velocity: 9,
            },
        }],
        controllers: vec![MidiControllerData {
            controller: 10,
            channel: 1,
            tick: 36,
            value: 22,
        }],
        pitch_bends: vec![MidiPitchBendData {
            channel: 1,
            tick: 60,
            value: 7_777,
        }],
        program_changes: vec![MidiProgramChangeData {
            channel: 1,
            tick: 84,
            program: 22,
        }],
        poly_aftertouches: vec![MidiPolyAftertouchData {
            channel: 1,
            tick: 108,
            note: 60,
            value: 33,
        }],
        channel_pressures: vec![MidiChannelPressureData {
            channel: 1,
            tick: 132,
            value: 44,
        }],
        opaque_events: vec![MidiOpaqueEventData {
            tick: 156,
            order: 2,
            kind: MidiOpaqueEventKind::SystemExclusive {
                status: 0xF0,
                data: vec![0x7D, 1, 2, 0xF7],
            },
        }],
        event_order: vec![MidiEventOrderData {
            target: MidiEventOrderTarget::Controller(MidiControllerData {
                controller: 10,
                channel: 1,
                tick: 36,
                value: 22,
            }),
            order: 6,
            note_off_order: None,
        }],
    }];

    let project = Project::from_snapshot(snapshot).unwrap();
    let expected = project.snapshot();
    let mut store = ProjectStore::open(path.path()).unwrap();
    store.save(&project).unwrap();
    assert_eq!(store.load().unwrap().snapshot(), expected);
}
