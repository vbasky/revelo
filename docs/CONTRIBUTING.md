# Contributing to revelo

A guide for anyone who wants to add a parser, fix a bug, or understand how
the engine works.

---

## Architecture

```bash
Cargo workspace root
├─ crates/zenlib              C++-style integer types + bitstream helpers
├─ crates/revelo-core        The parser engine (FileAnalyze, streams, config)
├─ crates/revelo-export      Output formatters (XML, Text, JSON, EBUCore, …)
├─ crates/revelo-cli         CLI tool (cargo run --bin revelo -- path/to/file)
├─ crates/revelo-cdylib      C dynamic library (MediaInfo_New/Open/Inform/…)
├─ crates/revelo-reader      Reader layer (File, Directory, HTTP, MMS)
├─ crates/revelo-diff        Differential testing against `mediainfo` oracle
├─ crates/revelo-parsers-audio       Audio codec parsers (56 parsers)
├─ crates/revelo-parsers-container   Container parsers (42 parsers)
├─ crates/revelo-parsers-image       Image format parsers (19 parsers)
├─ crates/revelo-parsers-tag         Tag/metadata parsers (12 parsers)
├─ crates/revelo-parsers-text        Text/subtitle parsers (21 parsers)
├─ crates/revelo-parsers-video       Video codec parsers (29 parsers)
└─ crates/revelo-parsers-archive     Archive format parsers (11 parsers)
```

### How a file gets parsed

1. **CLI** reads the file into a `Vec<u8>` buffer.
2. The **parser table** (a flat array of function pointers) is iterated.
3. Each parser function receives a `&mut FileAnalyze` — it peeks at bytes,
   tries to match a magic number or signature, and returns `true` if it
   recognized the format.
4. The first parser that returns `true` wins. It fills metadata into the
   `StreamCollection` via `fa.fill(StreamKind::Video, 0, "Width", "1920", false)`.
5. After parsing, **computed fields** are derived (Bits_Pixel_Frame,
   Compression_Ratio, Format_Profile, etc.).
6. The **export formatter** renders the `StreamCollection` into XML/Text/JSON.

### Key types

| Type | Purpose |
| --- | --- |
| `FileAnalyze` | Byte buffer + stream collection + element trace tree |
| `StreamCollection` | BTreeMap of `StreamKind` → Vec of `Stream` |
| `Stream` | One stream's fields (BTreeMap of String → Ztring) |
| `Ztring` | Thin wrapper around `String` for C++ parity |
| `MediaConfig` | Global config: demux level, trace format, multi-file |
| `DemuxState` | Per-stream frame counter, PTS/DTS tracking |
| `TraceNode` | Hierarchical trace tree for debug output |

---

## Adding a new parser

### Step 1: Create the file

Create `crates/revelo-parsers-<domain>/src/<format>.rs`:

```rust
use revelo_core::{FileAnalyze, StreamKind};

/// [Format name] parser.
///
/// Detection: looks for the magic bytes `XXXX` at offset 0.
pub fn parse_my_format(fa: &mut FileAnalyze) -> bool {
    let head = fa.peek_raw(4);
    let Some(h) = head else { return false };

    // Magic check
    if h != b"MYFM" { return false; }

    // Prepare streams
    fa.stream_prepare(StreamKind::General);
    fa.fill(StreamKind::General, 0, "Format", "MyFormat", false);

    // Read metadata
    let mut width: u32 = 0;
    fa.get_b4(&mut width, "Width");

    fa.stream_prepare(StreamKind::Video);
    fa.fill(StreamKind::Video, 0, "Width", width.to_string(), false);

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_my_format() {
        let data = b"MYFM\x00\x00\x02\x80"; // 640px width
        let mut fa = FileAnalyze::new(data);
        assert!(parse_my_format(&mut fa));
        let val = fa.streams()
            .stream(StreamKind::Video, 0)
            .and_then(|s| s.get("Width"))
            .map(|z| z.as_str());
        assert_eq!(val, Some("640"));
    }
}
```

### Step 2: Register in `lib.rs`

In `crates/revelo-parsers-<domain>/src/lib.rs`:

```rust
pub mod my_format;
pub use my_format::parse_my_format;
```

### Step 3: Add to the parser table

In `crates/revelo-cli/src/main.rs` and `crates/revelo-cdylib/src/lib.rs`:

