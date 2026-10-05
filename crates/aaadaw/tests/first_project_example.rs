use aaadaw_media::AudioStreamDecoder;
use aaadaw_storage::{ProjectStore, ResolvedAudioAsset};
use std::path::PathBuf;

const MEDIA_REF: &str = "asset://first-project/melody.wav";

#[test]
fn bundled_first_project_opens_with_decodable_embedded_audio() {
    let project_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/first-project/first-project.aaadaw");
    let store = ProjectStore::open(&project_path).expect("example project should open");
    let project = store.load().expect("example project should load");

    assert_eq!(project.tracks().len(), 1);
    assert_eq!(project.audio_items().len(), 1);
    assert_eq!(project.tracks()[0].name(), "Audio");
    let item = &project.audio_items()[0];
    assert_eq!(item.media_ref(), MEDIA_REF);
    assert_eq!(item.start_sample(), 0);
    assert_eq!(item.length_samples(), 192_000);

    let ResolvedAudioAsset::Embedded(reader) = store
        .resolve_audio_asset(MEDIA_REF)
        .expect("example audio should be embedded")
    else {
        panic!("example audio should not depend on an external file path");
    };
    let byte_len = reader.byte_len();
    let mut decoder = AudioStreamDecoder::from_reader(reader, Some(byte_len), Some("wav"))
        .expect("embedded example audio should decode");
    assert_eq!(decoder.metadata().sample_rate, Some(48_000));
    assert_eq!(decoder.metadata().channel_count, Some(1));
    let chunk = decoder
        .next_chunk()
        .expect("example audio should decode without errors")
        .expect("example audio should contain frames");
    assert!(chunk.frame_count() > 0);
    assert!(chunk.samples().iter().any(|sample| *sample != 0.0));
}
