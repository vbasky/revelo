//! Audio codec parsers: FLAC, MP3, AAC, AC3, DTS, etc.

#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

pub mod aac_adts;
pub mod ac3;
pub mod ac4;
pub mod adpcm;
pub mod als;
pub mod amr;
pub mod ape;
pub mod aptx100;
pub mod au;
pub mod caf;
pub mod dat;
pub mod dsdiff;
pub mod dsf;
pub mod dts;
pub mod dts_uhd;
pub mod extended_module;
pub mod flac;
pub mod iab;
pub mod iamf;
pub mod impulse_tracker;
pub mod la;
pub mod midi;
pub mod module;
pub mod mp3;
pub mod mpc;
pub mod open_mg;
pub mod rkau;
pub mod scream_tracker3;
pub mod speex;
pub mod tak;
pub mod tta;
pub mod twin_vq;
pub mod truehd;
pub mod celt;
pub mod dolby_e;
pub mod mpc_sv8;
pub mod mpegh3da;
pub mod pcm;
pub mod smpte_st0302;
pub mod smpte_st0331;
pub mod smpte_st0337;
pub mod channel_splitting;
pub mod channel_grouping;
pub mod ps2_audio;

pub mod wvpk;
pub mod opus;
pub mod vorbis;
pub mod usac;


pub use aac_adts::parse_aac_adts;
pub use ac3::parse_ac3;
pub use ac4::parse_ac4;
pub use adpcm::parse_adpcm;
pub use als::parse_als;
pub use amr::parse_amr;
pub use ape::parse_ape;
pub use aptx100::parse_aptx100;
pub use au::parse_au;
pub use caf::parse_caf;
pub use dat::parse_dat;
pub use dsdiff::parse_dsdiff;
pub use dsf::parse_dsf;
pub use dts::parse_dts;
pub use dts_uhd::parse_dts_uhd;
pub use extended_module::parse_extended_module;
pub use flac::parse_flac;
pub use iab::parse_iab;
pub use iamf::parse_iamf;
pub use impulse_tracker::parse_impulse_tracker;
pub use la::parse_la;
pub use midi::parse_midi;
pub use module::parse_module;
pub use mp3::parse_mp3;
pub use mpc::parse_mpc;
pub use open_mg::parse_open_mg;
pub use rkau::parse_rkau;
pub use scream_tracker3::parse_scream_tracker3;
pub use speex::parse_speex;
pub use tak::parse_tak;
pub use tta::parse_tta;
pub use twin_vq::parse_twin_vq;
pub use truehd::parse_truehd;
pub use celt::parse_celt;
pub use dolby_e::parse_dolby_e;
pub use mpc_sv8::parse_mpc_sv8;
pub use mpegh3da::parse_mpegh3da;
pub use pcm::parse_pcm;
pub use smpte_st0302::parse_smpte_st0302;
pub use smpte_st0331::parse_smpte_st0331;
pub use smpte_st0337::parse_smpte_st0337;
pub use channel_splitting::parse_channel_splitting;
pub use channel_grouping::parse_channel_grouping;
pub use ps2_audio::parse_ps2_audio;
pub use wvpk::parse_wvpk;
pub use opus::parse_opus;
pub use vorbis::parse_vorbis;
pub use usac::parse_usac;

