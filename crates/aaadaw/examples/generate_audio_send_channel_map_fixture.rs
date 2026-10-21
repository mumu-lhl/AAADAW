use aaadaw_core::{AudioSendParameters, AudioSendSourceChannels, DawAction, Project};
use aaadaw_storage::{ArrangementViewState, ProjectStore};
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("expected an output path")?,
    );
    let mut project = Project::new();
    for (index, name) in ["Audio Source", "Audio Receiver"].into_iter().enumerate() {
        project.apply(DawAction::CreateTrack {
            index,
            name: name.to_owned(),
        })?;
    }
    project.apply(DawAction::CreateAudioSend {
        track_id: project.tracks()[0].id(),
        destination: project.tracks()[1].id(),
        parameters: AudioSendParameters {
            source_channels: AudioSendSourceChannels::Channel1,
            ..Default::default()
        },
    })?;

    let mut store = ProjectStore::open(&path)?;
    store.save_with_arrangement_view_state(&project, &ArrangementViewState::default())?;
    store.close()?;
    println!("Wrote {}", path.display());
    Ok(())
}
