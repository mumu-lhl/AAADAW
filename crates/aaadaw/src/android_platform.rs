use aaadaw_core::{NoteId, TrackId};
use aaadaw_engine::{
    AndroidMidiOutputMessage, CpalMidiInputSender, CpalMidiOutputReceiver, MidiEventKind,
    MidiInputDecoder,
};
use android_activity::AndroidApp;
use jni::JavaVM;
use jni::objects::{Global, JByteArray, JObject, JString, JValue};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender, TryRecvError};
use std::sync::{Mutex, OnceLock, RwLock};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

static ANDROID_APP: OnceLock<RwLock<Option<AndroidApp>>> = OnceLock::new();
static APP_DATA_DIRECTORY: OnceLock<PathBuf> = OnceLock::new();
static SAF_LINKS: OnceLock<Mutex<HashMap<PathBuf, String>>> = OnceLock::new();
static MICROPHONE_PERMISSION_RESULT: OnceLock<Mutex<Option<tokio::sync::oneshot::Sender<bool>>>> =
    OnceLock::new();
static RECORDING_SERVICE_ACTIVE: AtomicBool = AtomicBool::new(false);
static PLAYBACK_SERVICE_ACTIVE: AtomicBool = AtomicBool::new(false);
static MIDI_INPUT_TARGET_TRACK: AtomicU64 = AtomicU64::new(0);
static MIDI_INPUT_COMMANDS: OnceLock<Sender<Option<CpalMidiInputSender>>> = OnceLock::new();
static MIDI_OUTPUT_COMMANDS: OnceLock<Sender<Option<CpalMidiOutputReceiver>>> = OnceLock::new();

pub(crate) fn initialize(app: AndroidApp) {
    if let Some(path) = app.internal_data_path() {
        let path = path.join("aaadaw");
        let _ = std::fs::create_dir_all(&path);
        let _ = APP_DATA_DIRECTORY.set(path.clone());
        let links_path = path.join("saf-links.json");
        let links = std::fs::read(&links_path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<HashMap<String, String>>(&bytes).ok())
            .unwrap_or_default()
            .into_iter()
            .map(|(path, uri)| (PathBuf::from(path), uri))
            .collect();
        let _ = SAF_LINKS.set(Mutex::new(links));
    }
    let slot = ANDROID_APP.get_or_init(|| RwLock::new(None));
    *slot
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(app);
    start_midi_workers();
}

fn start_midi_workers() {
    let (input_commands, input_receiver) = mpsc::channel();
    if MIDI_INPUT_COMMANDS.set(input_commands).is_ok() {
        let _ = thread::Builder::new()
            .name("aaadaw-android-midi-input".to_owned())
            .spawn(move || midi_input_worker(input_receiver));
    }
    let (output_commands, output_receiver) = mpsc::channel();
    if MIDI_OUTPUT_COMMANDS.set(output_commands).is_ok() {
        let _ = thread::Builder::new()
            .name("aaadaw-android-midi-output".to_owned())
            .spawn(move || midi_output_worker(output_receiver));
    }
}

