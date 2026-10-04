use aaadaw_core::{
    DawAction, GridFraction, MidiItem, MidiNoteData, Project, TempoCurve, TrackId,
    VolumeAutomationPoint,
};
use aaadaw_storage::{ProjectSessionLock, ProjectStore};
use rmcp::{
    ErrorData as McpError, ServerHandler, ServiceExt,
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, Implementation,
        ListResourceTemplatesResult, ListResourcesResult, ListToolsResult, PaginatedRequestParams,
        ProtocolVersion, ReadResourceRequestParams, ReadResourceResponse, ReadResourceResult,
        Resource, ResourceContents, ResourceTemplate, ServerCapabilities, ServerConfig, Tool,
        ToolAnnotations,
    },
    service::{RequestContext, RoleServer},
};
use serde_json::{Value, json};
use std::{
    error::Error,
    path::Path,
    sync::{Arc, Mutex},
};

const STRUCTURE_URI: &str = "daw://project/structure";
const MIDI_SUMMARY_TEMPLATE: &str = "daw://project/track/{track_id}/midi_summary";
const MAX_MAP_POINTS: usize = 256;
const MAX_TRACKS: usize = 512;
const MIDI_QUERY_TOOL: &str = "daw_scoped_query_notes";
const MAX_NOTE_RESULTS: usize = 512;
const DEFAULT_NOTE_RESULTS: usize = 256;
const MAX_NOTE_QUERY_TICKS: u64 = 245_760;
const CREATE_TRACK_TOOL: &str = "daw_create_track";
const MAX_TRACK_NAME_CHARS: usize = 128;
const INSERT_MIDI_NOTES_TOOL: &str = "daw_insert_midi_notes";
const MAX_MIDI_NOTES_PER_INSERT: usize = 512;
const QUANTIZE_MIDI_ITEM_TOOL: &str = "daw_quantize_midi_item";
const SET_VOLUME_AUTOMATION_POINT_TOOL: &str = "daw_set_volume_automation_point";
const SET_TRACK_RECORD_ARM_TOOL: &str = "daw_set_track_record_arm";

pub fn run(project_path: impl AsRef<Path>, writable: bool) -> Result<(), Box<dyn Error>> {
    let project_path = project_path.as_ref();
    let (project, store, _session_lock) = if writable {
        if !project_path.is_file() {
            return Err(format!("project file {} does not exist", project_path.display()).into());
        }
        let session_lock = ProjectSessionLock::acquire(project_path)?;
        let store = ProjectStore::open(project_path)?;
        let project = store.load()?;
        (project, Some(store), Some(session_lock))
    } else {
        (ProjectStore::load_read_only(project_path)?, None, None)
    };
    let project = Arc::new(Mutex::new(project));
    let store = Arc::new(Mutex::new(store));
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        let server = ProjectMcpServer {
            project,
            store: store.clone(),
            writable,
        };
        let service = server.serve(rmcp::transport::stdio()).await?;
        service.waiting().await?;
        if let Some(store) = store
            .lock()
            .map_err(|_| "project store lock was poisoned")?
            .take()
        {
            store.close()?;
        }
        Ok::<_, Box<dyn Error>>(())
    })
}

struct ProjectMcpServer {
    project: Arc<Mutex<Project>>,
    store: Arc<Mutex<Option<ProjectStore>>>,
    writable: bool,
}

