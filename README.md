# revelio

A Rust library and CLI for reading technical metadata from media files —
containers, audio codecs, video codecs, image formats, subtitle streams,
archive formats, and embedded tags.

Built as a port of MediaInfoLib, validated by differential testing against the
C++ `mediainfo` CLI.

## Status

**174 parsers** registered across 8 domains, **530 tests** passing.

| Category | Parsers | Coverage | Formats |
|---|---|---|---|
| Containers | 42 | 98% | MP4/MOV, MKV/WebM, AVI, MPEG-TS, MPEG-PS, WAV, AIFF, Ogg, FLV, MXF, +32 more |
| Audio | 50 | 86% | AAC, MP3, AC-3/4, DTS/DTS-UHD, FLAC, Opus, Vorbis, TrueHD, Dolby E, PCM, CELT, MPEG-H 3D, SMPTE ST 302/331/337, +35 more |
| Video | 28 | 90% | AVC, HEVC, VVC, AV1, VP8/VP9, MPEG-2, VC-1, VC-3/DNxHD, ProRes, FFV1, H.263, MPEG-4V, Theora, Y4M, Canopus HQ, CineForm, Fraps, FLIC, HuffYUV, Lagarith, AVS/AVS3, Dirac, HDR Vivid, Dolby Vision, AIC, AFD/Bar |
| Image | 19 | 100% | JPEG, PNG, GIF, BMP, TIFF, WebP, ICO, PSD, DPX, EXR, DDS, BPG, PCX, TGA, ArriRaw, Amiga Icon, RLE, AVIF Gain Map, HEIF |
| Text/Subtitles | 16 | 80% | SubRip, TTML, Timed Text, PGS, DVB Subtitle, Teletext, EIA-608/708, CDP, SCC, N19, Kate, CMML, ARIB STD-B24/B37, OtherText, WebVTT |
| Archives | 11 | 100% | ZIP, 7z, RAR, TAR, gzip, bzip2, ACE, ISO 9660, ELF, Mach-O, MZ/PE |
| Tags | 12 | 86% | ID3v1/v2, APE Tag, Vorbis Comment, Lyrics3, EXIF, XMP, ICC, IIM/IPTC, C2PA, PropertyList, SphericalVideo |
| Reader | 4 | 100% | File, Directory, HTTP, MMS |


Complete format catalog with spec references: **[docs/formats.md](docs/formats.md)**

### Deep codec analysis

- **AV1:** Profile-derived bit depth (0=8-bit, 1=10-bit, 2=12-bit) and chroma subsampling (0-1=4:2:0, 2=4:2:2). OBU sequence header parsing for frame dimensions, level from operating point. Container support via avcC-style config in MP4/WebM.
- **AVC/H.264:** Full SPS VUI (colour primaries/transfer/matrix, aspect ratio, chroma sample location, video full range), EncoderInfo with name/version/settings extraction from x264/x265 SEI, GOP detection (M=X, N=Y)
- **HEVC/H.265:** Full SPS VUI, HDR10 mastering display colour volume SEI (primaries, white point, luminance), content light level SEI (MaxCLL/MaxFALL), x265 encoder string extraction
- **Dolby Vision:** dvcC/dvvC configuration box parsing in MP4, codec ID recognition in MKV, standalone XML metadata parser, HDR format profile/level extraction

### Output formatters

- XML (byte-equal with MediaInfoLib oracle)
- Text (42-column layout, duration as `X s Y ms`)
- JSON (MediaInfo-compatible `{media:{@ref, track:[...]}}` structure)
- EBUCore, MPEG-7, PBCore, NISO, FIMS, Graph, reVTMD (7 domain formatters)

## Output Parity

Field-level coverage per stream kind. Tracks gaps between revelio output and
MediaInfoLib's `mediainfo` CLI (both XML and text). Percentages are rough
estimates — each kind has ~60-120 possible fields and the set varies by format.

