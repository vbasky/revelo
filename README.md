# revelio

A Rust library and CLI for reading technical metadata from media files —
containers (MP4, MKV, MPEG-TS, AVI, …), audio codecs, video codecs, image
formats, and subtitle streams.

Built as a port of MediaInfoLib, validated by differential testing against the
C++ `mediainfo` CLI.

## Status

114 parsers registered and wired through the diff harness:

| Category | Formats | Coverage |
|---|---|---|
| Containers | 42 | 98% |
| Audio | 33 | 57% |
| Video | 12 | 39% |
| Image | 18 | 100% |
| Text/Subtitles | 12 | 60% |

Many parsers are byte-equal with the C++ oracle. Remaining work: archive (Zip,
Rar, 7z, …), tag (ID3, EXIF, XMP, …), reader layer, full export formatters
(EBUCore, MPEG-7, JSON), and the C ABI shim.

## Building

```sh
cargo build --release
```

## Running

```sh
# differential test harness against installed mediainfo CLI
cargo run -p diff-harness -- /path/to/media-file.mp4

# standalone CLI
cargo run -p revelio-cli -- --json /path/to/media-file.mp4
```
