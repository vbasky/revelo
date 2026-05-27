//! Audio codec parsers: FLAC, MP3, AAC, AC3, DTS, etc.

#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

pub mod ac3;
pub mod flac;
pub mod mp3;

pub use ac3::parse_ac3;
pub use flac::parse_flac;
pub use mp3::parse_mp3;
