//! Audio-codec parsers for the revelo media-analysis library.
//!
//! This crate provides a parser for every audio codec and format that revelo
//! understands. Each parser follows the same contract:
//!
//! ```text
//! fn parse_<codec>(fa: &mut FileAnalyze) -> bool
//! ```
//!
//! The function inspects the byte buffer held by `fa`, returns `false` if the
//! data does not match, or fills the `Audio`/`General` stream fields and
//! returns `true` on success. Parsers are registered in the revelo dispatcher
//! inside `revelo-core`; application code does not call them directly.
//!
//! # Normal usage
//!
//! Use the [`revelo`](https://crates.io/crates/revelo) facade crate rather
//! than depending on this crate directly:
//!
//! ```ignore
//! // In your Cargo.toml: revelo = "0.5"
//! use revelo::Metadata;
//!
//! let meta = Metadata::from_file("audio.flac").unwrap();
//! for (key, value) in meta.audio() {
//!     println!("{key} = {value}");
//! }
//! ```
//!
//! # Parsers
//!
//! The full list of re-exported parser functions covers:
//!
//! - **Lossy speech/general:** `parse_amr`, `parse_mp3`, `parse_aac`,
//!   `parse_aac_adts`, `parse_speex`, `parse_opus`, `parse_vorbis`,
//!   `parse_celt`, `parse_usac`, `parse_mpegh3da`, `parse_twin_vq`
//! - **Lossless:** `parse_flac`, `parse_ape`, `parse_wvpk`, `parse_tak`,
//!   `parse_tta`, `parse_als`, `parse_la`, `parse_rkau`
//! - **Surround / broadcast:** `parse_ac3`, `parse_ac4`, `parse_truehd`,
//!   `parse_dts`, `parse_dts_uhd`, `parse_dolby_e`,
//!   `parse_dolby_audio_metadata`, `parse_adm`
//! - **Immersive:** `parse_iamf`, `parse_iab`
//! - **PCM variants:** `parse_pcm`, `parse_pcm_m2ts`, `parse_pcm_vob`,
//!   `parse_adpcm`, `parse_au`, `parse_dat`
//! - **Containers / file formats:** `parse_caf`, `parse_dsdiff`, `parse_dsf`,
//!   `parse_mga`, `parse_open_mg`, `parse_ps2_audio`, `parse_aptx100`
//! - **SMPTE standards:** `parse_smpte_st0302`, `parse_smpte_st0331`,
//!   `parse_smpte_st0337`
//! - **Tracker music:** `parse_module`, `parse_extended_module`,
//!   `parse_impulse_tracker`, `parse_scream_tracker3`, `parse_midi`
//! - **Musepack:** `parse_mpc`, `parse_mpc_sv8`
//! - **Utilities:** `extract_replay_gain`, `fill_id3_replay_gain`,
//!   `parse_channel_grouping`, `parse_channel_splitting`
//!
//! # Design
//!
//! All code in this crate is `#[deny(unsafe_code)]`. There are no system
//! library dependencies and no C FFI. The `FileAnalyze` type lives in
//! `revelo-core`.

#![allow(non_snake_case)]
#![deny(unsafe_code)]

pub mod aac;
pub mod aac_adts;
pub mod ac3;
pub mod ac4;
pub mod adm;
pub mod adpcm;
pub mod als;
pub mod amr;
pub mod ape;
pub mod aptx100;
pub mod au;
pub mod caf;
pub mod celt;
pub mod channel_grouping;
pub mod channel_splitting;
pub mod dat;
pub mod dolby_audio_metadata;
pub mod dolby_e;
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
pub mod mga;
pub mod midi;
pub mod module;
pub mod mp3;
pub mod mpc;
pub mod mpc_sv8;
pub mod mpegh3da;
pub mod open_mg;
pub mod pcm;
pub mod pcm_m2ts;
pub mod pcm_vob;
pub mod ps2_audio;
pub mod replay_gain;
pub mod rkau;
pub mod scream_tracker3;
pub mod smpte_st0302;
pub mod smpte_st0331;
pub mod smpte_st0337;
pub mod speex;
pub mod tak;
pub mod truehd;
pub mod tta;
pub mod twin_vq;

pub mod opus;
pub mod usac;
pub mod vorbis;
pub mod wvpk;

pub use aac::parse_aac;
pub use aac_adts::parse_aac_adts;
pub use ac3::parse_ac3;
pub use ac4::parse_ac4;
pub use adm::parse_adm;
pub use adpcm::parse_adpcm;
pub use als::parse_als;
pub use amr::parse_amr;
pub use ape::parse_ape;
pub use aptx100::parse_aptx100;
pub use au::parse_au;
pub use caf::parse_caf;
pub use celt::parse_celt;
pub use channel_grouping::parse_channel_grouping;
pub use channel_splitting::parse_channel_splitting;
pub use dat::parse_dat;
pub use dolby_audio_metadata::parse_dolby_audio_metadata;
pub use dolby_e::parse_dolby_e;
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
pub use mga::parse_mga;
pub use midi::parse_midi;
pub use module::parse_module;
pub use mp3::parse_mp3;
pub use mpc::parse_mpc;
pub use mpc_sv8::parse_mpc_sv8;
pub use mpegh3da::parse_mpegh3da;
pub use open_mg::parse_open_mg;
pub use opus::parse_opus;
pub use pcm::parse_pcm;
pub use pcm_m2ts::parse_pcm_m2ts;
pub use pcm_vob::parse_pcm_vob;
pub use ps2_audio::parse_ps2_audio;
pub use replay_gain::{extract_replay_gain, fill_id3_replay_gain};
pub use rkau::parse_rkau;
pub use scream_tracker3::parse_scream_tracker3;
pub use smpte_st0302::parse_smpte_st0302;
pub use smpte_st0331::parse_smpte_st0331;
pub use smpte_st0337::parse_smpte_st0337;
pub use speex::parse_speex;
pub use tak::parse_tak;
pub use truehd::parse_truehd;
pub use tta::parse_tta;
pub use twin_vq::parse_twin_vq;
pub use usac::parse_usac;
pub use vorbis::parse_vorbis;
pub use wvpk::parse_wvpk;

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
    fn audio_parsers_do_not_reintroduce_full_raw_scans() {
        assert_no_full_raw_scan_patterns(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("src").as_path(),
        );
    }
}
