use aaadaw_core::{
    DawAction, GridFraction, MidiControllerData, MidiItem, MidiNoteData, MidiPitchBendData,
    Project, TempoCurve, TimeSignature, TrackId, VolumeAutomationPoint,
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
const MAX_MIDI_QUERY_TICKS: u64 = 245_760;
const MIDI_EXPRESSION_QUERY_TOOL: &str = "daw_scoped_query_midi_expression";
const MAX_MIDI_EXPRESSION_RESULTS: usize = 512;
const DEFAULT_MIDI_EXPRESSION_RESULTS: usize = 256;
const VOLUME_AUTOMATION_QUERY_TOOL: &str = "daw_scoped_query_volume_automation";
const MAX_VOLUME_AUTOMATION_RESULTS: usize = 512;
const DEFAULT_VOLUME_AUTOMATION_RESULTS: usize = 256;
const CREATE_TRACK_TOOL: &str = "daw_create_track";
const MAX_TRACK_NAME_CHARS: usize = 128;
const CREATE_MIDI_ITEM_TOOL: &str = "daw_create_midi_item";
const MAX_MIDI_ITEM_LENGTH_TICKS: u64 = 3_840 * 256;
const EDIT_MIDI_ITEM_TOOL: &str = "daw_edit_midi_item";
const INSERT_MIDI_NOTES_TOOL: &str = "daw_insert_midi_notes";
const MAX_MIDI_NOTES_PER_INSERT: usize = 512;
const EDIT_MIDI_NOTE_TOOL: &str = "daw_edit_midi_note";
const DELETE_MIDI_NOTES_TOOL: &str = "daw_delete_midi_notes";
const MAX_MIDI_NOTES_PER_DELETE: usize = 512;
const UPSERT_MIDI_CONTROLLERS_TOOL: &str = "daw_upsert_midi_controllers";
const UPSERT_MIDI_PITCH_BENDS_TOOL: &str = "daw_upsert_midi_pitch_bends";
const MAX_MIDI_EVENTS_PER_UPSERT: usize = 512;
const QUANTIZE_MIDI_ITEM_TOOL: &str = "daw_quantize_midi_item";
const SET_VOLUME_AUTOMATION_POINT_TOOL: &str = "daw_set_volume_automation_point";
const SET_TRACK_RECORD_ARM_TOOL: &str = "daw_set_track_record_arm";
const SET_TRACK_MIX_TOOL: &str = "daw_set_track_mix";
const UNDO_TOOL: &str = "daw_undo";
const REDO_TOOL: &str = "daw_redo";
const SET_TEMPO_TOOL: &str = "daw_set_tempo_point";
const SET_TIME_SIGNATURE_TOOL: &str = "daw_set_time_signature_point";

#[derive(Clone, Copy, Debug, PartialEq)]
struct TrackMixChanges {
    track_id: u64,
    volume_db: Option<f32>,
    pan: Option<f32>,
    muted: Option<bool>,
    solo: Option<bool>,
}

#[derive(Clone, Copy)]
enum HistoryDirection {
    Undo,
    Redo,
}

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
        let resource = Resource::new(STRUCTURE_URI, "Project structure").with_description(
            "Bounded track names, types, mix/record state and routing, plus tempo and meter maps.",
        );
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
        let mut tools = vec![
            midi_query_tool(),
            midi_expression_query_tool(),
            volume_automation_query_tool(),
        ];
        if self.writable {
            tools.push(create_track_tool());
            tools.push(create_midi_item_tool());
            tools.push(edit_midi_item_tool());
            tools.push(insert_midi_notes_tool());
            tools.push(edit_midi_note_tool());
            tools.push(delete_midi_notes_tool());
            tools.push(upsert_midi_controllers_tool());
            tools.push(upsert_midi_pitch_bends_tool());
            tools.push(quantize_midi_item_tool());
            tools.push(set_volume_automation_point_tool());
            tools.push(set_track_record_arm_tool());
            tools.push(set_track_mix_tool());
            tools.push(set_tempo_tool());
            tools.push(set_time_signature_tool());
            tools.push(history_tool(HistoryDirection::Undo));
            tools.push(history_tool(HistoryDirection::Redo));
        }
        std::future::ready(Ok(ListToolsResult::with_all_items(tools)))
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        match name {
            MIDI_QUERY_TOOL => Some(midi_query_tool()),
            MIDI_EXPRESSION_QUERY_TOOL => Some(midi_expression_query_tool()),
            VOLUME_AUTOMATION_QUERY_TOOL => Some(volume_automation_query_tool()),
            CREATE_TRACK_TOOL if self.writable => Some(create_track_tool()),
            CREATE_MIDI_ITEM_TOOL if self.writable => Some(create_midi_item_tool()),
            EDIT_MIDI_ITEM_TOOL if self.writable => Some(edit_midi_item_tool()),
            INSERT_MIDI_NOTES_TOOL if self.writable => Some(insert_midi_notes_tool()),
            EDIT_MIDI_NOTE_TOOL if self.writable => Some(edit_midi_note_tool()),
            DELETE_MIDI_NOTES_TOOL if self.writable => Some(delete_midi_notes_tool()),
            UPSERT_MIDI_CONTROLLERS_TOOL if self.writable => Some(upsert_midi_controllers_tool()),
            UPSERT_MIDI_PITCH_BENDS_TOOL if self.writable => Some(upsert_midi_pitch_bends_tool()),
            QUANTIZE_MIDI_ITEM_TOOL if self.writable => Some(quantize_midi_item_tool()),
            SET_VOLUME_AUTOMATION_POINT_TOOL if self.writable => {
                Some(set_volume_automation_point_tool())
            }
            SET_TRACK_RECORD_ARM_TOOL if self.writable => Some(set_track_record_arm_tool()),
            SET_TRACK_MIX_TOOL if self.writable => Some(set_track_mix_tool()),
            SET_TEMPO_TOOL if self.writable => Some(set_tempo_tool()),
            SET_TIME_SIGNATURE_TOOL if self.writable => Some(set_time_signature_tool()),
            UNDO_TOOL if self.writable => Some(history_tool(HistoryDirection::Undo)),
            REDO_TOOL if self.writable => Some(history_tool(HistoryDirection::Redo)),
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
            MIDI_EXPRESSION_QUERY_TOOL => self
                .project
                .lock()
                .map_err(|_| "project lock was poisoned".to_owned())
                .and_then(|project| {
                    parse_midi_expression_query_arguments(request.arguments.as_ref()).and_then(
                        |(track_id, start_tick, end_tick, limit)| {
                            scoped_query_midi_expression(
                                &project, track_id, start_tick, end_tick, limit,
                            )
                        },
                    )
                }),
            VOLUME_AUTOMATION_QUERY_TOOL => {
                self.project
                    .lock()
                    .map_err(|_| "project lock was poisoned".to_owned())
                    .and_then(|project| {
                        parse_volume_automation_query_arguments(request.arguments.as_ref())
                            .and_then(|(track_id, start_sample, end_sample, limit)| {
                                scoped_query_volume_automation(
                                    &project,
                                    track_id,
                                    start_sample,
                                    end_sample,
                                    limit,
                                )
                            })
                    })
            }
            CREATE_TRACK_TOOL if self.writable => {
                parse_create_track_arguments(request.arguments.as_ref())
                    .and_then(|name| self.create_track(name))
            }
            CREATE_MIDI_ITEM_TOOL if self.writable => parse_create_midi_item_arguments(
                request.arguments.as_ref(),
            )
            .and_then(|(track_id, start_tick, length_ticks)| {
                self.create_midi_item(track_id, start_tick, length_ticks)
            }),
            EDIT_MIDI_ITEM_TOOL if self.writable => parse_edit_midi_item_arguments(
                request.arguments.as_ref(),
            )
            .and_then(|(item_id, start_tick, length_ticks)| {
                self.edit_midi_item(item_id, start_tick, length_ticks)
            }),
            INSERT_MIDI_NOTES_TOOL if self.writable => {
                parse_insert_midi_notes_arguments(request.arguments.as_ref()).and_then(
                    |(track_id, item_id, notes)| self.insert_midi_notes(track_id, item_id, notes),
                )
            }
            EDIT_MIDI_NOTE_TOOL if self.writable => parse_edit_midi_note_arguments(
                request.arguments.as_ref(),
            )
            .and_then(|(item_id, note_id, data)| self.edit_midi_note(item_id, note_id, data)),
            DELETE_MIDI_NOTES_TOOL if self.writable => {
                parse_delete_midi_notes_arguments(request.arguments.as_ref())
                    .and_then(|(item_id, note_ids)| self.delete_midi_notes(item_id, note_ids))
            }
            UPSERT_MIDI_CONTROLLERS_TOOL if self.writable => {
                parse_upsert_midi_controllers_arguments(request.arguments.as_ref()).and_then(
                    |(item_id, controllers)| self.upsert_midi_controllers(item_id, controllers),
                )
            }
            UPSERT_MIDI_PITCH_BENDS_TOOL if self.writable => {
                parse_upsert_midi_pitch_bends_arguments(request.arguments.as_ref()).and_then(
                    |(item_id, pitch_bends)| self.upsert_midi_pitch_bends(item_id, pitch_bends),
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
            SET_TRACK_MIX_TOOL if self.writable => {
                parse_track_mix_arguments(request.arguments.as_ref())
                    .and_then(|changes| self.set_track_mix(changes))
            }
            SET_TEMPO_TOOL if self.writable => {
                parse_set_tempo_arguments(request.arguments.as_ref())
                    .and_then(|(start_tick, bpm)| self.set_tempo_point(start_tick, bpm))
            }
            SET_TIME_SIGNATURE_TOOL if self.writable => {
                parse_set_time_signature_arguments(request.arguments.as_ref()).and_then(
                    |(start_tick, signature)| self.set_time_signature_point(start_tick, signature),
                )
            }
            UNDO_TOOL if self.writable => parse_empty_arguments(request.arguments.as_ref())
                .and_then(|()| self.move_history(HistoryDirection::Undo)),
            REDO_TOOL if self.writable => parse_empty_arguments(request.arguments.as_ref())
                .and_then(|()| self.move_history(HistoryDirection::Redo)),
            _ => Err("unknown or unavailable tool".to_owned()),
        };
        std::future::ready(Ok(match result {
            Ok(value) => CallToolResult::structured(value),
            Err(message) => {
                tracing::warn!(tool = request.name.as_ref(), error = %message, "MCP tool call failed");
                CallToolResult::structured_error(json!({"error": message}))
            }
        }
        .into()))
    }
}

impl ProjectMcpServer {
    fn set_time_signature_point(
        &self,
        start_tick: u64,
        signature: TimeSignature,
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
        let current = project
            .time_signature_points()
            .find(|(tick, _)| *tick == start_tick)
            .map(|(_, signature)| signature);
        let changed = current != Some(signature);
        if !changed {
            return Ok(json!({
                "tick": start_tick,
                "numerator": signature.numerator(),
                "denominator": signature.denominator(),
                "changed": false
            }));
        }
        project
            .apply(DawAction::SetTimeSignature {
                start_tick,
                signature,
            })
            .map_err(|error| error.to_string())?;
        persist_project_edit(&mut project, store, "set time-signature point")?;
        Ok(json!({
            "tick": start_tick,
            "numerator": signature.numerator(),
            "denominator": signature.denominator(),
            "changed": true
        }))
    }

    fn set_tempo_point(&self, start_tick: u64, bpm: f64) -> Result<Value, String> {
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
        let current_bpm = project
            .tempo_points()
            .find(|(tick, _, _)| *tick == start_tick)
            .map(|(_, bpm, _)| bpm);
        let changed = current_bpm != Some(bpm);
        if !changed {
            return Ok(json!({"tick": start_tick, "bpm": bpm, "changed": false}));
        }
        project
            .apply(DawAction::SetTempo { start_tick, bpm })
            .map_err(|error| error.to_string())?;
        persist_project_edit(&mut project, store, "set tempo point")?;
        Ok(json!({"tick": start_tick, "bpm": bpm, "changed": true}))
    }

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

    fn create_midi_item(
        &self,
        raw_track_id: u64,
        start_tick: u64,
        length_ticks: u64,
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
        let track_id = project
            .tracks()
            .iter()
            .find(|track| track.id().value() == raw_track_id)
            .map(|track| track.id())
            .ok_or_else(|| "unknown track id".to_owned())?;
        project
            .apply(DawAction::InsertMidiItem {
                track_id,
                start_tick,
                length_ticks,
            })
            .map_err(|error| error.to_string())?;
        let item_id = project
            .midi_items()
            .last()
            .ok_or_else(|| "MIDI item was not created".to_owned())?
            .id()
            .value();
        persist_project_edit(&mut project, store, "created MIDI item")?;
        Ok(json!({
            "track_id": track_id.value(),
            "item_id": item_id,
            "start_tick": start_tick,
            "length_ticks": length_ticks,
        }))
    }

    fn edit_midi_item(
        &self,
        raw_item_id: u64,
        start_tick: u64,
        length_ticks: u64,
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
        let item_id = project
            .midi_items()
            .iter()
            .find(|item| item.id().value() == raw_item_id)
            .map(|item| item.id())
            .ok_or_else(|| "unknown MIDI item id".to_owned())?;
        project
            .apply(DawAction::EditMidiItem {
                item_id,
                start_tick,
                length_ticks,
            })
            .map_err(|error| error.to_string())?;
        persist_project_edit(&mut project, store, "edited MIDI item")?;
        Ok(json!({
            "item_id": item_id.value(),
            "start_tick": start_tick,
            "length_ticks": length_ticks,
        }))
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

    fn edit_midi_note(
        &self,
        raw_item_id: u64,
        raw_note_id: u64,
        data: MidiNoteData,
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
        let item = project
            .midi_items()
            .iter()
            .find(|item| item.id().value() == raw_item_id)
            .ok_or_else(|| "unknown MIDI item id".to_owned())?;
        let item_id = item.id();
        let note_id = item
            .notes()
            .iter()
            .find(|note| note.id().value() == raw_note_id)
            .map(|note| note.id())
            .ok_or_else(|| "unknown MIDI note id for this item".to_owned())?;
        project
            .apply(DawAction::EditMidiNote {
                item_id,
                note_id,
                data,
            })
            .map_err(|error| error.to_string())?;
        let note = project
            .midi_items()
            .iter()
            .find(|item| item.id() == item_id)
            .and_then(|item| item.notes().iter().find(|note| note.id() == note_id))
            .ok_or_else(|| "MIDI note disappeared after editing".to_owned())?;
        let result = json!({
            "item_id": item_id.value(),
            "note_id": note.id().value(),
            "pitch": note.pitch(),
            "tick": note.tick(),
            "duration": note.duration(),
            "velocity": note.velocity(),
        });
        persist_project_edit(&mut project, store, "edited MIDI note")?;
        Ok(result)
    }

    fn delete_midi_notes(&self, raw_item_id: u64, raw_note_ids: Vec<u64>) -> Result<Value, String> {
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
        let item = project
            .midi_items()
            .iter()
            .find(|item| item.id().value() == raw_item_id)
            .ok_or_else(|| "unknown MIDI item id".to_owned())?;
        let item_id = item.id();
        let note_ids = raw_note_ids
            .iter()
            .map(|raw_note_id| {
                item.notes()
                    .iter()
                    .find(|note| note.id().value() == *raw_note_id)
                    .map(|note| note.id())
                    .ok_or_else(|| format!("unknown MIDI note id {raw_note_id} for this item"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        project
            .apply(DawAction::DeleteMidiNotes { item_id, note_ids })
            .map_err(|error| error.to_string())?;
        let result = json!({
            "item_id": item_id.value(),
            "deleted_note_ids": raw_note_ids,
        });
        persist_project_edit(&mut project, store, "deleted MIDI notes")?;
        Ok(result)
    }

    fn upsert_midi_controllers(
        &self,
        raw_item_id: u64,
        updates: Vec<MidiControllerData>,
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
        let item = project
            .midi_items()
            .iter()
            .find(|item| item.id().value() == raw_item_id)
            .ok_or_else(|| "unknown MIDI item id".to_owned())?;
        let item_id = item.id();
        let item_length = item.length_ticks();
        let mut controllers = item.controllers().to_vec();
        for update in &updates {
            if update.tick >= item_length {
                return Err(format!(
                    "controller tick {} is outside the MIDI item",
                    update.tick
                ));
            }
            if let Some(existing) = controllers
                .iter_mut()
                .find(|event| event.controller == update.controller && event.tick == update.tick)
            {
                *existing = *update;
            } else {
                controllers.push(*update);
            }
        }
        project
            .apply(DawAction::SetMidiControllers {
                item_id,
                controllers,
            })
            .map_err(|error| error.to_string())?;
        let points = updates
            .iter()
            .map(|point| {
                json!({
                    "controller": point.controller,
                    "tick": point.tick,
                    "value": point.value,
                })
            })
            .collect::<Vec<_>>();
        persist_project_edit(&mut project, store, "upserted MIDI controllers")?;
        Ok(json!({"item_id": item_id.value(), "points": points}))
    }

    fn upsert_midi_pitch_bends(
        &self,
        raw_item_id: u64,
        updates: Vec<MidiPitchBendData>,
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
        let item = project
            .midi_items()
            .iter()
            .find(|item| item.id().value() == raw_item_id)
            .ok_or_else(|| "unknown MIDI item id".to_owned())?;
        let item_id = item.id();
        let item_length = item.length_ticks();
        let mut pitch_bends = item.pitch_bends().to_vec();
        for update in &updates {
            if update.tick >= item_length {
                return Err(format!(
                    "pitch-bend tick {} is outside the MIDI item",
                    update.tick
                ));
            }
            if let Some(existing) = pitch_bends
                .iter_mut()
                .find(|event| event.tick == update.tick)
            {
                *existing = *update;
            } else {
                pitch_bends.push(*update);
            }
        }
        project
            .apply(DawAction::SetMidiPitchBends {
                item_id,
                pitch_bends,
            })
            .map_err(|error| error.to_string())?;
        let points = updates
            .iter()
            .map(|point| json!({"tick": point.tick, "value": point.value}))
            .collect::<Vec<_>>();
        persist_project_edit(&mut project, store, "upserted MIDI pitch bends")?;
        Ok(json!({"item_id": item_id.value(), "points": points}))
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

    fn set_track_mix(&self, changes: TrackMixChanges) -> Result<Value, String> {
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
            .find(|track| track.id().value() == changes.track_id)
            .ok_or_else(|| "unknown track id".to_owned())?;
        let track_id = track.id();
        let mut actions = Vec::new();
        if let Some(volume_db) = changes
            .volume_db
            .filter(|value| *value != track.volume_db())
        {
            actions.push(DawAction::SetTrackVolume {
                track_id,
                volume_db,
            });
        }
        if let Some(pan) = changes.pan.filter(|value| *value != track.pan()) {
            actions.push(DawAction::SetTrackPan { track_id, pan });
        }
        if let Some(muted) = changes.muted.filter(|value| *value != track.is_muted()) {
            actions.push(DawAction::SetTrackMute { track_id, muted });
        }
        if let Some(solo) = changes.solo.filter(|value| *value != track.is_solo()) {
            actions.push(DawAction::SetTrackSolo { track_id, solo });
        }
        if actions.is_empty() {
            return Ok(track_mix_result(track, false));
        }
        project
            .apply(DawAction::BatchTransaction {
                tx_id: track_id.value(),
                actions,
            })
            .map_err(|error| error.to_string())?;
        persist_project_edit(&mut project, store, "set track mix")?;
        let track = project
            .tracks()
            .iter()
            .find(|track| track.id() == track_id)
            .ok_or_else(|| "track disappeared after setting mix".to_owned())?;
        Ok(track_mix_result(track, true))
    }

    fn move_history(&self, direction: HistoryDirection) -> Result<Value, String> {
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
        let changed = match direction {
            HistoryDirection::Undo => project.undo(),
            HistoryDirection::Redo => project.redo(),
        }
        .map_err(|error| error.to_string())?;
        if !changed {
            return Ok(json!({"changed": false}));
        }
        if let Err(error) = store.save(&project) {
            let rollback = match direction {
                HistoryDirection::Undo => project.redo(),
                HistoryDirection::Redo => project.undo(),
            };
            match rollback {
                Ok(true) => {}
                Ok(false) => {
                    return Err(format!(
                        "failed to save history change: {error}; rollback had no history step"
                    ));
                }
                Err(rollback_error) => {
                    return Err(format!(
                        "failed to save history change: {error}; rollback failed: {rollback_error}"
                    ));
                }
            }
            return Err(format!("failed to save history change: {error}"));
        }
        Ok(json!({"changed": true}))
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
                "track_id": {"type": "integer", "minimum": 0},
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

fn midi_expression_query_tool() -> Tool {
    Tool::new(
        MIDI_EXPRESSION_QUERY_TOOL,
        "Read MIDI controller and pitch-bend events in a bounded project tick range.",
        rmcp::model::object(json!({
            "type": "object",
            "properties": {
                "track_id": {"type": "integer", "minimum": 0},
                "start_tick": {"type": "integer", "minimum": 0},
                "end_tick": {"type": "integer", "minimum": 1},
                "limit": {"type": "integer", "minimum": 1, "maximum": MAX_MIDI_EXPRESSION_RESULTS}
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

fn volume_automation_query_tool() -> Tool {
    Tool::new(
        VOLUME_AUTOMATION_QUERY_TOOL,
        "Read track volume automation points in a bounded project sample range.",
        rmcp::model::object(json!({
            "type": "object",
            "properties": {
                "track_id": {"type": "integer", "minimum": 0},
                "start_sample": {"type": "integer", "minimum": 0},
                "end_sample": {"type": "integer", "minimum": 1},
                "limit": {"type": "integer", "minimum": 1, "maximum": MAX_VOLUME_AUTOMATION_RESULTS}
            },
            "required": ["track_id", "start_sample", "end_sample"],
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

fn create_midi_item_tool() -> Tool {
    Tool::new(
        CREATE_MIDI_ITEM_TOOL,
        "Create one empty MIDI item on an existing track at an absolute project tick.",
        rmcp::model::object(json!({
            "type": "object",
            "properties": {
                "track_id": {"type": "integer", "minimum": 0},
                "start_tick": {"type": "integer", "minimum": 0},
                "length_ticks": {"type": "integer", "minimum": 1, "maximum": MAX_MIDI_ITEM_LENGTH_TICKS}
            },
            "required": ["track_id", "start_tick", "length_ticks"],
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

fn edit_midi_item_tool() -> Tool {
    Tool::new(
        EDIT_MIDI_ITEM_TOOL,
        "Move or resize an existing MIDI item without discarding notes.",
        rmcp::model::object(json!({
            "type": "object",
            "properties": {
                "item_id": {"type": "integer", "minimum": 0},
                "start_tick": {"type": "integer", "minimum": 0},
                "length_ticks": {"type": "integer", "minimum": 1, "maximum": MAX_MIDI_ITEM_LENGTH_TICKS}
            },
            "required": ["item_id", "start_tick", "length_ticks"],
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

fn edit_midi_note_tool() -> Tool {
    Tool::new(
        EDIT_MIDI_NOTE_TOOL,
        "Replace the timing, pitch, duration, and velocity of one note in a MIDI item.",
        rmcp::model::object(json!({
            "type": "object",
            "properties": {
                "item_id": {"type": "integer", "minimum": 0},
                "note_id": {"type": "integer", "minimum": 0},
                "pitch": {"type": "integer", "minimum": 0, "maximum": 127},
                "tick": {"type": "integer", "minimum": 0},
                "duration": {"type": "integer", "minimum": 1},
                "velocity": {"type": "integer", "minimum": 0, "maximum": 127}
            },
            "required": ["item_id", "note_id", "pitch", "tick", "duration", "velocity"],
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

fn delete_midi_notes_tool() -> Tool {
    Tool::new(
        DELETE_MIDI_NOTES_TOOL,
        "Delete a bounded set of notes from one MIDI item by stable note ID.",
        rmcp::model::object(json!({
            "type": "object",
            "properties": {
                "item_id": {"type": "integer", "minimum": 0},
                "note_ids": {
                    "type": "array",
                    "minItems": 1,
                    "maxItems": MAX_MIDI_NOTES_PER_DELETE,
                    "uniqueItems": true,
                    "items": {"type": "integer", "minimum": 0}
                }
            },
            "required": ["item_id", "note_ids"],
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

fn upsert_midi_controllers_tool() -> Tool {
    Tool::new(
        UPSERT_MIDI_CONTROLLERS_TOOL,
        "Insert or update a bounded batch of MIDI controller points, preserving other events.",
        rmcp::model::object(json!({
            "type": "object",
            "properties": {
                "item_id": {"type": "integer", "minimum": 0},
                "controllers": {
                    "type": "array",
                    "minItems": 1,
                    "maxItems": MAX_MIDI_EVENTS_PER_UPSERT,
                    "items": {
                        "type": "object",
                        "properties": {
                            "controller": {"type": "integer", "minimum": 0, "maximum": 127},
                            "tick": {"type": "integer", "minimum": 0},
                            "value": {"type": "integer", "minimum": 0, "maximum": 127}
                        },
                        "required": ["controller", "tick", "value"],
                        "additionalProperties": false
                    }
                }
            },
            "required": ["item_id", "controllers"],
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

fn upsert_midi_pitch_bends_tool() -> Tool {
    Tool::new(
        UPSERT_MIDI_PITCH_BENDS_TOOL,
        "Insert or update a bounded batch of MIDI pitch-bend points, preserving other ticks.",
        rmcp::model::object(json!({
            "type": "object",
            "properties": {
                "item_id": {"type": "integer", "minimum": 0},
                "pitch_bends": {
                    "type": "array",
                    "minItems": 1,
                    "maxItems": MAX_MIDI_EVENTS_PER_UPSERT,
                    "items": {
                        "type": "object",
                        "properties": {
                            "tick": {"type": "integer", "minimum": 0},
                            "value": {"type": "integer", "minimum": 0, "maximum": 16383}
                        },
                        "required": ["tick", "value"],
                        "additionalProperties": false
                    }
                }
            },
            "required": ["item_id", "pitch_bends"],
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

fn set_tempo_tool() -> Tool {
    Tool::new(
        SET_TEMPO_TOOL,
        "Insert or update one project tempo point at an absolute PPQ tick.",
        rmcp::model::object(json!({
            "type": "object",
            "properties": {
                "tick": {"type": "integer", "minimum": 0},
                "bpm": {"type": "number", "exclusiveMinimum": 0}
            },
            "required": ["tick", "bpm"],
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

fn set_time_signature_tool() -> Tool {
    Tool::new(
        SET_TIME_SIGNATURE_TOOL,
        "Insert or update one project time-signature point at a bar boundary.",
        rmcp::model::object(json!({
            "type": "object",
            "properties": {
                "tick": {"type": "integer", "minimum": 0},
                "numerator": {"type": "integer", "minimum": 1},
                "denominator": {"type": "integer", "minimum": 1}
            },
            "required": ["tick", "numerator", "denominator"],
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

fn parse_set_time_signature_arguments(
    arguments: Option<&serde_json::Map<String, Value>>,
) -> Result<(u64, TimeSignature), String> {
    let arguments = arguments.ok_or_else(|| "arguments are required".to_owned())?;
    if arguments
        .keys()
        .any(|key| !matches!(key.as_str(), "tick" | "numerator" | "denominator"))
    {
        return Err("arguments contain an unknown field".to_owned());
    }
    let integer = |key: &str| {
        arguments
            .get(key)
            .and_then(Value::as_u64)
            .ok_or_else(|| format!("{key} must be a non-negative integer"))
    };
    let tick = integer("tick")?;
    let numerator = u32::try_from(integer("numerator")?)
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| "numerator must be a positive 32-bit integer".to_owned())?;
    let denominator = u32::try_from(integer("denominator")?)
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| "denominator must be a positive 32-bit integer".to_owned())?;
    let signature =
        TimeSignature::new(numerator, denominator).map_err(|error| error.to_string())?;
    Ok((tick, signature))
}

fn parse_set_tempo_arguments(
    arguments: Option<&serde_json::Map<String, Value>>,
) -> Result<(u64, f64), String> {
    let arguments = arguments.ok_or_else(|| "arguments are required".to_owned())?;
    if arguments
        .keys()
        .any(|key| !matches!(key.as_str(), "tick" | "bpm"))
    {
        return Err("arguments contain an unknown field".to_owned());
    }
    let tick = arguments
        .get("tick")
        .and_then(Value::as_u64)
        .ok_or_else(|| "tick must be a non-negative integer".to_owned())?;
    let bpm = arguments
        .get("bpm")
        .and_then(Value::as_f64)
        .filter(|bpm| bpm.is_finite() && *bpm > 0.0)
        .ok_or_else(|| "bpm must be finite and greater than zero".to_owned())?;
    Ok((tick, bpm))
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

fn set_track_mix_tool() -> Tool {
    Tool::new(
        SET_TRACK_MIX_TOOL,
        "Set one or more base mixer controls for an existing track as one undoable edit.",
        rmcp::model::object(json!({
            "type": "object",
            "properties": {
                "track_id": {"type": "integer", "minimum": 0},
                "volume_db": {"type": "number"},
                "pan": {"type": "number", "minimum": -1, "maximum": 1},
                "muted": {"type": "boolean"},
                "solo": {"type": "boolean"}
            },
            "required": ["track_id"],
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

fn parse_track_mix_arguments(
    arguments: Option<&serde_json::Map<String, Value>>,
) -> Result<TrackMixChanges, String> {
    let arguments = arguments.ok_or_else(|| "arguments are required".to_owned())?;
    if arguments.keys().any(|key| {
        !matches!(
            key.as_str(),
            "track_id" | "volume_db" | "pan" | "muted" | "solo"
        )
    }) {
        return Err("arguments contain an unknown field".to_owned());
    }
    let track_id = arguments
        .get("track_id")
        .and_then(Value::as_u64)
        .ok_or_else(|| "track_id must be a non-negative integer".to_owned())?;
    let volume_db = arguments
        .get("volume_db")
        .map(|value| {
            value
                .as_f64()
                .filter(|value| value.is_finite() && (*value as f32).is_finite())
                .map(|value| value as f32)
                .ok_or_else(|| "volume_db must be a finite f32 value".to_owned())
        })
        .transpose()?;
    let pan = arguments
        .get("pan")
        .map(|value| {
            value
                .as_f64()
                .filter(|value| value.is_finite() && (-1.0..=1.0).contains(value))
                .map(|value| value as f32)
                .ok_or_else(|| "pan must be finite and within -1..=1".to_owned())
        })
        .transpose()?;
    let boolean = |key: &str| {
        arguments
            .get(key)
            .map(|value| {
                value
                    .as_bool()
                    .ok_or_else(|| format!("{key} must be a boolean"))
            })
            .transpose()
    };
    let changes = TrackMixChanges {
        track_id,
        volume_db,
        pan,
        muted: boolean("muted")?,
        solo: boolean("solo")?,
    };
    if changes.volume_db.is_none()
        && changes.pan.is_none()
        && changes.muted.is_none()
        && changes.solo.is_none()
    {
        return Err("at least one mixer field is required".to_owned());
    }
    Ok(changes)
}

fn track_mix_result(track: &aaadaw_core::Track, changed: bool) -> Value {
    json!({
        "track_id": track.id().value(),
        "volume_db": track.volume_db(),
        "pan": track.pan(),
        "muted": track.is_muted(),
        "solo": track.is_solo(),
        "changed": changed,
    })
}

fn history_tool(direction: HistoryDirection) -> Tool {
    let (name, description) = match direction {
        HistoryDirection::Undo => (
            UNDO_TOOL,
            "Undo the last project edit made during this MCP writer session.",
        ),
        HistoryDirection::Redo => (
            REDO_TOOL,
            "Redo the next project edit made during this MCP writer session.",
        ),
    };
    Tool::new(
        name,
        description,
        rmcp::model::object(json!({
            "type": "object",
            "properties": {},
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

fn parse_empty_arguments(arguments: Option<&serde_json::Map<String, Value>>) -> Result<(), String> {
    if arguments.is_some_and(|arguments| !arguments.is_empty()) {
        return Err("arguments must be empty".to_owned());
    }
    Ok(())
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

fn parse_create_midi_item_arguments(
    arguments: Option<&serde_json::Map<String, Value>>,
) -> Result<(u64, u64, u64), String> {
    let arguments = arguments.ok_or_else(|| "arguments are required".to_owned())?;
    if arguments
        .keys()
        .any(|key| !matches!(key.as_str(), "track_id" | "start_tick" | "length_ticks"))
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
    let start_tick = integer("start_tick")?;
    let length_ticks = integer("length_ticks")?;
    if length_ticks == 0 || length_ticks > MAX_MIDI_ITEM_LENGTH_TICKS {
        return Err(format!(
            "length_ticks must be in 1..={MAX_MIDI_ITEM_LENGTH_TICKS}"
        ));
    }
    if start_tick.checked_add(length_ticks).is_none() {
        return Err("start_tick plus length_ticks exceeds the supported range".to_owned());
    }
    Ok((track_id, start_tick, length_ticks))
}

fn parse_edit_midi_item_arguments(
    arguments: Option<&serde_json::Map<String, Value>>,
) -> Result<(u64, u64, u64), String> {
    let arguments = arguments.ok_or_else(|| "arguments are required".to_owned())?;
    if arguments
        .keys()
        .any(|key| !matches!(key.as_str(), "item_id" | "start_tick" | "length_ticks"))
    {
        return Err("arguments contain an unknown field".to_owned());
    }
    let integer = |key: &str| {
        arguments
            .get(key)
            .and_then(Value::as_u64)
            .ok_or_else(|| format!("{key} must be a non-negative integer"))
    };
    let item_id = integer("item_id")?;
    let start_tick = integer("start_tick")?;
    let length_ticks = integer("length_ticks")?;
    if length_ticks == 0 || length_ticks > MAX_MIDI_ITEM_LENGTH_TICKS {
        return Err(format!(
            "length_ticks must be in 1..={MAX_MIDI_ITEM_LENGTH_TICKS}"
        ));
    }
    if start_tick.checked_add(length_ticks).is_none() {
        return Err("start_tick plus length_ticks exceeds the supported range".to_owned());
    }
    Ok((item_id, start_tick, length_ticks))
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

fn parse_edit_midi_note_arguments(
    arguments: Option<&serde_json::Map<String, Value>>,
) -> Result<(u64, u64, MidiNoteData), String> {
    let arguments = arguments.ok_or_else(|| "arguments are required".to_owned())?;
    if arguments.keys().any(|key| {
        !matches!(
            key.as_str(),
            "item_id" | "note_id" | "pitch" | "tick" | "duration" | "velocity"
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
    let item_id = unsigned("item_id")?;
    let note_id = unsigned("note_id")?;
    let pitch =
        u8::try_from(unsigned("pitch")?).map_err(|_| "pitch must be in 0..=127".to_owned())?;
    if pitch > 127 {
        return Err("pitch must be in 0..=127".to_owned());
    }
    let tick = unsigned("tick")?;
    let duration = unsigned("duration")?;
    if duration == 0 {
        return Err("duration must be positive".to_owned());
    }
    let velocity = u8::try_from(unsigned("velocity")?)
        .map_err(|_| "velocity must be in 0..=127".to_owned())?;
    if velocity > 127 {
        return Err("velocity must be in 0..=127".to_owned());
    }
    Ok((
        item_id,
        note_id,
        MidiNoteData {
            pitch,
            tick,
            duration,
            velocity,
        },
    ))
}

fn parse_delete_midi_notes_arguments(
    arguments: Option<&serde_json::Map<String, Value>>,
) -> Result<(u64, Vec<u64>), String> {
    let arguments = arguments.ok_or_else(|| "arguments are required".to_owned())?;
    if arguments
        .keys()
        .any(|key| !matches!(key.as_str(), "item_id" | "note_ids"))
    {
        return Err("arguments contain an unknown field".to_owned());
    }
    let item_id = arguments
        .get("item_id")
        .and_then(Value::as_u64)
        .ok_or_else(|| "item_id must be a non-negative integer".to_owned())?;
    let note_ids = arguments
        .get("note_ids")
        .and_then(Value::as_array)
        .ok_or_else(|| "note_ids must be an array".to_owned())?;
    if note_ids.is_empty() || note_ids.len() > MAX_MIDI_NOTES_PER_DELETE {
        return Err(format!(
            "note_ids must contain 1..={MAX_MIDI_NOTES_PER_DELETE} identifiers"
        ));
    }
    let mut seen = std::collections::HashSet::with_capacity(note_ids.len());
    let note_ids = note_ids
        .iter()
        .map(|note_id| {
            let note_id = note_id
                .as_u64()
                .ok_or_else(|| "note_ids entries must be non-negative integers".to_owned())?;
            if !seen.insert(note_id) {
                return Err("note_ids must not contain duplicates".to_owned());
            }
            Ok(note_id)
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok((item_id, note_ids))
}

fn parse_upsert_midi_controllers_arguments(
    arguments: Option<&serde_json::Map<String, Value>>,
) -> Result<(u64, Vec<MidiControllerData>), String> {
    let arguments = arguments.ok_or_else(|| "arguments are required".to_owned())?;
    if arguments
        .keys()
        .any(|key| !matches!(key.as_str(), "item_id" | "controllers"))
    {
        return Err("arguments contain an unknown field".to_owned());
    }
    let item_id = arguments
        .get("item_id")
        .and_then(Value::as_u64)
        .ok_or_else(|| "item_id must be a non-negative integer".to_owned())?;
    let controllers = arguments
        .get("controllers")
        .and_then(Value::as_array)
        .ok_or_else(|| "controllers must be an array".to_owned())?;
    if controllers.is_empty() || controllers.len() > MAX_MIDI_EVENTS_PER_UPSERT {
        return Err(format!(
            "controllers must contain 1..={MAX_MIDI_EVENTS_PER_UPSERT} points"
        ));
    }
    let mut positions = std::collections::HashSet::with_capacity(controllers.len());
    let controllers = controllers
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let point = value
                .as_object()
                .ok_or_else(|| format!("controllers[{index}] must be an object"))?;
            if point
                .keys()
                .any(|key| !matches!(key.as_str(), "controller" | "tick" | "value"))
            {
                return Err(format!("controllers[{index}] contains an unknown field"));
            }
            let unsigned = |key: &str| {
                point.get(key).and_then(Value::as_u64).ok_or_else(|| {
                    format!("controllers[{index}].{key} must be a non-negative integer")
                })
            };
            let controller = u8::try_from(unsigned("controller")?)
                .map_err(|_| format!("controllers[{index}].controller must be in 0..=127"))?;
            if controller > 127 {
                return Err(format!(
                    "controllers[{index}].controller must be in 0..=127"
                ));
            }
            let tick = unsigned("tick")?;
            let value = u8::try_from(unsigned("value")?)
                .map_err(|_| format!("controllers[{index}].value must be in 0..=127"))?;
            if value > 127 {
                return Err(format!("controllers[{index}].value must be in 0..=127"));
            }
            if !positions.insert((controller, tick)) {
                return Err("controllers must not repeat a controller/tick pair".to_owned());
            }
            Ok(MidiControllerData {
                controller,
                tick,
                value,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok((item_id, controllers))
}

fn parse_upsert_midi_pitch_bends_arguments(
    arguments: Option<&serde_json::Map<String, Value>>,
) -> Result<(u64, Vec<MidiPitchBendData>), String> {
    let arguments = arguments.ok_or_else(|| "arguments are required".to_owned())?;
    if arguments
        .keys()
        .any(|key| !matches!(key.as_str(), "item_id" | "pitch_bends"))
    {
        return Err("arguments contain an unknown field".to_owned());
    }
    let item_id = arguments
        .get("item_id")
        .and_then(Value::as_u64)
        .ok_or_else(|| "item_id must be a non-negative integer".to_owned())?;
    let pitch_bends = arguments
        .get("pitch_bends")
        .and_then(Value::as_array)
        .ok_or_else(|| "pitch_bends must be an array".to_owned())?;
    if pitch_bends.is_empty() || pitch_bends.len() > MAX_MIDI_EVENTS_PER_UPSERT {
        return Err(format!(
            "pitch_bends must contain 1..={MAX_MIDI_EVENTS_PER_UPSERT} points"
        ));
    }
    let mut ticks = std::collections::HashSet::with_capacity(pitch_bends.len());
    let pitch_bends = pitch_bends
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let point = value
                .as_object()
                .ok_or_else(|| format!("pitch_bends[{index}] must be an object"))?;
            if point
                .keys()
                .any(|key| !matches!(key.as_str(), "tick" | "value"))
            {
                return Err(format!("pitch_bends[{index}] contains an unknown field"));
            }
            let tick = point.get("tick").and_then(Value::as_u64).ok_or_else(|| {
                format!("pitch_bends[{index}].tick must be a non-negative integer")
            })?;
            let raw_value = point.get("value").and_then(Value::as_u64).ok_or_else(|| {
                format!("pitch_bends[{index}].value must be an integer in 0..=16383")
            })?;
            let bend_value = u16::try_from(raw_value)
                .map_err(|_| format!("pitch_bends[{index}].value must be in 0..=16383"))?;
            if bend_value > 16_383 {
                return Err(format!("pitch_bends[{index}].value must be in 0..=16383"));
            }
            if !ticks.insert(tick) {
                return Err("pitch_bends must not repeat a tick".to_owned());
            }
            Ok(MidiPitchBendData {
                tick,
                value: bend_value,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok((item_id, pitch_bends))
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
    parse_midi_range_query_arguments(arguments, DEFAULT_NOTE_RESULTS, MAX_NOTE_RESULTS)
}

fn parse_midi_expression_query_arguments(
    arguments: Option<&serde_json::Map<String, Value>>,
) -> Result<(u64, u64, u64, usize), String> {
    parse_midi_range_query_arguments(
        arguments,
        DEFAULT_MIDI_EXPRESSION_RESULTS,
        MAX_MIDI_EXPRESSION_RESULTS,
    )
}

fn parse_midi_range_query_arguments(
    arguments: Option<&serde_json::Map<String, Value>>,
    default_limit: usize,
    maximum_limit: usize,
) -> Result<(u64, u64, u64, usize), String> {
    parse_bounded_range_query_arguments(
        arguments,
        "start_tick",
        "end_tick",
        default_limit,
        maximum_limit,
    )
}

fn parse_volume_automation_query_arguments(
    arguments: Option<&serde_json::Map<String, Value>>,
) -> Result<(u64, u64, u64, usize), String> {
    parse_bounded_range_query_arguments(
        arguments,
        "start_sample",
        "end_sample",
        DEFAULT_VOLUME_AUTOMATION_RESULTS,
        MAX_VOLUME_AUTOMATION_RESULTS,
    )
}

fn parse_bounded_range_query_arguments(
    arguments: Option<&serde_json::Map<String, Value>>,
    start_key: &str,
    end_key: &str,
    default_limit: usize,
    maximum_limit: usize,
) -> Result<(u64, u64, u64, usize), String> {
    let arguments = arguments.ok_or_else(|| "arguments are required".to_owned())?;
    let read_integer = |name: &str| {
        arguments
            .get(name)
            .and_then(Value::as_u64)
            .ok_or_else(|| format!("{name} must be a non-negative integer"))
    };
    let track_id = read_integer("track_id")?;
    let start = read_integer(start_key)?;
    let end = read_integer(end_key)?;
    let limit = arguments
        .get("limit")
        .map(|_| read_integer("limit"))
        .transpose()?
        .map(usize::try_from)
        .transpose()
        .map_err(|_| "limit is too large".to_owned())?
        .unwrap_or(default_limit);
    if arguments
        .keys()
        .any(|key| !["track_id", start_key, end_key, "limit"].contains(&key.as_str()))
    {
        return Err("arguments contain an unknown field".to_owned());
    }
    if !(1..=maximum_limit).contains(&limit) {
        return Err(format!("limit must be between 1 and {maximum_limit}"));
    }
    Ok((track_id, start, end, limit))
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct MidiExpressionCandidate {
    absolute_tick: u64,
    item_id: u64,
    kind: MidiExpressionKind,
    item_start_tick: u64,
    item_tick: u64,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum MidiExpressionKind {
    Controller { controller: u8, value: u8 },
    PitchBend { value: u16 },
}

fn scoped_query_midi_expression(
    project: &Project,
    track_id: u64,
    start_tick: u64,
    end_tick: u64,
    limit: usize,
) -> Result<Value, String> {
    let track_id = validate_midi_query_scope(
        project,
        track_id,
        start_tick,
        end_tick,
        limit,
        MAX_MIDI_EXPRESSION_RESULTS,
    )?;
    let mut events = std::collections::BinaryHeap::with_capacity(limit);
    let mut truncated = false;
    for item in project
        .midi_items()
        .iter()
        .filter(|item| item.track_id() == track_id)
    {
        for event in item.controllers() {
            let absolute_tick = item.start_tick().saturating_add(event.tick);
            if !(start_tick..end_tick).contains(&absolute_tick) {
                continue;
            }
            let candidate = MidiExpressionCandidate {
                absolute_tick,
                item_id: item.id().value(),
                kind: MidiExpressionKind::Controller {
                    controller: event.controller,
                    value: event.value,
                },
                item_start_tick: item.start_tick(),
                item_tick: event.tick,
            };
            insert_bounded_expression_candidate(&mut events, candidate, limit, &mut truncated);
        }
        for event in item.pitch_bends() {
            let absolute_tick = item.start_tick().saturating_add(event.tick);
            if !(start_tick..end_tick).contains(&absolute_tick) {
                continue;
            }
            let candidate = MidiExpressionCandidate {
                absolute_tick,
                item_id: item.id().value(),
                kind: MidiExpressionKind::PitchBend { value: event.value },
                item_start_tick: item.start_tick(),
                item_tick: event.tick,
            };
            insert_bounded_expression_candidate(&mut events, candidate, limit, &mut truncated);
        }
    }
    let events = events
        .into_sorted_vec()
        .into_iter()
        .map(|event| {
            let kind = match event.kind {
                MidiExpressionKind::Controller { controller, value } => {
                    json!({"type": "controller", "controller": controller, "value": value})
                }
                MidiExpressionKind::PitchBend { value } => {
                    json!({"type": "pitch_bend", "value": value})
                }
            };
            json!({
                "item_id": event.item_id,
                "item_start_tick": event.item_start_tick,
                "item_tick": event.item_tick,
                "tick": event.absolute_tick,
                "event": kind,
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({
        "track_id": track_id.value(),
        "start_tick": start_tick,
        "end_tick": end_tick,
        "events": events,
        "truncated": truncated,
    }))
}

fn scoped_query_volume_automation(
    project: &Project,
    track_id: u64,
    start_sample: u64,
    end_sample: u64,
    limit: usize,
) -> Result<Value, String> {
    if !(1..=MAX_VOLUME_AUTOMATION_RESULTS).contains(&limit) {
        return Err(format!(
            "limit must be between 1 and {MAX_VOLUME_AUTOMATION_RESULTS}"
        ));
    }
    if end_sample <= start_sample {
        return Err("end_sample must be greater than start_sample".to_owned());
    }
    let track = project
        .tracks()
        .iter()
        .find(|track| track.id().value() == track_id)
        .ok_or_else(|| "unknown track id".to_owned())?;
    let track_id = track.id();
    let mut points = track
        .volume_automation()
        .iter()
        .copied()
        .filter(|point| (start_sample..end_sample).contains(&point.sample()))
        .take(limit + 1)
        .collect::<Vec<_>>();
    let truncated = points.len() > limit;
    points.truncate(limit);
    let points = points
        .into_iter()
        .map(|point| json!({"sample": point.sample(), "gain_db": point.gain_db()}))
        .collect::<Vec<_>>();
    Ok(json!({
        "track_id": track_id.value(),
        "start_sample": start_sample,
        "end_sample": end_sample,
        "limit": limit,
        "points": points,
        "truncated": truncated,
    }))
}

fn insert_bounded_expression_candidate(
    events: &mut std::collections::BinaryHeap<MidiExpressionCandidate>,
    candidate: MidiExpressionCandidate,
    limit: usize,
    truncated: &mut bool,
) {
    if events.len() < limit {
        events.push(candidate);
    } else {
        *truncated = true;
        if events.peek().is_some_and(|latest| candidate < *latest) {
            events.pop();
            events.push(candidate);
        }
    }
}

fn scoped_query_notes(
    project: &Project,
    track_id: u64,
    start_tick: u64,
    end_tick: u64,
    limit: usize,
) -> Result<Value, String> {
    let track_id = validate_midi_query_scope(
        project,
        track_id,
        start_tick,
        end_tick,
        limit,
        MAX_NOTE_RESULTS,
    )?;
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

fn validate_midi_query_scope(
    project: &Project,
    track_id: u64,
    start_tick: u64,
    end_tick: u64,
    limit: usize,
    maximum_results: usize,
) -> Result<TrackId, String> {
    if !(1..=maximum_results).contains(&limit) {
        return Err(format!("limit must be between 1 and {maximum_results}"));
    }
    if end_tick <= start_tick {
        return Err("end_tick must be greater than start_tick".to_owned());
    }
    if end_tick - start_tick > MAX_MIDI_QUERY_TICKS {
        return Err(format!(
            "requested range exceeds {MAX_MIDI_QUERY_TICKS} ticks"
        ));
    }
    resolve_project_track_id(project, track_id)
}

fn resolve_project_track_id(project: &Project, track_id: u64) -> Result<TrackId, String> {
    project
        .tracks()
        .iter()
        .find(|track| track.id().value() == track_id)
        .map(|track| track.id())
        .ok_or_else(|| "unknown track id".to_owned())
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
            "volume_db": track.volume_db(),
            "pan": track.pan(),
            "muted": track.is_muted(),
            "solo": track.is_solo(),
            "record_armed": track.is_record_armed(),
            "output_track_id": track.output_track().map(|track_id| track_id.value()),
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
        DEFAULT_VOLUME_AUTOMATION_RESULTS, MAX_MAP_POINTS, MAX_MIDI_EVENTS_PER_UPSERT,
        MAX_MIDI_EXPRESSION_RESULTS, MAX_MIDI_ITEM_LENGTH_TICKS, MAX_MIDI_NOTES_PER_DELETE,
        MAX_MIDI_NOTES_PER_INSERT, MAX_MIDI_QUERY_TICKS, MAX_NOTE_RESULTS, MAX_TRACK_NAME_CHARS,
        MAX_TRACKS, MAX_VOLUME_AUTOMATION_RESULTS, parse_create_midi_item_arguments,
        parse_create_track_arguments, parse_delete_midi_notes_arguments,
        parse_edit_midi_item_arguments, parse_edit_midi_note_arguments,
        parse_insert_midi_notes_arguments, parse_midi_expression_query_arguments,
        parse_note_query_arguments, parse_quantize_midi_item_arguments, parse_set_tempo_arguments,
        parse_set_time_signature_arguments, parse_track_summary_uri,
        parse_upsert_midi_controllers_arguments, parse_upsert_midi_pitch_bends_arguments,
        parse_volume_automation_query_arguments, scoped_query_midi_expression, scoped_query_notes,
        scoped_query_volume_automation, structure_summary, track_midi_summary,
    };
    use aaadaw_core::{
        DawAction, MidiControllerData, MidiNoteData, MidiPitchBendData, Project, TimeSignature,
        VolumeAutomationPoint,
    };
    use serde_json::{Value, json};

    #[test]
    fn structure_summary_keeps_track_state_bounded() {
        let mut project = Project::new();
        for index in 0..=MAX_TRACKS {
            project
                .apply(DawAction::CreateTrack {
                    index,
                    name: format!("Track {index}"),
                })
                .unwrap();
        }

        let summary = structure_summary(&project);
        assert_eq!(summary["tracks"].as_array().unwrap().len(), MAX_TRACKS);
        assert_eq!(summary["tracks_truncated"], true);
    }

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
    fn create_midi_item_arguments_require_a_valid_bounded_range() {
        let parse = |value: Value| parse_create_midi_item_arguments(value.as_object());
        assert_eq!(
            parse(json!({"track_id": 0, "start_tick": 960, "length_ticks": 3840})).unwrap(),
            (0, 960, 3840)
        );
        for invalid in [
            json!({"track_id": -1, "start_tick": 0, "length_ticks": 1}),
            json!({"track_id": 1, "start_tick": 0, "length_ticks": 0}),
            json!({"track_id": 1, "start_tick": 0, "length_ticks": MAX_MIDI_ITEM_LENGTH_TICKS + 1}),
            json!({"track_id": 1, "start_tick": u64::MAX, "length_ticks": 1}),
            json!({"track_id": 1, "start_tick": 0, "length_ticks": 1, "extra": true}),
            json!({"track_id": 1, "start_tick": 1.5, "length_ticks": 1}),
        ] {
            assert!(parse(invalid).is_err());
        }
    }

    #[test]
    fn edit_midi_item_arguments_require_a_valid_bounded_range() {
        let parse = |value: Value| parse_edit_midi_item_arguments(value.as_object());
        assert_eq!(
            parse(json!({"item_id": 0, "start_tick": 960, "length_ticks": 3840})).unwrap(),
            (0, 960, 3840)
        );
        for invalid in [
            json!({"item_id": -1, "start_tick": 0, "length_ticks": 1}),
            json!({"item_id": 0, "start_tick": -1, "length_ticks": 1}),
            json!({"item_id": 0, "start_tick": 0, "length_ticks": 0}),
            json!({"item_id": 0, "start_tick": 0, "length_ticks": MAX_MIDI_ITEM_LENGTH_TICKS + 1}),
            json!({"item_id": 0, "start_tick": u64::MAX, "length_ticks": 1}),
            json!({"item_id": 0, "start_tick": 0, "length_ticks": 1, "extra": true}),
            json!({"item_id": 0, "start_tick": 1.5, "length_ticks": 1}),
        ] {
            assert!(parse(invalid).is_err());
        }
        assert!(parse_edit_midi_item_arguments(None).is_err());
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
    fn edit_midi_note_arguments_require_complete_valid_note_data() {
        let valid = json!({
            "item_id": 2,
            "note_id": 9,
            "pitch": 64,
            "tick": 120,
            "duration": 240,
            "velocity": 96
        });
        assert_eq!(
            parse_edit_midi_note_arguments(valid.as_object()).unwrap(),
            (
                2,
                9,
                MidiNoteData {
                    pitch: 64,
                    tick: 120,
                    duration: 240,
                    velocity: 96,
                }
            )
        );
        for invalid in [
            json!({"item_id": 2, "note_id": 9, "pitch": 128, "tick": 0, "duration": 1, "velocity": 1}),
            json!({"item_id": 2, "note_id": 9, "pitch": 60, "tick": 0, "duration": 0, "velocity": 1}),
            json!({"item_id": 2, "note_id": 9, "pitch": 60, "tick": -1, "duration": 1, "velocity": 1}),
            json!({"item_id": 2, "note_id": 9, "pitch": 60, "tick": 0, "duration": 1, "velocity": 128}),
            json!({"item_id": 2, "note_id": 9, "pitch": 60, "tick": 0, "duration": 1, "velocity": 1, "extra": true}),
        ] {
            assert!(
                parse_edit_midi_note_arguments(invalid.as_object()).is_err(),
                "accepted invalid note edit: {invalid}"
            );
        }
        assert!(parse_edit_midi_note_arguments(None).is_err());
    }

    #[test]
    fn delete_midi_notes_arguments_require_bounded_distinct_ids() {
        let parse = |note_ids: Value| {
            let value = json!({"item_id": 2, "note_ids": note_ids});
            parse_delete_midi_notes_arguments(value.as_object())
        };
        assert_eq!(parse(json!([4, 7])).unwrap(), (2, vec![4, 7]));
        for invalid in [
            json!([]),
            json!([4, 4]),
            json!([-1]),
            json!("4"),
            json!(vec![0; MAX_MIDI_NOTES_PER_DELETE + 1]),
        ] {
            assert!(parse(invalid).is_err());
        }
        let unknown_field = json!({"item_id": 2, "note_ids": [4], "extra": true});
        assert!(parse_delete_midi_notes_arguments(unknown_field.as_object()).is_err());
        assert!(parse_delete_midi_notes_arguments(None).is_err());
    }

    #[test]
    fn midi_controller_upsert_arguments_validate_bounded_unique_points() {
        let parse = |controllers: Value| {
            let value = json!({"item_id": 2, "controllers": controllers});
            parse_upsert_midi_controllers_arguments(value.as_object())
        };
        let too_many = Value::Array(
            (0..=MAX_MIDI_EVENTS_PER_UPSERT)
                .map(|_| json!({"controller": 11, "tick": 0, "value": 1}))
                .collect(),
        );
        assert_eq!(
            parse(json!([
                {"controller": 11, "tick": 0, "value": 100},
                {"controller": 10, "tick": 0, "value": 64}
            ]))
            .unwrap(),
            (
                2,
                vec![
                    MidiControllerData {
                        controller: 11,
                        tick: 0,
                        value: 100,
                    },
                    MidiControllerData {
                        controller: 10,
                        tick: 0,
                        value: 64,
                    }
                ]
            )
        );
        for invalid in [
            json!([]),
            json!([{"controller": 128, "tick": 0, "value": 1}]),
            json!([{"controller": 11, "tick": 0, "value": 128}]),
            json!([{"controller": 11, "tick": -1, "value": 1}]),
            json!([{"controller": 11, "tick": 0, "value": 1, "extra": true}]),
            json!([
                {"controller": 11, "tick": 0, "value": 1},
                {"controller": 11, "tick": 0, "value": 2}
            ]),
            too_many,
        ] {
            assert!(parse(invalid).is_err());
        }
        let unknown_field = json!({"item_id": 2, "controllers": [{"controller": 11, "tick": 0, "value": 1}], "extra": true});
        assert!(parse_upsert_midi_controllers_arguments(unknown_field.as_object()).is_err());
        assert!(parse_upsert_midi_controllers_arguments(None).is_err());
    }

    #[test]
    fn midi_pitch_bend_upsert_arguments_validate_bounded_unique_points() {
        let parse = |pitch_bends: Value| {
            let value = json!({"item_id": 2, "pitch_bends": pitch_bends});
            parse_upsert_midi_pitch_bends_arguments(value.as_object())
        };
        let too_many = Value::Array(
            (0..=MAX_MIDI_EVENTS_PER_UPSERT)
                .map(|_| json!({"tick": 0, "value": 8192}))
                .collect(),
        );
        assert_eq!(
            parse(json!([{"tick": 0, "value": 8192}, {"tick": 480, "value": 16383}])).unwrap(),
            (
                2,
                vec![
                    MidiPitchBendData {
                        tick: 0,
                        value: 8192,
                    },
                    MidiPitchBendData {
                        tick: 480,
                        value: 16383,
                    }
                ]
            )
        );
        for invalid in [
            json!([]),
            json!([{"tick": -1, "value": 8192}]),
            json!([{"tick": 0, "value": 16384}]),
            json!([{"tick": 0, "value": "8192"}]),
            json!([{"tick": 0, "value": 8192, "extra": true}]),
            json!([{"tick": 0, "value": 8192}, {"tick": 0, "value": 9000}]),
            too_many,
        ] {
            assert!(parse(invalid).is_err());
        }
        let unknown_field =
            json!({"item_id": 2, "pitch_bends": [{"tick": 0, "value": 1}], "extra": true});
        assert!(parse_upsert_midi_pitch_bends_arguments(unknown_field.as_object()).is_err());
        assert!(parse_upsert_midi_pitch_bends_arguments(None).is_err());
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
    fn set_tempo_arguments_require_integer_tick_and_finite_positive_bpm() {
        let arguments = |tick: Value, bpm: Value| {
            serde_json::Map::from_iter([("tick".to_owned(), tick), ("bpm".to_owned(), bpm)])
        };
        assert_eq!(
            parse_set_tempo_arguments(Some(&arguments(json!(960), json!(98.5)))).unwrap(),
            (960, 98.5)
        );
        for invalid in [
            arguments(json!(-1), json!(120)),
            arguments(json!(1.5), json!(120)),
            arguments(json!(0), json!(0)),
            arguments(json!(0), json!(-1)),
            arguments(json!(0), json!("120")),
            arguments(json!(0), json!(true)),
        ] {
            assert!(parse_set_tempo_arguments(Some(&invalid)).is_err());
        }
        for missing in [
            serde_json::Map::from_iter([("tick".to_owned(), json!(0))]),
            serde_json::Map::from_iter([("bpm".to_owned(), json!(120))]),
        ] {
            assert!(parse_set_tempo_arguments(Some(&missing)).is_err());
        }
        let mut extra = arguments(json!(0), json!(120));
        extra.insert("curve".to_owned(), json!("step"));
        assert!(parse_set_tempo_arguments(Some(&extra)).is_err());
        assert!(parse_set_tempo_arguments(None).is_err());
    }

    #[test]
    fn set_time_signature_arguments_require_bounded_positive_integers() {
        let arguments = |tick: Value, numerator: Value, denominator: Value| {
            serde_json::Map::from_iter([
                ("tick".to_owned(), tick),
                ("numerator".to_owned(), numerator),
                ("denominator".to_owned(), denominator),
            ])
        };
        assert_eq!(
            parse_set_time_signature_arguments(Some(&arguments(json!(11520), json!(3), json!(4))))
                .unwrap(),
            (11520, TimeSignature::new(3, 4).unwrap())
        );
        for invalid in [
            arguments(json!(-1), json!(3), json!(4)),
            arguments(json!(1.5), json!(3), json!(4)),
            arguments(json!(0), json!(0), json!(4)),
            arguments(json!(0), json!(3), json!(0)),
            arguments(json!(0), json!(u64::from(u32::MAX) + 1), json!(4)),
            arguments(json!(0), json!(3), json!("4")),
        ] {
            assert!(parse_set_time_signature_arguments(Some(&invalid)).is_err());
        }
        for missing in [
            serde_json::Map::from_iter([
                ("numerator".to_owned(), json!(3)),
                ("denominator".to_owned(), json!(4)),
            ]),
            serde_json::Map::from_iter([
                ("tick".to_owned(), json!(0)),
                ("denominator".to_owned(), json!(4)),
            ]),
            serde_json::Map::from_iter([
                ("tick".to_owned(), json!(0)),
                ("numerator".to_owned(), json!(3)),
            ]),
        ] {
            assert!(parse_set_time_signature_arguments(Some(&missing)).is_err());
        }
        let mut extra = arguments(json!(0), json!(3), json!(4));
        extra.insert("swing".to_owned(), json!(0.5));
        assert!(parse_set_time_signature_arguments(Some(&extra)).is_err());
        assert!(parse_set_time_signature_arguments(None).is_err());
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
    fn midi_expression_query_orders_events_bounds_results_and_validates_ranges() {
        let mut project = Project::new();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Expression".to_owned(),
            })
            .unwrap();
        let track_id = project.tracks()[0].id();
        for start_tick in [100, 110] {
            project
                .apply(DawAction::InsertMidiItem {
                    track_id,
                    start_tick,
                    length_ticks: 1_000,
                })
                .unwrap();
        }
        let first_item_id = project.midi_items()[0].id();
        let second_item_id = project.midi_items()[1].id();
        project
            .apply(DawAction::SetMidiControllers {
                item_id: first_item_id,
                controllers: vec![
                    MidiControllerData {
                        controller: 9,
                        tick: 19,
                        value: 10,
                    },
                    MidiControllerData {
                        controller: 11,
                        tick: 20,
                        value: 70,
                    },
                    MidiControllerData {
                        controller: 1,
                        tick: 20,
                        value: 50,
                    },
                    MidiControllerData {
                        controller: 7,
                        tick: 30,
                        value: 90,
                    },
                ],
            })
            .unwrap();
        project
            .apply(DawAction::SetMidiPitchBends {
                item_id: first_item_id,
                pitch_bends: vec![MidiPitchBendData {
                    tick: 20,
                    value: 10_000,
                }],
            })
            .unwrap();
        project
            .apply(DawAction::SetMidiControllers {
                item_id: second_item_id,
                controllers: vec![MidiControllerData {
                    controller: 11,
                    tick: 10,
                    value: 80,
                }],
            })
            .unwrap();

        let result =
            scoped_query_midi_expression(&project, track_id.value(), 120, 130, 10).unwrap();
        let events = result["events"].as_array().unwrap();
        assert_eq!(events.len(), 4);
        assert!(
            events
                .iter()
                .all(|event| (120..130).contains(&event["tick"].as_u64().unwrap()))
        );
        assert!(events.iter().all(|event| event["event"]["controller"] != 7));
        assert_eq!(result["truncated"], false);
        assert_eq!(events[0]["item_id"], first_item_id.value());
        assert_eq!(events[0]["item_start_tick"], 100);
        assert_eq!(events[0]["item_tick"], 20);
        assert_eq!(events[0]["tick"], 120);
        assert_eq!(events[0]["event"]["type"], "controller");
        assert_eq!(events[0]["event"]["controller"], 1);
        assert_eq!(events[0]["event"]["value"], 50);
        assert_eq!(events[1]["event"]["controller"], 11);
        assert_eq!(events[2]["event"]["type"], "pitch_bend");
        assert_eq!(events[2]["event"]["value"], 10_000);
        assert_eq!(events[3]["item_id"], second_item_id.value());
        assert_eq!(events[3]["item_tick"], 10);

        let truncated =
            scoped_query_midi_expression(&project, track_id.value(), 120, 130, 3).unwrap();
        assert_eq!(truncated["events"].as_array().unwrap().len(), 3);
        assert_eq!(truncated["truncated"], true);
        assert!(scoped_query_midi_expression(&project, track_id.value(), 120, 120, 10).is_err());
        assert!(
            scoped_query_midi_expression(
                &project,
                track_id.value(),
                0,
                MAX_MIDI_QUERY_TICKS + 1,
                10
            )
            .is_err()
        );
        assert!(scoped_query_midi_expression(&project, track_id.value() + 1, 0, 100, 10).is_err());
        assert!(scoped_query_midi_expression(&project, track_id.value(), 0, 100, 0).is_err());
        assert!(
            scoped_query_midi_expression(
                &project,
                track_id.value(),
                0,
                100,
                MAX_MIDI_EXPRESSION_RESULTS + 1,
            )
            .is_err()
        );
    }

    #[test]
    fn midi_expression_query_parser_rejects_unknown_fields_and_non_integer_values() {
        let arguments = json!({
            "track_id": 1,
            "start_tick": 0,
            "end_tick": 100,
            "limit": 12
        });
        assert_eq!(
            parse_midi_expression_query_arguments(arguments.as_object()).unwrap(),
            (1, 0, 100, 12)
        );
        for arguments in [
            json!({"track_id": 1, "start_tick": 0, "end_tick": 100, "other": true}),
            json!({"track_id": 1, "start_tick": -1, "end_tick": 100}),
            json!({"track_id": 1, "start_tick": 0, "end_tick": 100, "limit": 0}),
            json!({"track_id": 1, "start_tick": 0, "end_tick": 100, "limit": MAX_MIDI_EXPRESSION_RESULTS + 1}),
        ] {
            assert!(parse_midi_expression_query_arguments(arguments.as_object()).is_err());
        }
    }

    #[test]
    fn volume_automation_query_uses_half_open_ranges_and_caps_results() {
        let mut project = Project::new();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Volume".to_owned(),
            })
            .unwrap();
        let track_id = project.tracks()[0].id();
        project
            .apply(DawAction::SetTrackVolumeAutomation {
                track_id,
                points: vec![
                    VolumeAutomationPoint::new(99, -1.0).unwrap(),
                    VolumeAutomationPoint::new(100, -2.0).unwrap(),
                    VolumeAutomationPoint::new(110, -3.0).unwrap(),
                    VolumeAutomationPoint::new(120, -4.0).unwrap(),
                    VolumeAutomationPoint::new(130, -5.0).unwrap(),
                ],
            })
            .unwrap();

        let result =
            scoped_query_volume_automation(&project, track_id.value(), 100, 130, 10).unwrap();
        assert_eq!(result["track_id"], track_id.value());
        assert_eq!(
            result["points"],
            json!([
                {"sample": 100, "gain_db": -2.0},
                {"sample": 110, "gain_db": -3.0},
                {"sample": 120, "gain_db": -4.0}
            ])
        );
        assert_eq!(result["truncated"], false);

        let truncated =
            scoped_query_volume_automation(&project, track_id.value(), 100, 130, 2).unwrap();
        assert_eq!(truncated["points"].as_array().unwrap().len(), 2);
        assert_eq!(truncated["truncated"], true);
        assert!(scoped_query_volume_automation(&project, track_id.value(), 100, 100, 2).is_err());
        assert!(scoped_query_volume_automation(&project, track_id.value(), 130, 100, 2).is_err());
        assert!(scoped_query_volume_automation(&project, track_id.value() + 1, 0, 100, 2).is_err());
        assert!(scoped_query_volume_automation(&project, track_id.value(), 0, 100, 0).is_err());
        assert!(
            scoped_query_volume_automation(
                &project,
                track_id.value(),
                0,
                100,
                MAX_VOLUME_AUTOMATION_RESULTS + 1,
            )
            .is_err()
        );
    }

    #[test]
    fn volume_automation_query_parser_rejects_invalid_fields_and_bounds() {
        let default_limit = json!({
            "track_id": 7,
            "start_sample": 100,
            "end_sample": 200
        });
        assert_eq!(
            parse_volume_automation_query_arguments(default_limit.as_object()).unwrap(),
            (7, 100, 200, DEFAULT_VOLUME_AUTOMATION_RESULTS)
        );
        let valid = json!({
            "track_id": 7,
            "start_sample": 100,
            "end_sample": 200,
            "limit": 12
        });
        assert_eq!(
            parse_volume_automation_query_arguments(valid.as_object()).unwrap(),
            (7, 100, 200, 12)
        );
        for arguments in [
            json!({"track_id": 7, "start_sample": 100, "end_sample": 200, "other": true}),
            json!({"track_id": 7, "start_sample": -1, "end_sample": 200}),
            json!({"track_id": 7, "start_sample": 100, "end_sample": 200, "limit": 0}),
            json!({"track_id": 7, "start_sample": 100, "end_sample": 200, "limit": MAX_VOLUME_AUTOMATION_RESULTS + 1}),
        ] {
            assert!(parse_volume_automation_query_arguments(arguments.as_object()).is_err());
        }
        assert!(parse_volume_automation_query_arguments(None).is_err());
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
        for (start, end) in [(100, 100), (200, 100), (0, MAX_MIDI_QUERY_TICKS + 1)] {
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
