use aaadaw_core::{DawAction, Project, TempoCurve, TrackId};
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
        }
        std::future::ready(Ok(ListToolsResult::with_all_items(tools)))
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        match name {
            MIDI_QUERY_TOOL => Some(midi_query_tool()),
            CREATE_TRACK_TOOL if self.writable => Some(create_track_tool()),
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
        if let Err(error) = store.save(&project) {
            if let Err(undo_error) = project.undo() {
                return Err(format!(
                    "failed to save created track: {error}; rollback failed: {undo_error}"
                ));
            }
            return Err(format!("failed to save created track: {error}"));
        }
        Ok(result)
    }
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
        MAX_MAP_POINTS, MAX_NOTE_QUERY_TICKS, MAX_NOTE_RESULTS, MAX_TRACK_NAME_CHARS, MAX_TRACKS,
        parse_create_track_arguments, parse_note_query_arguments, parse_track_summary_uri,
        scoped_query_notes, structure_summary, track_midi_summary,
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
