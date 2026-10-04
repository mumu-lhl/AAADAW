use aaadaw_core::{Project, TempoCurve, TrackId};
use aaadaw_storage::ProjectStore;
use rmcp::{
    ErrorData as McpError, ServerHandler, ServiceExt,
    model::{
        Implementation, ListResourceTemplatesResult, ListResourcesResult, PaginatedRequestParams,
        ProtocolVersion, RawResource, RawResourceTemplate, ReadResourceRequestParams,
        ReadResourceResult, Resource, ResourceContents, ResourceTemplate, ServerCapabilities,
        ServerInfo,
    },
    service::{RequestContext, RoleServer},
};
use serde_json::{Value, json};
use std::{error::Error, path::Path};

const STRUCTURE_URI: &str = "daw://project/structure";
const MIDI_SUMMARY_TEMPLATE: &str = "daw://project/track/{track_id}/midi_summary";
const MAX_MAP_POINTS: usize = 256;
const MAX_TRACKS: usize = 512;

pub fn run(project_path: impl AsRef<Path>) -> Result<(), Box<dyn Error>> {
    let project = ProjectStore::load_read_only(project_path)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        let server = ProjectMcpServer { project };
        let service = server.serve(rmcp::transport::stdio()).await?;
        service.waiting().await?;
        Ok::<_, Box<dyn Error>>(())
    })
}

struct ProjectMcpServer {
    project: Project,
}

impl ServerHandler for ProjectMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_resources().build())
            .with_server_info(Implementation::new("aaadaw", env!("CARGO_PKG_VERSION")))
            .with_protocol_version(ProtocolVersion::default())
            .with_instructions(
                "Read-only snapshot of the saved AAADAW project loaded at server startup.",
            )
    }

    fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListResourcesResult, McpError>> + Send + '_ {
        let resource = Resource::new(
            RawResource::new(STRUCTURE_URI, "Project structure")
                .with_description("Track names and types, tempo map, and meter map."),
            None,
        );
        std::future::ready(Ok(ListResourcesResult::with_all_items(vec![resource])))
    }

    fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListResourceTemplatesResult, McpError>> + Send + '_
    {
        let template = ResourceTemplate::new(
            RawResourceTemplate::new(MIDI_SUMMARY_TEMPLATE, "Track MIDI summary")
                .with_description("Bounded aggregate counts and tick range for one track."),
            None,
        );
        std::future::ready(Ok(ListResourceTemplatesResult::with_all_items(vec![
            template,
        ])))
    }

    fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ReadResourceResult, McpError>> + Send + '_ {
        let result = if request.uri == STRUCTURE_URI {
            serde_json::to_string(&structure_summary(&self.project))
                .map(|text| (request.uri, text))
                .map_err(|error| McpError::internal_error(error.to_string(), None))
        } else if let Some(track_id) = parse_track_summary_uri(&request.uri) {
            self.project
                .tracks()
                .iter()
                .find(|track| track.id().value() == track_id)
                .ok_or_else(|| McpError::invalid_params("unknown track id", None))
                .and_then(|track| {
                    serde_json::to_string(&track_midi_summary(&self.project, track.id()))
                        .map(|text| (request.uri, text))
                        .map_err(|error| McpError::internal_error(error.to_string(), None))
                })
        } else {
            Err(McpError::invalid_params(
                "unknown project resource URI",
                None,
            ))
        };
        std::future::ready(result.map(|(uri, text)| {
            ReadResourceResult::new(vec![
                ResourceContents::text(text, uri).with_mime_type("application/json"),
            ])
        }))
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
        MAX_MAP_POINTS, MAX_TRACKS, parse_track_summary_uri, structure_summary, track_midi_summary,
    };
    use aaadaw_core::{DawAction, MidiNoteData, Project, TimeSignature};

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
}
