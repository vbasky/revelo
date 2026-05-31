//! Output formatters for the [revelo](https://github.com/vbasky/revelo)
//! media-metadata library.
//!
//! Each formatter takes a [`revelo_core::StreamCollection`] (the parsed
//! representation of a media file) plus the file path string, and returns
//! a `String` in the target format. The two primary formats — XML and JSON
//! — target byte-for-byte compatibility with MediaInfoLib's output so that
//! automated diffing against the reference implementation is possible.
//!
//! # Formatters
//!
//! | Function | Output |
//! |---|---|
//! | [`to_xml`] | MediaInfo-compatible XML (schema version 2.0) |
//! | [`to_json`] | MediaInfo-compatible JSON |
//! | [`to_text`] | Human-readable text report (MediaInfo default style) |
//! | [`to_csv`] | CSV with per-kind sections; pipe-friendly |
//! | [`to_summary`] | Compact aggregate statistics across all streams |
//! | [`to_ebu_core`] | EBU MXF Core Metadata XML (ebuCore 2015) |
//! | [`to_mpeg7`] | MPEG-7 `MediaInformation` XML |
//! | [`to_pbcore`] | PBCore 2.x `pbcoreDescriptionDocument` XML |
//! | [`to_niso`] | NISO Z39.87 (MIX) technical-metadata XML (pragmatic, not schema-validated) |
//! | [`to_fims`] | FIMS 1.0 media XML (pragmatic, not schema-validated) |
//! | [`to_graph`] | Graphviz DOT stream-field graph |
//! | [`to_revtmd`] | RevTMD custom XML |
//!
//! # Quick example
//!
//! ```no_run
//! use revelo_core::StreamCollection;
//! use revelo_export::{to_xml, to_json, to_text, to_csv, to_summary};
//!
//! // Obtain a StreamCollection from revelo-core (parsing happens there).
//! let streams: StreamCollection = todo!("parse your file here");
//! let path = "/media/video.mp4";
//!
//! let xml  = to_xml(&streams, path);
//! let json = to_json(&streams, path);
//! let text = to_text(&streams, path);
//! let csv  = to_csv(&streams, path);
//! let summary = to_summary(&streams, path);
//! ```
//!
//! # Format notes
//!
//! - **XML / JSON**: field ordering and Duration rendering (stored as integer
//!   milliseconds, emitted as decimal seconds with 3 fraction digits) match
//!   the MediaInfoLib oracle. The `<creatingLibrary>` / `creatingLibrary`
//!   header that upstream emits is intentionally omitted — it identifies the
//!   producing tool, not the file.
//! - **Text**: uses MediaInfo's friendly field labels ("Bit rate", "Frame
//!   rate mode") and humanised values ("5.26 MiB", "1 min 0 s", "734 kb/s").
//! - **CSV**: one section per stream kind; the first data row is field names,
//!   subsequent rows are stream positions; values containing commas or quotes
//!   are RFC 4180 escaped.
//! - **Summary**: aggregates codec lists, resolution ranges, channel counts,
//!   and sample rates across all streams of each kind.

#![allow(non_snake_case)]
#![deny(unsafe_code)]

pub mod csv;
pub mod ebu_core;
pub mod fims;
pub mod graph;
pub mod json;
pub mod mpeg7;
pub mod niso;
pub mod pbcore;
pub mod revtmd;
pub mod summary;
pub mod text;
pub mod xml;

pub use csv::to_csv;
pub use ebu_core::to_ebu_core;
pub use fims::to_fims;
pub use graph::to_graph;
pub use json::to_json;
pub use mpeg7::to_mpeg7;
pub use niso::to_niso;
pub use pbcore::to_pbcore;
pub use revtmd::to_revtmd;
pub use summary::to_summary;
pub use text::to_text;
pub use xml::to_xml;

/// Escapes XML text-content special characters (`&`, `<`, `>`) for the
/// auxiliary exporters (ebuCore / MPEG-7 / PBCore / RevTMD). Without this a
/// value such as `H.264 & AAC` or a path containing `<`/`&` produces malformed
/// XML. The primary [`xml`]/[`json`] formatters carry their own byte-exact
/// escaping and are intentionally not routed through here.
pub(crate) fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(ch),
        }
    }
    out
}
