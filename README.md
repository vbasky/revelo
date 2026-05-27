# MediaInfo Rust Engine — Port Plan

A behavior-equivalent Rust port of the MediaInfoLib engine. Goal is capability
parity with the C++ engine, validated by differential testing against the
existing `mediainfo` CLI. Idiomaticity is not a goal — transliteration is
acceptable.

## Status

Phase 0 — differential test harness only. No engine code yet.

## Strategy

1. **Differential harness first.** Treat the installed C++ `mediainfo` binary as
   the oracle. Every parser, every output formatter is "done" when its output
   matches the oracle's at schema level (not byte-for-byte string equality).
2. **Transliteration over re-architecture.** Mirror the C++ class hierarchy.
   `Box<dyn Trait>` for virtuals, `unsafe` and raw pointers where ownership
   doesn't fit Rust's model, `macro_rules!` for MediaInfoLib's macro-heavy
   parser idiom (`Get_B4`, `Skip_XX`, `Element_Begin`, etc.).
3. **ZenLib first, then `File__Analyze`, then parsers.** Nothing else builds
   without the foundational types. Parsers ported in test-coverage priority:
   containers → audio codecs → video codecs → long tail.
4. **C ABI shim last.** Once the engine works, expose a `cdylib` matching the
   `MediaInfo_*` C entry points so downstream tooling can swap implementations.

## Planned crates

| Crate | Purpose | Status |
|---|---|---|
| `diff-harness` | Drives differential testing against the C++ oracle | scaffolded |
| `zenlib` | Transliteration of ZenLib (`Ztring`, `File`, `BitStream`, etc.) | not started |
| `mediainfo-core` | `File__Analyze` infrastructure, stream model, config, events | not started |
| `mediainfo-parsers-container` | MP4, MKV, MPEG-TS, AVI, MPEG-PS, FLV, etc. | not started |
| `mediainfo-parsers-audio` | AAC, AC3, DTS, FLAC, MP3, Opus, etc. | not started |
| `mediainfo-parsers-video` | AVC, HEVC, AV1, VC1, MPEG-2, ProRes, etc. | not started |
| `mediainfo-export` | XML, JSON, Text, EBUCore, MPEG-7 formatters | not started |
| `mediainfo-cdylib` | C ABI shim exposing `MediaInfo_*` entry points | not started |

## Building

```sh
cd rust-engine
cargo build --release
```

## Running the harness

```sh
# Diff against the installed C++ mediainfo binary
cargo run -p diff-harness -- /path/to/media-file.mp4
```

Until a Rust engine exists, the harness only prints the C++ output. Once
`mediainfo-core` is wired up, the harness will run both engines and diff their
XML output.

## Reference

The C++ engine lives in a sibling repo: `../../MediaInfoLib`.