impl ServerHandler for ProjectMcpServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(
            ServerCapabilities::builder()
                .enable_resources()
                .enable_tools()
                .build(),
        )
        .with_server_info(Implementation::new("aaadaw", env!("CARGO_PKG_VERSION")))
        .with_protocol_version(ProtocolVersion::default())
        .with_instructions(
                if self.writable {
                    "Authorized writer for one AAADAW project session. All changes are validated through DawAction and saved atomically."
                } else {
                    "Read-only snapshot of the saved AAADAW project loaded at server startup."
                },
        )
    }

    fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListResourcesResult, McpError>> + Send + '_ {
        let resource = Resource::new(STRUCTURE_URI, "Project structure")
            .with_description("Track names and types, tempo map, and meter map.");
        std::future::ready(Ok(ListResourcesResult::with_all_items(vec![resource])))
    }

    fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListResourceTemplatesResult, McpError>> + Send + '_
    {
        let template = ResourceTemplate::new(MIDI_SUMMARY_TEMPLATE, "Track MIDI summary")
            .with_description("Bounded aggregate counts and tick range for one track.");
        std::future::ready(Ok(ListResourceTemplatesResult::with_all_items(vec![
            template,
        ])))
    }

    fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ReadResourceResponse, McpError>> + Send + '_ {
        let result = self
            .project
            .lock()
            .map_err(|_| McpError::internal_error("project lock was poisoned", None))
            .and_then(|project| {
                if request.uri == STRUCTURE_URI {
                    serde_json::to_string(&structure_summary(&project))
                        .map(|text| (request.uri, text))
                        .map_err(|error| McpError::internal_error(error.to_string(), None))
                } else if let Some(track_id) = parse_track_summary_uri(&request.uri) {
                    project
                        .tracks()
                        .iter()
                        .find(|track| track.id().value() == track_id)
                        .ok_or_else(|| McpError::invalid_params("unknown track id", None))
                        .and_then(|track| {
                            serde_json::to_string(&track_midi_summary(&project, track.id()))
                                .map(|text| (request.uri, text))
                                .map_err(|error| McpError::internal_error(error.to_string(), None))
                        })
                } else {
                    Err(McpError::invalid_params(
                        "unknown project resource URI",
                        None,
                    ))
                }
            });
        std::future::ready(result.map(|(uri, text)| {
            ReadResourceResult::new(vec![
                ResourceContents::text(text, uri).with_mime_type("application/json"),
            ])
            .into()
        }))
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListToolsResult, McpError>> + Send + '_ {
        let mut tools = vec![midi_query_tool()];
        if self.writable {
            tools.push(create_track_tool());
            tools.push(insert_midi_notes_tool());
            tools.push(quantize_midi_item_tool());
            tools.push(set_volume_automation_point_tool());
            tools.push(set_track_record_arm_tool());
        }
        std::future::ready(Ok(ListToolsResult::with_all_items(tools)))
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        match name {
            MIDI_QUERY_TOOL => Some(midi_query_tool()),
            CREATE_TRACK_TOOL if self.writable => Some(create_track_tool()),
            INSERT_MIDI_NOTES_TOOL if self.writable => Some(insert_midi_notes_tool()),
            QUANTIZE_MIDI_ITEM_TOOL if self.writable => Some(quantize_midi_item_tool()),
            SET_VOLUME_AUTOMATION_POINT_TOOL if self.writable => {
                Some(set_volume_automation_point_tool())
            }
            SET_TRACK_RECORD_ARM_TOOL if self.writable => Some(set_track_record_arm_tool()),
            _ => None,
        }
    }

    fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<CallToolResponse, McpError>> + Send + '_ {
        let result = match request.name.as_ref() {
            MIDI_QUERY_TOOL => self
                .project
                .lock()
                .map_err(|_| "project lock was poisoned".to_owned())
                .and_then(|project| {
                    parse_note_query_arguments(request.arguments.as_ref()).and_then(
                        |(track_id, start_tick, end_tick, limit)| {
                            scoped_query_notes(&project, track_id, start_tick, end_tick, limit)
                        },
                    )
                }),
            CREATE_TRACK_TOOL if self.writable => {
                parse_create_track_arguments(request.arguments.as_ref())
                    .and_then(|name| self.create_track(name))
            }
            INSERT_MIDI_NOTES_TOOL if self.writable => {
                parse_insert_midi_notes_arguments(request.arguments.as_ref()).and_then(
                    |(track_id, item_id, notes)| self.insert_midi_notes(track_id, item_id, notes),
                )
            }
            QUANTIZE_MIDI_ITEM_TOOL if self.writable => parse_quantize_midi_item_arguments(
                request.arguments.as_ref(),
            )
            .and_then(|(track_id, item_id, numerator, denominator, strength)| {
                self.quantize_midi_item(track_id, item_id, numerator, denominator, strength)
            }),
            SET_VOLUME_AUTOMATION_POINT_TOOL if self.writable => {
                parse_volume_automation_point_arguments(request.arguments.as_ref()).and_then(
                    |(track_id, sample, gain_db)| {
                        self.set_volume_automation_point(track_id, sample, gain_db)
                    },
                )
            }
            SET_TRACK_RECORD_ARM_TOOL if self.writable => {
                parse_record_arm_arguments(request.arguments.as_ref())
                    .and_then(|(track_id, armed)| self.set_track_record_arm(track_id, armed))
            }
            _ => Err("unknown or unavailable tool".to_owned()),
        };
        std::future::ready(Ok(match result {
            Ok(value) => CallToolResult::structured(value),
            Err(message) => CallToolResult::structured_error(json!({"error": message})),
        }
        .into()))
    }
}

impl ProjectMcpServer {
    fn create_track(&self, name: String) -> Result<Value, String> {
        let mut project = self
            .project
            .lock()
            .map_err(|_| "project lock was poisoned".to_owned())?;
        let mut store = self
            .store
            .lock()
            .map_err(|_| "project store lock was poisoned".to_owned())?;
        let store = store
            .as_mut()
            .ok_or_else(|| "project was opened read-only".to_owned())?;
        let index = project.tracks().len();
        project
            .apply(DawAction::CreateTrack { index, name })
            .map_err(|error| error.to_string())?;
        let track = &project.tracks()[index];
        let result = json!({
            "track_id": track.id().value(),
            "index": index,
            "name": track.name(),
        });
        persist_project_edit(&mut project, store, "created track")?;
        Ok(result)
    }

    fn insert_midi_notes(
        &self,
        track_id: u64,
        item_id: u64,
        notes: Vec<MidiNoteData>,
    ) -> Result<Value, String> {
        let mut project = self
            .project
            .lock()
            .map_err(|_| "project lock was poisoned".to_owned())?;
        let mut store = self
            .store
            .lock()
            .map_err(|_| "project store lock was poisoned".to_owned())?;
        let store = store
            .as_mut()
            .ok_or_else(|| "project was opened read-only".to_owned())?;
        let (track_id, item) = resolve_midi_item(&project, track_id, item_id)?;
        let item_id = item.id();
        let first_note_index = item.notes().len();
        project
            .apply(DawAction::AddMidiNotes { item_id, notes })
            .map_err(|error| error.to_string())?;
        let item = project
            .midi_items()
            .iter()
            .find(|item| item.id() == item_id)
            .ok_or_else(|| "MIDI item disappeared after note insertion".to_owned())?;
        let note_ids = item.notes()[first_note_index..]
            .iter()
            .map(|note| note.id().value())
            .collect::<Vec<_>>();
        let result = json!({
            "track_id": track_id.value(),
            "item_id": item_id.value(),
            "note_ids": note_ids,
        });
        persist_project_edit(&mut project, store, "inserted MIDI notes")?;
        Ok(result)
    }

