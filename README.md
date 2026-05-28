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

### Deep codec analysis

- **AVC/H.264:** Full SPS VUI (colour primaries/transfer/matrix, aspect ratio, chroma sample location, video full range), EncoderInfo with name/version/settings extraction from x264/x265 SEI, GOP detection (M=X, N=Y)
- **HEVC/H.265:** Full SPS VUI, HDR10 mastering display colour volume SEI (primaries, white point, luminance), content light level SEI (MaxCLL/MaxFALL), x265 encoder string extraction
- **Dolby Vision:** dvcC/dvvC configuration box parsing in MP4, codec ID recognition in MKV, standalone XML metadata parser, HDR format profile/level extraction

### Output formatters

- XML (byte-equal with MediaInfoLib oracle)
- Text (42-column layout, duration as `X s Y ms`)
- JSON (MediaInfo-compatible `{media:{@ref, track:[...]}}` structure)
- EBUCore, MPEG-7, PBCore, NISO, FIMS, Graph, reVTMD (7 domain formatters)

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
