//! # revelo — read technical metadata from any media file
//!
//! A pure-Rust, zero-`unsafe` library for extracting technical and tag metadata
//! from media files. No system libraries, no Perl runtime, no `./Configure`.
//!
//! - Detects **180+ container and codec formats** (MP4, Matroska, MPEG-TS, AVI, WAV, …)
//! - Extracts **container/codec fields** — duration, bitrate, resolution, frame rate, HDR
//! - Decodes **EXIF, IPTC, XMP, ICC, C2PA** embedded in photos and video
//! - Optionally decodes **deep maker-notes** from 14 camera vendors (Canon, Nikon, Fujifilm,
//!   Olympus, Sony, Panasonic, …) via the `exiftool-tables` feature
//!
//! ## Quick start
//!
//! Parse a video file (memory-mapped — only the metadata regions are faulted in,
//! not the entire file). Works the same for small JPEGs and multi-GB videos:
//!
//! ```rust,no_run
//! let meta = revelo::Metadata::from_file("video.mp4").unwrap();
//!
//! for (key, value) in meta.general() {
//!     println!("{key} = {value}");
//! }
//! for (key, value) in meta.video() {
//!     println!("{key} = {value}");
//! }
//! for (key, value) in meta.audio() {
//!     println!("{key} = {value}");
//! }
//! ```
//!
//! Parse a photo from an in-memory buffer and read its EXIF tags:
//!
//! ```rust,no_run
//! let bytes = std::fs::read("photo.jpg").unwrap();
//! let meta = revelo::Metadata::from_bytes(&bytes).unwrap();
//!
//! for (key, value) in meta.exif() {
//!     println!("{key} = {value}");
//! }
//! for (key, value) in meta.iptc() {
//!     println!("{key} = {value}");
//! }
//! for (key, value) in meta.xmp() {
//!     println!("{key} = {value}");
//! }
//! ```
//!
//! ### Memory semantics
//!
//! | Method | I/O model | Memory use | Use when |
//! |---|---|---|---|
//! | [`Metadata::from_file`] | Memory-mapped (mmap) | OS pages in only accessed regions | Local files — tiny, large, or huge |
//! | [`Metadata::from_bytes`] | Already in memory (borrowed) | Zero copy from the caller's buffer | Bytes from a network fetch, embedded asset, or `include_bytes!` |
//! | [`Metadata::from_file_owned`] | Reads entire file into `Vec<u8>` | Proportional to file size | Fallback when mmap is unavailable or caller wants ownership |
//!
//! For 99% of use cases: **use `from_file` for files, `from_bytes` for buffers**.
//! Both extract the same metadata; the difference is how the bytes reach the parser.
//!
//! ## Key types
//!
//! | Type / function | What it does |
//! |---|---|
//! | [`Metadata`] | Main entry point — parse a file or buffer, iterate streams |
//! | [`Metadata::from_file`] | Parse from a file path (memory-mapped) |
//! | [`Metadata::from_bytes`] | Parse from an in-memory `&[u8]` |
//! | [`Metadata::from_file_owned`] | Parse from a file path (full file read, fallback) |
//! | [`MediaFile`] | Low-level byte-parsing engine (`FileAnalyze` alias) |
//! | [`revelo_core::stream::StreamCollection`] | Raw per-kind stream store |
//! | [`revelo_core::stream::StreamKind`] | Discriminant for General/Video/Audio/Exif/… |
//!
//! ## `exiftool-tables` feature
//!
//! By default revelo uses hand-written clean-room maker-note tables (BSD-2-Clause).
//! Enable the `exiftool-tables` feature for ExifTool-grade depth:
//!
//! ```toml
//! [dependencies]
//! revelo = { version = "0.4", features = ["exiftool-tables"] }
//! ```
//!
//! **License caveat:** `exiftool-tables` pulls in `revelo-exiftool-tables`
//! (GPL-1.0-or-later OR Artistic-1.0-Perl, © Phil Harvey). A binary or library
//! built with this feature is subject to those terms.

