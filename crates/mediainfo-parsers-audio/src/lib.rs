//! Audio codec parsers: FLAC, MP3, AAC, AC3, DTS, etc.

#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

pub mod aac_adts;
pub mod ac3;
pub mod adpcm;
pub mod als;
pub mod amr;
pub mod ape;
pub mod au;
pub mod caf;
pub mod dat;
pub mod dsdiff;
pub mod dsf;
pub mod dts;
pub mod extended_module;
pub mod flac;
pub mod la;
pub mod mp3;
pub mod mpc;
pub mod speex;
pub mod tta;
pub mod twin_vq;
pub mod wvpk;

pub use aac_adts::parse_aac_adts;
pub use ac3::parse_ac3;
pub use adpcm::parse_adpcm;
pub use als::parse_als;
pub use amr::parse_amr;
pub use ape::parse_ape;
pub use au::parse_au;
pub use caf::parse_caf;
pub use dat::parse_dat;
pub use dsdiff::parse_dsdiff;
pub use dsf::parse_dsf;
pub use dts::parse_dts;
pub use extended_module::parse_extended_module;
pub use flac::parse_flac;
pub use la::parse_la;
pub use mp3::parse_mp3;
pub use mpc::parse_mpc;
pub use speex::parse_speex;
pub use tta::parse_tta;
pub use twin_vq::parse_twin_vq;
pub use wvpk::parse_wvpk;