fn midi_input_worker(commands: mpsc::Receiver<Option<CpalMidiInputSender>>) {
    let mut sender = None;
    let mut decoder = MidiInputDecoder::default();
    let mut messages = Vec::with_capacity(64);
    let mut active_note_tracks = HashMap::<NoteId, TrackId>::new();
    loop {
        loop {
            match commands.try_recv() {
                Ok(next) => {
                    sender = next;
                    decoder = MidiInputDecoder::default();
                    active_note_tracks.clear();
                    if sender.is_some() {
                        let _ = drain_midi_input_packets();
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }
        if let Some(sender) = &mut sender
            && let Ok(packets) = drain_midi_input_packets()
        {
            for packet in packets {
                messages.clear();
                decoder.push_bytes(&packet, &mut messages);
                for message in messages.drain(..) {
                    let selected_track =
                        TrackId::from_value(MIDI_INPUT_TARGET_TRACK.load(Ordering::Acquire));
                    let track_id = message
                        .note_id()
                        .and_then(|note_id| active_note_tracks.get(&note_id).copied())
                        .or(selected_track);
                    let Some(track_id) = track_id else {
                        continue;
                    };
                    if let Some(mut event) = message.scheduled_event(track_id) {
                        match event.kind {
                            MidiEventKind::NoteOn => {
                                if let Some(note_id) = event.note_id {
                                    let _ = active_note_tracks.insert(note_id, track_id);
                                }
                            }
                            MidiEventKind::NoteOff => {
                                if let Some(note_id) = event.note_id {
                                    event.track_id =
                                        active_note_tracks.remove(&note_id).unwrap_or(track_id);
                                }
                            }
                            MidiEventKind::ControllerChange | MidiEventKind::PitchBend => {}
                        }
                        let _ = sender.send(event);
                    }
                }
            }
        }
        thread::sleep(if sender.is_some() {
            Duration::from_millis(2)
        } else {
            Duration::from_millis(100)
        });
    }
}

fn midi_output_worker(commands: mpsc::Receiver<Option<CpalMidiOutputReceiver>>) {
    let mut receiver = None;
    let mut batch = Vec::with_capacity(256);
    loop {
        loop {
            match commands.try_recv() {
                Ok(next) => receiver = next,
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }
        if let Some(receiver) = &mut receiver {
            batch.clear();
            while batch.len() < batch.capacity()
                && let Some(message) = receiver.try_receive()
            {
                batch.push(message);
            }
            if !batch.is_empty() {
                send_android_midi_messages(&batch);
            }
        }
        thread::sleep(Duration::from_millis(1));
    }
}

pub(crate) fn set_midi_input_sender(sender: Option<CpalMidiInputSender>) {
    if let Some(commands) = MIDI_INPUT_COMMANDS.get() {
        let _ = commands.send(sender);
    }
}

pub(crate) fn set_midi_output_receiver(receiver: Option<CpalMidiOutputReceiver>) {
    if let Some(commands) = MIDI_OUTPUT_COMMANDS.get() {
        let _ = commands.send(receiver);
    }
}

pub(crate) fn set_midi_input_target_track(track_id: Option<TrackId>) {
    MIDI_INPUT_TARGET_TRACK.store(track_id.map_or(0, TrackId::value), Ordering::Release);
}

pub(crate) fn refresh_midi_devices() -> Result<(), String> {
    call_void_activity_method("refreshAndroidMidi")
}

pub(crate) fn midi_port_counts() -> Result<(usize, usize), String> {
    let app = current_app()?;
    let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) };
    vm.attach_current_thread(|env| -> jni::errors::Result<(usize, usize)> {
        let raw_activity = app.activity_as_ptr().cast();
        let activity = unsafe { env.as_cast_raw::<Global<JObject>>(&raw_activity)? };
        let summary = env
            .call_method(
                activity,
                jni::jni_str!("androidMidiPortSummary"),
                jni::jni_sig!("()Ljava/lang/String;"),
                &[],
            )?
            .l()?;
        let summary = env.cast_local::<JString>(summary)?;
        let summary = summary.try_to_string(env)?;
        let (inputs, outputs) = summary.split_once(',').unwrap_or(("0", "0"));
        Ok((inputs.parse().unwrap_or(0), outputs.parse().unwrap_or(0)))
    })
    .map_err(|error| error.to_string())
}

fn drain_midi_input_packets() -> Result<Vec<Vec<u8>>, String> {
    let app = current_app()?;
    let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) };
    let bytes = vm
        .attach_current_thread(|env| -> jni::errors::Result<Vec<u8>> {
            let raw_activity = app.activity_as_ptr().cast();
            let activity = unsafe { env.as_cast_raw::<Global<JObject>>(&raw_activity)? };
            let packed = env
                .call_method(
                    activity,
                    jni::jni_str!("drainAndroidMidiInput"),
                    jni::jni_sig!("()[B"),
                    &[],
                )?
                .l()?;
            let packed = env.cast_local::<JByteArray>(packed)?;
            env.convert_byte_array(&packed)
        })
        .map_err(|error| error.to_string())?;
    let mut packets = Vec::new();
    let mut offset = 0;
    while offset + 2 <= bytes.len() {
        let length = (usize::from(bytes[offset]) << 8) | usize::from(bytes[offset + 1]);
        offset += 2;
        let Some(end) = offset.checked_add(length) else {
            break;
        };
        if end > bytes.len() {
            break;
        }
        packets.push(bytes[offset..end].to_vec());
        offset = end;
    }
    Ok(packets)
}

