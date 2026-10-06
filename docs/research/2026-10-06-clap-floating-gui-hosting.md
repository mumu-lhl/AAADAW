# Hosting floating CLAP instrument editors in the isolated helper

Issue [#204](https://github.com/mumu-lhl/AAADAW/issues/204) adds a native floating editor for an isolated instrument. The existing in-process editor only negotiates embedded X11, so it cannot be reused across the helper-process boundary.

## CLAP lifecycle and thread requirements

The official [CLAP GUI extension](https://github.com/free-audio/clap/blob/main/include/clap/ext/gui.h) requires the host to negotiate an API with `get_preferred_api` and `is_api_supported(api, is_floating)`, create the GUI, and show it. For a floating window, call `set_transient` with the DAW main window and `suggest_title` after creation when those callbacks are available. A created editor can be hidden and shown repeatedly; the host calls `destroy` when the editor is finished. `set_parent`, sizing, and resize negotiation are for embedded windows and are not used here.

The plug-in's preferred API is only a hint, so the host still checks support and can fall back to another supported native API. This implementation negotiates X11 on Linux and Win32 on Windows. Other platforms and windowing APIs receive an explicit unsupported status until their event-loop integration is available.

The [CLAP thread-check extension](https://github.com/free-audio/clap/blob/main/include/clap/ext/thread-check.h) and GUI declarations require GUI lifecycle calls on the same plug-in main thread. The host must also service `request_callback` by invoking `on_main_thread` on that thread; see [host.h](https://github.com/free-audio/clap/blob/main/include/clap/host.h) and [plugin.h](https://github.com/free-audio/clap/blob/main/include/clap/plugin.h). Those callbacks are therefore handled in the helper's main loop. Audio processing remains on its existing worker thread and receives only shared-memory audio/MIDI slots.

The CLAP specification does not mandate one cross-platform operating-system message pump. Win32 requires dispatching the helper thread's window messages (Microsoft [`GetMessage`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getmessage)); X11 toolkits receive events through their X connection/event loop (Xlib [event handling](https://www.x.org/releases/current/doc/libX11/libX11/libX11.html#Event_Handling_Functions)). Native GUI calls and user-close notifications must be relayed to the helper main thread rather than performed from the audio worker or parent DAW callback.

## Helper protocol decisions

- GUI open/close requests and lifecycle status use bounded atomic control fields in the existing per-instance shared mapping. They do not share or block the audio callback's work slots.
- The helper main thread owns the `PluginInstance`, GUI extension, API negotiation, lifecycle calls, CLAP host GUI callbacks, `on_main_thread`, and native event servicing. Linux services the GLib default context and Windows dispatches the helper thread's Win32 messages. GUI errors are reported against that helper instance.
- Closing the editor hides or destroys the GUI but keeps the plug-in instance and audio worker alive. Reopening creates/shows the same instance's editor again.
- A native plug-in fault can still terminate its helper and silence that one instrument; unrelated tracks continue. Process isolation is crash containment, not a security sandbox.
- Linux lifecycle tests use an X11-capable fixture under Xvfb. Windows CI compiles the Win32 host path and checks unsupported-API status without interrupting audio. Wayland-only sessions without X11, macOS, CLAP effects, and embedded child windows remain outside this ticket.

## Sources

- [CLAP GUI extension](https://github.com/free-audio/clap/blob/main/include/clap/ext/gui.h)
- [CLAP host callbacks](https://github.com/free-audio/clap/blob/main/include/clap/host.h)
- [CLAP plug-in callbacks](https://github.com/free-audio/clap/blob/main/include/clap/plugin.h)
- [CLAP thread-check extension](https://github.com/free-audio/clap/blob/main/include/clap/ext/thread-check.h)
- [clack 0.2.0 host GUI wrapper](https://github.com/prokopyl/clack/blob/main/clack-extensions/src/gui/host.rs)
- [Microsoft `GetMessage`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getmessage)
- [Xlib event handling](https://www.x.org/releases/current/doc/libX11/libX11/libX11.html#Event_Handling_Functions)
