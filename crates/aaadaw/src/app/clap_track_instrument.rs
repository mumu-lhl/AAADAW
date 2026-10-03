use super::{App, Message};
#[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
use aaadaw_app::PreparedAudioPlayback;
use aaadaw_core::{DawAction, TrackId, TrackInstrument};
#[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
use aaadaw_engine::TrackInstrumentProcessor;
use iced::Task;
#[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
use std::path::Path;

impl App {
    pub(super) fn open_track_instrument_picker(&mut self, track_id: TrackId) -> Task<Message> {
        if !self
            .project
            .tracks()
            .iter()
            .any(|track| track.id() == track_id)
        {
            self.status = "Track no longer exists".to_owned();
            return Task::none();
        }
        self.plugin_picker_track_id = None;
        self.plugin_picker_instrument_track_id = Some(track_id);
        self.plugin_picker_search.clear();
        if self.plugin_picker_window_id.is_some() {
            return Task::none();
        }

        let (window_id, task) = iced::window::open(iced::window::Settings {
            size: iced::Size::new(600.0, 440.0),
            min_size: Some(iced::Size::new(460.0, 320.0)),
            ..iced::window::Settings::default()
        });
        self.plugin_picker_window_id = Some(window_id);
        task.discard()
    }

    pub(super) fn select_scanned_instrument(&mut self, plugin_id: &str) -> Task<Message> {
        let Some(track_id) = self.plugin_picker_instrument_track_id else {
            self.status = "Instrument selection has no target track".to_owned();
            return Task::none();
        };
        let Some(plugin) = self
            .clap_plugin_scan
            .plugins
            .iter()
            .find(|plugin| plugin.plugin_id == plugin_id && plugin.is_instrument())
            .cloned()
        else {
            self.status =
                "Instrument is no longer in the scan results; rescan the CLAP paths".to_owned();
            return Task::none();
        };
        let Some(track) = self
            .project
            .tracks()
            .iter()
            .find(|track| track.id() == track_id)
        else {
            self.status = "Track no longer exists".to_owned();
            return Task::none();
        };
        let Some(instrument) = TrackInstrument::new(
            plugin.plugin_id.clone(),
            plugin.entry_path.to_string_lossy().into_owned(),
        ) else {
            self.status = "Scanned instrument has an invalid CLAP reference".to_owned();
            return Task::none();
        };
        if track.instrument() == Some(&instrument) {
            self.status = format!("{} is already assigned to the track", plugin.name);
            return self.close_instrument_picker();
        }

        self.apply_action(
            DawAction::SetTrackInstrument {
                track_id,
                instrument: Some(instrument),
            },
            &format!("Assigned {} to the track", plugin.name),
        );
        self.close_instrument_picker()
    }

    pub(super) fn clear_track_instrument(&mut self, track_id: TrackId) {
        let Some(track) = self
            .project
            .tracks()
            .iter()
            .find(|track| track.id() == track_id)
        else {
            self.status = "Track no longer exists".to_owned();
            return;
        };
        if track.instrument().is_none() {
            self.status = "Track has no assigned instrument".to_owned();
            return;
        }
        self.apply_action(
            DawAction::SetTrackInstrument {
                track_id,
                instrument: None,
            },
            "Track instrument cleared",
        );
    }

    fn close_instrument_picker(&mut self) -> Task<Message> {
        self.plugin_picker_instrument_track_id = None;
        self.plugin_picker_search.clear();
        self.plugin_picker_window_id
            .take()
            .map_or_else(Task::none, iced::window::close)
    }

