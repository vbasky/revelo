//! Core of the Rust MediaInfo engine — transliteration of MediaInfoLib's
//! `File__Analyze` infrastructure. Provides the byte-reader surface that
//! every parser uses (`Get_B*`, `Get_L*`, `Peek_*`, `Skip_*`) plus, later,
//! the element tree, stream model, config, and event dispatch.
//!
//! Naming follows the C++ side verbatim. Idiomaticity is sacrificed for
//! 1:1 readability with the upstream parsers.

#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

pub mod element;
pub mod file_analyze;
pub mod stream;
mod zenlib_re_export;

pub use element::{ElementInfo, ElementNode, ElementTree};
pub use file_analyze::FileAnalyze;
pub use stream::{Stream, StreamCollection, StreamKind};
