use iced::Size;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ConnectionExt, CreateWindowAux, EventMask, WindowClass};
use x11rb::rust_connection::RustConnection;

const EDITOR_LEFT: f32 = 280.0;
const EDITOR_TOP: f32 = 68.0;
const EDITOR_RIGHT: f32 = 20.0;
const EDITOR_BOTTOM: f32 = 22.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct EditorRect {
    x: i16,
    y: i16,
    width: u16,
    height: u16,
}

impl EditorRect {
    fn from_window_size(size: Size, scale_factor: f32) -> Self {
        let scale = if scale_factor.is_finite() {
            scale_factor.max(1.0)
        } else {
            1.0
        };
        let physical = |value: f32| -> i32 { (value * scale).round() as i32 };
        let x = physical(EDITOR_LEFT).clamp(0, i32::from(i16::MAX));
        let y = physical(EDITOR_TOP).clamp(0, i32::from(i16::MAX));
        let width = physical(size.width - EDITOR_LEFT - EDITOR_RIGHT).clamp(1, i32::from(u16::MAX));
        let height =
            physical(size.height - EDITOR_TOP - EDITOR_BOTTOM).clamp(1, i32::from(u16::MAX));
        Self {
            x: x as i16,
            y: y as i16,
            width: width as u16,
            height: height as u16,
        }
    }

    fn available_size(size: Size, scale_factor: f32) -> (u32, u32) {
        let rect = Self::from_window_size(size, scale_factor);
        (u32::from(rect.width), u32::from(rect.height))
    }
}

pub(super) fn window_size_for_editor(editor_size: (u32, u32), scale_factor: f32) -> Size {
    let scale = if scale_factor.is_finite() {
        scale_factor.max(1.0)
    } else {
        1.0
    };
    Size::new(
        (editor_size.0 as f32 / scale + EDITOR_LEFT + EDITOR_RIGHT).ceil(),
        (editor_size.1 as f32 / scale + EDITOR_TOP + EDITOR_BOTTOM).ceil(),
    )
}

/// A child X11 window that provides the right-hand editor area to an embedded CLAP GUI.
pub(super) struct X11PluginEditorHost {
    connection: RustConnection,
    window: u32,
}

impl X11PluginEditorHost {
    pub(super) fn new(parent: u64, size: Size, scale_factor: f32) -> Result<Self, String> {
        let (connection, screen_index) = x11rb::connect(None)
            .map_err(|error| format!("Could not connect to the X11 display: {error}"))?;
        let screen = connection
            .setup()
            .roots
            .get(screen_index)
            .ok_or_else(|| "The X11 display has no screen".to_owned())?;
        let window = connection
            .generate_id()
            .map_err(|error| format!("Could not allocate the CLAP editor window: {error}"))?;
        let rect = EditorRect::from_window_size(size, scale_factor);
        connection
            .create_window(
                0,
                window,
                parent as u32,
                rect.x,
                rect.y,
                rect.width,
                rect.height,
                0,
                WindowClass::INPUT_OUTPUT,
                x11rb::COPY_FROM_PARENT,
                &CreateWindowAux::new()
                    .background_pixel(screen.black_pixel)
                    .event_mask(EventMask::STRUCTURE_NOTIFY),
            )
            .map_err(|error| format!("Could not create the CLAP editor area: {error}"))?
            .check()
            .map_err(|error| format!("Could not attach the CLAP editor area: {error}"))?;
        connection
            .map_window(window)
            .map_err(|error| format!("Could not show the CLAP editor area: {error}"))?
            .check()
            .map_err(|error| format!("Could not show the CLAP editor area: {error}"))?;
        connection
            .flush()
            .map_err(|error| format!("Could not update the CLAP editor area: {error}"))?;

        Ok(Self { connection, window })
    }

    pub(super) fn window_id(&self) -> u64 {
        u64::from(self.window)
    }

    pub(super) fn resize(&self, size: Size, scale_factor: f32) -> Result<(), String> {
        let rect = EditorRect::from_window_size(size, scale_factor);
        self.connection
            .configure_window(
                self.window,
                &x11rb::protocol::xproto::ConfigureWindowAux::new()
                    .x(i32::from(rect.x))
                    .y(i32::from(rect.y))
                    .width(u32::from(rect.width))
                    .height(u32::from(rect.height)),
            )
            .map_err(|error| format!("Could not resize the CLAP editor area: {error}"))?
            .check()
            .map_err(|error| format!("Could not resize the CLAP editor area: {error}"))?;
        self.connection
            .flush()
            .map_err(|error| format!("Could not update the CLAP editor area: {error}"))
    }

    pub(super) fn available_size(&self, size: Size, scale_factor: f32) -> (u32, u32) {
        EditorRect::available_size(size, scale_factor)
    }
}

impl Drop for X11PluginEditorHost {
    fn drop(&mut self) {
        let _ = self.connection.destroy_window(self.window);
        let _ = self.connection.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::EditorRect;
    use iced::Size;

    #[test]
    fn plugin_editor_area_tracks_window_size_and_scale() {
        let rect = EditorRect::from_window_size(Size::new(880.0, 560.0), 1.5);
        assert_eq!(rect.x, 420);
        assert_eq!(rect.y, 102);
        assert_eq!(rect.width, 870);
        assert_eq!(rect.height, 705);
    }

    #[test]
    fn plugin_editor_area_keeps_positive_size_in_narrow_windows() {
        let rect = EditorRect::from_window_size(Size::new(200.0, 100.0), 1.0);
        assert_eq!(rect.width, 1);
        assert_eq!(rect.height, 10);
    }

    #[test]
    fn fx_window_can_expand_to_fit_a_fixed_size_plugin_editor() {
        assert_eq!(
            super::window_size_for_editor((1080, 560), 1.0),
            Size::new(1380.0, 650.0)
        );
        assert_eq!(
            super::window_size_for_editor((1620, 840), 1.5),
            Size::new(1380.0, 650.0)
        );
    }
}
