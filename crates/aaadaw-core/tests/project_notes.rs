use aaadaw_core::{DawAction, FrameRate, Project, ProjectNotes};

#[test]
fn notes_preserve_existing_media_undo_redo_and_snapshot_roundtrip() {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Original".into(),
        })
        .unwrap();
    assert!(project.undo().unwrap());
    let notes = ProjectNotes {
        title: "标题".into(),
        author: "作者".into(),
        text: "first\r\n\r\n最后一行\r\n".into(),
        show_on_load: true,
        word_wrap: false,
    };
    project
        .apply(DawAction::SetProjectNotes {
            notes: notes.clone(),
        })
        .unwrap();
    assert!(!project.can_undo());
    assert!(project.can_redo());
    assert!(project.redo().unwrap());
    assert_eq!(project.project_notes(), &notes);
    assert!(project.undo().unwrap());
    assert_eq!(project.project_notes(), &notes);
    assert_eq!(
        Project::from_snapshot(project.snapshot())
            .unwrap()
            .snapshot(),
        project.snapshot()
    );
}

#[test]
fn notes_are_atomic_in_batches_but_excluded_from_media_history() {
    let mut project = Project::new();
    let original = project.snapshot();
    let notes = ProjectNotes {
        title: "Document".into(),
        ..ProjectNotes::default()
    };
    assert!(
        project
            .apply(DawAction::BatchTransaction {
                tx_id: 1,
                actions: vec![
                    DawAction::SetProjectNotes {
                        notes: notes.clone()
                    },
                    DawAction::CreateTrack {
                        index: 10,
                        name: "Invalid".into()
                    }
                ]
            })
            .is_err()
    );
    assert_eq!(project.snapshot(), original);
    assert!(!project.can_undo());
    project
        .apply(DawAction::BatchTransaction {
            tx_id: 2,
            actions: vec![
                DawAction::SetProjectNotes {
                    notes: notes.clone(),
                },
                DawAction::SetFrameRate {
                    rate: FrameRate::Fps25,
                },
            ],
        })
        .unwrap();
    assert!(project.undo().unwrap());
    assert_eq!(project.settings().frame_rate(), FrameRate::Fps30);
    assert_eq!(project.project_notes(), &notes);
    project
        .apply(DawAction::SetProjectNotes {
            notes: ProjectNotes::default(),
        })
        .unwrap();
    assert!(project.redo().unwrap());
    assert_eq!(project.settings().frame_rate(), FrameRate::Fps25);
    assert_eq!(project.project_notes(), &ProjectNotes::default());
}
