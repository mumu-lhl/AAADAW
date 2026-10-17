use super::{App, Message, PathPickerTarget, run_blocking};
use aaadaw_core::{
    MidiExportItemScope, MidiExportOptions, MidiExportSelection, MidiExportTimeScope, MidiFileType,
    MidiMarkerType, Project, ProjectSnapshot, encode_standard_midi_with_selection,
    midi_export_has_items_in_scope,
};
use iced::Task;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub(super) struct PendingEmptyMidiExport {
    pub(super) path: PathBuf,
    pub(super) snapshot: ProjectSnapshot,
    pub(super) options: MidiExportOptions,
    pub(super) selection: MidiExportSelection,
}

pub(super) fn empty_export_warning(options: MidiExportOptions) -> String {
    let mut detail = String::from("No unmuted ");
    match options.item_scope {
        MidiExportItemScope::All => detail.push_str("tracks containing MIDI items"),
        MidiExportItemScope::SelectedTracksOnly => {
            detail.push_str("selected tracks containing MIDI items");
        }
        MidiExportItemScope::SelectedItemsOnly => {
            detail.push_str("tracks containing selected MIDI items");
        }
    }
    if options.time_scope == MidiExportTimeScope::TimeSelectionOnly {
        detail.push_str(" within the time selection");
    }
    match options.file_type {
        MidiFileType::Type0 => format!("MIDI export failed!\n{detail} were found"),
        MidiFileType::Type1 => format!("{detail} were found. Write MIDI file header anyway?"),
    }
}

pub(super) fn empty_export_warning_width(options: MidiExportOptions) -> f32 {
    match (options.item_scope, options.time_scope) {
        (MidiExportItemScope::All, MidiExportTimeScope::EntireProject) => 324.0,
        (_, MidiExportTimeScope::TimeSelectionOnly)
            if options.item_scope != MidiExportItemScope::All =>
        {
            521.0
        }
        (MidiExportItemScope::SelectedTracksOnly | MidiExportItemScope::SelectedItemsOnly, _) => {
            377.0
        }
        (MidiExportItemScope::All, MidiExportTimeScope::TimeSelectionOnly) => 468.0,
    }
}

#[derive(Debug, Clone)]
pub(super) struct MidiExportDialogState {
    pub(super) destination: String,
    pub(super) file_type: MidiFileType,
    pub(super) time_scope: MidiExportTimeScope,
    pub(super) item_scope: MidiExportItemScope,
    pub(super) ppq: String,
    pub(super) embed_tempo_and_time_signature: bool,
    pub(super) embed_smpte_offset: bool,
    pub(super) export_markers: bool,
    pub(super) marker_type: MidiMarkerType,
    pub(super) only_markers_starting_with_hash: bool,
    pub(super) export_hidden_markers: bool,
}

impl MidiExportDialogState {
    fn for_project(project: &Project) -> Self {
        let options = MidiExportOptions::for_project(project);
        Self {
            destination: "midi_export.mid".to_owned(),
            file_type: options.file_type,
            time_scope: options.time_scope,
            item_scope: options.item_scope,
            ppq: options.ppq.to_string(),
            embed_tempo_and_time_signature: options.embed_tempo_and_time_signature,
            embed_smpte_offset: options.embed_smpte_offset,
            export_markers: options.export_markers,
            marker_type: options.marker_type,
            only_markers_starting_with_hash: options.only_markers_starting_with_hash,
            export_hidden_markers: options.export_hidden_markers,
        }
    }

    fn options(&self) -> Result<MidiExportOptions, String> {
        let ppq = self
            .ppq
            .trim()
            .parse::<u16>()
            .map_err(|_| "PPQN must be a whole number from 1 to 32767".to_owned())?;
        if !(1..=0x7fff).contains(&ppq) {
            return Err("PPQN must be a whole number from 1 to 32767".to_owned());
        }
        Ok(MidiExportOptions {
            file_type: self.file_type,
            ppq,
            time_scope: self.time_scope,
            item_scope: self.item_scope,
            embed_tempo_and_time_signature: self.embed_tempo_and_time_signature,
            embed_smpte_offset: self.embed_smpte_offset,
            export_markers: self.export_markers,
            marker_type: self.marker_type,
            only_markers_starting_with_hash: self.only_markers_starting_with_hash,
            export_hidden_markers: self.export_hidden_markers,
        })
    }
}

impl App {
    pub(super) fn open_midi_export_dialog(&mut self) -> Task<Message> {
        if let Some(window_id) = self.midi_export_window_id {
            return iced::window::gain_focus(window_id);
        }
        self.midi_export_dialog = Some(MidiExportDialogState::for_project(&self.project));
        let (window_id, task) = iced::window::open(iced::window::Settings {
            size: iced::Size::new(520.0, 452.0),
            min_size: Some(iced::Size::new(500.0, 420.0)),
            ..iced::window::Settings::default()
        });
        self.midi_export_window_id = Some(window_id);
        task.discard()
    }

