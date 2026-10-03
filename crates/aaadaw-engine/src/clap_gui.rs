use crate::clap_instrument::{ClapInstrumentError, restore_parameter_values};
use clack_extensions::gui::{GuiApiType, GuiConfiguration, PluginGui, Window as ClapWindow};
use clack_extensions::params::{ParamInfoBuffer, ParamInfoFlags, PluginParams};
use clack_extensions::state::PluginState;
use clack_host::events::Pckn;
use clack_host::events::event_types::{
    ParamGestureBeginEvent, ParamGestureEndEvent, ParamValueEvent,
};
use clack_host::events::io::{EventBuffer, InputEvents};
use clack_host::prelude::{HostInfo, PluginEntry, PluginInstance};
use std::ffi::CString;
use std::io::Cursor;
use std::os::raw::c_ulong;
use std::path::Path;

/// An embedded CLAP editor whose plugin instance is owned by the app control thread.
///
/// The editor must be destroyed before its native parent X11 window is destroyed.
pub struct ClapPluginGuiOwner {
    instance: PluginInstance<()>,
    gui: Option<PluginGui>,
    created: bool,
    visible: bool,
    show_warning: Option<String>,
}

impl ClapPluginGuiOwner {
    /// Lists visible parameter metadata and current values from the GUI's plugin instance.
    pub fn parameters(&mut self) -> Vec<crate::ClapParameterInfo> {
        let handle = self.instance.plugin_handle();
        let Some(params) = handle.get_extension::<PluginParams>() else {
            return Vec::new();
        };
        let count = params.count(&handle);
        let mut result = Vec::with_capacity(count as usize);
        let mut buffer = ParamInfoBuffer::new();
        for index in 0..count {
            let Some(info) = params.get_info(&handle, index, &mut buffer) else {
                continue;
            };
            if info.flags.contains(ParamInfoFlags::IS_HIDDEN)
                || !info.flags.contains(ParamInfoFlags::IS_AUTOMATABLE)
            {
                continue;
            }
            let id = info.id;
            let (min_value, max_value, stepped, read_only, default_value) = (
                info.min_value,
                info.max_value,
                info.flags.contains(ParamInfoFlags::IS_STEPPED),
                info.flags.contains(ParamInfoFlags::IS_READONLY),
                info.default_value,
            );
            let name = String::from_utf8_lossy(info.name).into_owned();
            let value = params.get_value(&handle, id).unwrap_or(default_value);
            if !min_value.is_finite()
                || !max_value.is_finite()
                || max_value < min_value
                || !default_value.is_finite()
                || !value.is_finite()
            {
                continue;
            }
            let mut display_buffer = [0_u8; 64];
            let display_value = params
                .value_to_text(&handle, id, value, &mut display_buffer)
                .map(|display| String::from_utf8_lossy(display).into_owned())
                .unwrap_or_else(|_| format!("{value:.3}"));
            result.push(crate::ClapParameterInfo {
                id: id.get(),
                name,
                min_value,
                max_value,
                default_value,
                value,
                display_value,
                stepped,
                read_only,
            });
        }
        result
    }

    /// Flushes one host parameter gesture event on the inactive GUI instance.
    pub fn apply_parameter_command(
        &mut self,
        command: crate::ClapParameterCommand,
    ) -> Result<(), String> {
        let events = match command {
            crate::ClapParameterCommand::Begin { id } => {
                let param_id = clack_host::prelude::ClapId::from_raw(id)
                    .ok_or_else(|| "CLAP parameter ID is invalid".to_owned())?;
                let mut events = EventBuffer::with_capacity(1);
                events.push(&ParamGestureBeginEvent::new(0, param_id));
                events
            }
            crate::ClapParameterCommand::Set { id, value } => {
                let param_id = clack_host::prelude::ClapId::from_raw(id)
                    .ok_or_else(|| "CLAP parameter ID is invalid".to_owned())?;
                let mut events = EventBuffer::with_capacity(1);
                events.push(&ParamValueEvent::new(0, param_id, Pckn::match_all(), value));
                events
            }
            crate::ClapParameterCommand::End { id } => {
                let param_id = clack_host::prelude::ClapId::from_raw(id)
                    .ok_or_else(|| "CLAP parameter ID is invalid".to_owned())?;
                let mut events = EventBuffer::with_capacity(1);
                events.push(&ParamGestureEndEvent::new(0, param_id));
                events
            }
        };
        let mut handle = self
            .instance
            .inactive_plugin_handle()
            .ok_or_else(|| "CLAP editor instance is unexpectedly active".to_owned())?;
        let Some(params) = handle.get_extension::<PluginParams>() else {
            return Err("CLAP plugin does not expose parameter controls".to_owned());
        };
        let input = InputEvents::from_buffer(&events);
        let mut output = EventBuffer::with_capacity(8);
        let mut output_events = output.as_output();
        params.flush(&mut handle, &input, &mut output_events);
        Ok(())
    }

