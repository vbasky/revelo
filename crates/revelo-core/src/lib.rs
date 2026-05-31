//! Core parsing engine for [revelo](https://github.com/vbasky/revelo).
//!
//! Transliterates MediaInfoLib's `File__Analyze` infrastructure — the
//! cursor-based byte reader every parser uses (`Get_B*`, `Get_L*`,
//! `Peek_*`, `Skip_*`, bitstream mode) plus the element trace tree, typed
//! stream collection, runtime config, and event dispatch — while also
//! exposing ExifTool-style stream kinds (`Exif`, `Iptc`, `Xmp`, `Icc`,
//! `C2pa`, `MakerNotes`) for camera-maker-note depth.
//!
//! All read methods return native Rust types (`u8`–`u128`, `f32`, `f64`)
//! rather than out-parameters or C-style aliases. Truncated reads return
//! `0` / empty slices and set a `truncated()` flag rather than panicking.
//!
//! # Key public types
//!
//! | Type | Role |
//! |------|------|
//! | [`FileAnalyze`] | Cursor over a `&[u8]` buffer; received by every parser |
//! | [`MediaFile`] | Public type alias for `FileAnalyze` |
//! | [`StreamCollection`] | All parsed fields, keyed by `(StreamKind, position)` |
//! | [`Stream`] | A single stream's fields in insertion order plus an `<extra>` bucket |
//! | [`StreamKind`] | Discriminant: `General`…`Menu` (MediaInfo-compatible 0–6) + `Exif`…`MakerNotes` (7–12) |
//! | [`ElementTree`] / [`ElementNode`] | Stack-based trace tree (mirrors MediaInfoLib `--trace` output) |
//! | [`Reader`] | Fluent `Option`-returning wrapper over `FileAnalyze` |
//! | `MediaConfig` | Demux level, trace verbosity, parse speed, multi-file options |
//!
//! # Writing a parser
//!
//! ```no_run
//! use revelo_core::{FileAnalyze, StreamKind};
//!
//! fn my_parser(fa: &mut FileAnalyze) -> bool {
//!     // Non-advancing magic check
//!     if !fa.peek_magic(b"RIFF") {
//!         return false;
//!     }
//!     let _chunk_size = fa.get_b4("ChunkSize");
//!     let _form_tag   = fa.get_c4("FormTag");
//!
//!     let pos = fa.stream_prepare(StreamKind::General);
//!     fa.set_field(StreamKind::General, pos, "Format", "MyFormat");
//!     true
//! }
//! ```
//!
//! Or via the higher-level [`Reader`] API (`None` signals truncation instead
//! of falling back to 0):
//!
//! ```no_run
//! use revelo_core::{FileAnalyze, Reader, StreamKind};
//!
//! fn my_parser(fa: &mut FileAnalyze) -> bool {
//!     let mut r = Reader::wrap(fa);
//!     let Some(_size) = r.be_u32("Size") else { return false; };
//!     let pos = r.stream_prepare(StreamKind::Audio);
//!     r.set_field(StreamKind::Audio, pos, "BitDepth", "24");
//!     true
//! }
//! ```
//!
//! # `#![deny(unsafe_code)]`
//!
//! The entire crate is enforced `unsafe`-free.

#![allow(non_snake_case)]
#![deny(unsafe_code)]

pub mod element;
pub mod file_analyze;
pub mod file_level;
pub mod prelude;
pub mod reader;
mod revelo_util_re_export;
pub mod stream;

pub use element::{ElementInfo, ElementNode, ElementTree};
pub use file_analyze::FileAnalyze;
/// Ergonomic alias for [`FileAnalyze`]. Both names refer to the same type.
pub type MediaFile<'a> = FileAnalyze<'a>;
pub use file_level::{FileLevelInfo, fill_file_level_fields};
pub use reader::Reader;
pub use stream::{Stream, StreamCollection, StreamKind};

pub mod channel_grouping;
pub mod channel_splitting;
pub mod events;
pub mod ibi;
pub mod interlacement;
pub mod mime;
pub mod timecode;
/// Container-level reference file tracker.
pub mod reference {

    pub struct ReferenceFile {
        pub path: String,
        pub format: &'static str,
        pub stream_id: u64,
    }
    pub struct ReferenceTracker {
        pub files: Vec<ReferenceFile>,
    }
    impl Default for ReferenceTracker {
        fn default() -> Self {
            Self::new()
        }
    }

    impl ReferenceTracker {
        pub fn new() -> Self {
            Self { files: Vec::new() }
        }
        pub fn add(&mut self, path: &str, format: &'static str, stream_id: u64) {
            self.files.push(ReferenceFile { path: path.to_string(), format, stream_id });
        }
        pub fn count(&self) -> usize {
            self.files.len()
        }
    }
    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn test_ref() {
            let mut t = ReferenceTracker::new();
            t.add("extra.m2ts", "BDAV", 0x1011);
            assert_eq!(t.count(), 1);
        }
    }
}
pub mod computed_fields;
pub mod config;
pub mod data_helpers;
pub mod multi_file;
