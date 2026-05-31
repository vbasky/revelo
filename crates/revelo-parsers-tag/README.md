# revelo-parsers-tag

Tag and metadata parsers for [**revelo**](https://github.com/vbasky/revelo) — a
fast, safe, pure-Rust port of [MediaInfoLib](https://mediaarea.net/en/MediaInfo)
with optional [ExifTool](https://exiftool.org/)-grade EXIF depth. This crate
covers embedded metadata streams: it locates and decodes ID3, APE, Vorbis
Comment, EXIF/TIFF, XMP, IIM/IPTC, ICC, C2PA, Apple PropertyList, and
Spherical Video blocks, populating the corresponding `FileAnalyze` streams.

Part of the [**revelo**](https://github.com/vbasky/revelo) project — see the
[project README](https://github.com/vbasky/revelo#readme) for the full picture.

## Normal use

Most users should depend on the [`revelo`](https://crates.io/crates/revelo)
facade crate rather than this crate directly. The facade re-exports every parser
and wires them into the dispatcher automatically.

## Supported tag formats

| Function | Tag format / Standard |
| --- | --- |
| `parse_id3v1` | ID3v1 — 128-byte trailer tag (title, artist, album, track, genre) |
| `parse_id3v2` | ID3v2.2 / v2.3 / v2.4 — modern ID3 header tag |
| `parse_ape_tag` | APEv1 / APEv2 tag (Monkey's Audio, WavPack, …) |
| `parse_vorbis_comment` | Vorbis Comment (Ogg, FLAC, Opus) |
| `parse_vorbis_comment_from_buf` | Vorbis Comment from a pre-read byte buffer |
| `parse_lyrics3` | Lyrics3 v1/v2 tag block |
| `parse_exif` | EXIF / TIFF IFD chain — Exif IFD, GPS IFD, Interop IFD |
| `parse_xmp` | XMP (Extensible Metadata Platform) packet |
| `parse_iim` | IIM / IPTC (IPTC-NAA record 2, Photoshop IRB framing) |
| `parse_iim_buf` | IIM from a pre-read byte buffer |
| `parse_icc` | ICC colour profile (v2 / v4) |
| `parse_c2pa` | C2PA (Coalition for Content Provenance and Authenticity) |
| `parse_property_list` | Apple PropertyList (binary plist / XML plist) |
| `parse_spherical_video` | Google Spherical Video v1/v2 metadata |
| `parse_tags` | Dispatcher: tries all tag parsers in priority order |

## Feature flags

### `exiftool-tables` (off by default)

```toml
[dependencies]
revelo-parsers-tag = { version = "0.4", features = ["exiftool-tables"] }
```

Enables ExifTool-grade maker-note decoding for camera-vendor proprietary tags.
When this feature is active the EXIF parser uses lookup tables derived from
Phil Harvey's [ExifTool](https://exiftool.org/) project, covering 14 camera
vendors (Canon, Nikon, Fujifilm, Olympus, Sony, Panasonic, and more).

> **License note:** the default build of this crate is **BSD-2-Clause**.
> Enabling `exiftool-tables` pulls in the `revelo-exiftool-tables` crate, whose
> tables are derived from ExifTool (© Phil Harvey) and are therefore subject to
> **GPL-1.0-or-later OR Artistic-1.0-Perl** terms. Any binary or library that
> links against `revelo-exiftool-tables` inherits those terms. If your project
> requires a permissive-only licence, do **not** enable this feature.

Without the feature, maker-note fields are decoded using hand-written,
clean-room tables and remain BSD-2-Clause.

## Usage

```no_run
use revelo_parsers_tag::{parse_exif, parse_id3v2, parse_xmp};
use revelo_core::FileAnalyze;

let data: Vec<u8> = std::fs::read("photo.jpg").unwrap();
let mut fa = FileAnalyze::new(&data);

// Each parser is idempotent — call only the one(s) relevant to your format,
// or call parse_tags() to let the dispatcher try them all.
parse_exif(&mut fa);
parse_xmp(&mut fa);
```

Prefer the `revelo` facade for everyday use — it handles format detection and
tag dispatch automatically.

## Safety

`#![deny(unsafe_code)]` — zero unsafe blocks.

## License

- **Core (default build):** BSD-2-Clause — see
  [LICENSE](https://github.com/vbasky/revelo/blob/main/LICENSE).
- **`exiftool-tables` feature:** `revelo-exiftool-tables` is
  **GPL-1.0-or-later OR Artistic-1.0-Perl** (tables derived from ExifTool,
  © Phil Harvey). A binary built with this feature inherits those terms.
