# revelio

A Rust library and CLI for reading technical metadata from media files —
containers, audio codecs, video codecs, image formats, subtitle streams,
archive formats, and embedded tags.

Built as a port of MediaInfoLib, validated by differential testing against the
C++ `mediainfo` CLI.

## Status

**147 parsers** registered across 6 domains, **496 tests** passing.

| Category | Formats | Coverage | Highlights |
|---|---|---|---|
| Containers | 42 | 98% | MP4/MOV, MKV/WebM, AVI, MPEG-PS/TS, Ogg, WAV, MXF, FLV, SWF, +32 more |
| Audio | 36 | 62% | AAC, AC-3/4, DTS/DTS-UHD, FLAC, MP3, Opus, Vorbis, USAC, +26 more |
| Video | 16 | 52% | AVC, HEVC (VUI + HDR10 SEI), AV1, VC-1, MPEG-2, VVC, ProRes, VC-3, +7 more |
| Image | 18 | 100% | JPEG (EXIF), PNG, TIFF, BMP, GIF, WebP, DDS, DPX, EXR, PSD, +8 more |
| Text/Subtitles | 15 | 75% | SubRip, TTML, PGS, EIA-608/708, DVB-Subtitle, Teletext, SCC, +7 more |
| **Archives** | 11 | 100% | ZIP, RAR, 7z, TAR, Gzip, Bzip2, ISO 9660, ELF, Mach-O, MZ/PE, ACE |
| **Tags** | 12 | 86% | ID3v1, ID3v2, APE, VorbisComment, Lyrics3, EXIF, XMP, ICC, C2PA, IIM, PropertyList, SphericalVideo |

### Deep codec analysis

- **AVC/H.264:** Full SPS VUI (colour primaries/transfer/matrix, aspect ratio, chroma
  sample location, video full range), EncoderInfo with name/version/settings
  extraction from x264/x265 SEI, GOP detection (`M=X, N=Y`)
- **HEVC/H.265:** Full SPS VUI, HDR10 mastering display colour volume SEI
  (primaries, white point, luminance), content light level SEI (MaxCLL/MaxFALL),
  x265 encoder string extraction
- **Dolby Vision:** dvcC/dvvC configuration box parsing in MP4, codec ID
  recognition in MKV, standalone XML metadata parser, HDR format profile/level
  extraction

### Output formatters

- XML (byte-equal with MediaInfoLib oracle)
- Text (42-column layout, duration as `X s Y ms`)
- JSON (MediaInfo-compatible `{media:{@ref, track:[...]}}` structure)

### C ABI

`revelio-cdylib` exposes `MediaInfo_New/Open/Close/Inform/Get/Count_Get/Option`
entry points for drop-in replacement of libmediainfo.

## Building

```sh
cargo build --release        # all crates including cdylib
cargo run -p revelio-cli -- --text /path/to/media.mp4
```

## Running

```sh
# differential test harness (requires mediainfo CLI installed)
cargo run -p diff-harness -- /path/to/media-file.mp4

# standalone CLI (default: text output)
cargo run -p revelio-cli -- --text /path/to/media.mp4
cargo run -p revelio-cli -- --json /path/to/media.mp4
cargo run -p revelio-cli -- /path/to/media.mp4  # XML output

# build the C shared library
cargo build -p revelio-cdylib --release
# output: target/release/librevelio_cdylib.dylib (or .so/.dll)
```
