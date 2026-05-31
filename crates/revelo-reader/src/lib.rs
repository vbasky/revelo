//! Input/reader registration layer for the
//! [revelo](https://github.com/vbasky/revelo) media-metadata library.
//!
//! `revelo-reader` is responsible for populating the `Reader` field on the
//! `General` stream of a [`revelo_core::FileAnalyze`] analysis context. The
//! `Reader` field records the source type through which a file (or URL) was
//! opened, mirroring the reader-registration step that MediaInfoLib performs
//! before dispatching to format parsers.
//!
//! # Public functions
//!
//! | Function | `Reader` value set |
//! | --- | --- |
//! | [`parse_file_reader`] | `"File"` — ordinary filesystem path |
//! | [`parse_directory_reader`] | `"Directory"` — directory source |
//! | [`parse_http_reader`] | `"HTTP"` — HTTP/HTTPS URL |
//! | [`parse_mms_reader`] | `"MMS"` — Microsoft Media Server URL |
//!
//! Each function accepts a mutable reference to a `FileAnalyze` context,
//! prepares the `General` stream if it does not already exist, writes the
//! appropriate `Reader` value, and returns `true`.
//!
//! # Example
//!
//! ```no_run
//! use revelo_core::FileAnalyze;
//! use revelo_reader::parse_file_reader;
//!
//! let mut fa = FileAnalyze::new(&[]);
//! parse_file_reader(&mut fa);
//! // The General stream now has Reader = "File".
//! ```

#![deny(unsafe_code)]

use revelo_core::{FileAnalyze, StreamKind};

pub fn parse_file_reader(fa: &mut FileAnalyze) -> bool {
    let pos = fa.stream_prepare(StreamKind::General);
    fa.set_field(StreamKind::General, pos, "Reader", "File");
    true
}

pub fn parse_directory_reader(fa: &mut FileAnalyze) -> bool {
    let pos = fa.stream_prepare(StreamKind::General);
    fa.set_field(StreamKind::General, pos, "Reader", "Directory");
    true
}

pub fn parse_http_reader(fa: &mut FileAnalyze) -> bool {
    let pos = fa.stream_prepare(StreamKind::General);
    fa.set_field(StreamKind::General, pos, "Reader", "HTTP");
    true
}

pub fn parse_mms_reader(fa: &mut FileAnalyze) -> bool {
    let pos = fa.stream_prepare(StreamKind::General);
    fa.set_field(StreamKind::General, pos, "Reader", "MMS");
    true
}
