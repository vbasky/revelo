//! Output formatters — XML (and later JSON, Text, EBUCore, MPEG-7).
//!
//! Targets byte-for-byte compatibility with MediaInfoLib's
//! `--Output=XML` so the diff-harness can do automated line-by-line
//! comparison.

#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

pub mod xml;

pub use xml::to_xml;
