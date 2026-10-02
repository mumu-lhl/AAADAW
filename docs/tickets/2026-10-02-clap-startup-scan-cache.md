# CLAP startup scan cache

## Goal

Make discovered CLAP plug-ins available immediately after launch while refreshing configured search paths in the background.

## Scope

- Persist the last successful plug-in catalog and scan metadata in application preferences.
- Load the cached catalog at startup so Settings and the FX picker can use it before a refresh finishes.
- Start one background scan of the configured paths and replace the cache after a successful scan.
- Keep the previous catalog available if refresh fails; report the refresh state and error in Settings.
- Ignore malformed or incompatible cache data and continue with an empty catalog.

## Out of scope

Out-of-process plug-in scanning or crash isolation; those remain in Phase 2.

## Dependencies

[`2026-10-02-clap-plugin-discovery.md`](2026-10-02-clap-plugin-discovery.md).

## Acceptance

- A valid cache populates the Settings catalog and Add picker at launch without waiting for disk scanning.
- Startup refresh runs once using saved search paths and updates the visible catalog when it completes.
- A failed refresh leaves the previous successful catalog usable and reports the failure.
- Cache writes are atomic, and unreadable or unsupported cache data does not block project startup.

## Verification

Test cache persistence, malformed data, refresh success/failure, and startup task dispatch. Run `cargo xtest` and strict Clippy; inspect cached catalog and refresh state in the real app using private Xvfb.

## UI acceptance

Settings distinguishes cached results from an active refresh or a failed refresh. The FX picker remains populated from the last successful catalog while a refresh runs.

## Status

Complete. The app restores the last successful catalog before launching its startup scan, refreshes configured paths in the background, and retains cached results if refresh fails. Verification passed with `cargo xtest` (194 workspace tests), strict app Clippy, and a private Xvfb run that scanned `/home/mumulhl/.clap` and wrote the cache without touching the normal user config.
