use aaadaw_core::{DawAction, Project};

#[test]
fn create_track_adds_it_to_the_project() {
    let mut project = Project::new();

    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Drums".to_owned(),
        })
        .expect("creating the first track should succeed");

    let track = project
        .tracks()
        .first()
        .expect("the created track should be queryable");
    assert_eq!(track.name(), "Drums");
}
