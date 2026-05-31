# revelo-core

Core engine for revelo: the FileAnalyze/MediaFile byte reader, stream collection,
element tree, and ergonomic Reader API.

**New users:** consider using the [`revelo`](https://crates.io/crates/revelo)
facade crate instead — it wraps detect, parse, and tag extraction into a single
`Metadata::from_bytes()` call.

Part of [**revelo**](https://github.com/vbasky/revelo) — a fast, safe Rust port
of MediaInfoLib for extracting technical and tag metadata from media files. See
the [project README](https://github.com/vbasky/revelo#readme) for the full
picture.

## License

BSD-2-Clause — see [LICENSE](https://github.com/vbasky/revelo/blob/main/LICENSE).
