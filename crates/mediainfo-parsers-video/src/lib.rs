//! Video codec parsers: Theora, VP8, VP9, Y4M, AVC/H.264, HEVC/H.265, AV1, etc.

#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

pub mod avc;
pub mod hevc;
pub mod theora;
pub mod vp8;
pub mod vp9;
pub mod y4m;

pub use avc::parse_avc;
pub use hevc::parse_hevc;
pub use theora::parse_theora;
pub use vp8::parse_vp8;
pub use vp9::{parse_vp9, parse_vp9_codec_config};
pub use y4m::parse_y4m;
