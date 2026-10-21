use aaadaw_core::{ActionError, DawAction, Project};

#[test]
fn regions_are_ordered_by_start_and_insert_update_delete_are_undoable() {
    let mut project = Project::new();
    project
        .apply(DawAction::InsertProjectRegion {
            start_ticks: 3_840,
            end_ticks: 7_680,
            name: "Verse".to_owned(),
        })
        .unwrap();
    let verse_id = project.regions()[0].id();
    assert_eq!(project.regions()[0].number(), 1);

    project
        .apply(DawAction::InsertProjectRegion {
            start_ticks: 0,
            end_ticks: 1_920,
            name: "Intro".to_owned(),
        })
        .unwrap();
    let intro_id = project.regions()[0].id();
    assert_eq!(project.regions()[0].id(), intro_id);
    assert_eq!(project.regions()[0].number(), 2);
    assert_eq!(project.regions()[1].id(), verse_id);
    assert_eq!(project.regions()[1].number(), 1);

    project
        .apply(DawAction::UpdateProjectRegion {
            region_id: verse_id,
            number: 5,
            start_ticks: 1_920,
            end_ticks: 5_760,
            name: "Chorus".to_owned(),
        })
        .unwrap();
    let chorus = project
        .regions()
        .iter()
        .find(|region| region.id() == verse_id)
        .unwrap();
    assert_eq!(chorus.number(), 5);
    assert_eq!(chorus.start_ticks(), 1_920);
    assert_eq!(chorus.end_ticks(), 5_760);
    assert_eq!(chorus.name(), "Chorus");

    assert!(project.undo().unwrap());
    let verse = project
        .regions()
        .iter()
        .find(|region| region.id() == verse_id)
        .unwrap();
    assert_eq!(verse.number(), 1);
    assert_eq!(verse.start_ticks(), 3_840);
    assert_eq!(verse.end_ticks(), 7_680);
    assert_eq!(verse.name(), "Verse");
    assert!(project.redo().unwrap());
    assert!(project.undo().unwrap());
    assert!(project.undo().unwrap());
    assert_eq!(project.regions().len(), 1);
    assert_eq!(project.regions()[0].id(), verse_id);
    assert!(project.redo().unwrap());
    assert_eq!(project.regions().len(), 2);

    let restored = Project::from_snapshot(project.snapshot()).unwrap();
    assert_eq!(restored.regions(), project.regions());
}

#[test]
fn invalid_regions_do_not_change_project_or_history() {
    let mut project = Project::new();
    for (start_ticks, end_ticks, name) in [
        (0, 0, "empty"),
        (1_920, 960, "reversed"),
        (0, u64::MAX, "too far"),
        (0, 960, "invalid\0name"),
    ] {
        assert!(matches!(
            project.apply(DawAction::InsertProjectRegion {
                start_ticks,
                end_ticks,
                name: name.to_owned(),
            }),
            Err(ActionError::InvalidProjectRegion)
        ));
    }
    assert!(project.regions().is_empty());
    assert!(!project.can_undo());

    project
        .apply(DawAction::InsertProjectRegion {
            start_ticks: 0,
            end_ticks: 960,
            name: "valid".to_owned(),
        })
        .unwrap();
    let region_id = project.regions()[0].id();
    let before = project.regions().to_vec();
    assert_eq!(
        project.apply(DawAction::UpdateProjectRegion {
            region_id,
            number: 1,
            start_ticks: 0,
            end_ticks: 0,
            name: String::new(),
        }),
        Err(ActionError::InvalidProjectRegion)
    );
    assert_eq!(project.regions(), before);
}

#[test]
fn regions_allow_duplicate_visible_numbers_and_auto_insert_lowest_available() {
    let mut project = Project::new();
    for start_ticks in [0, 1_920, 3_840] {
        project
            .apply(DawAction::InsertProjectRegion {
                start_ticks,
                end_ticks: start_ticks + 960,
                name: String::new(),
            })
            .unwrap();
    }
    let first_id = project.regions()[0].id();
    let third_id = project.regions()[2].id();
    project
        .apply(DawAction::UpdateProjectRegion {
            region_id: first_id,
            number: 5,
            start_ticks: 0,
            end_ticks: 960,
            name: String::new(),
        })
        .unwrap();
    project
        .apply(DawAction::UpdateProjectRegion {
            region_id: third_id,
            number: 5,
            start_ticks: 3_840,
            end_ticks: 4_800,
            name: String::new(),
        })
        .unwrap();
    assert_eq!(project.regions()[0].number(), 5);
    assert_eq!(project.regions()[2].number(), 5);

    project
        .apply(DawAction::DeleteProjectRegion {
            region_id: project.regions()[1].id(),
        })
        .unwrap();
    project
        .apply(DawAction::InsertProjectRegion {
            start_ticks: 5_760,
            end_ticks: 6_720,
            name: String::new(),
        })
        .unwrap();
    let last = project.regions().last().unwrap();
    assert_eq!(last.number(), 1);
}