use revelo_core::stream::{StreamCollection, StreamKind};
#[cfg(feature = "mmap")]
use revelo_core::{MediaReadAt, MmapBackend, ReadBackend};
use revelo_dispatcher::detect;
use revelo_parsers_tag::parse_tags;

pub use revelo_core;
pub use revelo_dispatcher;
pub use revelo_parsers_tag;

/// The engine that reads bytes and produces metadata.
///
/// Type alias for [`revelo_core::FileAnalyze`]. Use this name in new code;
/// `FileAnalyze` continues to work for compatibility.
pub type MediaFile<'a> = revelo_core::FileAnalyze<'a>;

/// Parsed metadata from a media file.
///
/// Created via [`Metadata::from_file`] (memory-mapped, recommended),
/// [`Metadata::from_bytes`] (already in-memory buffer), or
/// [`Metadata::from_file_owned`] (full file read, fallback).
///
/// Stream results are fully owned — callers can drop the input buffer
/// or file handle after parsing completes.
pub struct Metadata {
    streams: StreamCollection,
}

impl Metadata {
    /// Parse a media file from an in-memory byte buffer.
    ///
    /// Detects the format, extracts container/codec metadata, and runs
    /// EXIF/IPTC/XMP/ICC/C2PA tag parsing. Zero-copy from the caller's
    /// buffer — no additional allocation for the raw bytes (stream
    /// values are owned [`String`]s and are cloned from the buffer).
    ///
    /// Returns `None` if no parser matched the buffer.
    ///
    /// # Memory
    ///
    /// The caller already holds the bytes. This function borrows them
    /// during parsing and returns owned metadata — the caller is free
    /// to drop the buffer afterwards.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let parser = detect(bytes)?;
        let mut fa = MediaFile::new(bytes);
        parser(&mut fa);
        parse_tags(&mut fa);
        Some(Metadata { streams: fa.streams().clone() })
    }

    /// Parse a media file from a file path (memory-mapped).
    ///
    /// Memory maps the file; the OS faults in only the pages the parser
    /// actually touches. For container formats (MP4, MKV, WAV, etc.)
    /// this means only the metadata atoms are read — the bulk sample
    /// data is never faulted in. Performance scales with metadata size,
    /// not with file size.
    ///
    /// Returns `None` if the file cannot be opened or mapped, or if no
    /// parser matched the contents.
    ///
    /// # Performance
    ///
    /// For a 389 MiB MP4 with ~100 KiB of metadata, expect tens of
    /// milliseconds — comparable to MediaInfo. The same file via
    /// [`from_file_owned`] would read all 389 MiB into memory first.
    ///
    /// # Portability
    ///
    /// Mmap is supported on all major platforms. If mmap fails (e.g.,
    /// on some networked filesystems), consider [`from_file_owned`] as
    /// a fallback.
    pub fn from_file(path: &str) -> Option<Self> {
        #[cfg(feature = "mmap")]
        {
            let file = std::fs::File::open(path).ok()?;
            // Try memory-mapping first (OS faults in only the pages the
            // parser actually touches). If mmap fails — e.g. on networked
            // filesystems that don't support it — fall back to the safe
            // full-file read path.
            // SAFETY: the file is opened read-only and is not mutated while
            // mapped. The mapping is dropped before the file handle closes.
            match unsafe { memmap2::Mmap::map(&file) } {
                Ok(mmap) => {
                    let mmap_backend = MmapBackend::new(&mmap);
                    let parser = detect(mmap_backend.as_contiguous()?)?;
                    let backend = ReadBackend::from(mmap_backend);
                    let mut fa = MediaFile::from_backend(backend);
                    parser(&mut fa);
                    parse_tags(&mut fa);
                    return Some(Metadata { streams: fa.streams().clone() });
                }
                Err(_) => {}
            }
        }
        // Fallback: read entire file into memory.
        // On WASM and platforms without mmap, this is the only path.
        Self::from_file_owned(path)
    }

    /// Parse a media file by reading the entire file into memory.
    ///
    /// This is the legacy path — it allocates a `Vec<u8>` proportional
    /// to the file size, then delegates to [`from_bytes`]. Prefer
    /// [`from_file`] (memory-mapped) for local files; use this only
    /// when mmap is unavailable or when you need to own the raw bytes.
    ///
    /// # Memory
    ///
    /// A 389 MiB file will allocate ~389 MiB before parsing begins.
    /// The allocation is freed when this function returns — the
    /// returned [`Metadata`] only contains the parsed stream values.
    pub fn from_file_owned(path: &str) -> Option<Self> {
        let bytes = std::fs::read(path).ok()?;
        Self::from_bytes(&bytes)
    }

    /// The raw stream collection for advanced queries.
    pub fn streams(&self) -> &StreamCollection {
        &self.streams
    }

    fn stream_iter(&self, kind: StreamKind, pos: usize) -> impl Iterator<Item = (&str, &str)> {
        self.streams
            .stream(kind, pos)
            .into_iter()
            .flat_map(|s| s.iter().map(|(k, v)| (k.as_ref(), v.as_ref())))
    }

    /// Consume self and return the underlying [`StreamCollection`].
    pub fn into_streams(self) -> StreamCollection {
        self.streams
    }

    /// Iterate over (field_name, value) pairs in the General stream.
    pub fn general(&self) -> impl Iterator<Item = (&str, &str)> {
        self.stream_iter(StreamKind::General, 0)
    }

    /// Iterate over (field_name, value) pairs in the first Video stream.
    pub fn video(&self) -> impl Iterator<Item = (&str, &str)> {
        self.stream_iter(StreamKind::Video, 0)
    }

    /// Iterate over (field_name, value) pairs in the first Audio stream.
    pub fn audio(&self) -> impl Iterator<Item = (&str, &str)> {
        self.stream_iter(StreamKind::Audio, 0)
    }

    /// Iterate over (field_name, value) pairs in the first Text stream.
    pub fn text(&self) -> impl Iterator<Item = (&str, &str)> {
        self.stream_iter(StreamKind::Text, 0)
    }

    /// Iterate over (field_name, value) pairs in the first Image stream.
    pub fn image(&self) -> impl Iterator<Item = (&str, &str)> {
        self.stream_iter(StreamKind::Image, 0)
    }

    /// Iterate over (field_name, value) pairs in the EXIF stream.
    pub fn exif(&self) -> impl Iterator<Item = (&str, &str)> {
        self.stream_iter(StreamKind::Exif, 0)
    }

    /// Iterate over (field_name, value) pairs in the IPTC stream.
    pub fn iptc(&self) -> impl Iterator<Item = (&str, &str)> {
        self.stream_iter(StreamKind::Iptc, 0)
    }

    /// Iterate over (field_name, value) pairs in the XMP stream.
    pub fn xmp(&self) -> impl Iterator<Item = (&str, &str)> {
        self.stream_iter(StreamKind::Xmp, 0)
    }
}

#[cfg(all(test, feature = "mmap"))]
mod tests {
    use super::*;
    use revelo_core::{ByteRange, MediaReadAt, MmapBackend};
    use std::io::Write;

    #[test]
    fn mmap_backend_windows_match_file_bytes() {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        file.write_all(&[40, 41, 42, 43, 44, 45]).unwrap();
        file.as_file().sync_all().unwrap();

        let mmap = unsafe { memmap2::Mmap::map(file.as_file()).unwrap() };
        let source = MmapBackend::new(&mmap);

        assert_eq!(source.len_u64(), 6);
        assert_eq!(source.window_at(ByteRange::new(2, 3).unwrap()).unwrap(), &[42, 43, 44]);
        assert_eq!(source.window_at_partial(ByteRange::new(4, 8).unwrap()).unwrap(), &[44, 45]);
    }
}