    fn quantize_midi_item(
        &self,
        track_id: u64,
        item_id: u64,
        numerator: u32,
        denominator: u32,
        strength: f32,
    ) -> Result<Value, String> {
        let mut project = self
            .project
            .lock()
            .map_err(|_| "project lock was poisoned".to_owned())?;
        let mut store = self
            .store
            .lock()
            .map_err(|_| "project store lock was poisoned".to_owned())?;
        let store = store
            .as_mut()
            .ok_or_else(|| "project was opened read-only".to_owned())?;
        let (track_id, item) = resolve_midi_item(&project, track_id, item_id)?;
        let item_id = item.id();
        let before_ticks = item
            .notes()
            .iter()
            .map(|note| (note.id().value(), note.tick()))
            .collect::<std::collections::HashMap<_, _>>();
        let grid = GridFraction::new(numerator, denominator).map_err(|error| error.to_string())?;
        project
            .apply(DawAction::QuantizeItem {
                item_id,
                grid,
                strength,
            })
            .map_err(|error| error.to_string())?;
        let item = project
            .midi_items()
            .iter()
            .find(|item| item.id() == item_id)
            .ok_or_else(|| "MIDI item disappeared after quantization".to_owned())?;
        let changed_note_ids = item
            .notes()
            .iter()
            .filter(|note| before_ticks.get(&note.id().value()) != Some(&note.tick()))
            .map(|note| note.id().value())
            .collect::<Vec<_>>();
        let no_op = changed_note_ids.is_empty();
        let result = json!({
            "track_id": track_id.value(),
            "item_id": item_id.value(),
            "grid": {"numerator": numerator, "denominator": denominator},
            "strength": strength,
            "changed_note_ids": changed_note_ids,
        });
        if no_op {
            return Ok(result);
        }
        persist_project_edit(&mut project, store, "quantized MIDI notes")?;
        Ok(result)
    }

    fn set_volume_automation_point(
        &self,
        track_id: u64,
        sample: u64,
        gain_db: f32,
    ) -> Result<Value, String> {
        let mut project = self
            .project
            .lock()
            .map_err(|_| "project lock was poisoned".to_owned())?;
        let mut store = self
            .store
            .lock()
            .map_err(|_| "project store lock was poisoned".to_owned())?;
        let store = store
            .as_mut()
            .ok_or_else(|| "project was opened read-only".to_owned())?;
        let track = project
            .tracks()
            .iter()
            .find(|track| track.id().value() == track_id)
            .ok_or_else(|| "unknown track id".to_owned())?;
        let track_id = track.id();
        let mut points = track.volume_automation().to_vec();
        let point = VolumeAutomationPoint::new(sample, gain_db)
            .ok_or_else(|| "gain_db must be finite and within -60..=6".to_owned())?;
        let changed = match points.binary_search_by_key(&sample, |existing| existing.sample()) {
            Ok(index) if points[index] == point => false,
            Ok(index) => {
                points[index] = point;
                true
            }
            Err(index) => {
                points.insert(index, point);
                true
            }
        };
        let result = json!({
            "track_id": track_id.value(),
            "sample": sample,
            "gain_db": gain_db,
            "point_count": points.len(),
            "changed": changed,
        });
        if !changed {
            return Ok(result);
        }
        project
            .apply(DawAction::SetTrackVolumeAutomation { track_id, points })
            .map_err(|error| error.to_string())?;
        persist_project_edit(&mut project, store, "set volume automation point")?;
        Ok(result)
    }

    fn set_track_record_arm(&self, track_id: u64, armed: bool) -> Result<Value, String> {
        let mut project = self
            .project
            .lock()
            .map_err(|_| "project lock was poisoned".to_owned())?;
        let mut store = self
            .store
            .lock()
            .map_err(|_| "project store lock was poisoned".to_owned())?;
        let store = store
            .as_mut()
            .ok_or_else(|| "project was opened read-only".to_owned())?;
        let track = project
            .tracks()
            .iter()
            .find(|track| track.id().value() == track_id)
            .ok_or_else(|| "unknown track id".to_owned())?;
        let track_id = track.id();
        let changed = track.is_record_armed() != armed;
        let result = json!({
            "track_id": track_id.value(),
            "armed": armed,
            "changed": changed,
        });
        if !changed {
            return Ok(result);
        }
        project
            .apply(DawAction::SetTrackRecordArm { track_id, armed })
            .map_err(|error| error.to_string())?;
        persist_project_edit(&mut project, store, "set track record arm")?;
        Ok(result)
    }
}

fn persist_project_edit(
    project: &mut Project,
    store: &mut ProjectStore,
    operation: &str,
) -> Result<(), String> {
    if let Err(error) = store.save(project) {
        if let Err(undo_error) = project.undo() {
            return Err(format!(
                "failed to save {operation}: {error}; rollback failed: {undo_error}"
            ));
        }
        return Err(format!("failed to save {operation}: {error}"));
    }
    Ok(())
}

fn resolve_midi_item(
    project: &Project,
    track_id: u64,
    item_id: u64,
) -> Result<(TrackId, &MidiItem), String> {
    let track = project
        .tracks()
        .iter()
        .find(|track| track.id().value() == track_id)
        .ok_or_else(|| "unknown track id".to_owned())?;
    let item = project
        .midi_items()
        .iter()
        .find(|item| item.id().value() == item_id)
        .ok_or_else(|| "unknown MIDI item id".to_owned())?;
    if item.track_id() != track.id() {
        return Err("MIDI item does not belong to the specified track".to_owned());
    }
    Ok((track.id(), item))
}

fn midi_query_tool() -> Tool {
    Tool::new(
        MIDI_QUERY_TOOL,
        "Read MIDI note events whose absolute start tick falls within a bounded project range.",
        rmcp::model::object(json!({
            "type": "object",
            "properties": {
                "track_id": {"type": "integer", "minimum": 1},
                "start_tick": {"type": "integer", "minimum": 0},
                "end_tick": {"type": "integer", "minimum": 1},
                "limit": {"type": "integer", "minimum": 1, "maximum": MAX_NOTE_RESULTS}
            },
            "required": ["track_id", "start_tick", "end_tick"],
            "additionalProperties": false
        })),
    )
    .with_annotations(
        ToolAnnotations::new()
            .read_only(true)
            .idempotent(true)
            .open_world(false),
    )
}

fn create_track_tool() -> Tool {
    Tool::new(
        CREATE_TRACK_TOOL,
        "Create one audio track at the end of the project track list.",
        rmcp::model::object(json!({
            "type": "object",
            "properties": {
                "name": {"type": "string", "minLength": 1, "maxLength": MAX_TRACK_NAME_CHARS}
            },
            "required": ["name"],
            "additionalProperties": false
        })),
    )
    .with_annotations(
        ToolAnnotations::new()
            .read_only(false)
            .idempotent(false)
            .open_world(false),
    )
}

