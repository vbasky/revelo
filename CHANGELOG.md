# Changelog

## [0.2.0] - 2026-05-29

### Added

- Ergonomic `Reader` API (`revelio_core::reader`) — fluent, value-returning byte/bits reader wrapping `FileAnalyze` (native `u8`/`u16`/`u32` types, `Option`/`?`-based reads, no out-parameters)
- `revelio_core::prelude` module
- Workspace lints table (`Cargo.toml`) — `unreachable_pub`, `clippy::unwrap_used`, `clippy::doc_markdown`, `clippy::cast_possible_truncation`, `clippy::cast_sign_loss`
- `rustfmt.toml` — edition 2024, max_width 100, reorder_imports
- `#![deny(unsafe_code)]` on all 12 non-cdylib crate roots
- Intra-doc links between `FileAnalyze`, `StreamCollection`, `ElementTree`, `StreamKind`
- `rust-toolchain.toml` — stable channel with `clippy`/`rustfmt`/`rust-docs`
- GitHub Actions CI workflow (`.github/workflows/ci.yml`) — fmt, check, clippy, test, doc
- `LICENSE` — BSD-2-Clause
- `.cargo/config.toml` — build configuration
- `justfile` — developer task runner
- `deny.toml` — license audit configuration
- `CHANGELOG.md`

### Changed

- Migrated parsers off the C++-style `FileAnalyze::get_*` out-parameter API to the ergonomic `Reader` API across the audio, video, image, text, and container crates (including the 3.4k-line `mp4`); behavior-preserving, verified by the workspace test suite
- Toolchain switched from nightly to **stable** — edition 2024 and let-chains are both stabilized, so no nightly features are required
- Renamed `diff-harness` → `revelio-diff`; it now uses `revelio_dispatcher::detect()` instead of a hand-maintained parser table
- Parser dispatch table (`revelio-dispatcher`) now carries inline format-name comments
- `zenlib` `FromRadix`/`FromRadixSigned` traits changed to `pub(super)`; `zenlib_re_export` re-exports to `pub(crate)`
- `[workspace.lints.rustdoc]` allows intentional byte-layout notation (`invalid_html_tags`, `broken_intra_doc_links`)
- Bare URLs in doc comments wrapped in `<>`
- CI no longer fails the build on warnings (`-D warnings` dropped); `cargo fmt --check` is reported but non-blocking

### Fixed

- IAMF parser: reject OBU headers that claim to extend past the buffer instead of panicking on an out-of-bounds payload slice

### Removed

- Dead `fill_str` method from `FileAnalyze`
- `try_into().unwrap()` in `ac3.rs` — replaced with direct indexing

## 0.1.0

- 177 parsers across 7 domain crates (archive, audio, container, image, tag, text, video)
- Core engine (`revelio-core`): `FileAnalyze` byte reader, `StreamCollection`, `ElementTree`, bitstream mode
- Parallel parser dispatch via `rayon` (`revelio-dispatcher`)
- 10 output formatters: XML, JSON, Text, EBUCore, MPEG-7, PBCore, NISO, FIMS, Graph, RevTMD
- C ABI shim (`revelio-cdylib`) — drop-in replacement for `libmediainfo`
- CLI (`revelio-cli`) with `--xml`, `--json`, `--multi-file`, `--demux`, `--trace`
- 579 unit tests
- Differential testing harness against `mediainfo` oracle
