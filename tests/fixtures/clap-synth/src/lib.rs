use clack_common::stream::{InputStream, OutputStream};
use clack_common::utils::ClapId;
use clack_extensions::audio_ports::{
    AudioPortFlags, AudioPortInfo, AudioPortInfoWriter, AudioPortType, PluginAudioPorts,
    PluginAudioPortsImpl,
};
#[cfg(target_os = "linux")]
use clack_extensions::gui::HostGui;
use clack_extensions::gui::{
    GuiApiType, GuiConfiguration, GuiSize, PluginGui, PluginGuiImpl, Window,
};
use clack_extensions::note_ports::{
    NoteDialect, NoteDialects, NotePortInfo, NotePortInfoWriter, PluginNotePorts,
    PluginNotePortsImpl,
};
#[cfg(target_os = "linux")]
use clack_extensions::posix_fd::{FdFlags, HostPosixFd, PluginPosixFd, PluginPosixFdImpl};
use clack_extensions::state::{PluginState, PluginStateImpl};
use clack_plugin::entry::SinglePluginEntry;
use clack_plugin::events::spaces::CoreEventSpace;
use clack_plugin::plugin::{
    PluginAudioProcessor as ClackPluginAudioProcessor, features as plugin_features,
};
use clack_plugin::prelude::*;
#[cfg(target_os = "linux")]
use std::cell::RefCell;
use std::io::{Read, Write};
#[cfg(target_os = "linux")]
use std::os::fd::AsRawFd;
#[cfg(target_os = "linux")]
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
#[cfg(target_os = "linux")]
use x11rb::connection::Connection;
#[cfg(target_os = "linux")]
use x11rb::wrapper::ConnectionExt as WrapperConnectionExt;

const PLUGIN_ID: &str = "test.dynamic-clap";

struct TestSynth;
struct Shared(Arc<AtomicU8>);
struct MainThread<'a> {
    gain: Arc<AtomicU8>,
    #[cfg(target_os = "linux")]
    host: HostMainThreadHandle<'a>,
    #[cfg(not(target_os = "linux"))]
    _host: std::marker::PhantomData<&'a ()>,
    #[cfg(target_os = "linux")]
    host_posix_fd: Option<HostPosixFd>,
    #[cfg(target_os = "linux")]
    host_gui: Option<HostGui>,
    #[cfg(target_os = "linux")]
    main_thread_id: std::thread::ThreadId,
    #[cfg(target_os = "linux")]
    glib_fd_source: RefCell<Option<glib::SourceId>>,
    #[cfg(target_os = "linux")]
    glib_signal: RefCell<Option<UnixStream>>,
    #[cfg(target_os = "linux")]
    editor: RefCell<Option<(x11rb::rust_connection::RustConnection, u32)>>,
}
struct AudioProcessor {
    gain: Arc<AtomicU8>,
    active: bool,
}

impl PluginShared<'_> for Shared {}
impl<'a> PluginMainThread<'a, Shared> for MainThread<'a> {}

impl Plugin for TestSynth {
    type AudioProcessor<'a> = AudioProcessor;
    type Shared<'a> = Shared;
    type MainThread<'a> = MainThread<'a>;

    fn declare_extensions(builder: &mut PluginExtensions<Self>, _shared: Option<&Shared>) {
        builder.register::<PluginAudioPorts>();
        builder.register::<PluginNotePorts>();
        builder.register::<PluginState>();
        builder.register::<PluginGui>();
        #[cfg(target_os = "linux")]
        builder.register::<PluginPosixFd>();
    }
}

impl DefaultPluginFactory for TestSynth {
    fn get_descriptor() -> PluginDescriptor {
        PluginDescriptor::new(PLUGIN_ID, "AAADAW Child State Test Synth")
            .with_features([plugin_features::INSTRUMENT, plugin_features::SYNTHESIZER])
    }