    pub(super) fn close_midi_export_dialog(&mut self) -> Task<Message> {
        self.midi_export_dialog = None;
        self.midi_export_window_id
            .take()
            .map_or_else(Task::none, iced::window::close)
    }

    pub(super) fn browse_midi_export_destination(&mut self) -> Task<Message> {
        self.pick_path(PathPickerTarget::ExportMidi)
    }

    pub(super) fn confirm_midi_export(&mut self) -> Task<Message> {
        let Some(dialog) = self.midi_export_dialog.as_ref() else {
            return Task::none();
        };
        if dialog.destination.trim().is_empty() {
            self.status = "Enter an output MIDI file path".to_owned();
            return Task::none();
        }
        let options = match dialog.options() {
            Ok(options) => options,
            Err(error) => {
                self.status = error;
                return Task::none();
            }
        };
        let path = with_midi_extension(PathBuf::from(dialog.destination.trim()));
        let selection = MidiExportSelection {
            selected_track_ids: self.timeline.selected_tracks.iter().copied().collect(),
            selected_item_ids: self.timeline.selected_items.iter().copied().collect(),
            time_selection_ticks: self
                .timeline
                .time_selection
                .map(|selection| (selection.start_tick, selection.end_tick)),
        };
        let has_midi_items =
            match midi_export_has_items_in_scope(&self.project, options, &selection) {
                Ok(has_midi_items) => has_midi_items,
                Err(error) => {
                    self.status = error;
                    return Task::none();
                }
            };
        let pending = PendingEmptyMidiExport {
            path,
            snapshot: self.project.snapshot(),
            options,
            selection,
        };
        if !has_midi_items {
            self.pending_empty_midi_export = Some(pending);
            return self.close_midi_export_dialog();
        }
        let export = self.start_project_midi_export(pending);
        Task::batch([export, self.close_midi_export_dialog()])
    }

    pub(super) fn confirm_empty_midi_export(&mut self) -> Task<Message> {
        let Some(pending) = self.pending_empty_midi_export.take() else {
            return Task::none();
        };
        if pending.options.file_type == MidiFileType::Type0 {
            self.status = "MIDI export failed!".to_owned();
            return Task::none();
        }
        self.start_project_midi_export(pending)
    }

    pub(super) fn dismiss_empty_midi_export(&mut self) {
        if let Some(pending) = self.pending_empty_midi_export.take() {
            self.status = if pending.options.file_type == MidiFileType::Type0 {
                "MIDI export failed!".to_owned()
            } else {
                "MIDI export cancelled".to_owned()
            };
        }
    }

    fn start_project_midi_export(&mut self, pending: PendingEmptyMidiExport) -> Task<Message> {
        let PendingEmptyMidiExport {
            path,
            snapshot,
            options,
            selection,
        } = pending;
        let output = path.clone();
        self.status = format!("Exporting project MIDI to {}…", path.display());
        Task::perform(
            run_blocking("aaadaw-midi-export", move || {
                let result = write_project_midi(&output, snapshot, options, selection);
                #[cfg(target_os = "android")]
                if result.is_ok() {
                    crate::android_platform::sync_saf_document(&output)?;
                }
                result
            }),
            move |result| Message::ProjectMidiExported(path, result),
        )
    }
}

fn with_midi_extension(mut path: PathBuf) -> PathBuf {
    if path.extension().is_none() {
        path.set_extension("mid");
    }
    path
}

fn write_project_midi(
    path: &Path,
    snapshot: ProjectSnapshot,
    options: MidiExportOptions,
    selection: MidiExportSelection,
) -> Result<(), String> {
    let project = Project::from_snapshot(snapshot).map_err(|error| error.to_string())?;
    let conductor_track_name = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .filter(|stem| !stem.is_empty())
        .unwrap_or_else(|| "midi_export".to_owned());
    let bytes =
        encode_standard_midi_with_selection(&project, options, &conductor_track_name, &selection)?;
    super::config_paths::write_atomic(path, &bytes)
        .map_err(|error| format!("could not write {}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use aaadaw_core::{MidiExportOptions, Project};

    #[test]
    fn conductor_track_name_uses_the_export_destination_stem() {
        let project = Project::new();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("native-name.mid");

        write_project_midi(
            &path,
            project.snapshot(),
            MidiExportOptions::for_project(&project),
            MidiExportSelection::default(),
        )
        .unwrap();

        let bytes = std::fs::read(path).unwrap();
        let expected = b"native-name";
        assert!(bytes.windows(expected.len() + 3).any(|event| {
            event[..3] == [0xff, 0x03, expected.len() as u8] && &event[3..] == expected
        }));
    }
}
