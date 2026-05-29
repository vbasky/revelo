# Changelog

## [0.2.3] - 2026-05-29

### Added

- CSV output format (`--csv`)
- Summary output mode (`--summary`) with aggregate stream statistics
- `--inform-version` and `--inform-timestamp` flags for text output provenance
- `--log-file <FILE>` flag for writing output to file
- `Format_Level_Inferred` computed field for AV1 — shows the minimum required
  level based on resolution and frame rate (e.g. `4.1` for 1080p60)

### Fixed

- AV1 `Format_Profile` now includes level (e.g. `Main@L5.3` instead of just `Main`)
- AV1 `Format_Info` now shows `AOMedia Video 1`
- AV1 sequence header parser no longer returns `None` when timing info present
- AV1 level now reads from codec config record instead of last operating point
- BPG test helper `unimplemented!()` panic for VSI values 21-34 bits
- MKV `Title` field now populated from `HANDLER_NAME` and `TITLE` tags

## [0.2.2] - 2026-05-29

### Added

- CLI migration to clap derive API with `--help`, `--version`, `-h`, `-V`, subcommand skeleton (inspect/diff/batch/verify/extract)
- `--video-only` and `--audio-only` flags for stream filtering
- `--stream KIND:INDEX` flag for selecting specific streams (repeatable)
- `--verify` flag with `IsComplete` field derived from parser truncation detection
- `--text` flag (previously documented but missing from the parser)
- `StreamCollection::filter_keep()` method in `revelo-core`
- `IsComplete` field in text output display
- Future improvement roadmap in README

### Fixed

- `revelo --help` and `revelo -h` now print help instead of falling through as a file path
- Bare `revelo` with no arguments prints help and exits 0 instead of erroring
- Unknown flags like `--foobar` are rejected with a proper error message
- `--demux` and `--trace` values are validated at parse time

### Changed

- Stream filtering and verify flags added to CLI struct
- `StreamCollection` gains `filter_keep()` for removing streams by kind and position
- Bumped MSRV to 1.85 in workspace Cargo.toml (already the minimum Rust version)

## [0.2.0] - 2026-05-29

### Added

- APV (Advanced Professional Video) parser (`revelo_parsers_video::apv`) — detects the `aPv1` signature, walks to the first frame/access-unit PBU, and fills profile\@level, dimensions, chroma format, bit depth, and colour description. Ported from MediaInfoLib's `File_Apv.cpp`.
- Ergonomic `Reader` API (`revelo_core::reader`) — fluent, value-returning byte/bits reader wrapping `FileAnalyze` (native `u8`/`u16`/`u32` types, `Option`/`?`-based reads, no out-parameters)
- `revelo_core::prelude` module
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
- Renamed `diff-harness` → `revelo-diff`; it now uses `revelo_dispatcher::detect()` instead of a hand-maintained parser table
- Parser dispatch table (`revelo-dispatcher`) now carries inline format-name comments
- `revelo-util` `FromRadix`/`FromRadixSigned` traits changed to `pub(super)`; `revelo_util_re_export` re-exports to `pub(crate)`
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
- Core engine (`revelo-core`): `FileAnalyze` byte reader, `StreamCollection`, `ElementTree`, bitstream mode
- Parallel parser dispatch via `rayon` (`revelo-dispatcher`)
- 10 output formatters: XML, JSON, Text, EBUCore, MPEG-7, PBCore, NISO, FIMS, Graph, RevTMD
- C ABI shim (`revelo-cdylib`) — drop-in replacement for `libmediainfo`
- CLI (`revelo-cli`) with `--xml`, `--json`, `--multi-file`, `--demux`, `--trace`
- 579 unit tests
- Differential testing harness against `mediainfo` oracle
