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
pub mod vvc;
pub mod prores;
pub mod vc3;
pub mod dolby_vision;
pub mod ffv1;
pub mod canopus;
pub mod cineform;
pub mod fraps;
pub mod flic;
pub mod huffyuv;
pub mod lagarith;
pub mod avs;
pub mod avs3;
pub mod dirac;
pub mod hdr_vivid;
pub mod aic;
pub mod afd_bar_data;

pub mod h263;
pub mod mpeg4v;

pub use avc::{parse_avc, parse_sps as parse_avc_sps, AvcInfo, EncoderInfo, parse_x264_style_encoder, gop_detect};
pub use hevc::{parse_hevc, parse_hevc_sps, extract_encoder_from_sei_nalus, HevcInfo};
pub use theora::parse_theora;
pub use vp8::parse_vp8;
pub use vp9::{parse_vp9, parse_vp9_codec_config};
pub use y4m::parse_y4m;
pub use av1::{parse_av1, parse_av1_from_codec_config, Av1Info};
pub use vc1::{parse_vc1, parse_vc1_sequence_header, parse_vc1_codec_private, fill_vc1_streams, Vc1Info, Vc1Profile, Vc1Level};
pub use mpeg2::{parse_mpeg2, parse_mpeg2_sequence_header, fill_mpeg2_streams, Mpeg2Info, Mpeg2Profile, Mpeg2Level};
pub use vvc::{parse_vvc, VvcInfo};
pub use prores::{parse_prores, ProResInfo};
pub use vc3::parse_vc3;
pub use dolby_vision::parse_dolby_vision;
pub use canopus::parse_canopus;
pub use cineform::parse_cineform;
pub use fraps::parse_fraps;
pub use flic::parse_flic;
pub use huffyuv::parse_huffyuv;
pub use lagarith::parse_lagarith;
pub use avs::parse_avs;
pub use avs3::parse_avs3;
pub use dirac::parse_dirac;
pub use hdr_vivid::parse_hdr_vivid;
pub use aic::parse_aic;
pub use afd_bar_data::parse_afd_bar_data;
pub use ffv1::parse_ffv1;
pub use h263::parse_h263;
pub use mpeg4v::parse_mpeg4v;
