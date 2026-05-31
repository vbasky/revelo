# revelo-parsers-image

Image-format parsers for the [**revelo**](https://github.com/vbasky/revelo)
media-analysis library — a fast, safe, pure-Rust port of
[MediaInfoLib](https://mediaarea.net/en/MediaInfo) with optional
[ExifTool](https://exiftool.org/)-grade EXIF depth.

Each parser is a focused `fn(&mut FileAnalyze) -> bool` that identifies its
format by magic bytes or structural heuristics, then fills the relevant
`Image`/`General`/`Exif` fields on the `FileAnalyze` context. This is also
the crate where EXIF/IFD parsing lives: the TIFF and JPEG parsers implement
full IFD walking and surface GPS, datetime, camera make/model, and related
tags.  Parsers are registered in the revelo dispatcher and are not normally
called directly by application code.

Part of the [**revelo**](https://github.com/vbasky/revelo) project — see the
[project README](https://github.com/vbasky/revelo#readme) for the full picture.

## Normal usage

Application code should go through the [`revelo`](https://crates.io/crates/revelo)
facade crate, not this crate directly:

```toml
[dependencies]
revelo = "0.4"
```

This crate is a public implementation detail of the revelo workspace, exposed
separately so that container-parser crates can depend on it without pulling in
the entire stack.

## Supported formats

| Parser function | Format | Notes |
| --- | --- | --- |
| `parse_amiga_icon` | Amiga icon | AmigaOS `.info` icon format |
| `parse_arriraw` | ARRIRAW | ARRI raw cinema camera format |
| `parse_bmp` | BMP | Windows / OS/2 Bitmap |
| `parse_bpg` | BPG | Better Portable Graphics |
| `parse_cr2` | CR2 | Canon RAW 2 (TIFF-based) |
| `parse_dds` | DDS | DirectDraw Surface (GPU texture) |
| `parse_dpx` | DPX | Digital Picture Exchange (SMPTE 268M) |
| `parse_exr` | OpenEXR | ILM high-dynamic-range image |
| `parse_gain_map` | Gain map | ISO 21496-1 HDR gain map |
| `parse_gif` | GIF | Graphics Interchange Format (87a / 89a) |
| `parse_heif` | HEIF / HEIC | High Efficiency Image Format (ISO 23008-12) |
| `parse_ico` | ICO | Windows icon / cursor |
| `parse_jp2` | JPEG 2000 | ISO/IEC 15444-1 |
| `parse_jpeg` | JPEG / JFIF | JPEG with EXIF/IFD parsing |
| `parse_pcx` | PCX | ZSoft PC Paintbrush |
| `parse_png` | PNG | Portable Network Graphics |
| `parse_psd` | PSD / PSB | Adobe Photoshop Document |
| `parse_raf` | RAF | Fujifilm RAW format |
| `parse_rle` | RLE | Run-length encoded bitmap |
| `parse_tga` | TGA | Truevision TARGA |
| `parse_tiff` | TIFF | Tag Image File Format with full IFD/EXIF walking |
| `parse_webp` | WebP | Google WebP (VP8 / VP8L / VP8X) |

### EXIF / IFD parsing

`parse_tiff` and `parse_jpeg` implement full IFD (Image File Directory)
walking per the TIFF 6.0 and EXIF 2.3 specifications. Both little-endian
("II") and big-endian ("MM") byte orders are handled. Tags surfaced include:

- Camera make, model, software, artist, copyright
- `DateTime`, `DateTimeOriginal`, `DateTimeDigitized`
- Image dimensions, resolution, orientation, colour space
- GPS IFD (latitude, longitude, altitude, timestamp)
- Exif sub-IFD (exposure time, f-number, ISO, focal length, flash)
- IFD1 thumbnail dimensions

## Design

All parsers are pure Rust with `#[deny(unsafe_code)]`. They carry no system
dependencies and link no external libraries. The `FileAnalyze` type is defined
in `revelo-core` and holds the file buffer plus the stream field map that
accumulates results.

## License

BSD-2-Clause — see [LICENSE](https://github.com/vbasky/revelo/blob/main/LICENSE).
