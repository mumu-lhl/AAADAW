use aaadaw_core::{DawAction, Project};

#[test]
fn deleting_a_track_removes_it_from_the_project() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Bass".to_owned(),
        })
        .expect("creating the first track should succeed");
    project
        .apply(DawAction::CreateTrack {
            index: 1,
            name: "Drums".to_owned(),
        })
        .expect("creating the second track should succeed");
    let drums_id = project.tracks()[1].id();

    project
        .apply(DawAction::DeleteTrack {
            track_id: project.tracks()[0].id(),
        })
        .expect("deleting an existing track should succeed");

    assert_eq!(project.tracks().len(), 1);
    assert_eq!(project.tracks()[0].id(), drums_id);
    assert_eq!(project.tracks()[0].name(), "Drums");
}
