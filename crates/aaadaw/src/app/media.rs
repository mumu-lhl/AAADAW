use super::{
    App, Message, PathPickerTarget, PendingAudioImport, SharedAudioAssetManagementWorker,
    SharedAudioImportWorker, project_path_from_query, run_blocking,
};
use aaadaw_app::{
    AudioAssetManagementOperation, AudioAssetManagementProgress, AudioAssetManagementResult,
    AudioAssetSourceStatus, AudioAssetSourceStatusEntry, AudioItemImportProgress,
    relink_external_audio_source, start_audio_asset_management as start_asset_worker,
    start_audio_item_import,
};
use aaadaw_core::{DawAction, ItemId, Track};
use iced::Task;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

impl App {
    pub(super) fn pick_path(&mut self, target: PathPickerTarget) -> Task<Message> {
        if self.path_picker_busy {
            self.status = "A file dialog is already open".to_owned();
            return Task::none();
        }
        self.path_picker_busy = true;
        self.active_menu = None;
        Task::perform(
            run_blocking("aaadaw-native-file-dialog", move || {
                let path = match target {
                    PathPickerTarget::OpenProject => rfd::FileDialog::new()
                        .set_title("Open AAADAW project")
                        .add_filter("AAADAW project", &["aaadaw"])
                        .pick_file(),
                    PathPickerTarget::SaveProject => rfd::FileDialog::new()
                        .set_title("Save AAADAW project")
                        .set_file_name("project.aaadaw")
                        .add_filter("AAADAW project", &["aaadaw"])
                        .save_file(),
                    PathPickerTarget::ImportAudio => rfd::FileDialog::new()
                        .set_title("Choose audio to import")
                        .add_filter(
                            "Audio files",
                            &["wav", "flac", "mp3", "ogg", "aif", "aiff", "m4a"],
                        )
                        .pick_file(),
                    PathPickerTarget::RelinkAudio => rfd::FileDialog::new()
                        .set_title("Choose replacement audio")
                        .add_filter(
                            "Audio files",
                            &["wav", "flac", "mp3", "ogg", "aif", "aiff", "m4a"],
                        )
                        .pick_file(),
                };
                Ok(path)
            }),
            move |result| Message::PathPicked(target, result),
        )
    }

    pub(super) fn path_picked(
        &mut self,
        target: PathPickerTarget,
        result: Result<Option<PathBuf>, String>,
    ) -> Task<Message> {
        self.path_picker_busy = false;
        match result {
            Ok(Some(path)) => {
                let path = path.to_string_lossy().into_owned();
                match target {
                    PathPickerTarget::OpenProject => {
                        self.project_path_query = path;
                        self.active_menu = None;
                        self.open_project()
                    }
                    PathPickerTarget::SaveProject => {
                        self.project_path_query = path;
                        self.active_menu = None;
                        self.save_project()
                    }
                    PathPickerTarget::ImportAudio => {
                        self.audio_file_path_query = path;
                        Task::none()
                    }
                    PathPickerTarget::RelinkAudio => {
                        self.relink_source_path_query = path;
                        Task::none()
                    }
                }
            }
            Ok(None) => Task::none(),
            Err(error) => {
                self.status = format!("File dialog failed: {error}");
                Task::none()
            }
        }
    }

