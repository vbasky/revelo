# Hacking on revelo

Quick reference for the development workflow.

## Build

```bash
cargo build                    # Debug build
cargo build --release          # Optimized build
cargo run --bin revelo -- --text /path/to/file.mp4
cargo run --bin revelo -- --json /path/to/file.mp4
```

## Test

```bash
cargo test                     # All 576 tests
cargo test -p revelo-core     # Core engine tests only
cargo test -- avc              # Filter by name
```

## Diff harness

The diff harness runs revelo against MediaInfoLib's `mediainfo` binary
and compares output byte-for-byte or by structural equivalence.

```bash
cargo run --bin revelo-diff -- /path/to/media/file.mp4
```

Requires `mediainfo` on `$PATH`.

## CLI options

```bash
revelo --text video.mp4       # Text output (default)
revelo --xml video.mp4        # XML output
revelo --json video.mp4       # JSON output
revelo --demux=frame video.ts # Set demux level
revelo --trace=8 video.mp4    # Set trace verbosity
revelo --multi-file bdmv/     # Scan companion files
```

## Lint

```bash
cargo clippy                   # Run clippy lints (zero warnings expected)
cargo clippy -- -D warnings    # Treat warnings as errors
```

## File structure

```bash
crates/revelo-core/src/
├── file_analyze.rs    Parser byte-reader engine (get_b4, fill, remain, …)
├── stream.rs          Stream kind, stream fields, stream collection
├── element.rs         Trace tree node + element_end/begin/param
├── events.rs          Demux levels, DemuxEvent, DemuxState, TraceNode renderers
├── config.rs          MediaConfig (demux level, trace format, multi-file)
├── computed_fields.rs Post-parse: Bits_Pixel_Frame, Compression_Ratio, …
├── channel_splitting.rs  SMPTE ST 337 AES3 deinterleaving
├── channel_grouping.rs   SMPTE ST 337 merge
├── interlacement.rs   FieldTracker, ScanOrder, InterlacementMode
├── ibi.rs             Index of Binary Information (seek table)
├── mime.rs            MIME type mapping
├── multi_file.rs      Multi-file loader + duplicate detection
├── timecode.rs        SMPTE timecode parser (DF/NDF)
└── lib.rs             Crate root (module declarations)

crates/revelo-parsers-<domain>/src/
├── lib.rs             Module declarations + re-exports
└── <format>.rs        One parser per format (parse_<format>)
```

## Common patterns

### Reading bytes from the buffer

```rust
let mut val: u32 = 0;
fa.get_b4(&mut val, "field_name");   // Read 4 bytes BE, record in trace
fa.peek_b4(&mut val);                // Read without advancing buffer
fa.skip_hexa(16, "header");          // Skip 16 bytes
let head = fa.peek_raw(4);           // Peek raw byte slice
let count = fa.remain();             // Bytes remaining
```

### Writing fields

```rust
fa.fill(StreamKind::Video, 0, "Width", "1920", false);
//                                  ^field ^value  ^replace
```

The `replace` parameter: `false` = don't overwrite existing value,
`true` = overwrite.

## Debugging parsers

Add `#[test]` functions that validate each field the parser fills.
Use revelo-diff to compare against oracle output:

```bash
# Run against a real file and see what's missing
cargo run --bin revelo-diff -- /path/to/sample.mp4

# If XML doesn't match byte-for-byte, check:
# 1. Are all field names correct?
# 2. Are duration/bitrate computed the same way as MediaInfoLib?
# 3. Are numeric values rounded the same way?
```