fn insert_midi_notes_tool() -> Tool {
    Tool::new(
        INSERT_MIDI_NOTES_TOOL,
        "Insert a bounded batch of notes into an existing MIDI item.",
        rmcp::model::object(json!({
            "type": "object",
            "properties": {
                "track_id": {"type": "integer", "minimum": 0},
                "item_id": {"type": "integer", "minimum": 0},
                "notes": {
                    "type": "array",
                    "minItems": 1,
                    "maxItems": MAX_MIDI_NOTES_PER_INSERT,
                    "items": {
                        "type": "object",
                        "properties": {
                            "pitch": {"type": "integer", "minimum": 0, "maximum": 127},
                            "tick": {"type": "integer", "minimum": 0},
                            "duration": {"type": "integer", "minimum": 1},
                            "velocity": {"type": "integer", "minimum": 0, "maximum": 127}
                        },
                        "required": ["pitch", "tick", "duration", "velocity"],
                        "additionalProperties": false
                    }
                }
            },
            "required": ["track_id", "item_id", "notes"],
            "additionalProperties": false
        })),
    )
    .with_annotations(
        ToolAnnotations::new()
            .read_only(false)
            .idempotent(false)
            .open_world(false),
    )
}

fn quantize_midi_item_tool() -> Tool {
    Tool::new(
        QUANTIZE_MIDI_ITEM_TOOL,
        "Move note starts in an existing MIDI item toward a musical grid.",
        rmcp::model::object(json!({
            "type": "object",
            "properties": {
                "track_id": {"type": "integer", "minimum": 0},
                "item_id": {"type": "integer", "minimum": 0},
                "grid_numerator": {"type": "integer", "minimum": 1},
                "grid_denominator": {"type": "integer", "minimum": 1},
                "strength": {"type": "number", "minimum": 0, "maximum": 1, "default": 1}
            },
            "required": ["track_id", "item_id", "grid_numerator", "grid_denominator"],
            "additionalProperties": false
        })),
    )
    .with_annotations(
        ToolAnnotations::new()
            .read_only(false)
            .idempotent(false)
            .open_world(false),
    )
}

fn set_volume_automation_point_tool() -> Tool {
    Tool::new(
        SET_VOLUME_AUTOMATION_POINT_TOOL,
        "Insert or update a track volume automation point at an absolute project sample.",
        rmcp::model::object(json!({
            "type": "object",
            "properties": {
                "track_id": {"type": "integer", "minimum": 0},
                "sample": {"type": "integer", "minimum": 0},
                "gain_db": {"type": "number", "minimum": -60, "maximum": 6}
            },
            "required": ["track_id", "sample", "gain_db"],
            "additionalProperties": false
        })),
    )
    .with_annotations(
        ToolAnnotations::new()
            .read_only(false)
            .idempotent(true)
            .open_world(false),
    )
}

fn parse_volume_automation_point_arguments(
    arguments: Option<&serde_json::Map<String, Value>>,
) -> Result<(u64, u64, f32), String> {
    let arguments = arguments.ok_or_else(|| "arguments are required".to_owned())?;
    if arguments
        .keys()
        .any(|key| !matches!(key.as_str(), "track_id" | "sample" | "gain_db"))
    {
        return Err("arguments contain an unknown field".to_owned());
    }
    let track_id = arguments
        .get("track_id")
        .and_then(Value::as_u64)
        .ok_or_else(|| "track_id must be a non-negative integer".to_owned())?;
    let sample = arguments
        .get("sample")
        .and_then(Value::as_u64)
        .ok_or_else(|| "sample must be a non-negative integer".to_owned())?;
    let gain_db = arguments
        .get("gain_db")
        .and_then(Value::as_f64)
        .filter(|gain| gain.is_finite() && (-60.0..=6.0).contains(gain))
        .ok_or_else(|| "gain_db must be finite and within -60..=6".to_owned())?
        as f32;
    Ok((track_id, sample, gain_db))
}

fn set_track_record_arm_tool() -> Tool {
    Tool::new(
        SET_TRACK_RECORD_ARM_TOOL,
        "Arm or disarm an existing track for a later recording take.",
        rmcp::model::object(json!({
            "type": "object",
            "properties": {
                "track_id": {"type": "integer", "minimum": 0},
                "armed": {"type": "boolean"}
            },
            "required": ["track_id", "armed"],
            "additionalProperties": false
        })),
    )
    .with_annotations(
        ToolAnnotations::new()
            .read_only(false)
            .idempotent(true)
            .open_world(false),
    )
}

fn parse_record_arm_arguments(
    arguments: Option<&serde_json::Map<String, Value>>,
) -> Result<(u64, bool), String> {
    let arguments = arguments.ok_or_else(|| "arguments are required".to_owned())?;
    if arguments
        .keys()
        .any(|key| !matches!(key.as_str(), "track_id" | "armed"))
    {
        return Err("arguments contain an unknown field".to_owned());
    }
    let track_id = arguments
        .get("track_id")
        .and_then(Value::as_u64)
        .ok_or_else(|| "track_id must be a non-negative integer".to_owned())?;
    let armed = arguments
        .get("armed")
        .and_then(Value::as_bool)
        .ok_or_else(|| "armed must be a boolean".to_owned())?;
    Ok((track_id, armed))
}

fn parse_create_track_arguments(
    arguments: Option<&serde_json::Map<String, Value>>,
) -> Result<String, String> {
    let arguments = arguments.ok_or_else(|| "arguments are required".to_owned())?;
    if arguments.keys().any(|key| key != "name") {
        return Err("arguments contain an unknown field".to_owned());
    }
    let name = arguments
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| "name must be a string".to_owned())?;
    let name_length = name.chars().count();
    if name.trim().is_empty() {
        return Err("name must not be empty".to_owned());
    }
    if name_length > MAX_TRACK_NAME_CHARS {
        return Err(format!(
            "name must be at most {MAX_TRACK_NAME_CHARS} characters"
        ));
    }
    Ok(name.to_owned())
}

