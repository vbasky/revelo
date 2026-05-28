//! Core of the Rust MediaInfo engine — transliteration of MediaInfoLib's
//! `File__Analyze` infrastructure. Provides the byte-reader surface that
//! every parser uses (`Get_B*`, `Get_L*`, `Peek_*`, `Skip_*`) plus, later,
//! the element tree, stream model, config, and event dispatch.
//!
//! Naming follows the C++ side verbatim. Idiomaticity is sacrificed for
//! 1:1 readability with the upstream parsers.

#![allow(non_snake_case)]

pub mod element;
pub mod file_analyze;
pub mod file_level;
pub mod stream;
mod zenlib_re_export;

pub use element::{ElementInfo, ElementNode, ElementTree};
pub use file_analyze::FileAnalyze;
pub use file_level::{FileLevelInfo, fill_file_level_fields};
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
            self.files.push(ReferenceFile {
                path: path.to_string(),
                format,
                stream_id,
            });
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
pub mod data_helpers;
pub mod config;
pub mod multi_file;
