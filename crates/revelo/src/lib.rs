//! # revelo — read technical metadata from any media file
//!
//! A pure-Rust library that replaces both MediaInfoLib and ExifTool for metadata
//! extraction. Detects 180+ formats, extracts container/codec metadata, and
//! decodes EXIF/maker-note tags from 14 camera vendors.
//!
//! ```rust,no_run
//! let meta = revelo::Metadata::from_file("photo.jpg").unwrap();
//!
//! for (key, value) in meta.general() {
//!     println!("{key} = {value}");
//! }
//! for (key, value) in meta.exif() {
//!     println!("{key} = {value}");
//! }
//! ```

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
