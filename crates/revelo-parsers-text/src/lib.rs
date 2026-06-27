//! Text and subtitle format parsers for the
//! [revelo](https://github.com/vbasky/revelo) media-analysis library.
//!
//! Each public function has the signature `fn(&mut FileAnalyze) -> bool`.
//! It inspects the byte content of a stream, returns `false` if the format
//! is not recognised, or fills the
//! [`FileAnalyze`](revelo_core::FileAnalyze) stream graph with format,
//! encoding, and timing fields and returns `true`.
//!
//! # Supported formats
//!
//! | Function | Format |
//! |---|---|
//! | [`parse_sub_rip`] | SubRip (SRT) — plain-text timed subtitles |
//! | [`parse_webvtt`] | WebVTT — W3C Web Video Text Tracks |
//! | [`parse_ttml`] | TTML / DFXP — Timed Text Markup Language |
//! | [`parse_timed_text`] | 3GPP Timed Text (TX3G / QuickTime tx3g) |
//! | [`parse_cmml`] | CMML — Continuous Media Markup Language |
//! | [`parse_kate`] | Kate — Ogg Kate overlay stream |
//! | [`parse_pgs`] | PGS — Presentation Graphic Stream (Blu-ray) |
//! | [`parse_dvb_subtitle`] | DVB subtitle (ETSI EN 300 743) |
//! | [`parse_teletext`] | Teletext subtitle (ETSI EN 300 472) |
//! | [`parse_arib_std_b24_b37`] | ARIB STD-B24 / STD-B37 (Japanese digital broadcast) |
//! | [`parse_eia608`] | EIA-608 / CEA-608 (Line 21 closed captions) |
//! | [`parse_eia708`] | EIA-708 / CEA-708 (DTV closed captions) |
//! | [`parse_dtvcc_transport`] | DTVCC transport layer (CEA-708 packetisation) |
//! | [`parse_scc`] | SCC — Scenarist Closed Captions |
//! | [`parse_scte20`] | SCTE-20 — closed captions in MPEG-2 user data |
//! | [`parse_cdp`] | CDP — Caption Distribution Packet (SMPTE ST 334) |
//! | [`parse_n19`] | N19 / STL — EBU Subtitling Data Exchange Format |
//! | [`parse_pac`] | PAC — Cheetah caption file |
//! | [`parse_pdf`] | PDF — format identification for PDF documents |
//! | [`parse_sdp`] | SDP — Session Description Protocol |
//! | [`parse_other_text`] | Generic text-stream fallback |
//!
//! # Normal use
//!
//! Prefer the [`revelo`](https://crates.io/crates/revelo) facade, which
//! auto-detects formats and dispatches to the correct parser:
//!
//! ```no_run
//! use revelo_parsers_text::parse_sub_rip;
//! use revelo_core::FileAnalyze;
//!
//! let data = std::fs::read("subtitles.srt").unwrap();
//! let mut fa = FileAnalyze::new(&data);
//! if parse_sub_rip(&mut fa) {
//!     // fa contains a Text stream with format and encoding fields
//! }
//! ```
//!
//! # Safety
//!
//! `#![deny(unsafe_code)]` — zero unsafe blocks in this crate.

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

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    fn assert_no_full_raw_scan_patterns(src_dir: &Path) {
        for entry in fs::read_dir(src_dir).expect("read parser src dir") {
            let path = entry.expect("read parser src entry").path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
                continue;
            }

            let source = fs::read_to_string(&path).expect("read parser source");
            for forbidden in [
                concat!("peek_raw(", "fa.remain())"),
                concat!("read_raw(", "fa.remain())"),
                concat!("peek_raw(", "remain)"),
                concat!("read_raw(", "remain)"),
                concat!("peek_raw(", "total)"),
                concat!("read_raw(", "total)"),
                concat!("peek_raw(", "file_size)"),
                concat!("read_raw(", "file_size)"),
                concat!("peek_raw(", "avail)"),
                concat!("read_raw(", "avail)"),
                concat!("peek_raw_at(0, ", "fa.element_size())"),
            ] {
                assert!(
                    !source.contains(forbidden),
                    "{} contains forbidden full-scan pattern: {forbidden}",
                    path.display()
                );
            }
        }
    }

    #[test]
    fn text_parsers_do_not_reintroduce_full_raw_scans() {
        assert_no_full_raw_scan_patterns(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("src").as_path(),
        );
    }
}