fn send_android_midi_messages(messages: &[AndroidMidiOutputMessage]) {
    let Ok(app) = current_app() else {
        return;
    };
    let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) };
    if let Err(error) = vm.attach_current_thread(|env| -> jni::errors::Result<()> {
        let raw_activity = app.activity_as_ptr().cast();
        let activity = unsafe { env.as_cast_raw::<Global<JObject>>(&raw_activity)? };
        for message in messages {
            let bytes =
                env.byte_array_from_slice(&[message.status, message.data1, message.data2])?;
            let delay = (message.sample_offset as u128)
                .saturating_mul(1_000_000_000)
                .checked_div(u128::from(message.sample_rate.max(1)))
                .unwrap_or(0)
                .min(i64::MAX as u128) as i64;
            let args = [JValue::Object(bytes.as_ref()), JValue::Long(delay)];
            env.call_method(
                &activity,
                jni::jni_str!("sendAndroidMidi"),
                jni::jni_sig!("([BJ)V"),
                &args,
            )?;
        }
        Ok(())
    }) {
        tracing::warn!(error = %error, "Could not forward Android MIDI output");
    }
}

pub(crate) fn app_data_directory() -> Option<PathBuf> {
    APP_DATA_DIRECTORY.get().cloned()
}

pub(crate) fn project_directory() -> Result<PathBuf, String> {
    let path = app_data_directory()
        .ok_or_else(|| "Android app data directory is unavailable".to_owned())?
        .join("projects");
    std::fs::create_dir_all(&path).map_err(|error| error.to_string())?;
    Ok(path)
}

pub(crate) fn stage_picked_file(
    file: robius_file_picker::PickedFile,
    persistent_project: bool,
) -> Result<PathBuf, String> {
    let uri = file.uri().map(str::to_owned);
    let file_name = safe_file_name(file.file_name().unwrap_or("untitled"));
    let local = file
        .into_local_file()
        .map_err(|error| format!("could not read selected document: {error}"))?;
    let directory = if persistent_project {
        project_directory()?
    } else {
        let path = app_data_directory()
            .ok_or_else(|| "Android app data directory is unavailable".to_owned())?
            .join("imports");
        std::fs::create_dir_all(&path).map_err(|error| error.to_string())?;
        path
    };
    let destination = unique_path(&directory, &file_name)?;
    std::fs::copy(local.path(), &destination)
        .map_err(|error| format!("could not stage selected document: {error}"))?;
    if persistent_project && let Some(uri) = uri {
        register_saf_link(destination.clone(), uri)?;
    }
    Ok(destination)
}

pub(crate) fn stage_clap_plugin_file(
    file: robius_file_picker::PickedFile,
) -> Result<PathBuf, String> {
    let file_name = safe_file_name(file.file_name().unwrap_or("plugin.clap"));
    if Path::new(&file_name)
        .extension()
        .is_none_or(|extension| extension != "clap")
    {
        return Err("choose an Android ARM64 CLAP plugin with a .clap extension".to_owned());
    }
    let local = file
        .into_local_file()
        .map_err(|error| format!("could not read selected CLAP plugin: {error}"))?;
    let directory = app_data_directory()
        .ok_or_else(|| "Android app data directory is unavailable".to_owned())?
        .join("plugins");
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let destination = unique_path(&directory, &file_name)?;
    std::fs::copy(local.path(), &destination)
        .map_err(|error| format!("could not install selected CLAP plugin: {error}"))?;
    Ok(destination)
}

