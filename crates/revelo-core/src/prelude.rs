//! Convenience re-exports of the most commonly used types.
//!
//! ```ignore
//! use revelo_core::prelude::*;
//! ```

pub use crate::file_analyze::FileAnalyze;
pub use crate::file_level::{FileLevelInfo, fill_file_level_fields};
pub use crate::reader::Reader;
pub use crate::stream::{Stream, StreamCollection, StreamKind};
