# CLAP POSIX file descriptor support for native editor event loops

Issue [#206](https://github.com/mumu-lhl/AAADAW/issues/206) considers extending the isolated CLAP editor helper beyond its current GLib/X11 event pumping. The CLAP `clap.posix-fd-support` extension lets a plug-in ask the host to watch file-descriptor readiness and dispatch readiness back to the plug-in. It is an event-reactor bridge, not a request for the host to run a toolkit's event loop.

## Required call thread and dispatch

The official [CLAP POSIX FD header](https://github.com/free-audio/clap/blob/main/include/clap/ext/posix-fd-support.h) marks `register_fd`, `modify_fd`, `unregister_fd`, and the plug-in's `on_fd` callback as `[main-thread]`. Therefore the helper must invoke host callbacks on the same CLAP plug-in main thread and invoke `on_fd` there after readiness is observed. These calls must not run in the audio worker. The host advertises support by returning `CLAP_EXT_POSIX_FD_SUPPORT` from its host-extension query; the extension is optional, so plug-ins may omit it and a plug-in may decline to use it.

Clack 0.2.0 preserves this contract in its [`posix_fd` wrapper](https://github.com/prokopyl/clack/blob/27ca28391b57a61bad4bb5099288a7bf9247350d/extensions/src/posix_fd.rs): host registration methods are `HostPosixFdImpl` methods for the host main-thread handler, and `PluginPosixFd::on_fd` accepts a `PluginMainThreadHandle`. Its FFI wrappers convert the boolean CLAP results to `Result`/`FdError`; they do not provide a background dispatcher or move callbacks to another thread. The matching [clack host thread model](https://github.com/prokopyl/clack/blob/27ca28391b57a61bad4bb5099288a7bf9247350d/clack-host/src/host.rs) likewise represents main-thread callbacks separately from audio-thread callbacks.

## Descriptor lifecycle and readiness

The plug-in passes an integer POSIX descriptor and an interest mask to `register_fd`; `modify_fd` replaces the mask and `unregister_fd` removes the watch. The header describes these as reactor operations and contains no descriptor transfer, duplication, or close operation. Thus registration should be treated as a borrowed readiness watch: the plug-in remains responsible for opening and closing its descriptor, and should unregister it before closing or reusing its descriptor number. The host must not close the plug-in's descriptor. This ownership rule is inferred from the API shape and ordinary POSIX descriptor ownership; CLAP does not separately specify a transfer protocol.

`READ`, `WRITE`, and `ERROR` are both the requested-interest and reported-event bits. When a requested condition becomes ready, the host calls `on_fd(fd, flags)` with the original descriptor and the conditions observed. The callback is **level-triggered**: a descriptor that remains writable can cause repeated callbacks. The plug-in should drain/read or write as appropriate, or use `modify_fd` to remove an interest (especially `WRITE`) until it needs it again. The implementation should avoid a busy loop if readiness remains asserted and the callback cannot make progress. See the header comments and clack's [`FdFlags` / `PluginPosixFdImpl` documentation](https://github.com/prokopyl/clack/blob/27ca28391b57a61bad4bb5099288a7bf9247350d/extensions/src/posix_fd.rs).

## Toolkit-loop limitations

A POSIX fd reactor can notify a plug-in that an fd is readable or writable; it does not process the toolkit's timers, posted events, queued callbacks, input method integration, or native window messages on the plug-in's behalf. Qt's [`QSocketNotifier`](https://doc.qt.io/qt-6/qsocketnotifier.html) routes fd readiness into Qt, while [`QCoreApplication::exec()`](https://doc.qt.io/qt-6/qcoreapplication.html#exec) runs the Qt event loop that dispatches events; readiness notification alone is not Qt event dispatch. Likewise, an X11 connection fd can signal that events may be available, but Xlib still requires its event-reading/dispatch APIs such as [`XNextEvent`](https://www.x.org/releases/current/doc/libX11/libX11/libX11.html#Event_Handling_Functions) / `XPending` to consume and handle them.

Consequently, implementing CLAP POSIX FD support would improve compatibility for plug-ins that explicitly use this host extension to integrate their asynchronous I/O into the host reactor. It would not make the existing GLib main-context pump a general Qt/Xlib event loop, and it cannot guarantee arbitrary editors become interactive. A Qt-based plug-in must either integrate with the host through the CLAP fd callbacks or have a suitable Qt event loop on its required main thread; blindly adding its X connection fd to a poller does not dispatch Qt. Conversely, if the plug-in does use the extension, the helper must poll all registered descriptors without blocking CLAP main-thread lifecycle/UI requests, then dispatch each readiness notification on that same main thread.

## AAADAW implementation

The isolated Linux CLAP helper advertises the optional host extension and registers each descriptor with a GLib Unix-fd source. Readiness is queued and dispatched as `PluginPosixFd::on_fd` on the helper's CLAP main thread, after a nonblocking GLib context iteration. Unregister removes the source and queued events before the plug-in can close or reuse its descriptor. The deterministic X11 fixture exercises both GLib source dispatch and the host POSIX-fd path under Xvfb: expose and key events are serviced, a user-side window destroy reports CLAP GUI closure, and rendered audio remains active throughout. The host does not run arbitrary toolkit loops; this fixture proves the explicit GLib/fd integrations only.

## Sources

- [Official CLAP POSIX FD support header](https://github.com/free-audio/clap/blob/main/include/clap/ext/posix-fd-support.h)
- [Official CLAP host extension query API](https://github.com/free-audio/clap/blob/main/include/clap/host.h)
- [Clack 0.2.0 POSIX FD wrappers at the exact Cargo source revision](https://github.com/prokopyl/clack/blob/27ca28391b57a61bad4bb5099288a7bf9247350d/extensions/src/posix_fd.rs)
- [Clack 0.2.0 host thread model at the exact Cargo source revision](https://github.com/prokopyl/clack/blob/27ca28391b57a61bad4bb5099288a7bf9247350d/clack-host/src/host.rs)
- [Qt 6 `QSocketNotifier`](https://doc.qt.io/qt-6/qsocketnotifier.html)
- [Xlib event handling](https://www.x.org/releases/current/doc/libX11/libX11/libX11.html#Event_Handling_Functions)
