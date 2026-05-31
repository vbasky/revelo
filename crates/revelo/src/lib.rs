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
//! Parse a video file and print every container-level field:
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
//! ## Key types
//!
//! | Type / function | What it does |
//! |---|---|
//! | [`Metadata`] | Main entry point — parse a file or buffer, iterate streams |
//! | [`Metadata::from_file`] | Parse from a file path |
//! | [`Metadata::from_bytes`] | Parse from an in-memory `&[u8]` |
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
pub struct Metadata {
    streams: StreamCollection,
}

impl Metadata {
    /// Parse a media file from an in-memory byte buffer.
    ///
    /// Detects the format, extracts container/codec metadata, and runs
    /// EXIF/IPTC/XMP/ICC/C2PA tag parsing. Returns `None` if no parser
    /// matched the buffer.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let parser = detect(bytes)?;
        let mut fa = MediaFile::new(bytes);
        parser(&mut fa);
        parse_tags(&mut fa);
        Some(Metadata { streams: fa.streams().clone() })
    }

    /// Parse a media file from a file path.
    pub fn from_file(path: &str) -> Option<Self> {
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
