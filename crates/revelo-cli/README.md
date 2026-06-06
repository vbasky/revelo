# revelo-cli

![revelo — read technical metadata from any media file, in pure Rust](https://raw.githubusercontent.com/vbasky/revelo/main/docs/banner.png)

A fast, safe, pure-Rust command-line tool for extracting technical and tag
metadata from media files — a from-scratch Rust port of
[MediaInfoLib](https://mediaarea.net/en/MediaInfo) with optional
[ExifTool](https://exiftool.org/)-grade EXIF/maker-note depth.

No `./Configure`, no system libraries, no Perl runtime. Installs with one
`cargo install` and runs anywhere a Rust toolchain runs.

Part of the [**revelo**](https://github.com/vbasky/revelo) project — see the
[project README](https://github.com/vbasky/revelo#readme) for the full picture.

## Install

```sh
cargo install revelo-cli
```

The binary is named `revelo`.

For ExifTool-grade maker-note depth across 14 camera vendors, opt into the
`exiftool-tables` feature:

```sh
cargo install revelo-cli --features exiftool-tables
```

> **License note:** the default build is **BSD-2-Clause**. The
> `exiftool-tables` feature pulls in tables derived from ExifTool, so a binary
> built with it is subject to **GPL/Artistic** terms. See
> [Licensing](#license) below.

## Quick start

```sh
# Human-readable text report (default)
revelo video.mp4

# MediaInfoLib-compatible XML
revelo --xml video.mp4

# JSON, including EXIF / maker-note tags
revelo photo.jpg --json

# One-line aggregate summary
revelo --summary movie.mkv
```

Running `revelo` with no path prints the help banner.

## Output formats

| Flag | Format | Notes |
| --- | --- | --- |
| *(default)* | Text | Human-readable, MediaInfo-style report |
| `-x`, `--xml` | XML | Byte-compatible with MediaInfoLib's XML output |
| `-j`, `--json` | JSON | Structured; includes EXIF/IPTC/XMP/ICC tag streams |
| `--csv` | CSV | Flat field rows |
| `--summary` | Summary | Aggregate statistics across streams |

Only one format applies per run; text is used when none is given.

## Options

| Flag | Description |
| --- | --- |
| `-d`, `--demux <LEVEL>` | Demux depth: `frame` (default), `container`, `elementary` |
| `-r`, `--trace <N>` | Trace verbosity, `0`–`9` (default `0`) |
| `-m`, `--multi-file` | Scan companion files (BDMV M2TS playlists, sidecar subtitles) |
| `--video-only` | Keep only General + Video streams |
| `--audio-only` | Keep only General + Audio streams |
| `--stream <KIND:INDEX>` | Select specific streams; repeatable (see below) |
| `--verify` | Report structural integrity (`IsComplete`, truncation warnings) |
| `--inform-version` | Prepend the revelo library version to text output |
| `--inform-timestamp` | Prepend a report timestamp to text output |
| `--log-file <FILE>` | Write output to a file instead of stdout |
| `-h`, `--help` | Print help |
| `-V`, `--version` | Print version |

### Selecting streams

`--stream KIND:INDEX` picks an individual stream. `KIND` accepts either a
number or a name, and the flag may be repeated:

```sh
revelo movie.mkv --stream Video:0 --stream Audio:1
revelo movie.mkv --stream 1:0          # same as Video:0
```

| # | Kind | # | Kind |
| --- | --- | --- | --- |
| 0 | General | 7 | Exif |
| 1 | Video | 8 | Iptc |
| 2 | Audio | 9 | Xmp |
| 3 | Text | 10 | Icc |
| 4 | Other | 11 | C2pa |
| 5 | Image | 12 | MakerNotes |
| 6 | Menu | | |

## What it extracts

- **Containers & codecs** — MP4/MOV, Matroska/WebM, MPEG-TS, AVI, WAV, and ~200
  formats total, with codec details from AVC/HEVC/AV1/VP9 and AAC/AC-3/DTS/FLAC/Opus.
- **Photo metadata** — EXIF, GPS, IPTC, XMP, ICC, C2PA, and (with
  `exiftool-tables`) deep maker-notes for Canon, Nikon, Fujifilm, Olympus, Sony,
  Panasonic, and more.
- **HDR signalling** — HDR10+, Dolby Vision, HLG/PQ, plus Dolby Atmos / AC-4 and
  IAMF audio.

## Examples

```sh
# Audio tracks only, as JSON, written to a file
revelo concert.mkv --audio-only --json --log-file tracks.json

# Verify a possibly-truncated download
revelo --verify suspicious.mp4

# Deep camera maker-notes (requires the exiftool-tables build)
revelo IMG_1234.CR2 --json
```

## License

- **Core (default build):** BSD-2-Clause — see
  [LICENSE](https://github.com/vbasky/revelo/blob/main/LICENSE).
- **`exiftool-tables` feature:** pulls in `revelo-exiftool-tables`, which is
  **GPL-1.0-or-later OR Artistic-1.0-Perl** (tables derived from ExifTool,
  © Phil Harvey). A binary built with this feature inherits those terms.
