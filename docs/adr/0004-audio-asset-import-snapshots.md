# Audio asset imports are immutable snapshots

Importing an external audio file stores an immutable snapshot in the project and leaves the original file untouched. The project records the original path and a SHA-256 fingerprint for stale-source checks, but playback always uses embedded bytes; changing the original never changes existing AudioItems. Refreshing audio creates a new asset reference and updates placements explicitly through undoable project actions, avoiding silent changes to every item sharing the old asset.