#[test]
fn region_render_matrix_is_ordered_snapshot_safe_and_track_delete_undoable() {
    let mut project = Project::new();
    for (index, name) in [(0, "Drums"), (1, "Bass"), (2, "Guitar")] {
        project
            .apply(DawAction::CreateTrack {
                index,
                name: name.to_owned(),
            })
            .unwrap();
    }
    let drums = project.tracks()[0].id();
    let guitar = project.tracks()[2].id();
    project
        .apply(DawAction::InsertProjectRegion {
            start_ticks: 0,
            end_ticks: 1_920,
            name: "Intro".to_owned(),
        })
        .unwrap();
    let region_id = project.regions()[0].id();

    project
        .apply(DawAction::SetProjectRegionRenderMatrix {
            region_id,
            render_master_mix: true,
            render_track_ids: vec![guitar, drums],
        })
        .unwrap();
    let region = &project.regions()[0];
    assert!(region.renders_master_mix());
    assert_eq!(region.render_track_ids(), &[drums, guitar]);
    assert!(project.undo().unwrap());
    assert!(!project.regions()[0].renders_master_mix());
    assert!(project.regions()[0].render_track_ids().is_empty());
    assert!(project.redo().unwrap());

    project
        .apply(DawAction::UpdateProjectRegion {
            region_id,
            number: 1,
            start_ticks: 0,
            end_ticks: 2_880,
            name: "Intro updated".to_owned(),
        })
        .unwrap();
    assert_eq!(project.regions()[0].render_track_ids(), &[drums, guitar]);
    assert_eq!(project.regions()[0].name(), "Intro updated");
    assert!(project.undo().unwrap());
    assert_eq!(project.regions()[0].render_track_ids(), &[drums, guitar]);

    project
        .apply(DawAction::DeleteTrack { track_id: drums })
        .unwrap();
    assert_eq!(project.regions()[0].render_track_ids(), &[guitar]);
    assert!(project.undo().unwrap());
    assert_eq!(project.regions()[0].render_track_ids(), &[drums, guitar]);
    assert!(project.redo().unwrap());
    assert_eq!(project.regions()[0].render_track_ids(), &[guitar]);

    let restored = Project::from_snapshot(project.snapshot()).unwrap();
    assert_eq!(restored.regions(), project.regions());
    assert_eq!(restored.tracks(), project.tracks());
}

#[test]
fn invalid_region_render_matrix_assignments_are_atomic() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Drums".to_owned(),
        })
        .unwrap();
    project
        .apply(DawAction::InsertProjectRegion {
            start_ticks: 0,
            end_ticks: 960,
            name: "Intro".to_owned(),
        })
        .unwrap();
    let track_id = project.tracks()[0].id();
    let region_id = project.regions()[0].id();
    let before = project.regions().to_vec();

    assert_eq!(
        project.apply(DawAction::SetProjectRegionRenderMatrix {
            region_id,
            render_master_mix: true,
            render_track_ids: vec![track_id, track_id],
        }),
        Err(ActionError::InvalidProjectRegionRenderMatrix)
    );
    assert_eq!(project.regions(), before);
    assert!(project.undo().unwrap());
    assert!(project.regions().is_empty());
    assert!(project.redo().unwrap());

    let missing_track = aaadaw_core::TrackId::from_value(track_id.value() + 10).unwrap();
    assert!(matches!(
        project.apply(DawAction::SetProjectRegionRenderMatrix {
            region_id,
            render_master_mix: false,
            render_track_ids: vec![missing_track],
        }),
        Err(ActionError::TrackNotFound { track_id }) if track_id == missing_track
    ));
    assert_eq!(project.regions(), before);
    assert!(project.undo().unwrap());
    assert!(project.regions().is_empty());
}