pub(crate) fn prepare_saf_save(
    file: robius_file_picker::PickedFile,
    project: bool,
) -> Result<PathBuf, String> {
    let uri = file
        .uri()
        .ok_or_else(|| "Android document picker did not return a content URI".to_owned())?
        .to_owned();
    let file_name = safe_file_name(file.file_name().unwrap_or("project.aaadaw"));
    drop(file);
    let directory = if project {
        project_directory()?
    } else {
        let path = app_data_directory()
            .ok_or_else(|| "Android app data directory is unavailable".to_owned())?
            .join("exports");
        std::fs::create_dir_all(&path).map_err(|error| error.to_string())?;
        path
    };
    let destination = unique_path(&directory, &file_name)?;
    register_saf_link(destination.clone(), uri)?;
    Ok(destination)
}

pub(crate) fn sync_saf_document(path: &Path) -> Result<(), String> {
    let links = SAF_LINKS
        .get()
        .ok_or_else(|| "Android document links are not initialized".to_owned())?;
    let uri = links
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(path)
        .cloned();
    let Some(uri) = uri else {
        return Ok(());
    };
    let app = ANDROID_APP
        .get()
        .and_then(|slot| {
            slot.read()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone()
        })
        .ok_or_else(|| "Android activity is unavailable for SAF write".to_owned())?;
    let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) };
    let uri = uri;
    let local_path = path
        .to_str()
        .ok_or_else(|| "project path is not valid Unicode".to_owned())?
        .to_owned();
    let written = vm
        .attach_current_thread(|env| -> jni::errors::Result<bool> {
            let raw_activity = app.activity_as_ptr().cast();
            let activity = unsafe { env.as_cast_raw::<Global<JObject>>(&raw_activity)? };
            let uri_string = env.new_string(uri)?;
            let path_string = env.new_string(local_path)?;
            let args = [
                JValue::Object(uri_string.as_ref()),
                JValue::Object(path_string.as_ref()),
            ];
            let written = env
                .call_method(
                    activity,
                    jni::jni_str!("copyLocalFileToUri"),
                    jni::jni_sig!("(Ljava/lang/String;Ljava/lang/String;)Z"),
                    &args,
                )?
                .z()?;
            Ok(written)
        })
        .map_err(|error| error.to_string())?;
    if written {
        Ok(())
    } else {
        Err("Android could not write the project to the selected document".to_owned())
    }
}

pub(crate) fn request_microphone_permission()
-> Result<Option<tokio::sync::oneshot::Receiver<bool>>, String> {
    if has_microphone_permission()? {
        return Ok(None);
    }
    let (sender, receiver) = tokio::sync::oneshot::channel();
    *MICROPHONE_PERMISSION_RESULT
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(sender);
    if let Err(error) = call_void_activity_method("requestMicrophonePermission") {
        MICROPHONE_PERMISSION_RESULT
            .get()
            .expect("permission sender was initialized")
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        return Err(error);
    }
    Ok(Some(receiver))
}

pub(crate) fn start_recording_service() -> Result<(), String> {
    if RECORDING_SERVICE_ACTIVE.swap(true, Ordering::AcqRel) {
        return Ok(());
    }
    if let Err(error) = call_void_activity_method("startRecordingService") {
        RECORDING_SERVICE_ACTIVE.store(false, Ordering::Release);
        return Err(error);
    }
    Ok(())
}

pub(crate) fn stop_recording_service() {
    if RECORDING_SERVICE_ACTIVE.swap(false, Ordering::AcqRel) {
        let _ = call_void_activity_method("stopRecordingService");
    }
}

pub(crate) fn start_playback_service() -> Result<(), String> {
    if PLAYBACK_SERVICE_ACTIVE.swap(true, Ordering::AcqRel) {
        return Ok(());
    }
    if let Err(error) = call_void_activity_method("startPlaybackService") {
        PLAYBACK_SERVICE_ACTIVE.store(false, Ordering::Release);
        return Err(error);
    }
    Ok(())
}

pub(crate) fn stop_playback_service() {
    if PLAYBACK_SERVICE_ACTIVE.swap(false, Ordering::AcqRel) {
        let _ = call_void_activity_method("stopPlaybackService");
    }
}

