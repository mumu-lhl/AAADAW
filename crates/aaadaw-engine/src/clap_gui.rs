use crate::clap_instrument::ClapInstrumentError;
use clack_extensions::gui::{GuiApiType, GuiConfiguration, PluginGui, Window as ClapWindow};
use clack_extensions::state::PluginState;
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
    gui: PluginGui,
    created: bool,
    visible: bool,
    show_warning: Option<String>,
}

impl ClapPluginGuiOwner {
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
        let gui = instance
            .plugin_shared_handle()
            .get_extension::<PluginGui>()
            .ok_or_else(|| ClapInstrumentError::new("CLAP plugin does not provide a native GUI"))?;
        let configuration = GuiConfiguration {
            api_type: GuiApiType::X11,
            is_floating: false,
        };
        let plugin = instance.plugin_handle();
        if !gui.is_api_supported(&plugin, configuration) {
            return Err(ClapInstrumentError::new(
                "CLAP plugin does not support an embedded X11 editor",
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
            gui,
            created: true,
            visible: true,
            show_warning,
        })
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
            .get_size(&self.instance.plugin_handle())
            .map(|size| (size.width, size.height))
    }

    /// Negotiates and applies a new editor size if the plugin supports resizing.
    pub fn resize(&mut self, width: u32, height: u32) -> Result<bool, ClapInstrumentError> {
        let plugin = self.instance.plugin_handle();
        if !self.gui.can_resize(&plugin) {
            return Ok(false);
        }
        let Some(size) = self
            .gui
            .adjust_size(&plugin, clack_extensions::gui::GuiSize { width, height })
        else {
            return Ok(false);
        };
        self.gui.set_size(&plugin, size).map_err(|error| {
            ClapInstrumentError::new(format!("Could not resize plugin GUI: {error}"))
        })?;
        Ok(true)
    }

    /// Hides and destroys the native editor before its X11 parent is removed.
    pub fn close(&mut self) {
        if !self.created {
            return;
        }
        let plugin = self.instance.plugin_handle();
        if self.visible {
            let _ = self.gui.hide(&plugin);
        }
        self.gui.destroy(&plugin);
        self.visible = false;
        self.created = false;
    }
}

impl Drop for ClapPluginGuiOwner {
    fn drop(&mut self) {
        self.close();
    }
}
