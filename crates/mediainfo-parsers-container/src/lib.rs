//! Container parsers: RIFF/WAV, MKV/WebM, MP4/MOV, MPEG-TS, AVI, etc.

#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

pub mod aiff;
pub mod mkv;
pub mod mp4;
pub mod wav;

pub use aiff::parse_aiff;
pub use mkv::parse_mkv;
pub use mp4::parse_mp4;
pub use wav::parse_wav;