    fn new_shared(_host: HostSharedHandle<'_>) -> Result<Self::Shared<'_>, PluginError> {
        Ok(Shared(Arc::new(AtomicU8::new(32))))
    }

    fn new_main_thread<'a>(
        _host: HostMainThreadHandle<'a>,
        shared: &'a Self::Shared<'a>,
    ) -> Result<Self::MainThread<'a>, PluginError> {
        Ok(MainThread {
            gain: Arc::clone(&shared.0),
            #[cfg(target_os = "linux")]
            host: _host,
            #[cfg(not(target_os = "linux"))]
            _host: std::marker::PhantomData,
            #[cfg(target_os = "linux")]
            host_posix_fd: _host.get_extension::<HostPosixFd>(),
            #[cfg(target_os = "linux")]
            host_gui: _host.get_extension::<HostGui>(),
            #[cfg(target_os = "linux")]
            main_thread_id: std::thread::current().id(),
            #[cfg(target_os = "linux")]
            glib_fd_source: RefCell::new(None),
            #[cfg(target_os = "linux")]
            glib_signal: RefCell::new(None),
            #[cfg(target_os = "linux")]
            editor: RefCell::new(None),
        })
    }
}

impl PluginAudioPortsImpl for MainThread<'_> {
    fn count(&self, is_input: bool) -> u32 {
        u32::from(!is_input)
    }

    fn get(&self, index: u32, is_input: bool, writer: &mut AudioPortInfoWriter) {
        if index == 0 && !is_input {
            writer.set(&AudioPortInfo {
                id: ClapId::new(0),
                name: b"Stereo Out",
                channel_count: 2,
                flags: AudioPortFlags::IS_MAIN,
                port_type: Some(AudioPortType::STEREO),
                in_place_pair: None,
            });
        }
    }
}