fn has_microphone_permission() -> Result<bool, String> {
    let app = current_app()?;
    let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) };
    vm.attach_current_thread(|env| -> jni::errors::Result<bool> {
        let raw_activity = app.activity_as_ptr().cast();
        let activity = unsafe { env.as_cast_raw::<Global<JObject>>(&raw_activity)? };
        env.call_method(
            activity,
            jni::jni_str!("hasMicrophonePermission"),
            jni::jni_sig!("()Z"),
            &[],
        )?
        .z()
    })
    .map_err(|error| error.to_string())
}

fn call_void_activity_method(method: &str) -> Result<(), String> {
    let app = current_app()?;
    let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) };
    vm.attach_current_thread(|env| -> jni::errors::Result<()> {
        let raw_activity = app.activity_as_ptr().cast();
        let activity = unsafe { env.as_cast_raw::<Global<JObject>>(&raw_activity)? };
        match method {
            "requestMicrophonePermission" => env.call_method(
                activity,
                jni::jni_str!("requestMicrophonePermission"),
                jni::jni_sig!("()V"),
                &[],
            )?,
            "startRecordingService" => env.call_method(
                activity,
                jni::jni_str!("startRecordingService"),
                jni::jni_sig!("()V"),
                &[],
            )?,
            "stopRecordingService" => env.call_method(
                activity,
                jni::jni_str!("stopRecordingService"),
                jni::jni_sig!("()V"),
                &[],
            )?,
            "startPlaybackService" => env.call_method(
                activity,
                jni::jni_str!("startPlaybackService"),
                jni::jni_sig!("()V"),
                &[],
            )?,
            "stopPlaybackService" => env.call_method(
                activity,
                jni::jni_str!("stopPlaybackService"),
                jni::jni_sig!("()V"),
                &[],
            )?,
            "refreshAndroidMidi" => env.call_method(
                activity,
                jni::jni_str!("refreshAndroidMidi"),
                jni::jni_sig!("()V"),
                &[],
            )?,
            _ => unreachable!("unsupported Android activity method"),
        };
        Ok(())
    })
    .map_err(|error| error.to_string())
}

fn current_app() -> Result<AndroidApp, String> {
    ANDROID_APP
        .get()
        .and_then(|slot| {
            slot.read()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone()
        })
        .ok_or_else(|| "Android activity is unavailable".to_owned())
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aaadaw_app_MainActivity_nativeMicrophonePermissionResult(
    _env: *mut jni::sys::JNIEnv,
    _activity: jni::sys::jobject,
    granted: jni::sys::jboolean,
) {
    if let Some(sender) = MICROPHONE_PERMISSION_RESULT.get().and_then(|slot| {
        slot.lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
    }) {
        let _ = sender.send(granted);
    }
}

fn register_saf_link(path: PathBuf, uri: String) -> Result<(), String> {
    let links = SAF_LINKS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut links = links
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    links.insert(path, uri);
    let serializable = links
        .iter()
        .map(|(path, uri)| (path.to_string_lossy().into_owned(), uri.clone()))
        .collect::<HashMap<_, _>>();
    let bytes = serde_json::to_vec(&serializable).map_err(|error| error.to_string())?;
    let root = app_data_directory()
        .ok_or_else(|| "Android app data directory is unavailable".to_owned())?;
    let mut temp = tempfile::NamedTempFile::new_in(&root).map_err(|error| error.to_string())?;
    std::io::Write::write_all(&mut temp, &bytes).map_err(|error| error.to_string())?;
    temp.as_file()
        .sync_all()
        .map_err(|error| error.to_string())?;
    temp.persist(root.join("saf-links.json"))
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn unique_path(directory: &Path, file_name: &str) -> Result<PathBuf, String> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let parent = directory.join(timestamp.to_string());
    std::fs::create_dir_all(&parent).map_err(|error| error.to_string())?;
    Ok(parent.join(file_name.replace('/', "_")))
}

fn safe_file_name(file_name: &str) -> String {
    let name = Path::new(file_name)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("untitled");
    let cleaned = name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    if cleaned.is_empty() {
        "untitled".to_owned()
    } else {
        cleaned
    }
}
