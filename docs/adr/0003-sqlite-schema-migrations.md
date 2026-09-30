# SQLite Schema Migrations

**Status: accepted.** `PRAGMA user_version` is the schema-version source of truth. On open, the storage adapter applies each missing forward migration and updates `user_version` in the same exclusive transaction, so DDL and version changes commit or roll back together; databases from a newer unsupported version are rejected without modification. Migrations are owned by `aaadaw-storage`, while SQLite handles page layout, WAL, and transaction durability.