impl PluginGuiImpl for MainThread<'_> {
    fn is_api_supported(&self, configuration: GuiConfiguration<'_>) -> bool {
        configuration.is_floating && configuration.api_type == GuiApiType::X11
    }

    fn get_preferred_api(&self) -> Option<GuiConfiguration<'_>> {
        Some(GuiConfiguration {
            api_type: GuiApiType::X11,
            is_floating: true,
        })
    }

    fn create(&self, configuration: GuiConfiguration<'_>) -> Result<(), PluginError> {
        if !self.is_api_supported(configuration) {
            return Err(PluginError::Message("Fixture requires floating X11"));
        }
        #[cfg(target_os = "linux")]
        {
            use x11rb::connection::Connection;
            use x11rb::protocol::xproto::{
                AtomEnum, ConnectionExt, CreateWindowAux, EventMask, PropMode, WindowClass,
            };
            let (connection, screen_index) =
                x11rb::connect(None).map_err(|_| PluginError::Message("X11 connect failed"))?;
            let screen = connection
                .setup()
                .roots
                .get(screen_index)
                .ok_or(PluginError::Message("X11 screen unavailable"))?;
            let window = connection
                .generate_id()
                .map_err(|_| PluginError::Message("X11 window allocation failed"))?;
            connection
                .create_window(
                    0,
                    window,
                    screen.root,
                    0,
                    0,
                    320,
                    200,
                    0,
                    WindowClass::INPUT_OUTPUT,
                    x11rb::COPY_FROM_PARENT,
                    &CreateWindowAux::new().background_pixel(screen.white_pixel),
                )
                .map_err(|_| PluginError::Message("X11 window creation failed"))?
                .check()
                .map_err(|_| PluginError::Message("X11 window creation failed"))?;
            connection
                .change_window_attributes(
                    window,
                    &x11rb::protocol::xproto::ChangeWindowAttributesAux::new().event_mask(
                        EventMask::EXPOSURE | EventMask::KEY_PRESS | EventMask::STRUCTURE_NOTIFY,
                    ),
                )
                .map_err(|_| PluginError::Message("X11 event selection failed"))?
                .check()
                .map_err(|_| PluginError::Message("X11 event selection failed"))?;
            connection
                .change_property8(
                    PropMode::REPLACE,
                    window,
                    AtomEnum::WM_NAME,
                    AtomEnum::STRING,
                    format!("AAADAW CLAP fd fixture {}", std::process::id()).as_bytes(),
                )
                .map_err(|_| PluginError::Message("X11 window naming failed"))?
                .check()
                .map_err(|_| PluginError::Message("X11 window naming failed"))?;
            let posix_fd = self
                .host_posix_fd
                .ok_or(PluginError::Message("Host lacks POSIX fd support"))?;
            posix_fd
                .register_fd(&self.host, connection.stream().as_raw_fd(), FdFlags::READ)
                .map_err(|_| PluginError::Message("POSIX fd registration failed"))?;
            posix_fd
                .modify_fd(
                    &self.host,
                    connection.stream().as_raw_fd(),
                    FdFlags::READ | FdFlags::WRITE,
                )
                .map_err(|_| PluginError::Message("POSIX fd modification failed"))?;
            posix_fd
                .modify_fd(&self.host, connection.stream().as_raw_fd(), FdFlags::READ)
                .map_err(|_| PluginError::Message("POSIX fd modification failed"))?;
            let (mut reader, writer) =
                UnixStream::pair().map_err(|_| PluginError::Message("GLib fd pair failed"))?;
            reader
                .set_nonblocking(true)
                .map_err(|_| PluginError::Message("GLib fd setup failed"))?;
            let gain = Arc::clone(&self.gain);
            let main_thread_id = self.main_thread_id;
            let source = glib::source::unix_fd_add_local(
                reader.as_raw_fd(),
                glib::IOCondition::IN,
                move |_, _| {
                    let mut signal = [0];
                    if std::thread::current().id() == main_thread_id
                        && reader.read(&mut signal).is_ok()
                    {
                        gain.store(96, Ordering::Relaxed);
                    }
                    glib::ControlFlow::Continue
                },
            );
            *self.glib_fd_source.borrow_mut() = Some(source);
            *self.glib_signal.borrow_mut() = Some(writer);
            *self.editor.borrow_mut() = Some((connection, window));
        }
        Ok(())
    }

    fn destroy(&self) {
        #[cfg(target_os = "linux")]
        if std::thread::current().id() != self.main_thread_id {
            self.gain.store(0, Ordering::Relaxed);
        }
        #[cfg(target_os = "linux")]
        if let Some(source) = self.glib_fd_source.borrow_mut().take() {
            source.remove();
            self.glib_signal.borrow_mut().take();
        }
        #[cfg(target_os = "linux")]
        if let Some((connection, _)) = self.editor.borrow().as_ref()
            && let Some(posix_fd) = self.host_posix_fd
            && posix_fd
                .unregister_fd(&self.host, connection.stream().as_raw_fd())
                .is_err()
        {
            self.gain.store(0, Ordering::Relaxed);
        }
        #[cfg(target_os = "linux")]
        if let Some((connection, window)) = self.editor.borrow_mut().take() {
            use x11rb::protocol::xproto::ConnectionExt;
            let _ = connection.destroy_window(window);
            let _ = connection.flush();
        }
    }

    fn set_scale(&self, _scale: f64) -> Result<(), PluginError> {
        Ok(())
    }

    fn get_size(&self) -> Option<GuiSize> {
        Some(GuiSize {
            width: 320,
            height: 200,
        })
    }

    fn set_size(&self, _size: GuiSize) -> Result<(), PluginError> {
        Ok(())
    }

    fn set_parent(&self, _window: Window<'_, '_>) -> Result<(), PluginError> {
        Err(PluginError::Message("Fixture uses a floating editor"))
    }

    fn set_transient(&self, _window: Window<'_, '_>) -> Result<(), PluginError> {
        Ok(())
    }

    fn show(&self) -> Result<(), PluginError> {
        #[cfg(target_os = "linux")]
        if let Some((connection, window)) = self.editor.borrow().as_ref() {
            use x11rb::protocol::xproto::ConnectionExt;
            connection
                .map_window(*window)
                .map_err(|_| PluginError::Message("X11 map failed"))?
                .check()
                .map_err(|_| PluginError::Message("X11 map failed"))?;
            connection
                .flush()
                .map_err(|_| PluginError::Message("X11 flush failed"))?;
        }
        Ok(())
    }

    fn hide(&self) -> Result<(), PluginError> {
        #[cfg(target_os = "linux")]
        if let Some((connection, window)) = self.editor.borrow().as_ref() {
            use x11rb::protocol::xproto::ConnectionExt;
            connection
                .unmap_window(*window)
                .map_err(|_| PluginError::Message("X11 unmap failed"))?
                .check()
                .map_err(|_| PluginError::Message("X11 unmap failed"))?;
            connection
                .flush()
                .map_err(|_| PluginError::Message("X11 flush failed"))?;
        }
        Ok(())
    }
}

