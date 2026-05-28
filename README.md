# revelio

A Rust library and CLI for reading technical metadata from media files —
containers, audio codecs, video codecs, image formats, subtitle streams,
archive formats, and embedded tags.

Built as a port of MediaInfoLib, validated by differential testing against the
C++ `mediainfo` CLI.

```
┌──────────────────────────────────────────────────────────────────────┐
│                         R E V E L I O                               │
│              The Rust MediaInfo — Read What You See                  │
├──────────────────────────────────────────────────────────────────────┤
│                                                                      │
│   ┌──────────┐   ┌──────────┐   ┌──────────┐   ┌──────────┐        │
│   │ revelio  │   │ revelio  │   │  diff-   │   │  revelio │        │
│   │   -cli   │   │ -cdylib  │   │ harness  │   │ -reader  │        │
│   │          │   │          │   │          │   │          │        │
│   │ --text   │   │ New/Open │   │  oracle  │   │  File    │        │
│   │ --xml    │   │ Get/Info │   │  diff    │   │  Dir     │        │
│   │ --json   │   │ Option   │   │          │   │  HTTP    │        │
│   └────┬─────┘   └────┬─────┘   └────┬─────┘   │  MMS     │        │
│        │              │              │          └──────────┘        │
│   ┌────┴──────────────┴──────────────┴────────────────────────┐     │
│   │                     revelio-export                         │     │
│   │   XML │ Text │ JSON │ EBUCore │ MPEG-7 │ PBCore │ ...      │     │
│   └───────────────────────────┬───────────────────────────────┘     │
│                               │                                     │
│   ┌───────────────────────────┴───────────────────────────────┐     │
│   │                     revelio-core                           │     │
│   │  ┌──────────┐ ┌─────────┐ ┌────────────┐ ┌─────────────┐ │     │
│   │  │FileAnalyze│ │ Events  │ │computed_fld│ │channel_split│ │     │
│   │  │ +Config   │ │ Demux   │ │ BPP/CR/RG  │ │  + group    │ │     │
│   │  │ +Element  │ │ 4-level │ │ fmt_prof   │ │  ST 337     │ │     │
│   │  │  Tree     │ │ Trace   │ │ interlace  │ │  interlace  │ │     │
│   │  └─────┬─────┘ └────┬────┘ └─────┬──────┘ └──────┬──────┘ │     │
│   │        │            │            │               │         │     │
│   │  Timecode  │      MIME      │     IBI       │ Reference   │     │
│   │  DF/NDF    │   container→   │  frame seek   │  Tracker    │     │
│   │  ms conv   │   codec mapper │  table        │  multi-file │     │
│   └────────────┴────────────────┴───────────────┴─────────────┘     │
│                                                                      │
│   ┌───────────────────────────────────────────────────────────┐     │
│   │                  parsers (185 across 8 domains)            │     │
│   │  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────────┐ │     │
│   │  │container │ │  audio   │ │  video   │ │    image     │ │     │
│   │  │  MP4 MKV │ │ AAC MP3  │ │AVC HEVC  │ │ JPEG PNG GIF │ │     │
│   │  │  TS  Ogg │ │FLAC Opus │ │AV1 VVC   │ │ TIFF BMP PSD │ │     │
│   │  │ AVI  WAV │ │AC3  DTS  │ │VP9 ProRes│ │ 19 parsers   │ │     │
│   │  │ 42 total │ │DH TrueHD │ │28 parsers│ │              │ │     │
│   │  │          │ │56 parsers│ │          │ │              │ │     │
│   │  └──────────┘ └──────────┘ └──────────┘ └──────────────┘ │     │
│   │  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────────┐ │     │
│   │  │  text    │ │ archive  │ │   tag    │ │   reader     │ │     │
│   │  │SRT  PGS  │ │ZIP  RAR  │ │ ID3 EXIF │ │ File  Dir    │ │     │
│   │  │TTML 708  │ │TAR  7z   │ │ XMP ICC  │ │ HTTP  MMS    │ │     │
│   │  │PDF VTT   │ │GZ ELF PE │ │ APE Vorb │ │  4 parsers   │ │     │
│   │  │21 parsers│ │11 parsers│ │12 parsers│ │              │ │     │
│   │  └──────────┘ └──────────┘ └──────────┘ └──────────────┘ │     │
│   └───────────────────────────────────────────────────────────┘     │
│                                                                      │
│   zenlib (Ztring + bitstream helpers)                                │
│                                                                      │
│   576 tests  •  185 parsers  •  byte-equal XML against mediainfo     │
└──────────────────────────────────────────────────────────────────────┘
```

## Status

**185 parsers** registered across 8 domains, **571 tests** passing.

