# MCP project inspection and editing

AAADAW exposes a saved project to an MCP client over the standard input/output (STDIO) transport. The default server is read-only. Project edits are available only when explicitly enabled.

## Launching a server

Configure your MCP client to start the AAADAW executable with the project file to inspect. For example, a client using a JSON server configuration can use:

```json
{
  "mcpServers": {
    "aaadaw": {
      "command": "/absolute/path/to/aaadaw",
      "args": [
        "mcp",
        "--stdio",
        "--project",
        "/absolute/path/to/song.aaadaw"
      ]
    }
  }
}
```

The server provides:

- `daw://project/structure`: sample rate, PPQ, bounded track IDs/names/types, volume dB, pan, mute/solo and record-arm state, output bus IDs, and tempo/meter points. A null output bus ID means Master. At most 512 tracks and 256 points per map are returned; the response marks truncated collections.
- `daw://project/track/{track_id}/midi_summary`: a per-track MIDI item and note count with the overall project-tick range. It returns aggregates rather than individual notes.
- `daw_scoped_query_notes`: accepts `track_id`, `start_tick`, and `end_tick` for a half-open project-tick range `[start_tick, end_tick)`. It returns notes ordered by absolute tick, item ID, then note ID. The default response cap is 256 notes; callers may request up to 512. Ranges may span at most 245,760 project ticks. The result includes `truncated` when more matching notes exist than the requested limit.

## Read-only access boundary

Without `--write`, the server opens the project in SQLite read-only mode and does not migrate its schema. It currently requires the project to use the schema version supported by this AAADAW build. An older or newer schema is rejected without modification.

The read-only server reads the committed project state once at startup. Edits made in AAADAW after the server starts are not reflected until the client restarts the MCP process. It does not load plugins or access audio devices. The MCP client process can read the project file named in its launch configuration, so only configure it with files you intend to expose.

## Explicit write mode

Add `--write` after the project path to expose the project-editing tools:

```json
{
  "mcpServers": {
    "aaadaw-writer": {
      "command": "/absolute/path/to/aaadaw",
      "args": [
        "mcp",
        "--stdio",
        "--project",
        "/absolute/path/to/song.aaadaw",
        "--write"
      ]
    }
  }
}
```

The `daw_create_track` tool accepts one non-empty track name of at most 128 characters, creates a normal audio track at the end of the track list through `DawAction::CreateTrack`, and saves it before returning the new track ID. `daw_insert_midi_notes` accepts a track ID, an existing MIDI item ID, and 1–512 notes with clip-relative tick, duration, pitch, and velocity. It applies the whole batch through one `DawAction::AddMidiNotes`, saves it before returning the new note IDs, and rejects items that do not belong to the named track. `daw_quantize_midi_item` accepts a track and MIDI item ID, a musical grid fraction (for example `1/16`), and an optional strength from 0 to 1 (default 1). It applies `DawAction::QuantizeItem`, saves changed notes, and returns the stable IDs of notes whose starts moved. `daw_set_volume_automation_point` accepts a track ID, an absolute project sample position, and gain in dB from -60 to +6. It inserts or replaces that sample's point while preserving the rest of the lane, applies `DawAction::SetTrackVolumeAutomation`, and saves before success. Repeating the same value at the same sample is a no-op. Invalid requests do not change the project.

`daw_set_track_record_arm` accepts an existing track ID and explicit `armed` boolean, applies `DawAction::SetTrackRecordArm`, and saves the prepared record-arm state; it does not start recording or access an audio device. Repeating the existing state is a no-op. Invalid requests do not change the project.

`daw_set_track_mix` accepts a track ID and one or more of `volume_db`, `pan`, `muted`, and `solo`. Supplied controls are validated and applied together as one atomic `DawAction::BatchTransaction`; omitted controls remain unchanged. The response returns the full resulting mix state, and an unchanged request does not create history or write the project.

`daw_set_tempo_point` accepts an absolute project `tick` and a finite positive `bpm`. It inserts or replaces that point through `DawAction::SetTempo` and saves the project before returning. Setting a point to its existing BPM is a no-op. Invalid ticks or BPM values do not change the project. The edit participates in the same writer-session undo/redo history.

`daw_undo` and `daw_redo` move one edit through the in-memory project history for the current MCP writer process and save the resulting snapshot before success. Each new writer starts with empty history because project snapshots do not persist undo history; these tools therefore affect only edits made since that writer started. An empty undo/redo stack is a no-op.

Write mode takes an exclusive project session lock for the server's lifetime. The desktop app and a second MCP writer refuse to open the same project until the MCP server exits. Stop the writer before opening the project in the desktop app; its completed changes are then available when the project is reopened. A sibling `.lock` file may remain after the session ends; the operating-system lock is released automatically. Only enable write mode for a project you intend to modify.
