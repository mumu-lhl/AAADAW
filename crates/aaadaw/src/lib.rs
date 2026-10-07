#[cfg(target_os = "android")]
mod app;
#[cfg(target_os = "android")]
mod clap_scanner;
#[cfg(target_os = "android")]
mod logging;
#[cfg(target_os = "android")]
mod timeline;

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(android_app: android_activity::AndroidApp) {
    iced_winit::set_android_app(android_app);
    let _log_guard = logging::initialize();
    if let Err(error) = app::run() {
        tracing::error!(error = %error, "AAADAW Android activity stopped with an error");
    }
}
