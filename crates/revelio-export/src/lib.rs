//! Output formatters — XML, Text, JSON (and later EBUCore, MPEG-7).
//!
//! Targets byte-for-byte compatibility with MediaInfoLib's output
//! formatters so the diff-harness can do automated comparison.

#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

pub mod xml;
pub mod text;
pub mod json;
pub mod ebu_core;
pub mod mpeg7;
pub mod pbcore;
pub mod niso;
pub mod fims;
pub mod graph;
pub mod revtmd;


pub use xml::to_xml;
pub use text::to_text;
pub use json::to_json;
pub use ebu_core::to_ebu_core;
pub use mpeg7::to_mpeg7;
pub use pbcore::to_pbcore;
pub use niso::to_niso;
pub use fims::to_fims;
pub use graph::to_graph;
pub use revtmd::to_revtmd;