impl PluginNotePortsImpl for MainThread<'_> {
    fn count(&self, is_input: bool) -> u32 {
        u32::from(is_input)
    }

    fn get(&self, index: u32, is_input: bool, writer: &mut NotePortInfoWriter<'_>) {
        if index == 0 && is_input {
            writer.set(&NotePortInfo {
                id: ClapId::new(1),
                name: b"MIDI In",
                supported_dialects: NoteDialects::CLAP | NoteDialects::MIDI,
                preferred_dialect: Some(NoteDialect::Clap),
            });
        }
    }
}

impl PluginStateImpl for MainThread<'_> {
    fn save(&self, output: &mut OutputStream) -> Result<(), PluginError> {
        output.write_all(&[self.gain.load(Ordering::Relaxed)])?;
        Ok(())
    }

    fn load(&self, input: &mut InputStream) -> Result<(), PluginError> {
        let mut gain = [0];
        input.read_exact(&mut gain)?;
        self.gain.store(gain[0], Ordering::Relaxed);
        Ok(())
    }
}

#[cfg(target_os = "linux")]
impl PluginPosixFdImpl for MainThread<'_> {
    fn on_fd(&self, fd: i32, flags: FdFlags) {
        if !flags.contains(FdFlags::READ) || std::thread::current().id() != self.main_thread_id {
            return;
        }
        let editor = self.editor.borrow();
        let Some((connection, _)) = editor.as_ref() else {
            return;
        };
        if connection.stream().as_raw_fd() != fd {
            return;
        }
        while let Ok(Some(event)) = connection.poll_for_event() {
            match event {
                x11rb::protocol::Event::Expose(_) => self.gain.store(64, Ordering::Relaxed),
                x11rb::protocol::Event::KeyPress(_) => {
                    if let Some(signal) = self.glib_signal.borrow_mut().as_mut() {
                        let _ = signal.write_all(&[1]);
                    }
                }
                x11rb::protocol::Event::DestroyNotify(_) => {
                    if let Some(host_gui) = self.host_gui {
                        host_gui.closed(&self.host.shared(), true);
                    }
                }
                _ => {}
            }
        }
    }
}

impl<'a> ClackPluginAudioProcessor<'a, Shared, MainThread<'a>> for AudioProcessor {
    fn activate(
        _host: HostAudioProcessorHandle<'a>,
        _main_thread: &MainThread<'a>,
        shared: &'a Shared,
        _audio_config: PluginAudioConfiguration,
    ) -> Result<Self, PluginError> {
        Ok(Self {
            gain: Arc::clone(&shared.0),
            active: false,
        })
    }

    fn process(
        &mut self,
        _process: Process,
        mut audio: Audio,
        events: Events,
    ) -> Result<ProcessStatus, PluginError> {
        let frames = audio.frames_count() as usize;
        let mut input_events = events.input.iter().peekable();
        let mut output = audio
            .output_port(0)
            .expect("stereo output port is available");
        let mut channels = output
            .channels()
            .expect("output buffer is valid")
            .into_f32()
            .expect("host uses f32 output")
            .into_iter();
        let left = channels.next().expect("left output channel");
        let right = channels.next().expect("right output channel");

        for frame in 0..frames {
            while input_events
                .peek()
                .is_some_and(|event| event.header().time() as usize <= frame)
            {
                match input_events.next().and_then(|event| event.as_core_event()) {
                    Some(CoreEventSpace::NoteOn(_)) => self.active = true,
                    Some(CoreEventSpace::NoteOff(_)) => self.active = false,
                    Some(CoreEventSpace::Midi(event)) => {
                        let [status, controller, value] = event.data();
                        if status & 0xf0 == 0xb0 {
                            if controller == 7 {
                                self.gain.store(value, Ordering::Relaxed);
                            } else if matches!(controller, 120 | 123) {
                                self.active = false;
                            }
                        }
                    }
                    _ => {}
                }
            }
            let sample = if self.active {
                f32::from(self.gain.load(Ordering::Relaxed)) / 127.0
            } else {
                0.0
            };
            left[frame] = sample;
            right[frame] = sample;
        }
        Ok(ProcessStatus::Continue)
    }
}

clack_export_entry!(SinglePluginEntry::<TestSynth>);
