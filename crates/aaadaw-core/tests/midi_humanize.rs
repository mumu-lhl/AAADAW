use aaadaw_core::{ActionError, DawAction, GridFraction, MidiNoteData, Project};

fn project_with_midi_notes() -> (Project, aaadaw_core::ItemId) {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Piano".to_owned(),
        })
        .expect("test track should be created");
    let track_id = project.tracks()[0].id();
    project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 0,
            length_ticks: 3_840,
        })
        .expect("test MIDI item should be inserted");
    let item_id = project.midi_items()[0].id();
    project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![
                MidiNoteData {
                    channel: 0,
                    pitch: 60,
                    tick: 170,
                    duration: 90,
                    velocity: 70,
                },
                MidiNoteData {
                    channel: 2,
                    pitch: 64,
                    tick: 350,
                    duration: 110,
                    velocity: 80,
                },
                MidiNoteData {
                    channel: 0,
                    pitch: 67,
                    tick: 550,
                    duration: 150,
                    velocity: 90,
                },
            ],
        })
        .expect("test MIDI notes should be inserted");
    (project, item_id)
}

fn project_with_humanize_notes(
    note_count: usize,
    velocities: &[u8],
) -> (Project, aaadaw_core::ItemId) {
    assert!(!velocities.is_empty());
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Humanize probe".to_owned(),
        })
        .expect("test track should be created");
    let track_id = project.tracks()[0].id();
    project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 0,
            length_ticks: (note_count as u64) * 1_000 + 4_000,
        })
        .expect("test MIDI item should be inserted");
    let item_id = project.midi_items()[0].id();
    project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: (0..note_count)
                .map(|index| MidiNoteData {
                    channel: (index % 16) as u8,
                    pitch: 60 + (index % 60) as u8,
                    tick: 2_000 + (index as u64) * 1_000,
                    duration: 90,
                    velocity: velocities[index % velocities.len()],
                })
                .collect(),
        })
        .expect("test MIDI notes should be inserted");
    (project, item_id)
}

#[test]
fn humanize_moves_only_selected_notes_within_the_native_timing_range_and_is_replayable() {
    let (mut project, item_id) = project_with_midi_notes();
    let original = project.midi_items()[0]
        .notes()
        .iter()
        .map(|note| (note.tick(), note.duration(), note.velocity()))
        .collect::<Vec<_>>();
    let note_ids = project.midi_items()[0].notes()[..2]
        .iter()
        .map(|note| note.id())
        .collect::<Vec<_>>();
    let action = DawAction::HumanizeMidiNotes {
        item_id,
        note_ids,
        grid: GridFraction::new(1, 32).expect("1/32 is a valid grid"),
        timing_amount_percent: 100,
        velocity_amount_percent: 100,
        timing_bias_percent: 0,
        seed: 0x43b0_1a52_74c9_203f,
    };

    project
        .apply(action.clone())
        .expect("humanizing valid notes should succeed");
    let after = project.midi_items()[0]
        .notes()
        .iter()
        .map(|note| (note.tick(), note.duration(), note.velocity()))
        .collect::<Vec<_>>();
    assert_ne!(after, original);
    for index in 0..2 {
        assert!(original[index].0.abs_diff(after[index].0) <= 480);
        assert_eq!(original[index].1, after[index].1);
        assert!((1..=127).contains(&after[index].2));
    }
    assert_eq!(after[2], original[2]);

    assert!(project.undo().expect("humanize should undo"));
    assert_eq!(
        project.midi_items()[0]
            .notes()
            .iter()
            .map(|note| (note.tick(), note.duration(), note.velocity()))
            .collect::<Vec<_>>(),
        original
    );
    project
        .apply(action)
        .expect("the same seed should replay the same edit");
    assert_eq!(
        project.midi_items()[0]
            .notes()
            .iter()
            .map(|note| (note.tick(), note.duration(), note.velocity()))
            .collect::<Vec<_>>(),
        after
    );
    assert!(project.undo().expect("humanize should undo again"));
    assert!(project.redo().expect("humanize should redo"));
    assert_eq!(
        project.midi_items()[0]
            .notes()
            .iter()
            .map(|note| (note.tick(), note.duration(), note.velocity()))
            .collect::<Vec<_>>(),
        after
    );
}

