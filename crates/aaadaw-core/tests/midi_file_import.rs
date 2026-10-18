use aaadaw_core::{
    ActionError, DawAction, GridFraction, MidiChannelPressureData, MidiControllerData,
    MidiEventOrderTarget, MidiFileEventKind, MidiFileFormat, MidiFileImportOptions, MidiNoteData,
    MidiOpaqueEventKind, MidiPitchBendData, MidiPolyAftertouchData, MidiProgramChangeData, Project,
    decode_standard_midi,
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

fn midi_note_data(note: &aaadaw_core::MidiNote) -> MidiNoteData {
    MidiNoteData {
        pitch: note.pitch(),
        channel: note.channel(),
        tick: note.tick(),
        duration: note.duration(),
        velocity: note.velocity(),
        off_velocity: note.off_velocity(),
    }
}

#[test]
fn same_tick_import_records_typed_event_source_order_and_round_trips_in_snapshots() {
    let fixtures: [(&[u8], &[&str]); 2] = [
        (
            include_bytes!(
                "../../../docs/verification/reaper-parity/fixtures/midi-file-import-same-tick-order.mid"
            ),
            &[
                "pitch",
                "controller",
                "program",
                "poly-aftertouch",
                "channel-pressure",
                "note",
                "controller",
            ],
        ),
        (
            include_bytes!(
                "../../../docs/verification/reaper-parity/fixtures/midi-file-import-same-tick-order-reversed.mid"
            ),
            &[
                "note",
                "controller",
                "channel-pressure",
                "poly-aftertouch",
                "program",
                "controller",
                "pitch",
            ],
        ),
    ];

    for (fixture, expected_order) in fixtures {
        let decoded = decode_standard_midi(fixture).unwrap();
        let imported = decoded.to_project_import(960).unwrap();
        let track = imported.tracks.into_iter().next().unwrap();
        let expected_note_off_order = decoded.tracks[0]
            .events
            .iter()
            .enumerate()
            .find_map(|(index, event)| match &event.kind {
                MidiFileEventKind::Channel { status: 0x80, data }
                    if event.tick == 480 && data[0] == 0x60 =>
                {
                    Some(index as u32)
                }
                _ => None,
            })
            .unwrap();
        assert!(track.event_order.iter().any(|entry| {
            matches!(entry.target, MidiEventOrderTarget::Note(note) if note.tick == 240)
                && entry.note_off_order == Some(expected_note_off_order)
        }));
        let mut source_order = track
            .event_order
            .iter()
            .filter(|entry| match entry.target {
                MidiEventOrderTarget::Note(event) => event.tick == 240,
                MidiEventOrderTarget::Controller(event) => event.tick == 240,
                MidiEventOrderTarget::PitchBend(event) => event.tick == 240,
                MidiEventOrderTarget::ProgramChange(event) => event.tick == 240,
                MidiEventOrderTarget::PolyAftertouch(event) => event.tick == 240,
                MidiEventOrderTarget::ChannelPressure(event) => event.tick == 240,
            })
            .copied()
            .collect::<Vec<_>>();
        source_order.sort_unstable_by_key(|entry| entry.order);
        let actual_order = source_order
            .iter()
            .map(|entry| match entry.target {
                MidiEventOrderTarget::Note(_) => "note",
                MidiEventOrderTarget::Controller(_) => "controller",
                MidiEventOrderTarget::PitchBend(_) => "pitch",
                MidiEventOrderTarget::ProgramChange(_) => "program",
                MidiEventOrderTarget::PolyAftertouch(_) => "poly-aftertouch",
                MidiEventOrderTarget::ChannelPressure(_) => "channel-pressure",
            })
            .collect::<Vec<_>>();
        assert_eq!(actual_order, expected_order);

        let event_order = track.event_order.clone();
        let mut project = Project::new();
        project
            .apply(DawAction::ImportMidiTrack {
                start_tick: 0,
                track,
            })
            .unwrap();
        assert_eq!(project.midi_items()[0].event_order(), event_order);
        let snapshot = project.snapshot();
        assert_eq!(snapshot.midi_items[0].event_order, event_order);
        assert_eq!(
            Project::from_snapshot(snapshot.clone()).unwrap().snapshot(),
            snapshot
        );
    }
}

#[test]
fn same_tick_import_order_does_not_block_typed_event_edits_or_snapshot_round_trip() {
    let decoded = decode_standard_midi(include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/midi-file-import-same-tick-order.mid"
    ))
    .unwrap();
    let mut tracks = decoded.to_project_import(960).unwrap().tracks;
    let track = tracks.remove(0);
    let mut project = Project::new();
    project
        .apply(DawAction::ImportMidiTrack {
            start_tick: 0,
            track,
        })
        .unwrap();

    let item = &project.midi_items()[0];
    let item_id = item.id();
    let first_note = item.notes()[0].clone();
    let second_note_id = item.notes()[1].id();
    let first_note_data = MidiNoteData {
        pitch: first_note.pitch(),
        channel: first_note.channel(),
        tick: first_note.tick(),
        duration: first_note.duration(),
        velocity: first_note.velocity(),
        off_velocity: first_note.off_velocity(),
    };
    let first_note_order = item
        .event_order()
        .iter()
        .find(|entry| entry.target == MidiEventOrderTarget::Note(first_note_data))
        .copied()
        .unwrap();
    project
        .apply(DawAction::EditMidiNote {
            item_id,
            note_id: first_note.id(),
            data: aaadaw_core::MidiNoteData {
                pitch: first_note.pitch() + 1,
                channel: first_note.channel(),
                tick: first_note.tick(),
                duration: first_note.duration(),
                velocity: first_note.velocity(),
                off_velocity: first_note.off_velocity(),
            },
        })
        .unwrap();
    let edited_note_data = MidiNoteData {
        pitch: first_note.pitch() + 1,
        ..first_note_data
    };
    assert!(project.midi_items()[0].event_order().iter().any(|entry| {
        entry.target == MidiEventOrderTarget::Note(edited_note_data)
            && entry.order == first_note_order.order
            && entry.note_off_order == first_note_order.note_off_order
    }));
    assert!(project.undo().unwrap());
    assert!(project.midi_items()[0].event_order().iter().any(|entry| {
        entry.target == MidiEventOrderTarget::Note(first_note_data)
            && entry.order == first_note_order.order
            && entry.note_off_order == first_note_order.note_off_order
    }));
    assert!(project.redo().unwrap());
    assert!(project.midi_items()[0].event_order().iter().any(|entry| {
        entry.target == MidiEventOrderTarget::Note(edited_note_data)
            && entry.order == first_note_order.order
            && entry.note_off_order == first_note_order.note_off_order
    }));
    let first_controller = project.midi_items()[0].controllers()[0];
    let first_controller_order = project.midi_items()[0]
        .event_order()
        .iter()
        .find(|entry| entry.target == MidiEventOrderTarget::Controller(first_controller))
        .copied()
        .unwrap();
    let edited_controller = MidiControllerData {
        value: first_controller.value + 1,
        ..first_controller
    };
    let mut edited_controllers = project.midi_items()[0].controllers().to_vec();
    edited_controllers[0] = edited_controller;
    project
        .apply(DawAction::SetMidiControllers {
            item_id,
            controllers: edited_controllers,
        })
        .unwrap();
    assert!(project.midi_items()[0].event_order().iter().any(|entry| {
        entry.target == MidiEventOrderTarget::Controller(edited_controller)
            && entry.order == first_controller_order.order
    }));
    assert!(project.undo().unwrap());
    assert!(project.midi_items()[0].event_order().iter().any(|entry| {
        entry.target == MidiEventOrderTarget::Controller(first_controller)
            && entry.order == first_controller_order.order
    }));
    assert!(project.redo().unwrap());
    assert!(project.midi_items()[0].event_order().iter().any(|entry| {
        entry.target == MidiEventOrderTarget::Controller(edited_controller)
            && entry.order == first_controller_order.order
    }));
    project
        .apply(DawAction::DeleteMidiNotes {
            item_id,
            note_ids: vec![second_note_id],
        })
        .unwrap();
    let mut remaining_controllers = project.midi_items()[0].controllers().to_vec();
    remaining_controllers.remove(0);
    project
        .apply(DawAction::SetMidiControllers {
            item_id,
            controllers: remaining_controllers,
        })
        .unwrap();

    assert_eq!(project.midi_items()[0].notes().len(), 1);
    assert_eq!(
        project.midi_items()[0].notes()[0].pitch(),
        first_note.pitch() + 1
    );
    assert_eq!(
        Project::from_snapshot(project.snapshot())
            .unwrap()
            .midi_items()[0]
            .notes()[0]
            .pitch(),
        first_note.pitch() + 1
    );
}