| Kind | Fields | Coverage | Known Gaps |
|------|--------|----------|-------------|
| General | ~80 | 85% | `Format_Profile` (Base Media vs Version 2), `Encoded_Library_Name`, `Encoded_Library_Version`, `Encoded_Library_Settings` (not populated in all parser paths) |
| Video | ~70 | 78% | `Encoded_Library`/`_Name`/`_Version`/`_Settings` missing from text display and AVC-in-MP4 path; `Bits_Pixel_Frame`, `FrameRate_Mode_Original`, `BufferSize`, `BitRate_Maximum` (Video) never filled |
| Audio | ~60 | 82% | `Compression_Ratio`, `ReplayGain_*` never filled; sampling count / frame count off by 1 in some MP4 files; `BufferSize` never filled |
| Text | ~30 | 90% | Minor — most subtitle field coverage complete |
| Image | ~25 | 85% | ICC profile parse → `ICC_*` fields exposed in XML but not in text display |
| Other/Menu | ~25 | 70% | Chapter names/durations, timecode metadata not exposed in text display |

### Field gaps by priority

**Parser never fills the field:**
- `Bits_Pixel_Frame` — computed from bitrate/width/height/framerate
- `Compression_Ratio` — computed from stream size / uncompressed size
- `FrameRate_Mode_Original` — original frame rate mode before CFR override
- `BufferSize` — audio codec buffer size (from esds/mp4a)
- `ReplayGain_Gain` / `ReplayGain_Peak` — from audio tag parsers
- `BitRate_Maximum` / `BitRate_Minimum` / `OverallBitRate_Maximum` — from container hints

**Parser fills but text display_fields omits:**
- `Format_Profile` (General) — "Base media / Version 2" label
- `Encoded_Library` (Video) — "Writing library" label
- `Encoded_Date` (Video) — "Encoded date"
- `Tagged_Date` (Video) — "Tagged date"
- `Format_Settings` (combined, Video) — "CABAC / 5 Ref Frames" summary

**Parser fills only partially:**
- `Encoded_Library_Name`/`_Version`/`_Settings` for HEVC — `extract_encoder_from_sei_nalus` returns all three but only `.library` is stored
- `Format_Profile` for MP4 General — always emits "Base Media", missing " / Version 2" suffix from ftyp minor version
- AVC-in-MP4 `Encoded_Library` — x264 SEI extracted only in Annex-B path, not in `avcC` path

**Numeric precision (off by ≤1 unit):**
- SamplingCount off by 16 (1465280 vs 1465296) — MP4 sample table vs audio frame count
- FrameCount off by 1 (1430 vs 1431) — same root cause
- Duration off by −1 ms (30.526 vs 30.527) — derived from frame count × frame duration rounding
- Opus preskip delay: 6 ms vs 7 ms — `312/48000` rounding at `{:.3}`

**Text renderers missing:**
- `Bits_Pixel_Frame` — "Bits/(Pixel*Frame)" label, humanised to 3 decimal places
- `Compression_Ratio` — "Compression ratio" label, percentage string
- `Format_Settings` (combined) — joins CABAC/RefFrames/GOP into a single line
- `BufferSize` — "Buffer size" label

### C ABI + Reader + Core

- `revelio-cdylib`: `MediaInfo_New/Open/Close/Inform/Get/Count_Get/Option` entry points
- `revelio-reader`: File, Directory, HTTP, MMS reader layer
- `revelio-core`: SMPTE timecode parser (DF/NDF, milliseconds conversion)
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

## Pending

**Audio (remainder):** ADM, Dolby Audio Metadata, PcmVob, PcmM2ts, MGA, MPEG-4 AAC full, ChannelSplitting/Grouping depth

**Text (remainder):** PDF, SDP, PAC, DTvCC Transport, SCTE-20

**Container infrastructure:** RIFF elements helper, Ogg sub-elements, MPEG-4 descriptors, PSI table depth, IBI creation, reference files

**Core:** MediaInfo_Config field ordering, trace/demux events, multi-file support, MIME type detection, duplicate/reference parsing
