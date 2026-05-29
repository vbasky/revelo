//! Video codec parsers: Theora, VP8, VP9, Y4M, AVC/H.264, HEVC/H.265, AV1, etc.

#![allow(non_snake_case)]
#![deny(unsafe_code)]

pub mod afd_bar_data;
pub mod aic;
pub mod apv;
pub mod av1;
pub mod avc;
pub mod avs;
pub mod avs3;
pub mod canopus;
pub mod cineform;
pub mod dirac;
pub mod dolby_vision;
pub mod dolby_vision_rpu;
pub mod ffv1;
pub mod flic;
pub mod fraps;
pub mod hdr_vivid;
pub mod hevc;
pub mod huffyuv;
pub mod lagarith;
pub mod mpeg2;
pub mod prores;
pub mod theora;
pub mod vc1;
pub mod vc3;
pub mod vp8;
pub mod vp9;
pub mod vvc;
pub mod y4m;

pub mod h263;
pub mod mpeg4v;

pub use afd_bar_data::parse_afd_bar_data;
pub use aic::parse_aic;
pub use apv::parse_apv;
pub use av1::{Av1Info, parse_av1, parse_av1_from_codec_config};
pub use avc::{
    AvcInfo, EncoderInfo, extract_encoder_from_avc_sei_nalus, gop_detect, parse_avc,
    parse_sps as parse_avc_sps, parse_x264_style_encoder,
};
pub use avs::parse_avs;
pub use avs3::parse_avs3;
pub use canopus::parse_canopus;
pub use cineform::parse_cineform;
pub use dirac::parse_dirac;
pub use dolby_vision::parse_dolby_vision;
pub use dolby_vision_rpu::{DolbyVisionRpuInfo, fill_dv_rpu_fields, parse_dv_rpu};
pub use ffv1::parse_ffv1;
pub use flic::parse_flic;
pub use fraps::parse_fraps;
pub use h263::parse_h263;
pub use hdr_vivid::parse_hdr_vivid;
pub use hevc::{HevcInfo, extract_encoder_from_sei_nalus, parse_hevc, parse_hevc_sps};
pub use huffyuv::parse_huffyuv;
pub use lagarith::parse_lagarith;
pub use mpeg2::{
    Mpeg2Info, Mpeg2Level, Mpeg2Profile, fill_mpeg2_streams, parse_mpeg2,
    parse_mpeg2_sequence_header,
};
pub use mpeg4v::parse_mpeg4v;
pub use prores::{ProResInfo, parse_prores};
pub use theora::parse_theora;
pub use vc1::{
    Vc1Info, Vc1Level, Vc1Profile, fill_vc1_streams, parse_vc1, parse_vc1_codec_private,
    parse_vc1_sequence_header,
};
pub use vc3::parse_vc3;
pub use vp8::parse_vp8;
pub use vp9::{parse_vp9, parse_vp9_codec_config};
pub use vvc::{VvcInfo, parse_vvc};
pub use y4m::parse_y4m;