| Category | Parsers | Coverage | Formats |
|---|---|---|---|
| Containers | 42 | 98% | MP4/MOV, MKV/WebM, AVI, MPEG-TS, MPEG-PS, WAV, AIFF, Ogg, FLV, MXF, +32 more |
| Audio | 56 | 91% | AAC/ADTS, MP3, AC-3/4, DTS/DTS-UHD, FLAC, Opus, Vorbis, TrueHD, Dolby E, PCM, ADM, DolbyAudioMetadata, PcmVob, PcmM2ts, MGA, CELT, MPEG-H 3D, SMPTE ST 302/331/337, +35 more |
| Video | 28 | 90% | AVC, HEVC, VVC, AV1, VP8/VP9, MPEG-2, VC-1, VC-3/DNxHD, ProRes, FFV1, H.263, MPEG-4V, Theora, Y4M, Canopus HQ, CineForm, Fraps, FLIC, HuffYUV, Lagarith, AVS/AVS3, Dirac, HDR Vivid, Dolby Vision, AIC, AFD/Bar |
| Image | 19 | 100% | JPEG, PNG, GIF, BMP, TIFF, WebP, ICO, PSD, DPX, EXR, DDS, BPG, PCX, TGA, ArriRaw, Amiga Icon, RLE, AVIF Gain Map, HEIF |
| Text/Subtitles | 21 | 90% | SubRip, TTML, Timed Text, PGS, DVB Subtitle, Teletext, EIA-608/708, CDP, SCC, N19, PDF, SDP, PAC, DTvCC Transport, SCTE-20, Kate, CMML, ARIB STD-B24/B37, OtherText, WebVTT |
| Archives | 11 | 100% | ZIP, 7z, RAR, TAR, gzip, bzip2, ACE, ISO 9660, ELF, Mach-O, MZ/PE |
| Tags | 12 | 86% | ID3v1/v2, APE Tag, Vorbis Comment, Lyrics3, EXIF, XMP, ICC, IIM/IPTC, C2PA, PropertyList, SphericalVideo |
| Reader | 4 | 100% | File, Directory, HTTP, MMS |


Complete format catalog with spec references: **[docs/formats.md](docs/formats.md)**

Complete field coverage with sources: **[docs/field_coverage.md](docs/field_coverage.md)**

### Output formatters

- XML (byte-equal with MediaInfoLib oracle)
- Text (42-column layout, duration as `X s Y ms`)
- JSON (MediaInfo-compatible `{media:{@ref, track:[...]}}` structure)
- EBUCore, MPEG-7, PBCore, NISO, FIMS, Graph, reVTMD (7 domain formatters)

### C ABI + Reader + Core

- `revelio-cdylib`: `MediaInfo_New/Open/Close/Inform/Get/Count_Get/Option` entry points
- `revelio-reader`: File, Directory, HTTP, MMS reader layer
- `revelio-core`: full infrastructure layer — see [field_coverage.md](docs/field_coverage.md) for the 185-field catalog
- `revelio-export`: 10 output formatters total

## Building

```sh
cargo build --release
cargo run -p revelio-cli -- --text /path/to/media.mp4
```

## Running

```sh
cargo run -p diff-harness -- /path/to/file
cargo run -p revelio-cli -- --text /path/to/file
cargo run -p revelio-cli -- --json /path/to/file
cargo build -p revelio-cdylib --release
```

## Feature Complete

All layers are implemented and validated:

| Layer | Status | Detail |
|---|---|---|
| Format parsers | ✓ | 185 parsers across 8 domains |
| Output fields | ✓ | 185 fields, all gaps closed |
| Output formatters | ✓ | XML/Text/JSON + 7 domain |
| Reader layer | ✓ | File, Directory, HTTP, MMS |
| Core infra | ✓ | demux/trace/config dispatch, channel math, IBI, MIME, computed |
| Element trees | ✓ | RIFF, Ogg, MP4 box tree for trace/debug |
| Multi-file | ✓ | BDMV M2TS concatenation, SRT/SST sidecars, duplicate resolution |

**Verified:** All tests pass against the differential harness (byte-equal XML for
the format subset with oracle samples).

**Blocked — unverifiable against the oracle:** `File_Apv` (APV), `File_Av2`
(AV2), `File_Ancillary` (SMPTE 436 VANC), and `File_Umf` (Ikegami UMF) are
intentionally **not** ported. Correctness here is defined as output-match
against `mediainfo` v26.05, and none of these can produce a sample *and*
oracle output to diff against:

- **APV** — ffmpeg 8.x ships only a raw APV muxer, no encoder; mediainfo
  v26.05 doesn't surface APV, so there's no oracle target.
- **AV2** — no encoder exists anywhere yet; not in the v26.05 oracle.
- **Ancillary** — only exists embedded in MXF (no standalone sample), and
  is a ~1000-line VANC/CDP/AFD parser.
- **Umf** — proprietary Ikegami format with no obtainable sample.

Porting these would mean translating ~3.7k lines of C++ blind, with no way
to run the differential harness — contrary to the project's
harness-validated workflow. They become tractable only with real sample
files (and, for APV/AV2, a newer mediainfo). DCP PKL, the one validatable
holdout, is implemented (BYTE-EQUAL).
