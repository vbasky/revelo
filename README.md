# 🦀 revelio

**Read technical metadata from any media file — in pure Rust.**

A library and CLI for containers, audio & video codecs, image formats, subtitle
streams, archives, and embedded tags. A port of MediaInfoLib, validated
byte-for-byte against the C++ `mediainfo` oracle through differential testing.

[![License](https://img.shields.io/badge/license-BSD--2--Clause-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.85+-orange.svg)](https://www.rust-lang.org)
[![Version](https://img.shields.io/badge/version-0.1.0-green.svg)](https://github.com/vbasky/revelio)
[![Parsers](https://img.shields.io/badge/parsers-193-blue.svg)](docs/formats.md)
[![Tests](https://img.shields.io/badge/tests-579%20passing-brightgreen.svg)](https://github.com/vbasky/revelio)

## How it reads a file

First revelio detects the format (every parser races to claim the bytes), then it
walks the container's *native* on-disk structure, reading fields box-by-box. An
MP4/MOV file is a tree of boxes ("atoms") — `moov` holds the metadata, `mdat` the
coded samples:

```text
  ftyp                          file brand: isom · qt · M4A …
  moov                          movie-level metadata
    mvhd                        timescale, duration → General: Duration
    trak                        one per track
      tkhd                      track geometry → Video: Width, Height
      edts ▸ elst               edit list / start delay
      mdia
        mdhd                    media header → Language
        hdlr                    vide·soun·subt·text → selects stream kind
        minf ▸ stbl             sample table
          stsd                  codec sample entry → Format (avc1/hvc1/mp4a…)
            avcC ▸ hvcC ▸ esds  codec config → Profile, Level, BitDepth
            colr ▸ pasp         colour, pixel aspect → primaries, DAR
            dvcC ▸ dvvC         Dolby Vision → HDR_Format
          stts                  time-to-sample → FrameRate
          stsz                  sample sizes → StreamSize, BitRate
          stco ▸ co64           chunk offsets
    udta ▸ meta ▸ ilst          iTunes tags, cover art → General: Title
  mdat                          coded samples — sized; scanned for x264/x265 tags
```

(`▸` marks a box that just wraps the next.) revelio walks every container's
*native* structure the same way — here are the other big families.

**Matroska / WebM** — EBML elements (`id ▸ size ▸ data`) under one Segment:

```text
  EBML                          header: DocType (matroska / webm), version
  Segment                       the single top-level element
    SeekHead                    index of the elements below
    Info                        TimestampScale, Duration, Segment UUID
    Tracks ▸ TrackEntry         CodecID → Format; Video/Audio sub-elements
                                → Width, Height, Channels, SamplingRate
    Tags                        Title, Encoder, cover art
    Cluster ▸ SimpleBlock       coded frames (sized; timestamps only)
```

**MPEG-TS** — a flat stream of 188-byte packets, demuxed by PID:

```text
  packet · 188 B                sync 0x47 · PID · payload-unit-start
  PAT  (PID 0)                  lists each program's PMT PID
  PMT                           elementary streams: PID + stream_type
  PES                           → Video/Audio: Format + codec params
```

**RIFF** — chunked; AVI and WAV share the layout:

```text
  RIFF 'AVI '                   LIST hdrl ▸ avih (frame rate, dimensions)
    LIST strl ▸ strh/strf       per-stream header + format → codec params
    LIST movi                   interleaved samples (00dc, 01wb, …)
  RIFF 'WAVE'                   fmt (codec, rate, channels) · data
    bext ▸ iXML ▸ axml          BWF metadata → Title, timecode, loudness
```

Whatever the container, the output is the same: one **General** stream for the
file plus one stream per track — **Video · Audio · Text · Image · Menu · Other**.

## MediaInfoLib comparison

Revelio is a from-scratch port of MediaInfoLib v26.05. Every line is new Rust —
no C++ translation, no FFI wrappers, no generated bindings.

| | MediaInfoLib | Revelio |
|---|---|---|
| **Language** | C++ | Pure Rust |
| **Memory safety** | Manual | Compile-time guaranteed |
| **Build** | `./Configure` + `make` + 10 system deps | `cargo build` (no system libs) |
| **Install** | `apt install mediainfo` / `brew install mediainfo` | `cargo install revelio-cli` |
| **Parser model** | Virtual `File__Analyze` hierarchy | `fn(&mut FileAnalyze) -> bool` flat table, parallel race via rayon |
| **Output fidelity** | Reference oracle | Byte-equal XML (differential harness) |
| **License** | BSD-2-Clause | BSD-2-Clause |
| **Format support** | ~200 formats | 193 parsers, 185 fields |
| **WASM** | No | Compiles on `wasm32-unknown-unknown` |

## Project scale

| Metric | Value |
| --- | --- |
| Parsers | 193 across 8 domains |
| Output fields | 185 (all gaps closed) |
| Output formatters | 10 — XML / Text / JSON + 7 domain |
| Workspace crates | 15 |
| Tests | 579 passing |
| License | BSD-2-Clause |
| MSRV | Rust 1.85+ (edition 2024) |

## Format coverage

| Category | Parsers | Coverage | Formats |
| --- | --- | --- | --- |
| Containers | 42 | 98% | MP4/MOV, MKV/WebM, AVI, MPEG-TS, MPEG-PS, WAV, AIFF, Ogg, FLV, MXF, +32 more |
| Audio | 56 | 100% | AAC/ADTS, MP3, AC-3/4, DTS/DTS-UHD, FLAC, Opus, Vorbis, TrueHD/Atmos, Dolby E, PCM, ADM, MGA, CELT, MPEG-H 3D, **IAMF/Eclipsa Audio**, **AC-4 IMS/JOC**, SMPTE ST 302/331/337, +37 more |
| Video | 28 | 100% | AVC, HEVC, VVC, AV1, VP8/VP9, MPEG-2, VC-1, VC-3/DNxHD, ProRes, FFV1, H.263, MPEG-4V, Theora, Y4M, Canopus HQ, CineForm, Dirac, HDR Vivid, **Dolby Vision RPU**, **HDR10+**, **SL-HDR1**, **HLG/PQ**, **CTA-861**, +8 more |
| Image | 19 | 100% | JPEG, PNG, GIF, BMP, TIFF, WebP, ICO, PSD, DPX, EXR, DDS, BPG, PCX, TGA, ArriRaw, Amiga Icon, RLE, AVIF Gain Map, HEIF |
| Text/Subtitles | 21 | 100% | SubRip, TTML, Timed Text, PGS, DVB Subtitle, Teletext, EIA-608/708, CDP, SCC, N19, PDF, SDP, PAC, DTvCC, SCTE-20, Kate, CMML, ARIB STD-B24/B37, WebVTT |
| Archives | 11 | 100% | ZIP, 7z, RAR, TAR, gzip, bzip2, ACE, ISO 9660, ELF, Mach-O, MZ/PE |
| Tags | 12 | 86% | ID3v1/v2, APE Tag, Vorbis Comment, Lyrics3, EXIF, XMP, ICC, IIM/IPTC, C2PA, PropertyList, SphericalVideo |
| Reader | 4 | 100% | File, Directory, HTTP, MMS |

Complete format catalog with spec references: **[docs/formats.md](docs/formats.md)**

Complete field coverage with sources: **[docs/field_coverage.md](docs/field_coverage.md)**

## Architecture

The crate stack, from consumers at the top down to the foundation:

```text
┌────────────────────────────────────────────────────────────┐
│  CLI (revelio)        C ABI (revelio-cdylib)                 │  frontends
├────────────────────────────────────────────────────────────┤
│  revelio-export       revelio-reader                         │  output / input
├────────────────────────────────────────────────────────────┤
│  revelio-dispatcher — parser table + parallel detect()       │  dispatch
├────────────────────────────────────────────────────────────┤
│  container · audio · video · image · text · tag · archive    │  parsers (193)
├────────────────────────────────────────────────────────────┤
│  revelio-core (FileAnalyze engine)     zenlib                │  foundation
└────────────────────────────────────────────────────────────┘
   revelio-diff — differential harness, diffs output vs the mediainfo oracle
```

## Crates

Fifteen workspace crates: a foundation layer, seven format-parser domains, the
output/API surface, and the differential test harness.

### Foundation

| Crate | Role | Status |
| --- | --- | --- |
| `zenlib` | ZenLib port — `Ztring`, bit reader, integer/float types | Stable |
| `revelio-core` | Analysis engine — `FileAnalyze` byte reader, stream model, demux/trace, config dispatch, computed fields | Stable |
| `revelio-dispatcher` | Parser table (single source of truth) + parallel `detect()` | Stable |

### Format parsers

| Crate | Role | Status |
| --- | --- | --- |
| `revelio-parsers-container` | 42 containers — MP4, MKV, AVI, MPEG-TS/PS, WAV, Ogg, MXF, … | Stable |
| `revelio-parsers-audio` | 56 audio codecs — AAC, MP3, AC-3/4, DTS, FLAC, Opus, TrueHD, … | Stable |
| `revelio-parsers-video` | 28 video codecs — AVC, HEVC, VVC, AV1, VP9, ProRes, … | Stable |
| `revelio-parsers-image` | 19 image formats — JPEG, PNG, TIFF, WebP, DPX, EXR, … | Stable |
| `revelio-parsers-text` | 21 subtitle/caption formats — SubRip, PGS, Teletext, EIA-608/708, … | Stable |
| `revelio-parsers-tag` | 12 tag formats — ID3v1/v2, EXIF, XMP, Vorbis Comment, … | Stable |
| `revelio-parsers-archive` | 11 archive/binary formats — ZIP, 7z, RAR, TAR, ELF, Mach-O, … | Stable |

### Output & API

| Crate | Role | Status |
| --- | --- | --- |
| `revelio-export` | 10 output formatters — XML, Text, JSON + EBUCore/MPEG-7/PBCore/NISO/FIMS/Graph/reVTMD | Stable |
| `revelio-reader` | Input layer — File, Directory, HTTP, MMS | Stable |
| `revelio-cli` | The `revelio` command-line binary | Stable |
| `revelio-cdylib` | C ABI (`MediaInfo_*`) — drop-in replacement for libmediainfo | Stable |

### Tooling

| Crate | Role | Status |
| --- | --- | --- |
| `revelio-diff` | Differential harness — diffs revelio's output against the `mediainfo` oracle | Stable |

## Installation

```sh
cargo install revelio-cli                   # CLI only (binary)
cargo add revelio-core                      # library (core + dispatcher)
cargo add revelio-export                    # library (output formatters)
```

Or add individual parser crates as needed:

```sh
cargo add revelio-parsers-container
cargo add revelio-parsers-audio
cargo add revelio-parsers-video
cargo add revelio-parsers-image
cargo add revelio-parsers-text
cargo add revelio-parsers-tag
cargo add revelio-parsers-archive
```

## Library usage

Probe a file from Rust — the full analysis engine is exposed through `revelio-core`:

```rust
use revelio_core::{FileAnalyze, FileLevelInfo, fill_file_level_fields};
use revelio_dispatcher::detect;

let bytes = std::fs::read("video.mp4")?;

// 1. Detect the format (parallel parser race)
let parser = detect(&bytes).expect("no parser matched");

// 2. Run the winner on a fresh state
let mut fa = FileAnalyze::new(&bytes);
parser(&mut fa);

// 3. Fill derived General-stream fields
fill_file_level_fields(&mut fa, &FileLevelInfo {
    file_size: bytes.len() as u64,
    extension: Some("mp4"),
    modified_unix_secs: None,
    local_offset_secs: 0,
});

// 4. Read back metadata
for stream in fa.streams() {
    println!("{:?}:", stream.kind());
    for (key, value) in stream.iter() {
        println!("  {key} = {value}");
    }
}
```

For XML output (byte-equal with mediainfo):

```rust
use revelio_export::to_xml;
let xml = to_xml(fa.streams(), "video.mp4", "26.05");
```

## Design

| Principle | Description |
| --- | --- |
| **Pure Rust** | No C++ FFI, no system DLLs, no `pkg-config` — a single `cargo build` |
| **Harness-validated** | Every ported parser is diffed against the `mediainfo` oracle for byte-equal XML |
| **Race + walk** | All parsers race in parallel to detect the format; the winner re-parses from a fresh state to extract every field |
| **Container-native** | Each container is walked through its *own* on-disk structure (boxes, EBML, RIFF chunks, PES packets) — not a unified abstraction |
| **No unsafe** | `#![deny(unsafe_code)]` enforced workspace-wide |
| **BSD-2-Clause** | Permissive license, no GPL restrictions |

## Output formats

- **XML** — byte-equal with the MediaInfoLib oracle
- **Text** — 42-column layout, duration as `X s Y ms`
- **JSON** — MediaInfo-compatible `{media:{@ref, track:[...]}}` structure
- **EBUCore, MPEG-7, PBCore, NISO, FIMS, Graph, reVTMD** — 7 domain formatters

## Quick start

```sh
# build the workspace
cargo build --release

# inspect a file (text / json / xml)
cargo run -p revelio-cli -- --text path/to/media.mp4
cargo run -p revelio-cli -- --json path/to/media.mp4
cargo run -p revelio-cli -- --xml  path/to/media.mp4

# build the C ABI shared library (libmediainfo drop-in)
cargo build -p revelio-cdylib --release

# diff revelio's output against the mediainfo oracle
cargo run -p revelio-diff -- path/to/file
```

## Status

All layers are implemented and validated:

| Layer | Status | Detail |
| --- | --- | --- |
| Format parsers | ✓ | 193 parsers across 8 domains |
| Output fields | ✓ | 185 fields, all gaps closed |
| Output formatters | ✓ | XML/Text/JSON + 7 domain |
| Reader layer | ✓ | File, Directory, HTTP, MMS |
| Core infra | ✓ | demux/trace/config dispatch, channel math, IBI, MIME, computed |
| Element trees | ✓ | RIFF, Ogg, MP4 box tree for trace/debug |
| Multi-file | ✓ | BDMV M2TS concatenation, SRT/SST sidecars, duplicate resolution |

**Verified:** all tests pass against the differential harness (byte-equal XML for
the format subset with oracle samples).

**Blocked — unverifiable against the oracle:** `File_Apv` (APV), `File_Av2`
(AV2), `File_Ancillary` (SMPTE 436 VANC), and `File_Umf` (Ikegami UMF) are
intentionally **not** ported. Correctness here is defined as output-match
against `mediainfo` v26.05, and none of these can produce a sample *and* oracle
output to diff against:

- **APV** — ffmpeg 8.x ships only a raw APV muxer, no encoder; mediainfo v26.05
  doesn't surface APV, so there's no oracle target.
- **AV2** — no encoder exists anywhere yet; not in the v26.05 oracle.
- **Ancillary** — only exists embedded in MXF (no standalone sample), and is a
  ~1000-line VANC/CDP/AFD parser.
- **Umf** — proprietary Ikegami format with no obtainable sample.

Porting these would mean translating ~3.7k lines of C++ blind, with no way to run
the differential harness — contrary to the project's harness-validated workflow.
They become tractable only with real sample files (and, for APV/AV2, a newer
mediainfo). DCP PKL, the one validatable holdout, is implemented (BYTE-EQUAL).
