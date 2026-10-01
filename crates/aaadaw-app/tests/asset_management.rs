use aaadaw_app::{
    AudioAssetManagementOperation, AudioAssetManagementProgress, AudioAssetManagementResult,
    relink_external_audio_source, start_audio_asset_management,
};
use aaadaw_storage::{ProjectStore, ResolvedAudioAsset};
use std::io::Read;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static NEXT_TEST_FILE: AtomicU64 = AtomicU64::new(0);

fn temp_path(suffix: &str) -> std::path::PathBuf {
    let id = NEXT_TEST_FILE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "aaadaw-app-assets-{}-{id}{suffix}",
        std::process::id()
    ))
}

fn remove_project_files(path: &std::path::Path) {
    let _ = std::fs::remove_file(path);
    for suffix in ["-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
    }
}

#[test]
fn source_scan_and_pack_run_through_the_application_worker() {
    let project_path = temp_path(".aaadaw");
    let source_path = temp_path(".wav");
    std::fs::write(&source_path, b"external audio bytes").expect("source file should exist");
    let mut store = ProjectStore::open(&project_path).expect("project should open");
    let media_ref = store
        .link_external_audio_file(&source_path)
        .expect("external source should link");
    let embedded_source_path = temp_path("-embedded.wav");
    std::fs::write(&embedded_source_path, b"embedded snapshot bytes")
        .expect("embedded source should exist");
    let embedded_ref = store
        .import_audio_file(&embedded_source_path)
        .expect("source should embed");
    store.close().expect("project should close");
    std::fs::remove_file(&embedded_source_path)
        .expect("original embedded source should be removed");
    let replacement_path = temp_path("-replacement.wav");
    std::fs::write(&replacement_path, b"replacement audio bytes")
        .expect("replacement source should exist");
    relink_external_audio_source(
        project_path.clone(),
        media_ref.clone(),
        replacement_path.clone(),
    )
    .expect("external link should relink");

    let scan = start_audio_asset_management(
        project_path.clone(),
        AudioAssetManagementOperation::ScanSources,
    )
    .expect("source scan should start");
    let mut scan_progress = Vec::new();
    while !scan.is_finished() {
        scan_progress.extend(scan.progress());
        std::thread::sleep(Duration::from_millis(1));
    }
    scan_progress.extend(scan.progress());
    let scan_result = scan.join().expect("source scan should finish");
    let results = match scan_result {
        AudioAssetManagementResult::SourceScan(results) => results,
        AudioAssetManagementResult::Packed(_) => Vec::new(),
    };
    assert_eq!(results.len(), 2);
    let linked_status = results
        .iter()
        .find(|entry| entry.media_ref == media_ref)
        .expect("live link status should be reported");
    assert_eq!(
        linked_status.status,
        aaadaw_storage::AudioAssetSourceStatus::Linked
    );
    assert!(linked_status.is_external_link);
    let embedded_status = results
        .iter()
        .find(|entry| entry.media_ref == embedded_ref)
        .expect("embedded source status should be reported");
    assert_eq!(
        embedded_status.status,
        aaadaw_storage::AudioAssetSourceStatus::Missing
    );
    assert!(!embedded_status.is_external_link);
    assert!(scan_progress.iter().any(|progress| matches!(
        progress,
        AudioAssetManagementProgress::SourceScan(update)
            if update.media_ref == media_ref
    )));

    let pack = start_audio_asset_management(
        project_path.clone(),
        AudioAssetManagementOperation::PackExternalAssets,
    )
    .expect("asset packing should start");
    while !pack.is_finished() {
        let _ = pack.progress();
        std::thread::sleep(Duration::from_millis(1));
    }
    let pack_result = pack.join().expect("asset packing should finish");
    assert!(matches!(
        pack_result,
        AudioAssetManagementResult::Packed(ref refs) if refs == &vec![media_ref.clone()]
    ));

    let store = ProjectStore::open(&project_path).expect("project should reopen");
    match store
        .resolve_audio_asset(&media_ref)
        .expect("packed asset should resolve")
    {
        ResolvedAudioAsset::Embedded(mut reader) => {
            let mut bytes = Vec::new();
            reader
                .read_to_end(&mut bytes)
                .expect("asset bytes should read");
            assert_eq!(bytes, b"replacement audio bytes");
        }
        ResolvedAudioAsset::LinkedFile { .. } => panic!("packed asset should be embedded"),
    }
    store.close().expect("project should close");
    remove_project_files(&project_path);
    let _ = std::fs::remove_file(source_path);
    let _ = std::fs::remove_file(replacement_path);
    let _ = std::fs::remove_file(embedded_source_path);
}
