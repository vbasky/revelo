//! Output formatters — XML, Text, JSON (and later EBUCore, MPEG-7).
//!
//! Targets byte-for-byte compatibility with MediaInfoLib's output
//! formatters so the revelo-diff can do automated comparison.

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