#[test]
fn humanize_rejects_values_outside_native_dialog_ranges_without_editing() {
    let (mut project, item_id) = project_with_midi_notes();
    let note_id = project.midi_items()[0].notes()[0].id();
    let before = project.midi_items()[0].notes()[0].clone();
    let history_cursor = project.history_cursor();
    let result = project.apply(DawAction::HumanizeMidiNotes {
        item_id,
        note_ids: vec![note_id],
        grid: GridFraction::new(1, 32).expect("1/32 is a valid grid"),
        timing_amount_percent: 101,
        velocity_amount_percent: 0,
        timing_bias_percent: 0,
        seed: 1,
    });

    assert_eq!(result, Err(ActionError::InvalidMidiHumanizeSettings));
    assert_eq!(project.midi_items()[0].notes()[0], before);
    assert_eq!(project.history_cursor(), history_cursor);
}

#[test]
fn humanize_timing_bias_uses_a_wider_range_than_the_grid_only_jitter() {
    let (mut project, item_id) = project_with_midi_notes();
    let note_ids = project.midi_items()[0]
        .notes()
        .iter()
        .map(|note| note.id())
        .collect::<Vec<_>>();
    let original_ticks = project.midi_items()[0]
        .notes()
        .iter()
        .map(|note| note.tick())
        .collect::<Vec<_>>();
    project
        .apply(DawAction::HumanizeMidiNotes {
            item_id,
            note_ids,
            grid: GridFraction::new(1, 32).expect("1/32 is a valid grid"),
            timing_amount_percent: 100,
            velocity_amount_percent: 0,
            timing_bias_percent: 100,
            seed: 0x43b0_1a52_74c9_203f,
        })
        .expect("biased humanize should succeed");
    let notes = project.midi_items()[0].notes();
    assert!(
        notes
            .iter()
            .zip(&original_ticks)
            .any(|(note, tick)| note.tick().abs_diff(*tick) > 60)
    );
    assert!(
        notes
            .iter()
            .zip(&original_ticks)
            .all(|(note, tick)| { note.tick().abs_diff(*tick) <= 480 && note.velocity() > 0 })
    );
}

#[test]
fn humanize_timing_is_independent_of_the_midi_editor_grid() {
    let (mut fine_grid_project, fine_grid_item) = project_with_midi_notes();
    let (mut coarse_grid_project, coarse_grid_item) = project_with_midi_notes();
    let fine_grid_notes = fine_grid_project.midi_items()[0]
        .notes()
        .iter()
        .map(|note| note.id())
        .collect::<Vec<_>>();
    let coarse_grid_notes = coarse_grid_project.midi_items()[0]
        .notes()
        .iter()
        .map(|note| note.id())
        .collect::<Vec<_>>();
    let seed = 0x7041_74a9_c5b2_31df;

    for (project, item_id, note_ids, grid) in [
        (
            &mut fine_grid_project,
            fine_grid_item,
            fine_grid_notes,
            GridFraction::new(1, 32).expect("1/32 is a valid grid"),
        ),
        (
            &mut coarse_grid_project,
            coarse_grid_item,
            coarse_grid_notes,
            GridFraction::new(1, 4).expect("1/4 is a valid grid"),
        ),
    ] {
        project
            .apply(DawAction::HumanizeMidiNotes {
                item_id,
                note_ids,
                grid,
                timing_amount_percent: 100,
                velocity_amount_percent: 0,
                timing_bias_percent: 0,
                seed,
            })
            .expect("humanize should succeed on either editor grid");
    }

    let fine_ticks = fine_grid_project.midi_items()[0]
        .notes()
        .iter()
        .map(|note| note.tick())
        .collect::<Vec<_>>();
    let coarse_ticks = coarse_grid_project.midi_items()[0]
        .notes()
        .iter()
        .map(|note| note.tick())
        .collect::<Vec<_>>();
    assert_eq!(fine_ticks, coarse_ticks);
}