    pub(super) fn start_audio_import(&mut self) -> Task<Message> {
        if self.import_busy || self.io_busy {
            self.status = "Wait for the current project operation to finish".to_owned();
            return Task::none();
        }
        #[cfg(feature = "jack-backend")]
        if self.playback.is_some() {
            self.status = "Close JACK output before importing audio".to_owned();
            return Task::none();
        }
        let Some(project_path) = self.project_path.clone() else {
            self.status = "Save the project before importing audio".to_owned();
            return Task::none();
        };
        let Some(track_id) = self.project.tracks().first().map(Track::id) else {
            self.status = "Add a track before importing audio".to_owned();
            return Task::none();
        };
        let Some(source_path) = project_path_from_query(&self.audio_file_path_query) else {
            self.status = "Enter an audio file path first".to_owned();
            return Task::none();
        };
        let start_sample = self
            .project
            .audio_items()
            .iter()
            .filter(|item| item.track_id() == track_id)
            .filter_map(|item| item.start_sample().checked_add(item.length_samples()))
            .max()
            .unwrap_or(0);
        let project_sample_rate = self.project.settings().sample_rate();

        self.import_busy = true;
        self.import_finalizing = false;
        self.import_cancel_requested = false;
        self.import_bytes = 0;
        self.import_total_bytes = None;
        self.status = format!("Starting import of {}…", source_path.display());
        Task::perform(
            run_blocking("aaadaw-audio-import-start", move || {
                start_audio_item_import(
                    project_path,
                    source_path,
                    track_id,
                    start_sample,
                    project_sample_rate,
                )
                .map_err(|error| error.to_string())
            }),
            move |result| {
                Message::AudioImportStarted(SharedAudioImportWorker(Arc::new(Mutex::new(Some(
                    result,
                )))))
            },
        )
    }

    pub(super) fn audio_import_started(&mut self, worker: SharedAudioImportWorker) {
        let worker = worker.0.lock().ok().and_then(|mut result| result.take());
        match worker {
            Some(Ok(worker)) => {
                if self.import_cancel_requested {
                    worker.cancel();
                    self.status = "Cancelling audio import…".to_owned();
                } else {
                    self.status = "Importing audio into the project…".to_owned();
                }
                self.import_worker = Some(PendingAudioImport { worker });
            }
            Some(Err(error)) => {
                self.import_busy = false;
                self.import_finalizing = false;
                self.import_cancel_requested = false;
                self.status = format!("Audio import could not start: {error}");
            }
            None => {
                self.import_busy = false;
                self.import_finalizing = false;
                self.import_cancel_requested = false;
                self.status = "Audio import worker result was unavailable".to_owned();
            }
        }
    }

    pub(super) fn cancel_audio_import(&mut self) {
        if !self.import_busy {
            self.status = "No audio import is active".to_owned();
            return;
        }
        if self.import_finalizing {
            self.status = "Audio bytes are committed; finishing item placement".to_owned();
            return;
        }
        self.import_cancel_requested = true;
        if let Some(import) = self.import_worker.as_ref() {
            import.worker.cancel();
        }
        self.status = "Cancelling audio import…".to_owned();
    }

    pub(super) fn start_audio_asset_management(
        &mut self,
        operation: AudioAssetManagementOperation,
    ) -> Task<Message> {
        if self.io_busy || self.import_busy || self.audio_asset_management_busy {
            self.status = "Wait for the current project operation to finish".to_owned();
            return Task::none();
        }
        #[cfg(feature = "jack-backend")]
        if self.playback.is_some() {
            self.status = "Close JACK output before maintaining audio assets".to_owned();
            return Task::none();
        }
        if self.is_dirty() {
            self.status = "Save the project before scanning or packing audio assets".to_owned();
            return Task::none();
        }
        let Some(project_path) = self.project_path.clone() else {
            self.status = "Save the project before scanning or packing audio assets".to_owned();
            return Task::none();
        };
        self.audio_asset_management_busy = true;
        self.audio_asset_management_finalizing = false;
        self.audio_asset_management_cancel_requested = false;
        self.audio_asset_management_operation = Some(operation);
        if operation == AudioAssetManagementOperation::ScanSources {
            self.audio_asset_source_statuses.clear();
        }
        self.audio_asset_management_status = match operation {
            AudioAssetManagementOperation::ScanSources => "Starting source scan…".to_owned(),
            AudioAssetManagementOperation::PackExternalAssets => {
                "Starting asset packing…".to_owned()
            }
        };
        Task::perform(
            run_blocking("aaadaw-audio-asset-operation-start", move || {
                start_asset_worker(project_path, operation)
            }),
            move |result| {
                Message::AudioAssetManagementStarted(SharedAudioAssetManagementWorker(Arc::new(
                    Mutex::new(Some(result)),
                )))
            },
        )
    }

