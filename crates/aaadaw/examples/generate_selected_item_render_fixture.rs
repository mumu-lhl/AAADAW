use aaadaw_app::{WavExport, WavExportOptions};
use aaadaw_core::{DawAction, Project};
use aaadaw_storage::ProjectStore;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let project_path = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/selected-item-render-fixture.aaadaw"));
    let source_path = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| project_path.with_extension("source.wav"));

    let mut source = WavExport::create(&source_path, 48_000, WavExportOptions::default())?;
    source.write_frames(&vec![[0.125, 0.375]; 48_000])?;
    source.finish()?;

    let mut project = Project::new();
    project.apply(DawAction::CreateTrack {
        index: 0,
        name: "Render fixture".to_owned(),
    })?;
    let track_id = project.tracks()[0].id();
    let mut store = ProjectStore::open(&project_path)?;
    let media_ref = store.link_external_audio_file(&source_path)?;
    for start_sample in [0, 96_000, 192_000] {
        project.apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: media_ref.clone(),
            start_sample,
            source_offset_samples: 0,
            length_samples: 48_000,
        })?;
    }
    store.save(&project)?;
    store.close()?;
    println!("{}", project_path.display());
    Ok(())
}
