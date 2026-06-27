//! Video-codec parsers for the revelo media-analysis library.
//!
//! This crate provides a parser for every video codec and related metadata
//! stream that revelo understands. Each parser follows the same contract:
//!
//! ```text
//! fn parse_<codec>(fa: &mut FileAnalyze) -> bool
//! ```
//!
//! The function inspects the byte buffer held by `fa`, returns `false` if the
//! data does not match, or fills the `Video`/`General` stream fields and
//! returns `true` on success. Parsers are registered in the revelo dispatcher
//! inside `revelo-core`; application code does not call them directly.
//!
//! # Normal usage
//!
//! Use the [`revelo`](https://crates.io/crates/revelo) facade crate rather
//! than depending on this crate directly:
//!
//! ```ignore
//! // In your Cargo.toml: revelo = "0.4"
//! use revelo::Metadata;
//!
//! let meta = Metadata::from_file("video.mkv").unwrap();
//! for (key, value) in meta.video() {
//!     println!("{key} = {value}");
//! }
//! ```
//!
//! # Parsers
//!
//! The full list of re-exported parser functions covers:
//!
//! - **Modern standards:** `parse_avc` / `parse_avc_sps`, `parse_hevc` /
//!   `parse_hevc_sps`, `parse_av1` / `parse_av1_from_codec_config`,
//!   `parse_vvc`, `parse_apv`
//! - **MPEG legacy:** `parse_mpeg2` / `parse_mpeg2_sequence_header`,
//!   `parse_mpeg4v`, `parse_h263`
//! - **Chinese standards:** `parse_avs`, `parse_avs3`
//! - **VP family:** `parse_vp8`, `parse_vp9` / `parse_vp9_codec_config`,
//!   `parse_theora`
//! - **VC-1:** `parse_vc1` / `parse_vc1_sequence_header` /
//!   `parse_vc1_codec_private`
//! - **Professional / intermediate:** `parse_prores`, `parse_vc3`,
//!   `parse_cineform`, `parse_canopus`, `parse_aic`, `parse_ffv1`,
//!   `parse_huffyuv`, `parse_lagarith`, `parse_dirac`, `parse_fraps`
//! - **Animation / legacy:** `parse_flic`, `parse_y4m`
//! - **HDR metadata:** `parse_dolby_vision`, `parse_dv_rpu`,
//!   `parse_hdr_vivid`, `parse_afd_bar_data`
//!
//! Several parsers also expose structured info types (`AvcInfo`, `HevcInfo`,
//! `Av1Info`, `Mpeg2Info`, `ProResInfo`, `Vc1Info`, `VvcInfo`,
//! `DolbyVisionRpuInfo`) and utility functions
//! (`extract_encoder_from_avc_sei_nalus`, `extract_encoder_from_sei_nalus`,
//! `gop_detect`, `fill_mpeg2_streams`, `fill_vc1_streams`,
//! `fill_dv_rpu_fields`, `parse_x264_style_encoder`) used by container
//! parsers elsewhere in the workspace.
//!
//! # Design
//!
//! All code in this crate is `#[deny(unsafe_code)]`. There are no system
//! library dependencies and no C FFI. The `FileAnalyze` type lives in
//! `revelo-core`.

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
    fn video_parsers_do_not_reintroduce_full_raw_scans() {
        assert_no_full_raw_scan_patterns(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("src").as_path(),
        );
    }
}