#[test]
fn editing_one_of_identical_imported_notes_keeps_its_paired_off_order() {
    let track = [
        0x00, 0x90, 60, 100, // First Note On.
        0x00, 0x90, 60, 100, // Identical overlapping Note On.
        0x83, 0x60, 0x80, 60, 20, // FIFO Note Off at tick 480.
        0x00, 0x80, 60, 20, // Second FIFO Note Off at tick 480.
        0x00, 0xFF, 0x2F, 0x00,
    ];
    let decoded = decode_standard_midi(&smf(0, 960, &[&track])).unwrap();
    let imported = decoded.to_project_import(960).unwrap();
    let mut project = Project::new();
    project
        .apply(DawAction::ImportMidiTrack {
            start_tick: 0,
            track: imported.tracks[0].clone(),
        })
        .unwrap();
    let item = &project.midi_items()[0];
    let item_id = item.id();
    let identical_data = MidiNoteData {
        pitch: 60,
        channel: 0,
        tick: 0,
        duration: 480,
        velocity: 100,
        off_velocity: 20,
    };
    let second_note_order = item
        .event_order()
        .iter()
        .filter(|entry| entry.target == MidiEventOrderTarget::Note(identical_data))
        .nth(1)
        .copied()
        .unwrap();
    let second_note_id = item.notes()[1].id();

    project
        .apply(DawAction::EditMidiNote {
            item_id,
            note_id: second_note_id,
            data: MidiNoteData {
                pitch: 61,
                ..identical_data
            },
        })
        .unwrap();

    assert!(project.midi_items()[0].event_order().iter().any(|entry| {
        entry.target
            == MidiEventOrderTarget::Note(MidiNoteData {
                pitch: 61,
                ..identical_data
            })
            && entry.order == second_note_order.order
            && entry.note_off_order == second_note_order.note_off_order
    }));
}

