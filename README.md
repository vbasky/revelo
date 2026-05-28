# revelio

A Rust library and CLI for reading technical metadata from media files —
containers, audio codecs, video codecs, image formats, subtitle streams,
archive formats, and embedded tags.

Built as a port of MediaInfoLib, validated by differential testing against the
C++ `mediainfo` CLI.

## Status

**155 parsers** registered across 7 domains, **515 tests** passing.

| Category | Count | Coverage | Formats |
|---|---|---|---|
| Containers | 42 | 98% | MP4/MOV, MKV/WebM, AVI, MPEG-TS, MPEG-PS, WAV, AIFF, Ogg, FLV, MXF, ASF/WM, RealMedia, WTV, NUT, DV-DIF, GXF, LXF, IVF, BDMV, DVD-Video, CDXA, AMV, SWF, DPG, NSV, PMP, AAF, P2-Clip, XDCAM-Clip, SKM, Ptx, Ibi, HLS, DASH-MPD, HDS-F4M, ISM, DCP-AM, DCP-CPL, DXW, MediaInfo-XML, SequenceInfo, VBI |
| Audio | 44 | 76% | AAC, MP3, AC-3, AC-4, DTS, DTS-UHD, TrueHD, Dolby E, FLAC, Opus, Vorbis, Speex, ALAC/CAF, AU, PCM, APE, WavPack, TAK, TTA, Musepack, LA, RKAU, OpenMG, ADPCM, aptX-100, TwinVQ, USAC, MPEG-H 3D, CELT, IAB, IAMF, ALS, AMR, DSF, DSDIFF, SMPTE ST 302, SMPTE ST 331, SMPTE ST 337, MIDI, DAT, Module/MOD, XM, Impulse Tracker, ScreamTracker3 |
| Video | 16 | 52% | AVC, HEVC, VVC, AV1, VP8, VP9, MPEG-2, MPEG-4 Visual, VC-1, VC-3/DNxHD, FFV1, ProRes, H.263, Theora, Dolby Vision, Y4M |
| Image | 19 | 100% | JPEG, PNG, GIF, BMP, TIFF, WebP, ICO, PSD, DPX, EXR, DDS, BPG, PCX, TGA, ArriRaw, Amiga Icon, RLE, AVIF Gain Map, HEIF |
| Text/Subtitles | 16 | 80% | SubRip, TTML, Timed Text, PGS, DVB Subtitle, Teletext, EIA-608, EIA-708, CDP, SCC, N19/EBU-STL, Kate, CMML, ARIB STD-B24/B37, OtherText, WebVTT |
| **Archives** | 11 | 100% | ZIP, 7z, RAR, TAR, gzip, bzip2, ACE, ISO 9660, ELF, Mach-O, MZ/PE |
| **Tags** | 12 | 86% | ID3v1, ID3v2, APE Tag, Vorbis Comment, Lyrics3, EXIF, XMP, ICC, IIM/IPTC, C2PA, PropertyList, Spherical Video |

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