#[test]
fn humanize_timing_uses_the_measured_zero_bias_scale_and_bias_shift() {
    const NOTE_COUNT: usize = 64;
    let projects = [-100, 0, 100].map(|bias| {
        let (mut project, item_id) = project_with_humanize_notes(NOTE_COUNT, &[64]);
        let note_ids = project.midi_items()[0]
            .notes()
            .iter()
            .map(|note| note.id())
            .collect::<Vec<_>>();
        project
            .apply(DawAction::HumanizeMidiNotes {
                item_id,
                note_ids,
                grid: GridFraction::new(1, 32).expect("1/32 is a valid grid"),
                timing_amount_percent: 100,
                velocity_amount_percent: 0,
                timing_bias_percent: bias,
                seed: 0x51a8_7d32_e4c9_0b6f,
            })
            .expect("humanize should succeed for every timing bias");
        project.midi_items()[0]
            .notes()
            .iter()
            .map(|note| note.tick())
            .zip((0..NOTE_COUNT).map(|index| 2_000 + (index as u64) * 1_000))
            .map(|(after, before)| after as i64 - before as i64)
            .collect::<Vec<_>>()
    });
    let mean = |offsets: &[i64]| {
        offsets.iter().map(|offset| *offset as f64).sum::<f64>() / offsets.len() as f64
    };
    let standard_deviation = |offsets: &[i64]| {
        let average = mean(offsets);
        (offsets
            .iter()
            .map(|offset| (*offset as f64 - average).powi(2))
            .sum::<f64>()
            / offsets.len() as f64)
            .sqrt()
    };

    // Startup-preloaded native samples at ±50% and ±100% support this signed
    // PPQ/8 mean shift; the exact native random sequence remains open.
    assert!(mean(&projects[1]).abs() <= 30.0);
    assert!((-150.0..=-90.0).contains(&mean(&projects[0])));
    assert!((90.0..=150.0).contains(&mean(&projects[2])));
    for offsets in &projects {
        let spread = standard_deviation(offsets);
        assert!(
            (100.0..=145.0).contains(&spread),
            "unexpected spread: {spread}"
        );
        assert!(offsets.iter().all(|offset| offset.abs() <= 480));
    }
}

#[test]
fn humanize_velocity_uses_measured_normal_scale_and_midi_velocity_boundaries() {
    const NOTE_COUNT: usize = 512;
    let velocities = [1, 64, 127];
    let seed = 0x72d4_9a13_6ef0_5b28;
    let sample = |amount| {
        let (mut project, item_id) = project_with_humanize_notes(NOTE_COUNT, &velocities);
        let original = project.midi_items()[0]
            .notes()
            .iter()
            .map(|note| (note.id(), note.velocity(), note.tick(), note.duration()))
            .collect::<Vec<_>>();
        project
            .apply(DawAction::HumanizeMidiNotes {
                item_id,
                note_ids: original.iter().map(|note| note.0).collect(),
                grid: GridFraction::new(1, 32).expect("1/32 is a valid grid"),
                timing_amount_percent: 0,
                velocity_amount_percent: amount,
                timing_bias_percent: 0,
                seed,
            })
            .expect("humanize should succeed for valid note velocities");
        project.midi_items()[0]
            .notes()
            .iter()
            .zip(original)
            .map(|(after, (_, velocity, tick, duration))| {
                assert_eq!(after.tick(), tick);
                assert_eq!(after.duration(), duration);
                assert!((1..=127).contains(&after.velocity()));
                (velocity, i16::from(after.velocity()) - i16::from(velocity))
            })
            .collect::<Vec<_>>()
    };
    let full = sample(100);
    let half = sample(50);
    let zero = sample(0);

    let summarize = |samples: &[(u8, i16)], velocity| {
        let offsets = samples
            .iter()
            .filter_map(|(initial, offset)| (*initial == velocity).then_some(f64::from(*offset)))
            .collect::<Vec<_>>();
        let average = offsets.iter().sum::<f64>() / offsets.len() as f64;
        let spread = (offsets
            .iter()
            .map(|offset| (offset - average).powi(2))
            .sum::<f64>()
            / offsets.len() as f64)
            .sqrt();
        (offsets, average, spread)
    };
    let (full_middle, full_mean, full_spread) = summarize(&full, 64);
    let (_, half_mean, half_spread) = summarize(&half, 64);
    let (upper_boundary, _, _) = summarize(&full, 127);

    assert!(full_mean.abs() <= 3.0, "unexpected 100% mean: {full_mean}");
    assert!(
        (12.0..=18.0).contains(&full_spread),
        "unexpected 100% velocity spread: {full_spread}"
    );
    assert!(half_mean.abs() <= 3.0, "unexpected 50% mean: {half_mean}");
    assert!(
        (6.0..=10.0).contains(&half_spread),
        "unexpected 50% velocity spread: {half_spread}"
    );
    assert!((0.42..=0.58).contains(&(half_spread / full_spread)));
    assert!(
        upper_boundary
            .iter()
            .filter(|offset| **offset == 0.0)
            .count()
            >= 60
    );
    assert!(full_middle.iter().any(|offset| *offset != 0.0));
    assert!(
        zero.iter()
            .all(|(velocity, offset)| *velocity != 64 || *offset == 0)
    );
}
