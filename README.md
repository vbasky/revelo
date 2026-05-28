# revelio

A Rust library and CLI for reading technical metadata from media files —
containers, audio codecs, video codecs, image formats, subtitle streams,
archive formats, and embedded tags.

Built as a port of MediaInfoLib, validated by differential testing against the
C++ `mediainfo` CLI.

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

### Deep codec analysis

- **AV1:** Profile-derived bit depth (0=8-bit, 1=10-bit, 2=12-bit) and chroma subsampling (0-1=4:2:0, 2=4:2:2). OBU sequence header parsing for frame dimensions, level from operating point. Container support via avcC-style config in MP4/WebM.
- **AVC/H.264:** Full SPS VUI (colour primaries/transfer/matrix, aspect ratio, chroma sample location, video full range), EncoderInfo with name/version/settings extraction from x264/x265 SEI, GOP detection (M=X, N=Y)
- **HEVC/H.265:** Full SPS VUI, HDR10 mastering display colour volume SEI (primaries, white point, luminance), content light level SEI (MaxCLL/MaxFALL), x265 encoder string extraction
- **Dolby Vision:** dvcC/dvvC configuration box parsing in MP4, codec ID recognition in MKV, standalone XML metadata parser, HDR format profile/level extraction

### Computed fields (post-parse)
- **Bits_Pixel_Frame** — computed from BitRate / (Width × Height × FrameRate)
- **Compression_Ratio** — computed from uncompressed size / StreamSize
- **FrameRate_Mode_Original** — preserved from original frame rate mode
- **BufferSize** — filled from MPEG-4 esds DecoderConfigDescriptor
- **ReplayGain** — extracted from LAME Gaia + ID3v2 RVA2/TXXX frames
- **BitRate_Maximum / BitRate_Minimum / OverallBitRate_Maximum / OverallBitRate_Minimum** — from container hints + post-parse aggregation
- **Format_Profile** (General) — "Base Media / Version 2" labels from ftyp

### Output formatters

- XML (byte-equal with MediaInfoLib oracle)
- Text (42-column layout, duration as `X s Y ms`)
- JSON (MediaInfo-compatible `{media:{@ref, track:[...]}}` structure)
- EBUCore, MPEG-7, PBCore, NISO, FIMS, Graph, reVTMD (7 domain formatters)

### C ABI + Reader + Core

- `revelio-cdylib`: `MediaInfo_New/Open/Close/Inform/Get/Count_Get/Option` entry points
- `revelio-reader`: File, Directory, HTTP, MMS reader layer
- `revelio-core`: SMPTE timecode parser (DF/NDF, ms conversion), demux/event framework (4-level bitmask, ContentType enum, DemuxState), trace system (Tree/CSV/XML/MicroXml renderers), channel splitting (SMPTE ST 337 AES3 deinterleaving), channel grouping (mono→stereo interleaving), IBI seek table builder, MIME type mapping (40+ container+codec entries), field/interlacement tracker (TFF/BFF/Progressive/PsF), reference file tracker (multi-file BDMV/SMPTE packages), computed_fields (post-parse Bits_Pixel_Frame, Compression_Ratio, FrameRate_Mode_Original, BitRate ranges, Format_Profile), replay_gain (LAME Gaia + ID3v2 RVA2/TXXX extraction)
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

**Audio (remainder):** None — all parsers + infrastructure complete

**Text (remainder):** None — all formats covered

**Container infrastructure:** RIFF/Ogg element tree depth wiring into parsers

**Core:** Config option dispatch wiring (Config->Demux/Trace level activation), multi-file concatenation pipeline, duplicate/reference resolution in output
