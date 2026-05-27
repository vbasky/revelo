//! Container parsers: RIFF/WAV, MKV/WebM, MP4/MOV, MPEG-TS, AVI, etc.

#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

pub mod aiff;
pub mod amv;
pub mod avi;
pub mod cdxa;
pub mod dash_mpd;
pub mod dcp_am;
pub mod dcp_cpl;
pub mod dpg;
pub mod hds_f4m;
pub mod hls;
pub mod ibi;
pub mod mkv;
pub mod mp4;
pub mod mpeg_ps;
pub mod mpeg_ts;
pub mod ogg;
pub mod skm;
pub mod swf;
pub mod wav;

pub use aiff::parse_aiff;
pub use amv::parse_amv;
pub use avi::parse_avi;
pub use cdxa::parse_cdxa;
pub use dash_mpd::parse_dash_mpd;
pub use dcp_am::parse_dcp_am;
pub use dcp_cpl::parse_dcp_cpl;
pub use dpg::parse_dpg;
pub use hds_f4m::parse_hds_f4m;
pub use hls::parse_hls;
pub use ibi::parse_ibi;
pub use mkv::parse_mkv;
pub use mp4::parse_mp4;
pub use mpeg_ps::parse_mpeg_ps;
pub use mpeg_ts::parse_mpeg_ts;
pub use ogg::parse_ogg;
pub use skm::parse_skm;
pub use swf::parse_swf;
pub use wav::parse_wav;
