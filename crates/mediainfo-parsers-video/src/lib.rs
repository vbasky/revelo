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
pub mod av1;
pub mod vc1;
pub mod mpeg2;

pub use avc::{parse_avc, parse_sps as parse_avc_sps, AvcInfo};
pub use hevc::{parse_hevc, parse_hevc_sps, extract_encoder_from_sei_nalus, HevcInfo};
pub use theora::parse_theora;
pub use vp8::parse_vp8;
pub use vp9::{parse_vp9, parse_vp9_codec_config};
pub use y4m::parse_y4m;
pub use av1::{parse_av1, parse_av1_from_codec_config, Av1Info};
pub use vc1::{parse_vc1, parse_vc1_sequence_header, parse_vc1_codec_private, fill_vc1_streams, Vc1Info, Vc1Profile, Vc1Level};
pub use mpeg2::{parse_mpeg2, parse_mpeg2_sequence_header, fill_mpeg2_streams, Mpeg2Info, Mpeg2Profile, Mpeg2Level};