1. Add `parse_my_format` to the `use revelo_parsers_*` import.
2. Add `parse_my_format` to the parser array.
3. Update the array size count: `[fn(&mut FileAnalyze) -> bool; 151]`

### Step 4: Add to docs

Add your format to `docs/formats.md` with detection method and spec reference.

### Step 5: Run revelo-diff

```bash
cargo run --bin revelo-diff -- /path/to/sample.myformat
```

If the output doesn't byte-match `mediainfo`, fix fields until it does.

---

## API conventions

### Method naming

All public parser API methods use **snake_case**:

- `fa.get_b4(&mut val, "Name")` — read 4 bytes big-endian
- `fa.peek_b4(&mut val)` — peek without advancing
- `fa.skip_b4("Name")` — skip 4 bytes
- `fa.fill(StreamKind, pos, "Parameter", "Value", replace_bool)` — write a field
- `fa.stream_prepare(StreamKind)` — allocate a new stream slot
- `fa.remain()` — bytes remaining in buffer
- `fa.element_begin("name")` / `fa.element_end()` — trace tree

### Types

Type aliases from `zenlib::types` (used in declarations):

- `Int8u` = `u8`, `Int32u` = `u32`, `Int64u` = `u64`
- `Float32` = `f32`, `Float64` = `f64`

### Filling output

Fields are written to `StreamCollection` via `fa.fill()`. The field name
must match what MediaInfoLib outputs, because the diff harness compares
XML byte-for-byte.

Common field names by stream kind:

**General:** `Format`, `Format_Profile`, `Duration`, `OverallBitRate`,
  `FileSize`, `AudioCount`, `VideoCount`, `StreamSize`

**Video:** `Width`, `Height`, `BitDepth`, `ChromaSubsampling`,
  `FrameRate`, `FrameRate_Mode`, `ColorSpace`, `ScanType`,
  `DisplayAspectRatio`, `Encoded_Library`, `Format_Profile`,
  `Format_Level`, `colour_primaries`, `transfer_characteristics`,
  `matrix_coefficients`

**Audio:** `SamplingRate`, `Channels`, `BitDepth`, `BitRate`,
  `BitRate_Mode`, `Format`, `Format_Profile`, `Compression_Mode`,
  `ChannelPositions`, `Language`

**Text:** `Format`, `Format_Info`, `Language`, `MuxingMode`

---

## Glossary

| Term | Meaning |
| --- | --- |
| **Annex B** | Byte-stream format for AVC/HEVC with `0x000001` start codes |
| **NAL unit** | Network Abstraction Layer — a coded slice/picture in AVC/HEVC/VVC |
| **SPS / PPS / VPS** | Sequence/Picture/Video Parameter Set — codec configuration |
| **VUI** | Video Usability Information — colour, aspect, timing metadata |
| **OBU** | Open Bitstream Unit — AV1 frame/subframe container |
| **esds** | Elementary Stream Descriptor (MPEG-4 Systems) — AAC config |
| **ftyp** | File Type box (MP4) — identifies brand/variant |
| **EBML** | Extensible Binary Meta Language — Matroska's encoding scheme |
| **CodecPrivate** | Codec initialization data embedded in Matroska container |
| **ADTS** | Audio Data Transport Stream — AAC frame format with sync word |
| **SEI** | Supplemental Enhancement Information — encoder metadata in AVC/HEVC |
| **HDR10** | High Dynamic Range: PQ EOTF + BT.2020 + ST 2086 metadata |
| **dvcC / dvvC** | Dolby Vision configuration boxes in MP4 |
| **PSI** | Program Specific Information — PAT/PMT tables in MPEG-TS |
| **PTS / DTS** | Presentation/Decode Time Stamp |
| **IBI** | Index of Binary Information — frame seek table |

---

## Conventions

- **Every parser returns `bool`:** `true` if it recognized the format, `false` otherwise.
- **First match wins:** The parser table is ordered. Containers run before elementary codec parsers.
- **Tests are mandatory:** Every parser module must have at least one `#[test]` that validates the parser recognizes valid input and rejects invalid input.
- **Field names must match MediaInfoLib exactly:** revelo-diff does byte-level comparison of XML output.
- **Doc comments use `//!`** for module-level docs and `///` for function/struct docs.