    /// Loads a trusted CLAP plugin and embeds its X11 editor in the supplied parent window.
    ///
    /// CLAP initialization and GUI calls execute third-party native code on the calling thread.
    /// The caller must keep `parent_x11_window` alive until this owner is dropped.
    ///
    /// # Safety
    ///
    /// `entry_path` must identify a valid, trusted CLAP entry library, and the parent handle must
    /// be a live X11 window on the current display.
    pub unsafe fn open_embedded_x11(
        entry_path: &Path,
        plugin_id: &str,
        parent_x11_window: u64,
        requested_width: u32,
        requested_height: u32,
        scale_factor: f32,
        state: Option<&[u8]>,
        parameter_values: &[(u32, f64)],
    ) -> Result<Self, ClapInstrumentError> {
        // SAFETY: The caller guarantees the selected entry is trusted and valid.
        let entry = unsafe { PluginEntry::load(entry_path) }.map_err(|error| {
            ClapInstrumentError::new(format!("Could not load CLAP entry: {error}"))
        })?;
        let plugin_id_c = CString::new(plugin_id)
            .map_err(|_| ClapInstrumentError::new("CLAP plugin ID contains a null byte"))?;
        let factory = entry
            .get_plugin_factory()
            .ok_or_else(|| ClapInstrumentError::new("CLAP entry has no plugin factory"))?;
        let plugin_exists = factory.plugin_descriptors().any(|descriptor| {
            descriptor
                .id()
                .is_some_and(|id| id.to_bytes() == plugin_id.as_bytes())
        });
        if !plugin_exists {
            return Err(ClapInstrumentError::new(format!(
                "CLAP plugin {plugin_id:?} was not found"
            )));
        }
        let host_info = HostInfo::new(
            "AAADAW",
            "AAADAW",
            "https://github.com/mumu-lhl/AAADAW",
            "0.1.0",
        )
        .map_err(|error| {
            ClapInstrumentError::new(format!("Invalid CLAP host metadata: {error}"))
        })?;
        let mut instance =
            PluginInstance::<()>::new(|_| (), |_| (), &entry, &plugin_id_c, &host_info).map_err(
                |error| {
                    ClapInstrumentError::new(format!(
                        "Could not create CLAP plugin editor: {error}"
                    ))
                },
            )?;
        if let Some(state) = state {
            let plugin = instance.plugin_handle();
            let state_extension = plugin.get_extension::<PluginState>().ok_or_else(|| {
                ClapInstrumentError::state_restore(
                    "CLAP plugin does not implement the state extension",
                )
            })?;
            state_extension
                .load(&plugin, &mut Cursor::new(state))
                .map_err(|error| {
                    ClapInstrumentError::state_restore(format!(
                        "Could not restore CLAP state: {error}"
                    ))
                })?;
        }
        if state.is_none() {
            restore_parameter_values(&mut instance, parameter_values);
        }
        let Some(gui) = instance.plugin_shared_handle().get_extension::<PluginGui>() else {
            return Ok(Self::control_only(
                instance,
                "This plugin has no native editor; use the host parameter controls below.",
            ));
        };
        let configuration = GuiConfiguration {
            api_type: GuiApiType::X11,
            is_floating: false,
        };
        let plugin = instance.plugin_handle();
        if !gui.is_api_supported(&plugin, configuration) {
            return Ok(Self::control_only(
                instance,
                "This plugin has no embedded X11 editor; use the host parameter controls below.",
            ));
        }
        gui.create(&plugin, configuration).map_err(|error| {
            ClapInstrumentError::new(format!("Could not create plugin GUI: {error}"))
        })?;
        let scale = if scale_factor.is_finite() {
            f64::from(scale_factor.max(1.0))
        } else {
            1.0
        };
        let _ = gui.set_scale(&plugin, scale);
        if gui.can_resize(&plugin) {
            let requested_size = clack_extensions::gui::GuiSize {
                width: requested_width.max(1),
                height: requested_height.max(1),
            };
            let size = gui
                .adjust_size(&plugin, requested_size)
                .unwrap_or(requested_size);
            gui.set_size(&plugin, size).map_err(|error| {
                gui.destroy(&plugin);
                ClapInstrumentError::new(format!("Could not size plugin GUI: {error}"))
            })?;
        } else if gui.get_size(&plugin).is_none() {
            gui.destroy(&plugin);
            return Err(ClapInstrumentError::new(
                "CLAP plugin did not report its embedded GUI size",
            ));
        }

        // SAFETY: The caller guarantees this is a live parent X11 window and keeps it alive until
        // this owner is dropped.
        if let Err(error) = unsafe {
            gui.set_parent(
                &plugin,
                ClapWindow::from_x11_handle(parent_x11_window as c_ulong),
            )
        } {
            gui.destroy(&plugin);
            return Err(ClapInstrumentError::new(format!(
                "Could not attach plugin GUI to the FX editor: {error}"
            )));
        }
        // Some established CLAP frameworks initialize embedded editors during `set_parent`, then
        // return false from `show` because they do not implement the embedded visibility callback.
        // Keep the editor attached and expose that limitation in the UI.
        let show_warning = gui
            .show(&plugin)
            .err()
            .map(|_| {
                "The plugin did not confirm the show request. Its embedded editor may still appear after a moment."
                    .to_owned()
            });
        Ok(Self {
            instance,
            gui: Some(gui),
            created: true,
            visible: true,
            show_warning,
        })
    }