#[test]
fn same_tick_import_order_tracks_edited_channel_event_values() {
    let decoded = decode_standard_midi(include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/midi-file-import-same-tick-order.mid"
    ))
    .unwrap();
    let imported = decoded.to_project_import(960).unwrap();
    let mut project = Project::new();
    project
        .apply(DawAction::ImportMidiTrack {
            start_tick: 0,
            track: imported.tracks[0].clone(),
        })
        .unwrap();
    let item_id = project.midi_items()[0].id();

    let bend = project.midi_items()[0].pitch_bends()[0];
    let bend_order = project.midi_items()[0]
        .event_order()
        .iter()
        .find(|entry| entry.target == MidiEventOrderTarget::PitchBend(bend))
        .unwrap()
        .order;
    let edited_bend = MidiPitchBendData {
        value: bend.value + 1,
        ..bend
    };
    let mut bends = project.midi_items()[0].pitch_bends().to_vec();
    bends[0] = edited_bend;
    project
        .apply(DawAction::SetMidiPitchBends {
            item_id,
            pitch_bends: bends,
        })
        .unwrap();
    assert!(project.midi_items()[0].event_order().iter().any(|entry| {
        entry.target == MidiEventOrderTarget::PitchBend(edited_bend) && entry.order == bend_order
    }));

    let program = project.midi_items()[0].program_changes()[0];
    let program_order = project.midi_items()[0]
        .event_order()
        .iter()
        .find(|entry| entry.target == MidiEventOrderTarget::ProgramChange(program))
        .unwrap()
        .order;
    let edited_program = MidiProgramChangeData {
        program: program.program + 1,
        ..program
    };
    let mut programs = project.midi_items()[0].program_changes().to_vec();
    programs[0] = edited_program;
    project
        .apply(DawAction::SetMidiProgramChanges {
            item_id,
            program_changes: programs,
        })
        .unwrap();
    assert!(project.midi_items()[0].event_order().iter().any(|entry| {
        entry.target == MidiEventOrderTarget::ProgramChange(edited_program)
            && entry.order == program_order
    }));

    let aftertouch = project.midi_items()[0].poly_aftertouches()[0];
    let aftertouch_order = project.midi_items()[0]
        .event_order()
        .iter()
        .find(|entry| entry.target == MidiEventOrderTarget::PolyAftertouch(aftertouch))
        .unwrap()
        .order;
    let edited_aftertouch = MidiPolyAftertouchData {
        value: aftertouch.value + 1,
        ..aftertouch
    };
    let mut aftertouches = project.midi_items()[0].poly_aftertouches().to_vec();
    aftertouches[0] = edited_aftertouch;
    project
        .apply(DawAction::SetMidiPolyAftertouches {
            item_id,
            poly_aftertouches: aftertouches,
        })
        .unwrap();
    assert!(project.midi_items()[0].event_order().iter().any(|entry| {
        entry.target == MidiEventOrderTarget::PolyAftertouch(edited_aftertouch)
            && entry.order == aftertouch_order
    }));

    let pressure = project.midi_items()[0].channel_pressures()[0];
    let pressure_order = project.midi_items()[0]
        .event_order()
        .iter()
        .find(|entry| entry.target == MidiEventOrderTarget::ChannelPressure(pressure))
        .unwrap()
        .order;
    let edited_pressure = MidiChannelPressureData {
        value: pressure.value + 1,
        ..pressure
    };
    let mut pressures = project.midi_items()[0].channel_pressures().to_vec();
    pressures[0] = edited_pressure;
    project
        .apply(DawAction::SetMidiChannelPressures {
            item_id,
            channel_pressures: pressures,
        })
        .unwrap();
    assert!(project.midi_items()[0].event_order().iter().any(|entry| {
        entry.target == MidiEventOrderTarget::ChannelPressure(edited_pressure)
            && entry.order == pressure_order
    }));
}

