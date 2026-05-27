//! Container parsers: RIFF/WAV, MKV/WebM, MP4/MOV, MPEG-TS, AVI, etc.

#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

pub mod aiff;
pub mod avi;
pub mod cdxa;
pub mod mkv;
pub mod mp4;
pub mod mpeg_ps;
pub mod mpeg_ts;
pub mod ogg;
pub mod skm;
pub mod swf;
pub mod wav;

pub use aiff::parse_aiff;
pub use avi::parse_avi;
pub use cdxa::parse_cdxa;
pub use mkv::parse_mkv;
pub use mp4::parse_mp4;
pub use mpeg_ps::parse_mpeg_ps;
pub use mpeg_ts::parse_mpeg_ts;
pub use ogg::parse_ogg;
pub use skm::parse_skm;
pub use swf::parse_swf;
pub use wav::parse_wav;