    pub(super) fn audio_asset_management_started(
        &mut self,
        result: SharedAudioAssetManagementWorker,
    ) {
        let worker = result.0.lock().ok().and_then(|mut result| result.take());
        match worker {
            Some(Ok(worker)) => {
                if self.audio_asset_management_cancel_requested {
                    worker.cancel();
                    self.audio_asset_management_status = "Cancelling asset operation…".to_owned();
                }
                self.audio_asset_management_worker = Some(worker);
            }
            Some(Err(error)) => {
                self.audio_asset_management_busy = false;
                self.audio_asset_management_finalizing = false;
                self.audio_asset_management_cancel_requested = false;
                self.audio_asset_management_operation = None;
                self.audio_asset_management_status =
                    format!("Could not start asset operation: {error}");
                self.status = self.audio_asset_management_status.clone();
            }
            None => {
                self.audio_asset_management_busy = false;
                self.audio_asset_management_finalizing = false;
                self.audio_asset_management_cancel_requested = false;
                self.audio_asset_management_operation = None;
                self.audio_asset_management_status =
                    "Asset worker result was unavailable".to_owned();
                self.status = self.audio_asset_management_status.clone();
            }
        }
    }

    pub(super) fn cancel_audio_asset_management(&mut self) {
        if !self.audio_asset_management_busy {
            self.status = "No audio asset operation is active".to_owned();
            return;
        }
        if self.audio_asset_management_finalizing {
            self.status = "Asset operation is finishing and can no longer be cancelled".to_owned();
            return;
        }
        self.audio_asset_management_cancel_requested = true;
        if let Some(worker) = self.audio_asset_management_worker.as_ref() {
            worker.cancel();
        }
        self.audio_asset_management_status = "Cancelling asset operation…".to_owned();
        self.status = self.audio_asset_management_status.clone();
    }

    pub(super) fn update_audio_asset_management(&mut self) -> Task<Message> {
        let Some(worker) = self.audio_asset_management_worker.as_ref() else {
            return Task::none();
        };
        for progress in worker.progress() {
            match progress {
                AudioAssetManagementProgress::SourceScan(update) => {
                    self.audio_asset_source_statuses.insert(
                        update.media_ref.clone(),
                        AudioAssetSourceStatusEntry {
                            media_ref: update.media_ref.clone(),
                            status: update.status,
                            is_external_link: false,
                        },
                    );
                    self.audio_asset_management_status = format!(
                        "Scanned {}/{} · {}: {:?}",
                        update.scanned_assets, update.total_assets, update.media_ref, update.status
                    );
                }
                AudioAssetManagementProgress::Pack(update) => {
                    self.audio_asset_management_status = format!(
                        "Packing {}/{} · {} · {} / {} bytes",
                        update.completed_assets,
                        update.total_assets,
                        update.media_ref,
                        update.bytes_imported,
                        update.total_bytes
                    );
                }
            }
        }
        if !worker.is_finished() {
            return Task::none();
        }
        let Some(worker) = self.audio_asset_management_worker.take() else {
            return Task::none();
        };
        self.audio_asset_management_finalizing = true;
        self.audio_asset_management_status = "Finishing asset operation…".to_owned();
        Task::perform(
            run_blocking("aaadaw-audio-asset-operation-finish", move || worker.join()),
            Message::AudioAssetManagementFinished,
        )
    }

    pub(super) fn finish_audio_asset_management(
        &mut self,
        result: Result<AudioAssetManagementResult, String>,
    ) {
        let was_cancelled = self.audio_asset_management_cancel_requested;
        let operation = self.audio_asset_management_operation.take();
        self.audio_asset_management_busy = false;
        self.audio_asset_management_finalizing = false;
        self.audio_asset_management_cancel_requested = false;
        match result {
            Ok(AudioAssetManagementResult::SourceScan(results)) => {
                self.audio_asset_source_statuses = results
                    .into_iter()
                    .map(|entry| (entry.media_ref.clone(), entry))
                    .collect();
                self.audio_asset_management_status = format!(
                    "Scanned {} audio assets",
                    self.audio_asset_source_statuses.len()
                );
            }
            Ok(AudioAssetManagementResult::Packed(media_refs)) => {
                self.audio_asset_source_statuses.clear();
                self.audio_asset_management_status =
                    format!("Packed {} external audio assets", media_refs.len());
            }
            Err(error) if was_cancelled => {
                self.audio_asset_management_status = match operation {
                    Some(AudioAssetManagementOperation::PackExternalAssets) => {
                        format!("Packing cancelled; completed assets remain embedded: {error}")
                    }
                    _ => format!("Source scan cancelled: {error}"),
                };
            }
            Err(error) => {
                self.audio_asset_management_status = format!("Asset operation failed: {error}");
            }
        }
        self.status = self.audio_asset_management_status.clone();
    }

