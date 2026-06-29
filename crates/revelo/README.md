# revelo

![revelo — read technical metadata from any media file, in pure Rust](https://raw.githubusercontent.com/vbasky/revelo/main/docs/banner.png)

Read technical metadata from any media file — pure Rust, no system dependencies.

A from-scratch Rust port of [MediaInfoLib](https://mediaarea.net/en/MediaInfo) with optional
[ExifTool](https://exiftool.org/)-grade EXIF/maker-note depth. Detects 180+ formats, decodes
EXIF/IPTC/XMP tags, and handles maker-notes from 14 camera vendors.

No `./Configure`, no system libraries, no Perl runtime. Zero unsafe code.

Part of the [**revelo**](https://github.com/vbasky/revelo) project — see the
[project README](https://github.com/vbasky/revelo#readme) for the full picture.

## Install

```toml
[dependencies]
revelo = "0.5"
```

Or via Cargo:

```sh
cargo add revelo
```

For ExifTool-grade maker-note depth across 14 camera vendors, opt into the `exiftool-tables`
feature:

```toml
[dependencies]
revelo = { version = "0.5", features = ["exiftool-tables"] }
```

> **License note:** the default build is **BSD-2-Clause**. The `exiftool-tables` feature pulls in
> tables derived from ExifTool, so a binary or library built with it is subject to **GPL/Artistic**
> terms. See [Licensing](#license) below.

## Quick start

```rust,no_run
fn main() {
    // Parse from a file path
    let meta = revelo::Metadata::from_file("video.mp4").unwrap();

    // Container/codec fields (format, duration, bitrate, …)
    for (key, value) in meta.general() {
        println!("{key} = {value}");
    }

    // First video stream
    for (key, value) in meta.video() {
        println!("{key} = {value}");
    }

    // First audio stream
    for (key, value) in meta.audio() {
        println!("{key} = {value}");
    }
}
```

```rust,no_run
fn main() {
    // Parse from an in-memory buffer (e.g. downloaded bytes)
    let bytes = std::fs::read("photo.jpg").unwrap();
    let meta = revelo::Metadata::from_bytes(&bytes).unwrap();

    for (key, value) in meta.exif() {
        println!("{key} = {value}");
    }
    for (key, value) in meta.iptc() {
        println!("{key} = {value}");
    }
    for (key, value) in meta.xmp() {
        println!("{key} = {value}");
    }
}
```

## API surface

### `Metadata` — the main type

| Method | Returns | Description |
| --- | --- | --- |
| `Metadata::from_file(path)` | `Option<Metadata>` | Parse a file by path |
| `Metadata::from_bytes(buf)` | `Option<Metadata>` | Parse an in-memory byte slice |
| `meta.streams()` | `&StreamCollection` | Raw stream collection for advanced queries |
| `meta.into_streams()` | `StreamCollection` | Consume and return the underlying streams |

### Stream accessors

Each method returns an `Iterator<Item = (&str, &str)>` of `(field_name, value)` pairs.

| Method | Stream kind | Typical use |
| --- | --- | --- |
| `meta.general()` | General | Format, duration, file size, overall bitrate |
| `meta.video()` | Video | Codec, resolution, frame rate, HDR signalling |
| `meta.audio()` | Audio | Codec, channels, sample rate, language |
| `meta.text()` | Text | Subtitle/caption tracks |
| `meta.image()` | Image | Still-image metadata |
| `meta.exif()` | Exif | EXIF tags (camera settings, GPS, timestamps) |
| `meta.iptc()` | Iptc | IPTC press-photo metadata |
| `meta.xmp()` | Xmp | XMP/RDF metadata |

All accessors refer to the first stream of that kind (index 0). For files with multiple video or
audio tracks, use `meta.streams()` directly and call
`streams.stream(StreamKind::Video, index)` for each index.

### `MediaFile<'a>` — low-level engine

`MediaFile` (a type alias for `revelo_core::FileAnalyze`) is the byte-level parsing engine.
It is re-exported for callers that need to feed raw `FileAnalyze` state into downstream crates
(`revelo-dispatcher`, `revelo-parsers-tag`, etc.). Most users should prefer `Metadata`.

## What it extracts

- **Containers & codecs** — MP4/MOV, Matroska/WebM, MPEG-TS, AVI, WAV, and 180+ formats
  total, with codec details from AVC/HEVC/AV1/VP9 and AAC/AC-3/DTS/FLAC/Opus.
- **Photo metadata** — EXIF, GPS, IPTC, XMP, ICC, C2PA, and (with `exiftool-tables`) deep
  maker-notes for Canon, Nikon, Fujifilm, Olympus, Sony, Panasonic, and more.
- **HDR signalling** — HDR10+, Dolby Vision, HLG/PQ, plus Dolby Atmos / AC-4 and IAMF audio.

## Re-exported crates

`revelo` re-exports its three underlying crates for callers that need direct access:

| Re-export | Crate | Purpose |
| --- | --- | --- |
| `revelo::revelo_core` | `revelo-core` | Stream model, `FileAnalyze`, `StreamCollection`, `StreamKind` |
| `revelo::revelo_dispatcher` | `revelo-dispatcher` | Format detection and parser dispatch |
| `revelo::revelo_parsers_tag` | `revelo-parsers-tag` | EXIF/IPTC/XMP/ICC/C2PA tag parsing |

## License

- **Core (default build):** BSD-2-Clause — see
  [LICENSE](https://github.com/vbasky/revelo/blob/main/LICENSE).
- **`exiftool-tables` feature:** pulls in `revelo-exiftool-tables`, which is
  **GPL-1.0-or-later OR Artistic-1.0-Perl** (tables derived from ExifTool, © Phil Harvey).
  A binary or library built with this feature inherits those terms.
