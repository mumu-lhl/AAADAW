# MCP project inspection

AAADAW can expose a saved project to an MCP client over the standard input/output (STDIO) transport. This first server is read-only and loads one snapshot when it starts.

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

## Snapshot and access boundary

The server opens the project in SQLite read-only mode and does not migrate its schema. It currently requires the project to use the schema version supported by this AAADAW build. An older or newer schema is rejected without modification.

The server reads the committed project state once at startup. Edits made in AAADAW after the server starts are not reflected until the client restarts the MCP process. The MCP server has no write tools, does not load plugins, and does not access audio devices. The MCP client process can read the project file named in its launch configuration, so only configure it with files you intend to expose.
