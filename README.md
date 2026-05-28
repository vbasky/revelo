# revelio

A Rust library and CLI for reading technical metadata from media files —
containers (MP4, MKV, MPEG-TS, AVI, …), audio codecs, video codecs, image
formats, and subtitle streams.

Built as a port of MediaInfoLib, validated by differential testing against the
C++ `mediainfo` CLI.

## Status

Summary (format parsers implemented per category):

| Category | Implemented | Crate |
|---|---|---|
| Containers | 42 | `revelio-parsers-container` |
| Video codecs | 16 | `revelio-parsers-video` |
| Audio codecs | 36 | `revelio-parsers-audio` |
| Image | 18 | `revelio-parsers-image` |
| Text / Subtitles | 15 | `revelio-parsers-text` |
| Tag / Metadata | 12 | `revelio-parsers-tag` |
| Archives | 11 | `revelio-parsers-archive` |

Output formatters: XML ✅, JSON ✅, Text ✅; EBUCore / MPEG-7 / HTML ⬜.
C ABI shim (`revelio-cdylib`) 🚧. Reader layer ⬜.

**Legend:** ✅ implemented & wired into CLI/harness dispatch · 🚧 implemented
in-crate, not yet wired into dispatch · ⬜ planned.

### Containers

| Format | Status | Format | Status |
|---|---|---|---|
| MP4 / MOV | ✅ | MPEG-TS | ✅ |
| Matroska / WebM | ✅ | MPEG-PS | ✅ |
| AVI | ✅ | WAV | ✅ |
| Ogg | ✅ | AIFF | ✅ |
| FLV | ✅ | MXF | ✅ |
| ASF / WM | ✅ | RealMedia | ✅ |
| WTV | ✅ | NUT | ✅ |
| FLAC (native) | ✅ | CAF | ✅ |
| DV (DIF) | ✅ | GXF | ✅ |
| LXF | ✅ | IVF | ✅ |
| BDMV | ✅ | DVD-Video (IFO) | ✅ |
| CDXA | ✅ | AMV | ✅ |
| SWF | ✅ | DPG | ✅ |
| NSV | ✅ | PMP | ✅ |
| AAF | ✅ | P2 Clip | ✅ |
| XDCAM Clip | ✅ | SKM | ✅ |
| Ptx (Pro Tools) | ✅ | Ibi (MediaInfo index) | ✅ |
| HLS (m3u8) | ✅ | DASH MPD | ✅ |
| HDS (F4M) | ✅ | Smooth Streaming (ISM) | ✅ |
| DCP AssetMap | ✅ | DCP CPL / IMF CPL | ✅ |
| DXW | ✅ | MediaInfo XML | ✅ |
| SequenceInfo | ✅ | VBI | ✅ |

### Video codecs

| Format | Status | Format | Status |
|---|---|---|---|
| AVC / H.264 | ✅ | HEVC / H.265 | ✅ |
| VVC / H.266 | ✅ | AV1 | ✅ |
| VP8 | ✅ | VP9 | ✅ |
| MPEG-2 Video | ✅ | MPEG-4 Visual | ✅ |
| VC-1 | ✅ | VC-3 / DNxHD | ✅ |
| FFV1 | ✅ | ProRes | ✅ |
| H.263 | ✅ | Theora | ✅ |
| Dolby Vision (config) | ✅ | Y4M | ✅ |

### Audio codecs

| Format | Status | Format | Status |
|---|---|---|---|
| AAC (ADTS) | ✅ | MP3 / MPEG Audio | ✅ |
| AC-3 | ✅ | E-AC-3 / AC-4 | ✅ |
| DTS | ✅ | DTS-UHD | ✅ |
| FLAC | ✅ | Opus | ✅ |
| Vorbis | ✅ | Speex | ✅ |
| ALAC / CAF | ✅ | AU / SND | ✅ |
| Monkey's Audio (APE) | ✅ | WavPack | ✅ |
| TAK | ✅ | TrueAudio (TTA) | ✅ |
| Musepack (MPC) | ✅ | LA | ✅ |
| RKAU | ✅ | OpenMG / ATRAC | ✅ |
| ADPCM | ✅ | aptX-100 | ✅ |
| TwinVQ | ✅ | USAC | ✅ |
| IAB | ✅ | IAMF | ✅ |
| ALS | ✅ | AMR | ✅ |
| DSF | ✅ | DSDIFF | ✅ |
| MIDI | ✅ | DAT | ✅ |
| Module (MOD) | ✅ | Extended Module (XM) | ✅ |
| Impulse Tracker (IT) | ✅ | ScreamTracker 3 (S3M) | ✅ |

### Image