    #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
    pub(super) fn install_track_instrument_processors(
        &mut self,
        prepared: &mut PreparedAudioPlayback,
    ) -> Result<Vec<u64>, String> {
        let mut owners = Vec::new();
        let mut owner_targets = Vec::new();
        let mut processors = Vec::new();
        let sample_rate = self.project.settings().sample_rate();
        let max_block_frames = prepared.graph().max_block_frames();

        for track in self.project.tracks() {
            let Some(instrument) = track.instrument() else {
                continue;
            };
            let max_events = prepared.graph().midi_event_capacity_for_track(track.id());
            // SAFETY: playback was explicitly requested for a project containing a saved CLAP
            // instrument reference. In-process plugins are trusted native code and are not isolated.
            let loaded = unsafe {
                aaadaw_engine::ClapInstrumentOwner::load_with_state(
                    Path::new(instrument.bundle_path()),
                    instrument.plugin_id(),
                    instrument.state(),
                    sample_rate,
                    max_block_frames,
                    max_events,
                )
            };
            let loaded = match loaded {
                Err(state_error) if instrument.state().is_some() => {
                    self.clap_plugin_warnings.push(format!(
                        "{} state could not be restored; using its default state ({state_error})",
                        instrument.plugin_id()
                    ));
                    // SAFETY: same trusted plugin entry selected by the project; retry only omits
                    // its optional saved state after the plugin rejected that state.
                    unsafe {
                        aaadaw_engine::ClapInstrumentOwner::load(
                            Path::new(instrument.bundle_path()),
                            instrument.plugin_id(),
                            sample_rate,
                            max_block_frames,
                            max_events,
                        )
                    }
                }
                result => result,
            };
            match loaded {
                Ok((owner, processor)) => {
                    owner_targets.push((owner.instance_id(), track.id()));
                    owners.push((owner.instance_id(), owner));
                    processors.push(TrackInstrumentProcessor::new(track.id(), processor));
                }
                Err(error) => {
                    self.clap_plugin_warnings.push(format!(
                        "Could not activate instrument {}; its track will be silent ({error})",
                        instrument.plugin_id()
                    ));
                }
            }
        }

        if let Err(error) = prepared
            .graph_mut()
            .install_instrument_processors(&self.project, &mut processors)
        {
            deactivate_uninstalled_instruments(owners, processors);
            return Err(format!("Could not prepare track CLAP instruments: {error}"));
        }

        let ids = owners.iter().map(|(instance_id, _)| *instance_id).collect();
        self.clap_instrument_targets.extend(owner_targets);
        for (instance_id, owner) in owners {
            self.clap_instrument_owners.insert(instance_id, owner);
        }
        Ok(ids)
    }

    #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
    pub(super) fn discard_unused_instrument_owners(&mut self, ids: &[u64]) -> Option<String> {
        let mut error_message = None;
        for id in ids {
            let result = self
                .clap_instrument_owners
                .get_mut(id)
                .map(|owner| owner.try_deactivate_unused());
            match result {
                Some(Ok(())) => {
                    self.clap_instrument_owners.remove(id);
                    self.clap_instrument_targets.remove(id);
                }
                Some(Err(error)) => {
                    error_message = Some(format!(
                        "Could not deactivate unused CLAP instrument {id}: {error}"
                    ));
                }
                None => {}
            }
        }
        error_message
    }

    #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
    pub(super) fn deactivate_stopped_instruments(
        &mut self,
        processors: Vec<aaadaw_engine::StoppedTrackInstrument>,
    ) {
        for stopped in processors {
            let instance_id = stopped.instance_id();
            if let Some(owner) = self.clap_instrument_owners.remove(&instance_id) {
                self.clap_instrument_targets.remove(&instance_id);
                let (_, processor) = stopped.into_parts();
                owner.deactivate(processor);
            } else {
                self.status = format!(
                    "Stopped CLAP instrument {instance_id} has no matching owner; plugin cleanup was skipped"
                );
            }
        }
    }
}

#[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
fn deactivate_uninstalled_instruments(
    owners: Vec<(u64, aaadaw_engine::ClapInstrumentOwner)>,
    processors: Vec<TrackInstrumentProcessor>,
) {
    debug_assert_eq!(owners.len(), processors.len());
    for ((owner_id, owner), instrument) in owners.into_iter().zip(processors) {
        let processor_id = instrument.instance_id();
        let (_, processor) = instrument.into_parts();
        debug_assert_eq!(owner_id, processor_id);
        owner.deactivate(processor.stop());
    }
}
