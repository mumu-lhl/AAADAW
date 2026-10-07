use clack_common::stream::{InputStream, OutputStream};
use clack_common::utils::ClapId;
use clack_extensions::audio_ports::{
    AudioPortFlags, AudioPortInfo, AudioPortInfoWriter, AudioPortType, PluginAudioPorts,
    PluginAudioPortsImpl,
};
use clack_extensions::gui::{
    GuiApiType, GuiConfiguration, GuiSize, PluginGui, PluginGuiImpl, Window,
};
use clack_extensions::note_ports::{
    NoteDialect, NoteDialects, NotePortInfo, NotePortInfoWriter, PluginNotePorts,
    PluginNotePortsImpl,
};
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
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
#[cfg(target_os = "linux")]
use x11rb::connection::Connection;

const PLUGIN_ID: &str = "test.dynamic-clap";

struct TestSynth;
struct Shared(Arc<AtomicU8>);
struct MainThread {
    gain: Arc<AtomicU8>,
    #[cfg(target_os = "linux")]
    editor: RefCell<Option<(x11rb::rust_connection::RustConnection, u32)>>,
}
struct AudioProcessor {
    gain: Arc<AtomicU8>,
    active: bool,
}

impl PluginShared<'_> for Shared {}
impl PluginMainThread<'_, Shared> for MainThread {}

impl Plugin for TestSynth {
    type AudioProcessor<'a> = AudioProcessor;
    type Shared<'a> = Shared;
    type MainThread<'a> = MainThread;

    fn declare_extensions(builder: &mut PluginExtensions<Self>, _shared: Option<&Shared>) {
        builder.register::<PluginAudioPorts>();
        builder.register::<PluginNotePorts>();
        builder.register::<PluginState>();
        builder.register::<PluginGui>();
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
            editor: RefCell::new(None),
        })
    }
}

impl PluginAudioPortsImpl for MainThread {
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

impl PluginGuiImpl for MainThread {
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
            use x11rb::protocol::xproto::{ConnectionExt, CreateWindowAux, WindowClass};
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
            *self.editor.borrow_mut() = Some((connection, window));
        }
        Ok(())
    }

    fn destroy(&self) {
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

impl PluginNotePortsImpl for MainThread {
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

impl PluginStateImpl for MainThread {
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

impl<'a> ClackPluginAudioProcessor<'a, Shared, MainThread> for AudioProcessor {
    fn activate(
        _host: HostAudioProcessorHandle<'a>,
        _main_thread: &MainThread,
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
