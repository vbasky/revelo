//! Text/subtitle format parsers (SubRip, SSA/ASS, TTML, Kate, etc.).

#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

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
pub mod ttml;
pub mod teletext;
pub mod scc;
pub mod timed_text;
pub mod webvtt;

pub use ttml::parse_ttml;
pub use teletext::parse_teletext;
pub use scc::parse_scc;
pub use timed_text::parse_timed_text;
pub use webvtt::parse_webvtt;