    pub(super) fn relink_audio_item(&mut self, item_id: ItemId) -> Task<Message> {
        if self.io_busy || self.import_busy || self.audio_asset_management_busy {
            self.status = "Wait for the current project operation to finish".to_owned();
            return Task::none();
        }
        #[cfg(feature = "jack-backend")]
        if self.playback.is_some() {
            self.status = "Close JACK output before relinking audio".to_owned();
            return Task::none();
        }
        if self.is_dirty() {
            self.status = "Save the project before relinking audio".to_owned();
            return Task::none();
        }
        let Some(project_path) = self.project_path.clone() else {
            self.status = "Save the project before relinking audio".to_owned();
            return Task::none();
        };
        let Some(source_path) = project_path_from_query(&self.relink_source_path_query) else {
            self.status = "Enter the replacement source path first".to_owned();
            return Task::none();
        };
        let Some(media_ref) = self
            .project
            .audio_items()
            .iter()
            .find(|item| item.id() == item_id)
            .map(|item| item.media_ref().to_owned())
        else {
            self.status = "Audio item no longer exists".to_owned();
            return Task::none();
        };
        self.io_busy = true;
        self.status = format!("Relinking {media_ref}…");
        Task::perform(
            run_blocking("aaadaw-audio-relink", move || {
                relink_external_audio_source(project_path, media_ref, source_path)
            }),
            move |result| Message::AudioItemRelinked(item_id, result),
        )
    }

    pub(super) fn finish_audio_item_relink(&mut self, item_id: ItemId, result: Result<(), String>) {
        self.io_busy = false;
        match result {
            Ok(()) => {
                if let Some(media_ref) = self
                    .project
                    .audio_items()
                    .iter()
                    .find(|item| item.id() == item_id)
                    .map(|item| item.media_ref().to_owned())
                {
                    self.audio_asset_source_statuses.insert(
                        media_ref.clone(),
                        AudioAssetSourceStatusEntry {
                            media_ref,
                            status: AudioAssetSourceStatus::Linked,
                            is_external_link: true,
                        },
                    );
                }
                self.status = "External audio source relinked".to_owned();
            }
            Err(error) => self.status = format!("Audio relink failed: {error}"),
        }
    }

    pub(super) fn update_audio_import(&mut self) -> Task<Message> {
        let Some(import) = self.import_worker.as_mut() else {
            return Task::none();
        };
        let progress = import.worker.progress().try_iter().collect::<Vec<_>>();
        let finished = import.worker.is_finished();
        for AudioItemImportProgress {
            bytes_imported,
            total_bytes,
        } in progress
        {
            self.import_bytes = bytes_imported;
            self.import_total_bytes = Some(total_bytes);
        }
        if !finished {
            return Task::none();
        }

        let import = self
            .import_worker
            .take()
            .expect("finished import is present");
        self.import_finalizing = true;
        self.status = "Finalizing audio metadata and placement…".to_owned();
        Task::perform(
            run_blocking("aaadaw-audio-import-finish", move || {
                import.worker.finish().map_err(|error| error.to_string())
            }),
            Message::AudioImportFinished,
        )
    }

    pub(super) fn finish_audio_import(&mut self, result: Result<DawAction, String>) {
        self.import_busy = false;
        self.import_finalizing = false;
        self.import_cancel_requested = false;
        match result {
            Ok(action) => {
                self.apply_action(action, "Audio imported and appended to the first track")
            }
            Err(error) => self.status = format!("Audio import could not be finalized: {error}"),
        }
    }
}
