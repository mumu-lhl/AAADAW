use aaadaw_core::{DawAction, Project};

#[test]
fn tracks_can_be_renamed_reordered_and_undone() {
    let mut project = Project::new();
    for (index, name) in [(0, "Audio"), (1, "MIDI")] {
        project
            .apply(DawAction::CreateTrack {
                index,
                name: name.to_owned(),
            })
            .expect("creating tracks should succeed");
    }
    let audio_id = project.tracks()[0].id();

    project
        .apply(DawAction::SetTrackName {
            track_id: audio_id,
            name: "Drums".to_owned(),
        })
        .expect("renaming an existing track should succeed");
    project
        .apply(DawAction::MoveTrack {
            track_id: audio_id,
            index: 1,
        })
        .expect("moving a track should succeed");

    assert_eq!(project.tracks()[0].name(), "MIDI");
    assert_eq!(project.tracks()[1].name(), "Drums");

    assert!(project.undo().expect("undoing a move should succeed"));
    assert_eq!(project.tracks()[0].name(), "Drums");
    assert!(project.undo().expect("undoing a rename should succeed"));
    assert_eq!(project.tracks()[0].name(), "Audio");
}
