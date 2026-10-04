# Local cryoglyph patch

This is the unmodified `cryoglyph 0.1.0` crate source from crates.io, under the included MIT, Apache-2.0, and Zlib licenses. The only source-manifest change is the `lru` requirement, raised from `0.16` to `0.18.2` to carry upstream's fix for RUSTSEC-2026-0253 while preserving Iced 0.14's `wgpu 27` dependency.

The same dependency update is in upstream commit [`a18bea2`](https://github.com/iced-rs/cryoglyph/commit/a18bea27afd50e1841e56e167d3bb892a8d366bc). Remove this local patch when a compatible crates.io release of `cryoglyph` includes it.
