# MCP project inspection and editing

AAADAW exposes a saved project to an MCP client over the standard input/output (STDIO) transport. The default server is read-only. Track creation is available only when explicitly enabled.

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

- `daw://project/structure`: sample rate, PPQ, track IDs/names/types, and tempo/meter points. At most 512 tracks and 256 points per map are returned; the response marks truncated collections.
- `daw://project/track/{track_id}/midi_summary`: a per-track MIDI item and note count with the overall project-tick range. It returns aggregates rather than individual notes.
- `daw_scoped_query_notes`: accepts `track_id`, `start_tick`, and `end_tick` for a half-open project-tick range `[start_tick, end_tick)`. It returns notes ordered by absolute tick, item ID, then note ID. The default response cap is 256 notes; callers may request up to 512. Ranges may span at most 245,760 project ticks. The result includes `truncated` when more matching notes exist than the requested limit.

## Read-only access boundary

Without `--write`, the server opens the project in SQLite read-only mode and does not migrate its schema. It currently requires the project to use the schema version supported by this AAADAW build. An older or newer schema is rejected without modification.

The read-only server reads the committed project state once at startup. Edits made in AAADAW after the server starts are not reflected until the client restarts the MCP process. It does not load plugins or access audio devices. The MCP client process can read the project file named in its launch configuration, so only configure it with files you intend to expose.

## Explicit write mode

Add `--write` after the project path to expose `daw_create_track`:

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

The tool accepts one non-empty track name of at most 128 characters, creates a normal audio track at the end of the track list through `DawAction::CreateTrack`, and saves it before returning the new track ID. Invalid requests do not change the project. This opt-in currently enables track creation only.

Write mode takes an exclusive project session lock for the server's lifetime. The desktop app and a second MCP writer refuse to open the same project until the MCP server exits. Stop the writer before opening the project in the desktop app; its completed changes are then available when the project is reopened. A sibling `.lock` file may remain after the session ends; the operating-system lock is released automatically. Only enable write mode for a project you intend to modify.
