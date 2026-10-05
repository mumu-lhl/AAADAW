# SQLite WAL Project Lifecycle

**Status: accepted.** A `.aaadaw` project is a SQLite database. While a writable
`ProjectStore` is open, SQLite WAL sidecars are part of the live database state;
the main file alone is not a complete backup. A successful checkpoint after all
other connections have released their read snapshots is the supported boundary
for treating the project as one portable file.

## Decision

- Writable opens create the database when needed, require `journal_mode=WAL`,
  set a five-second busy timeout, `synchronous=NORMAL`, and
  `wal_autocheckpoint=1000` pages. The store rejects a journal mode SQLite could
  not set to WAL.
- Project saves replace the persisted snapshot inside one immediate SQLite
  transaction. Schema migrations update DDL and `user_version` in one exclusive
  transaction. A failed transaction does not expose a partially replaced
  snapshot.
- `ProjectStore::checkpoint` runs `wal_checkpoint(TRUNCATE)`. If another
  connection pins a WAL read snapshot, the operation returns
  `StorageError::CheckpointBusy`; retry after that connection closes its read
  transaction. `ProjectStore::close` checkpoints before dropping its connection.
  If close reports a checkpoint error, callers must not assume sidecars were
  folded into the main file; after other readers release the project, reopen it
  and retry the checkpoint.
- Copy, move, or back up a project as one `.aaadaw` file only after all writable
  stores have closed successfully and no other connection is keeping the WAL
  active. While a project is open, preserve the main database and its `-wal` and
  `-shm` sidecars together. Never delete sidecars by hand to make the project
  appear self-contained.
- SQLite replays committed WAL frames when a database is reopened after a
  process exits without running `ProjectStore::close`. This recovers committed
  work after process termination; it does not make the current `NORMAL`
  synchronous policy equivalent to syncing every commit to stable storage. A
  sudden power loss can discard recent acknowledged transactions, although
  SQLite's WAL consistency guarantees still apply.
- `ProjectStore::load_read_only` uses a read-only SQLite connection, accepts
  only the current schema, and does not migrate or modify the project. It is
  intended for inspection, not as a substitute for a closed single-file backup.
  A live WAL project must retain the sidecars needed by SQLite readers.

## Consequences

- Cleanly closed projects can be copied with ordinary file-copy tools and opened
  as a single file.
- Crash recovery remains SQLite's responsibility; the next writable open sees
  committed WAL state and then follows the normal checkpoint lifecycle.
- Callers must propagate checkpoint errors instead of silently deleting WAL
  files or claiming that a single-file export succeeded.
- The selected `NORMAL` policy favors lower commit latency over a forced storage
  sync for every transaction. Changing that tradeoff requires a separate
  measured durability decision.

## Verification

`crates/aaadaw-storage/tests/project_store.rs` verifies that close removes WAL
sidecars, a closed project can be copied and reopened as one file, and a child
process's committed WAL state is recovered after it exits without destructors.
The same integration suite checks that a checkpoint blocked by an active reader
returns `CheckpointBusy` and succeeds after that reader closes.

## References

- [SQLite Write-Ahead Logging](https://www.sqlite.org/wal.html)
- [SQLite `synchronous` pragma](https://www.sqlite.org/pragma.html#pragma_synchronous)
- [SQLite Atomic Commit](https://www.sqlite.org/atomiccommit.html)