#[test]
fn imported_note_order_targets_follow_quantize_and_humanize_undo_redo() {
    let decoded = decode_standard_midi(include_bytes!(
        "../../../docs/verification/reaper-parity/fixtures/midi-file-import-same-tick-order.mid"
    ))
    .unwrap();
    let imported = decoded.to_project_import(960).unwrap();
    let mut project = Project::new();
    project
        .apply(DawAction::ImportMidiTrack {
            start_tick: 0,
            track: imported.tracks[0].clone(),
        })
        .unwrap();
    let item_id = project.midi_items()[0].id();
    let note = project.midi_items()[0]
        .notes()
        .iter()
        .find(|note| note.tick() == 240)
        .unwrap()
        .clone();
    let note_id = note.id();
    let source_data = midi_note_data(&note);
    let source_order = project.midi_items()[0]
        .event_order()
        .iter()
        .find(|entry| entry.target == MidiEventOrderTarget::Note(source_data))
        .copied()
        .unwrap();

    project
        .apply(DawAction::QuantizeMidiNotes {
            item_id,
            note_ids: vec![note_id],
            grid: GridFraction::new(1, 8).unwrap(),
            strength: 1.0,
        })
        .unwrap();
    let quantized_note = project.midi_items()[0]
        .notes()
        .iter()
        .find(|note| note.id() == note_id)
        .unwrap()
        .clone();
    let quantized_data = midi_note_data(&quantized_note);
    assert_ne!(quantized_data.tick, source_data.tick);
    assert!(project.midi_items()[0].event_order().iter().any(|entry| {
        entry.target == MidiEventOrderTarget::Note(quantized_data)
            && entry.order == source_order.order
            && entry.note_off_order == source_order.note_off_order
    }));
    assert!(project.undo().unwrap());
    assert!(project.midi_items()[0].event_order().iter().any(|entry| {
        entry.target == MidiEventOrderTarget::Note(source_data)
            && entry.order == source_order.order
            && entry.note_off_order == source_order.note_off_order
    }));
    assert!(project.redo().unwrap());

    project
        .apply(DawAction::HumanizeMidiNotes {
            item_id,
            note_ids: vec![note_id],
            grid: GridFraction::new(1, 32).unwrap(),
            timing_amount_percent: 100,
            velocity_amount_percent: 100,
            timing_bias_percent: 0,
            seed: 0x43b0_1a52_74c9_203f,
        })
        .unwrap();
    let humanized_note = project.midi_items()[0]
        .notes()
        .iter()
        .find(|note| note.id() == note_id)
        .unwrap()
        .clone();
    let humanized_data = midi_note_data(&humanized_note);
    assert!(project.midi_items()[0].event_order().iter().any(|entry| {
        entry.target == MidiEventOrderTarget::Note(humanized_data)
            && entry.order == source_order.order
            && entry.note_off_order == source_order.note_off_order
    }));
    assert!(project.undo().unwrap());
    assert!(project.midi_items()[0].event_order().iter().any(|entry| {
        entry.target == MidiEventOrderTarget::Note(quantized_data)
            && entry.order == source_order.order
            && entry.note_off_order == source_order.note_off_order
    }));
    assert!(project.redo().unwrap());
    assert!(project.midi_items()[0].event_order().iter().any(|entry| {
        entry.target == MidiEventOrderTarget::Note(humanized_data)
            && entry.order == source_order.order
            && entry.note_off_order == source_order.note_off_order
    }));
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
fn project_import_merges_f7_continuations_into_open_f0_at_the_final_packet_tick() {
    let track = [
        0x3C, 0xF0, 0x02, 0x43, 0x12, // F0 start at tick 60
        0x3C, 0xF7, 0x02, 0x34, 0xF7, // final F7 packet at tick 120
        0x00, 0xFF, 0x2F, 0x00,
    ];
    let decoded = decode_standard_midi(&smf(0, 480, &[&track])).unwrap();
    assert_eq!(decoded.tracks[0].events.len(), 3);
    assert!(matches!(
        decoded.tracks[0].events[0].kind,
        MidiFileEventKind::SystemExclusive { status: 0xF0, .. }
    ));
    assert!(matches!(
        decoded.tracks[0].events[1].kind,
        MidiFileEventKind::SystemExclusive { status: 0xF7, .. }
    ));

    let imported = decoded.to_project_import(480).unwrap();
    assert_eq!(imported.tracks[0].opaque_events.len(), 1);
    assert_eq!(imported.tracks[0].opaque_events[0].tick, 120);
    assert_eq!(imported.tracks[0].opaque_events[0].order, 1);
    assert_eq!(
        imported.tracks[0].opaque_events[0].kind,
        MidiOpaqueEventKind::SystemExclusive {
            status: 0xF0,
            data: vec![0x43, 0x12, 0x34, 0xF7],
        }
    );
}

#[test]
fn project_import_merges_multiple_f7_continuations_and_drops_unmatched_f7() {
    let chained = [
        0x3C, 0xF0, 0x01, 0x43, // F0 start at tick 60
        0x1E, 0xF7, 0x01, 0x12, // continuation at tick 90
        0x1E, 0xF7, 0x02, 0x34, 0xF7, // final continuation at tick 120
        0x00, 0xFF, 0x2F, 0x00,
    ];
    let imported = decode_standard_midi(&smf(0, 480, &[&chained]))
        .unwrap()
        .to_project_import(480)
        .unwrap();
    assert_eq!(imported.tracks[0].opaque_events.len(), 1);
    assert_eq!(imported.tracks[0].opaque_events[0].tick, 120);
    assert_eq!(
        imported.tracks[0].opaque_events[0].kind,
        MidiOpaqueEventKind::SystemExclusive {
            status: 0xF0,
            data: vec![0x43, 0x12, 0x34, 0xF7],
        }
    );

    let unmatched = [0x3C, 0xF7, 0x02, 0x34, 0xF7, 0x00, 0xFF, 0x2F, 0x00];
    let imported = decode_standard_midi(&smf(0, 480, &[&unmatched]))
        .unwrap()
        .to_project_import(480)
        .unwrap();
    assert!(imported.tracks[0].opaque_events.is_empty());
}

#[test]
fn project_import_terminates_open_f0_messages_and_continuations() {
    let unterminated_f0 = [
        0x3C, 0xF0, 0x02, 0x43, 0x12, // F0 at tick 60, no F7 terminator
        0x00, 0xFF, 0x2F, 0x00,
    ];
    let imported = decode_standard_midi(&smf(0, 480, &[&unterminated_f0]))
        .unwrap()
        .to_project_import(480)
        .unwrap();
    assert_eq!(imported.tracks[0].opaque_events.len(), 1);
    assert_eq!(imported.tracks[0].opaque_events[0].tick, 60);
    assert_eq!(
        imported.tracks[0].opaque_events[0].kind,
        MidiOpaqueEventKind::SystemExclusive {
            status: 0xF0,
            data: vec![0x43, 0x12, 0xF7],
        }
    );

    let second_f0 = [
        0x3C, 0xF0, 0x01, 0x43, // First F0 at tick 60
        0x3C, 0xF0, 0x02, 0x12, 0xF7, // New F0 at tick 120 closes the first
        0x00, 0xFF, 0x2F, 0x00,
    ];
    let imported = decode_standard_midi(&smf(0, 480, &[&second_f0]))
        .unwrap()
        .to_project_import(480)
        .unwrap();
    assert_eq!(imported.tracks[0].opaque_events.len(), 2);
    assert_eq!(imported.tracks[0].opaque_events[0].tick, 60);
    assert_eq!(
        imported.tracks[0].opaque_events[0].kind,
        MidiOpaqueEventKind::SystemExclusive {
            status: 0xF0,
            data: vec![0x43, 0xF7],
        }
    );
    assert_eq!(imported.tracks[0].opaque_events[1].tick, 120);
    assert_eq!(
        imported.tracks[0].opaque_events[1].kind,
        MidiOpaqueEventKind::SystemExclusive {
            status: 0xF0,
            data: vec![0x12, 0xF7],
        }
    );

    let unterminated_continuation = [
        0x3C, 0xF0, 0x01, 0x43, // F0 at tick 60
        0x3C, 0xF7, 0x01, 0x12, // F7 continuation at tick 120, no terminator
        0x00, 0xFF, 0x2F, 0x00,
    ];
    let imported = decode_standard_midi(&smf(0, 480, &[&unterminated_continuation]))
        .unwrap()
        .to_project_import(480)
        .unwrap();
    assert_eq!(imported.tracks[0].opaque_events.len(), 1);
    assert_eq!(imported.tracks[0].opaque_events[0].tick, 120);
    assert_eq!(
        imported.tracks[0].opaque_events[0].kind,
        MidiOpaqueEventKind::SystemExclusive {
            status: 0xF0,
            data: vec![0x43, 0x12, 0xF7],
        }
    );
}

#[test]
fn project_import_closes_sysex_before_channel_and_meta_events_and_drops_later_f7() {
    let track = [
        0x3C, 0xF0, 0x02, 0x43, 0x12, // open F0 at tick 60
        0x1E, 0xB0, 0x07, 0x40, // Control Change at tick 90 closes the F0
        0x1E, 0xF7, 0x02, 0x34, 0xF7, // unmatched continuation at tick 120
        0x1E, 0xF0, 0x01, 0x56, // second open F0 at tick 150
        0x1E, 0xFF, 0x01, 0x01, b'x', // Text Meta at tick 180 closes it
        0x1E, 0xF7, 0x02, 0x78, 0xF7, // unmatched continuation at tick 210
        0x00, 0xFF, 0x2F, 0x00,
    ];
    let imported = decode_standard_midi(&smf(0, 480, &[&track]))
        .unwrap()
        .to_project_import(480)
        .unwrap();
    assert_eq!(imported.tracks[0].controllers.len(), 1);
    assert_eq!(imported.tracks[0].controllers[0].tick, 90);
    assert_eq!(imported.tracks[0].opaque_events.len(), 3);
    assert_eq!(imported.tracks[0].opaque_events[0].tick, 60);
    assert_eq!(
        imported.tracks[0].opaque_events[0].kind,
        MidiOpaqueEventKind::SystemExclusive {
            status: 0xF0,
            data: vec![0x43, 0x12, 0xF7],
        }
    );
    assert_eq!(imported.tracks[0].opaque_events[1].tick, 150);
    assert_eq!(
        imported.tracks[0].opaque_events[1].kind,
        MidiOpaqueEventKind::SystemExclusive {
            status: 0xF0,
            data: vec![0x56, 0xF7],
        }
    );
    assert_eq!(imported.tracks[0].opaque_events[2].tick, 180);
    assert_eq!(
        imported.tracks[0].opaque_events[2].kind,
        MidiOpaqueEventKind::Meta {
            kind: 0x01,
            data: vec![b'x'],
        }
    );
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
fn converts_conductor_data_and_selects_reaper_default_or_expanded_tracks() {
    let conductor = [
        0x00, 0xFF, 0x03, 0x09, b'C', b'o', b'n', b'd', b'u', b'c', b't', b'o', b'r', 0x00, 0xFF,
        0x51, 0x03, 0x07, 0xA1, 0x20, // 120 BPM at tick 0
        0x00, 0xFF, 0x58, 0x04, 0x04, 0x02, 0x18, 0x08, // 4/4 at tick 0
        0x3C, 0xFF, 0x51, 0x03, 0x06, 0x1A, 0x80, // 150 BPM at tick 60
        0x00, 0xFF, 0x58, 0x04, 0x03, 0x02, 0x18, 0x08, // 3/4 at tick 60
        0x00, 0xFF, 0x2F, 0x00,
    ];
    let lead = [
        0x00, 0xFF, 0x03, 0x04, b'L', b'e', b'a', b'd', 0x00, 0x92, 60, 100, 0x3C, 0xFF, 0x01,
        0x04, b'T', b'e', b'x', b't', // text at tick 60
        0x3C, 0xF0, 0x02, 0x43, 0x12, // SysEx at tick 120
        0x3C, 0xFF, 0x07, 0x03, b'C', b'u', b'e', // Cue at tick 180
        0x3C, 0x82, 60, 37, // Note Off at tick 240
        0x00, 0xFF, 0x2F, 0x00,
    ];
    let meta_only = [
        0x00, 0xFF, 0x03, 0x04, b'M', b'e', b't', b'a', 0x00, 0xFF, 0x06, 0x06, b'M', b'a', b'r',
        b'k', b'e', b'r', 0x00, 0xFF, 0x2F, 0x00,
    ];
    let decoded = decode_standard_midi(&smf(1, 480, &[&conductor, &lead, &meta_only])).unwrap();
    let imported = decoded.to_project_import(480).unwrap();

    assert_eq!(imported.tracks.len(), 3);
    assert!(imported.tracks[1].has_channel_events);
    assert!(!imported.tracks[2].has_channel_events);
    assert_eq!(imported.tracks[1].opaque_events.len(), 4);
    assert_eq!(imported.tracks[1].opaque_events[1].tick, 60);
    assert_eq!(imported.tempo_points.len(), 2);
    assert_eq!(imported.tempo_points[0].bpm, 120.0);
    assert_eq!(imported.tempo_points[1].bpm, 150.0);
    assert_eq!(imported.meter_points[0].numerator, 4);
    assert_eq!(imported.meter_points[1].numerator, 3);
    assert_eq!(imported.markers.len(), 2);
    assert_eq!(imported.markers[0].name, "Marker");
    assert_eq!(imported.markers[1].name, "Cue");

    let merged = imported.tracks_for_options(MidiFileImportOptions::default());
    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].name, "Conductor");
    assert_eq!(merged[0].notes.len(), 1);
    assert!(merged[0].opaque_events.len() > imported.tracks[1].opaque_events.len());
    let expanded = imported.tracks_for_options(MidiFileImportOptions {
        expand_tracks: true,
        ..MidiFileImportOptions::default()
    });
    assert_eq!(expanded.len(), 2);
    assert_eq!(expanded[0].name, "Lead");
    assert_eq!(expanded[1].name, "Meta");
}

