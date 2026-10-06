use super::App;
use aaadaw_core::DawAction;
use std::collections::HashMap;

impl App {
    /// Copies active plugin state into the project through the existing undoable assignment
    /// actions. CLAP state calls run on the control thread and never from the audio callback.
    pub(super) fn persist_clap_plugin_states(&mut self) -> Result<(), String> {
        let mut actions = Vec::new();
        let mut instrument_states = Vec::new();
        let mut errors = Vec::new();
        for (instance_id, track_id) in &self.clap_instrument_targets {
            match self.clap_instrument_owners.get_mut(instance_id) {
                Some(owner) => match owner.save_state() {
                    Ok(Some(state)) => instrument_states.push((*track_id, state)),
                    Ok(None) => {}
                    Err(error) => errors.push(format!(
                        "Could not save CLAP instrument state for track {track_id:?}: {error}"
                    )),
                },
                None => errors.push(format!("CLAP instrument owner {instance_id} is missing")),
            }
        }
        for (instance_id, (track_id, plugin_id)) in &self.clap_instrument_helper_targets {
            match self.clap_instrument_helper_owners.get_mut(instance_id) {
                Some(owner) => match owner.save_plugin_state() {
                    Ok(Some(state)) => instrument_states.push((*track_id, state)),
                    Ok(None) => {}
                    Err(error) => errors.push(format!(
                        "Could not save isolated CLAP instrument state for {}: {error}",
                        plugin_id
                    )),
                },
                None => errors.push(format!("CLAP helper owner {instance_id} is missing")),
            }
        }

        let mut effect_states = HashMap::new();
        if let (Some(gui), Some((track_id, chain_index, plugin_id))) = (
            self.fx_chain_plugin_gui.as_mut(),
            self.fx_chain_plugin_gui_identity.clone(),
        ) {
            match gui.save_state() {
                Ok(Some(state)) => {
                    self.clap_effect_state_overrides.insert((
                        track_id,
                        chain_index,
                        plugin_id.clone(),
                    ));
                    effect_states.insert((track_id, chain_index), (plugin_id, state));
                }
                Ok(None) => {}
                Err(error) => errors.push(format!("Could not save CLAP editor state: {error}")),
            }
        }

        for (instance_id, (track_id, chain_index, plugin_id)) in &self.clap_effect_targets {
            if self.clap_effect_state_overrides.contains(&(
                *track_id,
                *chain_index,
                plugin_id.clone(),
            )) {
                continue;
            }
            match self.clap_effect_owners.get_mut(instance_id) {
                Some(owner) => match owner.save_state() {
                    Ok(Some(state)) => {
                        effect_states.insert((*track_id, *chain_index), (plugin_id.clone(), state));
                    }
                    Ok(None) => {}
                    Err(error) => errors.push(format!(
                        "Could not save CLAP effect state for {}: {error}",
                        plugin_id
                    )),
                },
                None => errors.push(format!("CLAP effect owner {instance_id} is missing")),
            }
        }

        for (track_id, state) in instrument_states {
            let Some(instrument) = self
                .project
                .tracks()
                .iter()
                .find(|track| track.id() == track_id)
                .and_then(|track| track.instrument())
                .cloned()
            else {
                continue;
            };
            if instrument.state() == Some(state.as_slice()) {
                continue;
            }
            actions.push(DawAction::SetTrackInstrument {
                track_id,
                instrument: Some(instrument.with_state(Some(state))),
            });
        }

        for track in self.project.tracks() {
            let mut plugins = track.fx_chain().to_vec();
            let mut changed = false;
            for ((track_id, chain_index), (plugin_id, state)) in &effect_states {
                if *track_id != track.id() {
                    continue;
                }
                let Some(plugin) = plugins.get_mut(*chain_index) else {
                    continue;
                };
                if plugin.plugin_id() != plugin_id || plugin.state() == Some(state.as_slice()) {
                    continue;
                }
                *plugin = plugin.clone().with_state(Some(state.clone()));
                changed = true;
            }
            if changed {
                actions.push(DawAction::SetTrackFxChain {
                    track_id: track.id(),
                    plugins,
                });
            }
        }

        if !actions.is_empty() {
            let previous_revision = self.revision;
            self.apply_action(
                DawAction::BatchTransaction {
                    tx_id: self.revision,
                    actions,
                },
                "Saved CLAP plugin state",
            );
            if self.revision == previous_revision {
                return Err(self.status.clone());
            }
            self.playback_graph_dirty = true;
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }
}