    fn control_only(instance: PluginInstance<()>, warning: &str) -> Self {
        Self {
            instance,
            gui: None,
            created: false,
            visible: false,
            show_warning: Some(warning.to_owned()),
        }
    }

    /// Returns a warning when the plugin rejected the CLAP show callback after being attached.
    pub fn show_warning(&self) -> Option<&str> {
        self.show_warning.as_deref()
    }

    /// Saves the editor instance's opaque state on the control thread.
    pub fn save_state(&mut self) -> Result<Option<Vec<u8>>, ClapInstrumentError> {
        let plugin = self.instance.plugin_handle();
        let Some(state_extension) = plugin.get_extension::<PluginState>() else {
            return Ok(None);
        };
        let mut state = Vec::new();
        state_extension.save(&plugin, &mut state).map_err(|error| {
            ClapInstrumentError::new(format!("Could not save CLAP editor state: {error}"))
        })?;
        Ok(Some(state))
    }

    /// Returns the plugin's preferred editor size, when provided.
    pub fn preferred_size(&mut self) -> Option<(u32, u32)> {
        self.gui
            .as_ref()?
            .get_size(&self.instance.plugin_handle())
            .map(|size| (size.width, size.height))
    }

    /// Negotiates and applies a new editor size if the plugin supports resizing.
    pub fn resize(&mut self, width: u32, height: u32) -> Result<bool, ClapInstrumentError> {
        let plugin = self.instance.plugin_handle();
        let Some(gui) = self.gui.as_ref() else {
            return Ok(false);
        };
        if !gui.can_resize(&plugin) {
            return Ok(false);
        }
        let Some(size) = self
            .gui
            .as_ref()
            .expect("GUI checked above")
            .adjust_size(&plugin, clack_extensions::gui::GuiSize { width, height })
        else {
            return Ok(false);
        };
        gui.set_size(&plugin, size).map_err(|error| {
            ClapInstrumentError::new(format!("Could not resize plugin GUI: {error}"))
        })?;
        Ok(true)
    }

    /// Hides and destroys the native editor before its X11 parent is removed.
    pub fn close(&mut self) {
        if !self.created {
            return;
        }
        let Some(gui) = self.gui.as_ref() else {
            self.created = false;
            self.visible = false;
            return;
        };
        let plugin = self.instance.plugin_handle();
        if self.visible {
            let _ = gui.hide(&plugin);
        }
        gui.destroy(&plugin);
        self.visible = false;
        self.created = false;
    }
}

impl Drop for ClapPluginGuiOwner {
    fn drop(&mut self) {
        self.close();
    }
}