#[test]
fn expanded_type1_import_keeps_channel_events_in_the_first_source_track() {
    let first = [
        0x00, 0xFF, 0x03, 0x0D, b'F', b'i', b'r', b's', b't', b' ', b'C', b'h', b'a', b'n', b'n',
        b'e', b'l', 0x00, 0x90, 60, 100, 0x78, 0x80, 60, 64, 0x00, 0xFF, 0x2F, 0x00,
    ];
    let second = [
        0x00, 0xFF, 0x03, 0x04, b'L', b'e', b'a', b'd', 0x00, 0x91, 64, 96, 0x78, 0x81, 64, 32,
        0x00, 0xFF, 0x2F, 0x00,
    ];
    let imported = decode_standard_midi(&smf(1, 480, &[&first, &second]))
        .unwrap()
        .to_project_import(480)
        .unwrap();

    let expanded = imported.tracks_for_options(MidiFileImportOptions {
        expand_tracks: true,
        ..MidiFileImportOptions::default()
    });

    assert_eq!(expanded.len(), 2);
    assert_eq!(expanded[0].name, "First Channel");
    assert_eq!(expanded[0].notes.len(), 1);
    assert_eq!(expanded[0].notes[0].pitch, 60);
    assert_eq!(expanded[1].name, "Lead");
    assert_eq!(expanded[1].notes.len(), 1);
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
    assert_eq!(converted.unsupported_event_count, 0);
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
    assert_eq!(converted.tracks[0].opaque_events.len(), 3);

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
    assert_eq!(item.opaque_events().len(), 3);

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
                    source_index: 0,
                    has_channel_events: true,
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
                    opaque_events: Vec::new(),
                    event_order: Vec::new(),
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
