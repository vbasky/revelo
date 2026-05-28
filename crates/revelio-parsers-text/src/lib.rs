//! Text/subtitle format parsers (SubRip, SSA/ASS, TTML, Kate, etc.).

#![allow(non_snake_case)]
#![deny(unsafe_code)]

pub mod arib_std_b24_b37;
pub use arib_std_b24_b37::parse_arib_std_b24_b37;
pub mod cdp;
pub use cdp::parse_cdp;
pub mod cmml;
pub use cmml::parse_cmml;
pub mod dvb_subtitle;
pub use dvb_subtitle::parse_dvb_subtitle;
pub mod eia608;
pub use eia608::parse_eia608;
pub mod eia708;
pub use eia708::parse_eia708;
pub mod kate;
pub use kate::parse_kate;
pub mod n19;
pub use n19::parse_n19;
pub mod other_text;
pub use other_text::parse_other_text;
pub mod pgs;
pub use pgs::parse_pgs;
pub mod sub_rip;
pub use sub_rip::parse_sub_rip;
pub mod dtvcc_transport;
pub mod pac;
pub mod pdf;
pub mod scc;
pub mod scte20;
pub mod sdp;
pub mod teletext;
pub mod timed_text;
pub mod ttml;
pub mod webvtt;

pub use dtvcc_transport::parse_dtvcc_transport;
pub use pac::parse_pac;
pub use pdf::parse_pdf;
pub use scc::parse_scc;
pub use scte20::parse_scte20;
pub use sdp::parse_sdp;
pub use teletext::parse_teletext;
pub use timed_text::parse_timed_text;
pub use ttml::parse_ttml;
pub use webvtt::parse_webvtt;