fn parse_insert_midi_notes_arguments(
    arguments: Option<&serde_json::Map<String, Value>>,
) -> Result<(u64, u64, Vec<MidiNoteData>), String> {
    let arguments = arguments.ok_or_else(|| "arguments are required".to_owned())?;
    if arguments
        .keys()
        .any(|key| !matches!(key.as_str(), "track_id" | "item_id" | "notes"))
    {
        return Err("arguments contain an unknown field".to_owned());
    }
    let integer = |key: &str| {
        arguments
            .get(key)
            .and_then(Value::as_u64)
            .ok_or_else(|| format!("{key} must be a non-negative integer"))
    };
    let track_id = integer("track_id")?;
    let item_id = integer("item_id")?;
    let notes = arguments
        .get("notes")
        .and_then(Value::as_array)
        .ok_or_else(|| "notes must be an array".to_owned())?;
    if notes.is_empty() {
        return Err("notes must not be empty".to_owned());
    }
    if notes.len() > MAX_MIDI_NOTES_PER_INSERT {
        return Err(format!(
            "notes must contain at most {MAX_MIDI_NOTES_PER_INSERT} entries"
        ));
    }
    let notes = notes
        .iter()
        .enumerate()
        .map(|(index, note)| {
            let note = note
                .as_object()
                .ok_or_else(|| format!("notes[{index}] must be an object"))?;
            if note
                .keys()
                .any(|key| !matches!(key.as_str(), "pitch" | "tick" | "duration" | "velocity"))
            {
                return Err(format!("notes[{index}] contains an unknown field"));
            }
            let unsigned = |key: &str| {
                note.get(key)
                    .and_then(Value::as_u64)
                    .ok_or_else(|| format!("notes[{index}].{key} must be a non-negative integer"))
            };
            let pitch = u8::try_from(unsigned("pitch")?)
                .map_err(|_| format!("notes[{index}].pitch must be in 0..=127"))?;
            let tick = unsigned("tick")?;
            let duration = unsigned("duration")?;
            let velocity = u8::try_from(unsigned("velocity")?)
                .map_err(|_| format!("notes[{index}].velocity must be in 0..=127"))?;
            Ok(MidiNoteData {
                pitch,
                tick,
                duration,
                velocity,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok((track_id, item_id, notes))
}

fn parse_quantize_midi_item_arguments(
    arguments: Option<&serde_json::Map<String, Value>>,
) -> Result<(u64, u64, u32, u32, f32), String> {
    let arguments = arguments.ok_or_else(|| "arguments are required".to_owned())?;
    if arguments.keys().any(|key| {
        !matches!(
            key.as_str(),
            "track_id" | "item_id" | "grid_numerator" | "grid_denominator" | "strength"
        )
    }) {
        return Err("arguments contain an unknown field".to_owned());
    }
    let unsigned = |key: &str| {
        arguments
            .get(key)
            .and_then(Value::as_u64)
            .ok_or_else(|| format!("{key} must be a non-negative integer"))
    };
    let bounded_u32 =
        |key: &str| u32::try_from(unsigned(key)?).map_err(|_| format!("{key} is out of range"));
    let track_id = unsigned("track_id")?;
    let item_id = unsigned("item_id")?;
    let grid_numerator = bounded_u32("grid_numerator")?;
    let grid_denominator = bounded_u32("grid_denominator")?;
    if grid_numerator == 0 || grid_denominator == 0 {
        return Err("grid numerator and denominator must be positive".to_owned());
    }
    let strength = match arguments.get("strength") {
        Some(value) => {
            let strength = value
                .as_f64()
                .ok_or_else(|| "strength must be a number in 0..=1".to_owned())?;
            if !strength.is_finite() || !(0.0..=1.0).contains(&strength) {
                return Err("strength must be a number in 0..=1".to_owned());
            }
            strength as f32
        }
        None => 1.0,
    };
    Ok((
        track_id,
        item_id,
        grid_numerator,
        grid_denominator,
        strength,
    ))
}

fn parse_note_query_arguments(
    arguments: Option<&serde_json::Map<String, Value>>,
) -> Result<(u64, u64, u64, usize), String> {
    let arguments = arguments.ok_or_else(|| "arguments are required".to_owned())?;
    let read_integer = |name: &str| {
        arguments
            .get(name)
            .and_then(Value::as_u64)
            .ok_or_else(|| format!("{name} must be a non-negative integer"))
    };
    let track_id = read_integer("track_id")?;
    let start_tick = read_integer("start_tick")?;
    let end_tick = read_integer("end_tick")?;
    let limit = arguments
        .get("limit")
        .map(|_| read_integer("limit"))
        .transpose()?
        .map(usize::try_from)
        .transpose()
        .map_err(|_| "limit is too large".to_owned())?
        .unwrap_or(DEFAULT_NOTE_RESULTS);
    if arguments
        .keys()
        .any(|key| !["track_id", "start_tick", "end_tick", "limit"].contains(&key.as_str()))
    {
        return Err("arguments contain an unknown field".to_owned());
    }
    if !(1..=MAX_NOTE_RESULTS).contains(&limit) {
        return Err(format!("limit must be between 1 and {MAX_NOTE_RESULTS}"));
    }
    Ok((track_id, start_tick, end_tick, limit))
}

fn scoped_query_notes(
    project: &Project,
    track_id: u64,
    start_tick: u64,
    end_tick: u64,
    limit: usize,
) -> Result<Value, String> {
    if !(1..=MAX_NOTE_RESULTS).contains(&limit) {
        return Err(format!("limit must be between 1 and {MAX_NOTE_RESULTS}"));
    }
    if end_tick <= start_tick {
        return Err("end_tick must be greater than start_tick".to_owned());
    }
    if end_tick - start_tick > MAX_NOTE_QUERY_TICKS {
        return Err(format!(
            "requested range exceeds {MAX_NOTE_QUERY_TICKS} ticks"
        ));
    }
    let track_id = project
        .tracks()
        .iter()
        .find(|track| track.id().value() == track_id)
        .map(|track| track.id())
        .ok_or_else(|| "unknown track id".to_owned())?;
    let mut notes = std::collections::BinaryHeap::with_capacity(limit);
    let mut truncated = false;
    for item in project
        .midi_items()
        .iter()
        .filter(|item| item.track_id() == track_id)
    {
        for note in item.notes() {
            let absolute_tick = item.start_tick().saturating_add(note.tick());
            if !(start_tick..end_tick).contains(&absolute_tick) {
                continue;
            }
            let candidate = NoteCandidate {
                absolute_tick,
                item_id: item.id().value(),
                note_id: note.id().value(),
                pitch: note.pitch(),
                duration: note.duration(),
                velocity: note.velocity(),
            };
            if notes.len() < limit {
                notes.push(candidate);
            } else {
                truncated = true;
                if notes.peek().is_some_and(|latest| candidate < *latest) {
                    notes.pop();
                    notes.push(candidate);
                }
            }
        }
    }
    let notes = notes
        .into_sorted_vec()
        .into_iter()
        .map(|note| {
            json!({
                "item_id": note.item_id,
                "note_id": note.note_id,
                "tick": note.absolute_tick,
                "pitch": note.pitch,
                "duration": note.duration,
                "velocity": note.velocity,
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({
        "track_id": track_id.value(),
        "start_tick": start_tick,
        "end_tick": end_tick,
        "limit": limit,
        "truncated": truncated,
        "notes": notes,
    }))
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct NoteCandidate {
    absolute_tick: u64,
    item_id: u64,
    note_id: u64,
    pitch: u8,
    duration: u64,
    velocity: u8,
}

impl Ord for NoteCandidate {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (self.absolute_tick, self.item_id, self.note_id).cmp(&(
            other.absolute_tick,
            other.item_id,
            other.note_id,
        ))
    }
}

impl PartialOrd for NoteCandidate {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

fn parse_track_summary_uri(uri: &str) -> Option<u64> {
    let id = uri
        .strip_prefix("daw://project/track/")?
        .strip_suffix("/midi_summary")?;
    if id.is_empty() || !id.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    id.parse::<u64>().ok()
}

fn structure_summary(project: &Project) -> Value {
    let tempo_points = project
        .tempo_points()
        .take(MAX_MAP_POINTS)
        .map(|(tick, bpm, curve)| {
            let curve = match curve {
                TempoCurve::Step => "step",
                TempoCurve::Linear => "linear",
                TempoCurve::Logarithmic => "logarithmic",
                TempoCurve::Bézier => "bezier",
            };
            json!({"tick": tick, "bpm": bpm, "curve_to_next": curve})
        })
        .collect::<Vec<_>>();
    let meter_points = project
        .time_signature_points()
        .take(MAX_MAP_POINTS)
        .map(|(tick, signature)| {
            json!({
                "tick": tick,
                "numerator": signature.numerator(),
                "denominator": signature.denominator(),
            })
        })
        .collect::<Vec<_>>();
    json!({
        "sample_rate": project.settings().sample_rate(),
        "ppq": project.settings().ppq(),
        "tracks": project.tracks().iter().take(MAX_TRACKS).map(|track| json!({
            "id": track.id().value(),
            "name": track.name().chars().take(128).collect::<String>(),
            "type": if track.is_bus() { "bus" } else { "track" },
        })).collect::<Vec<_>>(),
        "tracks_truncated": project.tracks().len() > MAX_TRACKS,
        "tempo_points": tempo_points,
        "tempo_points_truncated": project.tempo_points().count() > MAX_MAP_POINTS,
        "meter_points": meter_points,
        "meter_points_truncated": project.time_signature_points().count() > MAX_MAP_POINTS,
        "midi_summary_template": MIDI_SUMMARY_TEMPLATE,
    })
}

fn track_midi_summary(project: &Project, track_id: TrackId) -> Value {
    let items = project
        .midi_items()
        .iter()
        .filter(|item| item.track_id() == track_id);
    let (item_count, note_count, tick_range) = items.fold(
        (0usize, 0usize, None::<(u64, u64)>),
        |(item_count, note_count, range), item| {
            let next_range = item.notes().iter().fold(range, |range, note| {
                let start = item.start_tick().saturating_add(note.tick());
                let end = start.saturating_add(note.duration());
                Some(match range {
                    Some((minimum, maximum)) => (minimum.min(start), maximum.max(end)),
                    None => (start, end),
                })
            });
            (
                item_count.saturating_add(1),
                note_count.saturating_add(item.notes().len()),
                next_range,
            )
        },
    );
    json!({
        "track_id": track_id.value(),
        "midi_item_count": item_count,
        "note_count": note_count,
        "note_tick_range": tick_range.map(|(start, end)| json!({"start": start, "end": end})),
    })
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_MAP_POINTS, MAX_MIDI_NOTES_PER_INSERT, MAX_NOTE_QUERY_TICKS, MAX_NOTE_RESULTS,
        MAX_TRACK_NAME_CHARS, MAX_TRACKS, parse_create_track_arguments,
        parse_insert_midi_notes_arguments, parse_note_query_arguments,
        parse_quantize_midi_item_arguments, parse_track_summary_uri, scoped_query_notes,
        structure_summary, track_midi_summary,
    };
    use aaadaw_core::{DawAction, MidiNoteData, Project, TimeSignature};
    use serde_json::{Value, json};

    #[test]
    fn create_track_arguments_require_a_bounded_non_empty_name() {
        let arguments = |name: Value| serde_json::Map::from_iter([("name".to_owned(), name)]);
        assert_eq!(
            parse_create_track_arguments(Some(&arguments(json!("Lead")))).unwrap(),
            "Lead"
        );
        for name in ["", "   "] {
            assert!(parse_create_track_arguments(Some(&arguments(json!(name)))).is_err());
        }
        assert!(
            parse_create_track_arguments(Some(&arguments(json!(
                "x".repeat(MAX_TRACK_NAME_CHARS + 1)
            ))))
            .is_err()
        );
        assert!(parse_create_track_arguments(None).is_err());
    }

    #[test]
    fn insert_midi_notes_arguments_require_a_bounded_non_empty_batch() {
        let arguments = |notes: Value| {
            serde_json::Map::from_iter([
                ("track_id".to_owned(), json!(1)),
                ("item_id".to_owned(), json!(2)),
                ("notes".to_owned(), notes),
            ])
        };
        let valid_note = json!({
            "pitch": 60,
            "tick": 0,
            "duration": 480,
            "velocity": 100
        });
        let parsed =
            parse_insert_midi_notes_arguments(Some(&arguments(json!([valid_note.clone()]))))
                .unwrap();
        assert_eq!(parsed.0, 1);
        assert_eq!(parsed.1, 2);
        assert_eq!(parsed.2.len(), 1);
        assert!(parse_insert_midi_notes_arguments(Some(&arguments(json!([])))).is_err());
        assert!(
            parse_insert_midi_notes_arguments(Some(&arguments(json!(vec![
                valid_note.clone();
                MAX_MIDI_NOTES_PER_INSERT
                    + 1
            ]))))
            .is_err()
        );
        for invalid in [
            json!({"pitch": -1, "tick": 0, "duration": 1, "velocity": 1}),
            json!({"pitch": 60, "tick": 0, "duration": 1}),
            json!({"pitch": 60, "tick": 0, "duration": 1, "velocity": 1, "extra": true}),
        ] {
            assert!(parse_insert_midi_notes_arguments(Some(&arguments(json!([invalid])))).is_err());
        }
    }

    #[test]
    fn quantize_arguments_validate_grid_and_strength_and_default_to_full_strength() {
        let arguments = |grid_numerator: Value, grid_denominator: Value| {
            serde_json::Map::from_iter([
                ("track_id".to_owned(), json!(0)),
                ("item_id".to_owned(), json!(0)),
                ("grid_numerator".to_owned(), grid_numerator),
                ("grid_denominator".to_owned(), grid_denominator),
            ])
        };
        let parsed =
            parse_quantize_midi_item_arguments(Some(&arguments(json!(1), json!(16)))).unwrap();
        assert_eq!(parsed, (0, 0, 1, 16, 1.0));

        let mut partial = arguments(json!(1), json!(16));
        partial.insert("strength".to_owned(), json!(0.5));
        assert_eq!(
            parse_quantize_midi_item_arguments(Some(&partial)).unwrap(),
            (0, 0, 1, 16, 0.5)
        );
        for strength in [json!(-0.1), json!(1.1), json!("1"), json!(true)] {
            let mut invalid = arguments(json!(1), json!(16));
            invalid.insert("strength".to_owned(), strength);
            assert!(parse_quantize_midi_item_arguments(Some(&invalid)).is_err());
        }
        for (numerator, denominator) in [
            (json!(0), json!(16)),
            (json!(1), json!(0)),
            (json!(u64::from(u32::MAX) + 1), json!(16)),
        ] {
            assert!(
                parse_quantize_midi_item_arguments(Some(&arguments(numerator, denominator)))
                    .is_err()
            );
        }
    }

    #[test]
    fn track_resource_uri_requires_a_decimal_numeric_id() {
        assert_eq!(
            parse_track_summary_uri("daw://project/track/42/midi_summary"),
            Some(42)
        );
        for uri in [
            "daw://project/track/-1/midi_summary",
            "daw://project/track/4x/midi_summary",
            "daw://project/track/18446744073709551616/midi_summary",
            "daw://project/track/4/midi_summary/extra",
        ] {
            assert_eq!(parse_track_summary_uri(uri), None);
        }
    }

    #[test]
    fn project_structure_caps_tempo_and_meter_points() {
        let project = Project::new();
        let summary = structure_summary(&project);
        assert!(summary["tempo_points"].as_array().unwrap().len() <= MAX_MAP_POINTS);
        assert!(summary["meter_points"].as_array().unwrap().len() <= MAX_MAP_POINTS);
    }

    #[test]
    fn large_project_structure_marks_every_truncated_collection() {
        let mut project = Project::new();
        for index in 0..=MAX_TRACKS {
            project
                .apply(DawAction::CreateTrack {
                    index,
                    name: format!("Track {index}"),
                })
                .unwrap();
        }
        for index in 0..=MAX_MAP_POINTS {
            project
                .apply(DawAction::SetTempo {
                    start_tick: (index as u64) * 960,
                    bpm: 120.0,
                })
                .unwrap();
        }
        let three_four = TimeSignature::new(3, 4).unwrap();
        for index in 1..=MAX_MAP_POINTS {
            project
                .apply(DawAction::SetTimeSignature {
                    start_tick: (index as u64) * 11_520,
                    signature: three_four,
                })
                .unwrap();
        }

        let summary = structure_summary(&project);
        assert_eq!(summary["tracks"].as_array().unwrap().len(), MAX_TRACKS);
        assert!(summary["tracks_truncated"].as_bool().unwrap());
        assert_eq!(
            summary["tempo_points"].as_array().unwrap().len(),
            MAX_MAP_POINTS
        );
        assert!(summary["tempo_points_truncated"].as_bool().unwrap());
        assert_eq!(
            summary["meter_points"].as_array().unwrap().len(),
            MAX_MAP_POINTS
        );
        assert!(summary["meter_points_truncated"].as_bool().unwrap());
    }

    #[test]
    fn track_summary_reports_aggregate_count_and_project_tick_range() {
        let mut project = Project::new();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Keys".to_owned(),
            })
            .unwrap();
        let track_id = project.tracks()[0].id();
        project
            .apply(DawAction::InsertMidiItem {
                track_id,
                start_tick: 960,
                length_ticks: 3840,
            })
            .unwrap();
        let item_id = project.midi_items()[0].id();
        project
            .apply(DawAction::AddMidiNotes {
                item_id,
                notes: vec![
                    MidiNoteData {
                        pitch: 60,
                        tick: 120,
                        duration: 240,
                        velocity: 100,
                    },
                    MidiNoteData {
                        pitch: 64,
                        tick: 480,
                        duration: 480,
                        velocity: 90,
                    },
                ],
            })
            .unwrap();

        let summary = track_midi_summary(&project, track_id);
        assert_eq!(summary["midi_item_count"], 1);
        assert_eq!(summary["note_count"], 2);
        assert_eq!(summary["note_tick_range"]["start"], 1080);
        assert_eq!(summary["note_tick_range"]["end"], 1920);
        assert!(summary.get("notes").is_none());
    }

    fn project_with_notes(notes: Vec<MidiNoteData>) -> (Project, aaadaw_core::TrackId) {
        let mut project = Project::new();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Piano".to_owned(),
            })
            .unwrap();
        let track_id = project.tracks()[0].id();
        project
            .apply(DawAction::InsertMidiItem {
                track_id,
                start_tick: 480,
                length_ticks: 16_000,
            })
            .unwrap();
        let item_id = project.midi_items()[0].id();
        project
            .apply(DawAction::AddMidiNotes { item_id, notes })
            .unwrap();
        (project, track_id)
    }

    #[test]
    fn scoped_query_uses_half_open_project_ranges_and_stable_ordering() {
        let (project, track_id) = project_with_notes(vec![
            MidiNoteData {
                pitch: 67,
                tick: 480,
                duration: 120,
                velocity: 90,
            },
            MidiNoteData {
                pitch: 60,
                tick: 0,
                duration: 240,
                velocity: 100,
            },
            MidiNoteData {
                pitch: 64,
                tick: 240,
                duration: 120,
                velocity: 80,
            },
        ]);

        let result = scoped_query_notes(&project, track_id.value(), 720, 960, 10).unwrap();
        let notes = result["notes"].as_array().unwrap();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0]["tick"], 720);
        assert_eq!(notes[0]["pitch"], 64);
        assert_eq!(result["truncated"], false);
    }

    #[test]
    fn scoped_query_breaks_equal_tick_ties_by_item_then_note_id() {
        let (mut project, track_id) = project_with_notes(vec![
            MidiNoteData {
                pitch: 67,
                tick: 480,
                duration: 120,
                velocity: 90,
            },
            MidiNoteData {
                pitch: 60,
                tick: 480,
                duration: 120,
                velocity: 90,
            },
        ]);
        project
            .apply(DawAction::InsertMidiItem {
                track_id,
                start_tick: 480,
                length_ticks: 16_000,
            })
            .unwrap();
        let second_item_id = project.midi_items()[1].id();
        project
            .apply(DawAction::AddMidiNotes {
                item_id: second_item_id,
                notes: vec![MidiNoteData {
                    pitch: 72,
                    tick: 480,
                    duration: 120,
                    velocity: 90,
                }],
            })
            .unwrap();

        let result = scoped_query_notes(&project, track_id.value(), 960, 961, 10).unwrap();
        let notes = result["notes"].as_array().unwrap();
        assert_eq!(notes.len(), 3);
        assert_eq!(notes[0]["item_id"], project.midi_items()[0].id().value());
        assert_eq!(notes[1]["item_id"], project.midi_items()[0].id().value());
        assert!(notes[0]["note_id"].as_u64().unwrap() < notes[1]["note_id"].as_u64().unwrap());
        assert_eq!(notes[2]["item_id"], second_item_id.value());
    }

    #[test]
    fn scoped_query_keeps_the_earliest_notes_and_caps_its_output() {
        let notes = (0..600)
            .rev()
            .map(|tick| MidiNoteData {
                pitch: 60,
                tick,
                duration: 1,
                velocity: 100,
            })
            .collect();
        let (project, track_id) = project_with_notes(notes);

        let result = scoped_query_notes(&project, track_id.value(), 0, 1000, 3).unwrap();
        let notes = result["notes"].as_array().unwrap();
        assert_eq!(notes.len(), 3);
        assert_eq!(notes[0]["tick"], 480);
        assert_eq!(notes[1]["tick"], 481);
        assert_eq!(notes[2]["tick"], 482);
        assert!(result["truncated"].as_bool().unwrap());
    }

    #[test]
    fn scoped_query_rejects_bad_ids_ranges_and_limits() {
        let (project, track_id) = project_with_notes(Vec::new());
        for (start, end) in [(100, 100), (200, 100), (0, MAX_NOTE_QUERY_TICKS + 1)] {
            assert!(scoped_query_notes(&project, track_id.value(), start, end, 10).is_err());
        }
        assert!(scoped_query_notes(&project, track_id.value() + 1, 0, 100, 10).is_err());
        assert!(scoped_query_notes(&project, track_id.value(), 0, 100, 0).is_err());
        assert!(
            scoped_query_notes(&project, track_id.value(), 0, 100, MAX_NOTE_RESULTS + 1).is_err()
        );
    }

    #[test]
    fn tool_arguments_require_nonnegative_integers_and_known_fields() {
        let arguments = serde_json::from_value(serde_json::json!({
            "track_id": 1,
            "start_tick": 0,
            "end_tick": 100
        }))
        .unwrap();
        assert_eq!(
            parse_note_query_arguments(Some(&arguments)).unwrap(),
            (1, 0, 100, 256)
        );

        for value in [
            serde_json::json!({"track_id": -1, "start_tick": 0, "end_tick": 1}),
            serde_json::json!({"track_id": 1, "start_tick": 2.5, "end_tick": 3}),
            serde_json::json!({"track_id": 1, "start_tick": 0, "end_tick": 1, "extra": true}),
        ] {
            let arguments = serde_json::from_value(value).unwrap();
            assert!(parse_note_query_arguments(Some(&arguments)).is_err());
        }
        assert!(parse_note_query_arguments(None).is_err());
    }
}
