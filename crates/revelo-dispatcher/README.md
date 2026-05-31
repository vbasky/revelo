# revelo-dispatcher

Parallel format-detection dispatch table for revelo's media parsers.

Holds 180 parser function pointers ordered so that containers are checked
before elementary streams, races them all across CPU cores via
[rayon](https://docs.rs/rayon), and returns the first match in table order.
Both `revelo-cli` and `revelo-cdylib` depend on this crate as their single
entry point to format detection.

Part of the [**revelo**](https://github.com/vbasky/revelo) project — a fast,
safe, pure-Rust port of [MediaInfoLib](https://mediaarea.net/en/MediaInfo) for
extracting technical and tag metadata from media files. See the
[project README](https://github.com/vbasky/revelo#readme) for the full picture.

## Public API

| Symbol | Signature | Description |
| --- | --- | --- |
| `table` | `fn() -> [fn(&mut FileAnalyze) -> bool; 180]` | Returns the complete ordered dispatch table |
| `detect` | `fn(bytes: &[u8]) -> Option<fn(&mut FileAnalyze) -> bool>` | Races all parsers and returns the first match, or `None` |

## Usage

```rust,no_run
use revelo_core::FileAnalyze;
use revelo_dispatcher::detect;

fn analyze(bytes: &[u8]) {
    let Some(parser) = detect(bytes) else {
        eprintln!("unrecognized format");
        return;
    };

    // Run the winning parser against a fresh FileAnalyze to extract metadata.
    let mut fa = FileAnalyze::new(bytes);
    parser(&mut fa);

    let count = fa.stream_count(revelo_core::StreamKind::Video);
    println!("{count} video stream(s) found");
}
```

For an even higher-level entry point, use the
[`revelo`](https://crates.io/crates/revelo) facade crate, which combines
`detect`, parsing, and tag extraction into `Metadata::from_bytes()`.

## How detection works

`detect` calls `table()`, then runs `par_iter().find_first()` over the result.
Each candidate parser is invoked on a fresh, zero-cost `FileAnalyze` view over
the same buffer — parsers only peek at magic bytes or inspect the first few
hundred bytes, so the detection pass is cheap. `find_first` returns the
leftmost match in table order even though candidates are evaluated in parallel,
preserving the priority rule that containers are preferred over elementary
streams.

Once a winner is found, the caller re-runs it against a fresh `FileAnalyze` to
extract full metadata.

## Dispatch table ordering

| Group | Count | Examples |
| --- | --- | --- |
| Containers | ~47 | WAV, AVI, MP4, Matroska, MPEG-TS, MXF, BDMV, FLV, Ogg, … |
| Subtitles / Text | ~11 | SCTE-35, PGS, DVB, TTML, SubRip, … |
| Audio codecs | ~40 | FLAC, AC-3, AC-4, DTS, DTS-UHD, MP3, TrueHD, OPUS, AAC, … |
| Image formats | ~21 | JPEG, PNG, TIFF, CR2, RAF, OpenEXR, WebP, DPX, PSD, … |
| Video codecs | ~30 | AVC, HEVC, AV1, VP9, MPEG-2, VVC, ProRes, Dolby Vision, … |
| Archives | ~11 | ZIP, RAR, 7-Zip, TAR, Gzip, ISO 9660, ELF, Mach-O, … |

Containers come first because an elementary-codec parser could false-match on
random bytes inside a container. Late-matching groups (Opus, Vorbis, Teletext,
…) appear at the end of their category to avoid false positives.

## Zero unsafe code

`#![deny(unsafe_code)]` is enforced across this crate.

## License

BSD-2-Clause — see [LICENSE](https://github.com/vbasky/revelo/blob/main/LICENSE).
