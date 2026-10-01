# Repository guidance

- Run Rust tests with `cargo xtest` (`cargo nextest run --workspace`) so the workspace uses nextest.
- Keep project state changes behind `aaadaw-core`'s public `Project` interface and `DawAction`; UI, audio, storage, and MCP code must not mutate domain state directly.
- Consult `ROADMAP.md` for implementation scope and `docs/design/AAADAW_System_Architecture_Design.md` for architecture requirements.
- Before implementing or changing UI, follow [`DESIGN.md`](DESIGN.md) as the UI/UX contract.
