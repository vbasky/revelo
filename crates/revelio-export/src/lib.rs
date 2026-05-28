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

pub use xml::to_xml;
pub use text::to_text;
pub use json::to_json;
