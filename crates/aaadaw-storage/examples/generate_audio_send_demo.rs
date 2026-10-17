use aaadaw_core::{DawAction, Project};
use aaadaw_storage::ProjectStore;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/audio-send-demo.aaadaw"));
    let mut project = Project::new();
    for name in ["Audio 1", "Audio 2", "Receiver"] {
        project.apply(DawAction::CreateTrack {
            index: project.tracks().len(),
            name: name.to_owned(),
        })?;
    }
    let receiver = project.tracks()[2].id();
    for source in [project.tracks()[0].id(), project.tracks()[1].id()] {
        project.apply(DawAction::CreateAudioSend {
            track_id: source,
            destination: receiver,
            parameters: Default::default(),
        })?;
    }
    let mut store = ProjectStore::open(&path)?;
    store.save(&project)?;
    store.close()?;
    println!("{}", path.display());
    Ok(())
}