| Format | Status | Format | Status |
|---|---|---|---|
| JPEG | ✅ | PNG | ✅ |
| GIF | ✅ | BMP | ✅ |
| TIFF | ✅ | WebP | ✅ |
| ICO | ✅ | PSD | ✅ |
| DPX | ✅ | EXR | ✅ |
| DDS | ✅ | BPG | ✅ |
| PCX | ✅ | TGA | ✅ |
| ArriRaw | ✅ | Amiga Icon | ✅ |
| RLE (Utah) | ✅ | AVIF Gain Map | ✅ |

### Text / Subtitles

| Format | Status | Format | Status |
|---|---|---|---|
| SubRip (SRT) | ✅ | TTML | ✅ |
| Timed Text | ✅ | PGS | ✅ |
| DVB Subtitle | ✅ | Teletext | ✅ |
| EIA-608 | ✅ | EIA-708 | ✅ |
| CDP | ✅ | SCC | ✅ |
| N19 / EBU STL | ✅ | Kate | ✅ |
| CMML | ✅ | ARIB STD-B24/B37 | ✅ |
| Other (SAMI/SSA/ASS/…) | ✅ | | |

### Tag / Metadata

| Format | Status | Format | Status |
|---|---|---|---|
| ID3v1 | ✅ | ID3v2 | ✅ |
| APE Tag | ✅ | Vorbis Comment | ✅ |
| Lyrics3 | ✅ | EXIF | ✅ |
| XMP | ✅ | ICC Profile | ✅ |
| IIM / IPTC | ✅ | C2PA | ✅ |
| PropertyList (plist) | ✅ | Spherical Video | ✅ |

### Archives

| Format | Status | Format | Status |
|---|---|---|---|
| ZIP | 🚧 | 7z | 🚧 |
| RAR | 🚧 | TAR | 🚧 |
| gzip | 🚧 | bzip2 | 🚧 |
| ACE | 🚧 | ISO 9660 | 🚧 |
| ELF | 🚧 | Mach-O | 🚧 |
| MZ / PE (EXE) | 🚧 | | |

> Archive parsers live in `revelio-parsers-archive` but are not yet wired
> into the CLI/harness dispatch (🚧). Other categories are dispatched and
> validated against the C++ `mediainfo` oracle via the diff harness.
>
> **Keep this list in sync:** when a parser is added, append it with ✅ (or
> 🚧 until it's wired into `revelio-cli`/`diff-harness`); when one is
> removed, drop its row.

## Not yet implemented

Tracked against MediaInfoLib (~208 format parsers + the full output/ABI
surface). ⬜ = planned, not started.

### Formats

| Category | Missing |
|---|---|
| Video codecs | ⬜ AVS / AVS3, ⬜ Dirac, ⬜ Canopus HQ, ⬜ CineForm, ⬜ Apple Intermediate (AIC), ⬜ APV, ⬜ ProRes RAW |
| Audio codecs | ⬜ MLP / Dolby TrueHD, ⬜ Dolby E, ⬜ CELT, ⬜ ADM / Dolby Atmos metadata, ⬜ Dolby Audio metadata |
| Image | ⬜ HEIF / HEIC, ⬜ JPEG 2000, ⬜ JPEG XL |
| Text / Subtitles | ⬜ WebVTT, ⬜ SCTE-35, ⬜ DolbyVision RPU sidecar |
| Containers | ⬜ misc long-tail wrappers; ⬜ deeper MXF essence/partition parsing |

### Cross-stream / reference handling

- ⬜ Reference-file resolution — BDMV folders, split MXF essence, DCP
  CPL → asset graph, IMF (`ReferenceFilesHelper` in C++).
- ⬜ Duplicate / multi-file aggregation.

### Engine core

- ⬜ Trace output (`--Details` / `mediatrace`).
- ⬜ Event / callback system (Demux, SliceInfo, parser-selected events).
- 🚧 C ABI shim (`revelio-cdylib`) — basic `MediaInfo_*` entry points only;
  ⬜ `MediaInfoList`, full `Get()` parameter coverage, option handling.
- ⬜ Streaming / partial-read reader layer (currently whole-file in memory).

### Output

- ⬜ HTML, ⬜ EBUCore, ⬜ MPEG-7, ⬜ PBCore, ⬜ Graphviz.
- ⬜ Customizable `Inform` template + `--Output=Field;…` selectors.

### Depth gaps in implemented parsers

- AV1 OBU sequence-header dimensions still mis-parse (container
  PixelWidth/Height used as the authoritative fallback).
- HEVC SPS VUI colour is heuristic (not a full VUI bit-walk).
- HDR / Dolby Vision (`dvvC`/`dvcC`) profile/level surfacing.
- x264/x265 `Encoded_Library_Settings` transcription.

## Building

```sh
cargo build --release
```

## Running

```sh
# differential test harness against installed mediainfo CLI
cargo run -p diff-harness -- /path/to/media-file.mp4

# standalone CLI
cargo run -p revelio-cli -- --json /path/to/media-file.mp4
```
