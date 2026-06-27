//! Container-format parsers for the [revelo](https://github.com/vbasky/revelo)
//! media-analysis library.
//!
//! Each public function in this crate has the signature
//! `fn(&mut FileAnalyze) -> bool`. It inspects the byte content of a file,
//! returns `false` if the format is not recognised, or fills the
//! [`FileAnalyze`](revelo_core::FileAnalyze) stream graph with codec, timing,
//! track, and metadata fields and returns `true`.
//!
//! # Supported formats
//!
//! | Function | Format |
//! |---|---|
//! | [`parse_mp4`] | MP4 / MOV / QuickTime (ISO Base Media) |
//! | [`parse_mkv`] | Matroska / WebM (EBML) |
//! | [`parse_mpeg_ts`] | MPEG Transport Stream (TS / M2TS) |
//! | [`parse_mpeg_ps`] | MPEG Program Stream (VOB, SVCD) |
//! | [`parse_avi`] | AVI / RIFF |
//! | [`parse_wav`] | WAV / RIFF audio |
//! | [`parse_aiff`] | AIFF / AIFF-C |
//! | [`parse_ogg`] | Ogg (Vorbis, FLAC, Opus, Theora, …) |
//! | [`parse_flv`] | Flash Video (FLV, F4V) |
//! | [`parse_rm`] | RealMedia (RM / RMVB) |
//! | [`parse_wm`] | Windows Media (WMV / WMA / ASF) |
//! | [`parse_wtv`] | Windows Recorded TV (WTV) |
//! | [`parse_mxf`] | Material Exchange Format (MXF, SMPTE 377) |
//! | [`parse_gxf`] | General eXchange Format (GXF, SMPTE 360) |
//! | [`parse_lxf`] | Leitch/Harris eXchange Format (LXF) |
//! | [`parse_nsv`] | Nullsoft Streaming Video (NSV) |
//! | [`parse_nut`] | NUT multimedia container |
//! | [`parse_ivf`] | IVF (VP8/VP9/AV1 raw bitstream container) |
//! | [`parse_swf`] | Adobe/Macromedia SWF |
//! | [`parse_aaf`] | Advanced Authoring Format (AAF) |
//! | [`parse_amv`] | AMV video |
//! | [`parse_dpg`] | DPG video (Nintendo DS) |
//! | [`parse_pmp`] | PMP video (PSP) |
//! | [`parse_skm`] | SKM video (Samsung) |
//! | [`parse_ptx`] | PTX video |
//! | [`parse_dxw`] | DXW video |
//! | [`parse_dv_dif`] | DV / DIF raw bitstream |
//! | [`parse_cdxa`] | CD-XA / CDROM/XA sectors |
//! | [`parse_vbi`] | VBI data |
//! | [`parse_ibi`] | Index-Byte Information (IBI) |
//! | [`parse_bdmv`] | Blu-ray BDMV / BDAV playlist |
//! | [`parse_dvdv`] | DVD Video |
//! | [`parse_hls`] | HLS / M3U8 playlist |
//! | [`parse_dash_mpd`] | MPEG-DASH MPD manifest |
//! | [`parse_hds_f4m`] | Adobe HDS F4M manifest |
//! | [`parse_ism`] | Smooth Streaming ISM manifest |
//! | [`parse_scte35`] | SCTE-35 splice-info section |
//! | [`parse_sequence_info`] | Sequence-info sidecar |
//! | [`parse_dcp_am`] | DCP Asset Map (digital cinema) |
//! | [`parse_dcp_cpl`] | DCP Composition Playlist (digital cinema) |
//! | [`parse_dcp_pkl`] | DCP Packing List (digital cinema) |
//! | [`parse_p2_clip`] | Panasonic P2 clip XML |
//! | [`parse_xdcam_clip`] | Sony XDCAM clip XML |
//! | [`parse_mi_xml`] | MediaInfoLib XML re-import |
//!
//! # Normal use
//!
//! Prefer the [`revelo`](https://crates.io/crates/revelo) facade, which
//! auto-detects formats and dispatches to the correct parser:
//!
//! ```no_run
//! use revelo_parsers_container::parse_mp4;
//! use revelo_core::FileAnalyze;
//!
//! let data = std::fs::read("video.mp4").unwrap();
//! let mut fa = FileAnalyze::new(&data);
//! if parse_mp4(&mut fa) {
//!     // fa contains General, Video, Audio, … streams
//! }
//! ```
//!
//! # Safety
//!
//! `#![deny(unsafe_code)]` — zero unsafe blocks in this crate.

#![allow(non_snake_case)]
#![deny(unsafe_code)]

pub mod aaf;
pub mod aiff;
pub mod amv;
pub mod avi;
pub mod bdmv;
pub mod cdxa;
pub mod dash_mpd;
pub mod dcp_am;
pub mod dcp_cpl;
pub mod dcp_pkl;
pub mod dpg;
pub mod dv_dif;
pub mod dvdv;
pub mod dxw;
pub mod flv;
pub mod gxf;
pub mod hds_f4m;
pub mod hls;
pub mod ibi;
pub mod ism;
pub mod ivf;
pub mod lxf;
pub mod mi_xml;
pub mod mkv;
pub mod mp4;
pub mod mpeg_ps;
pub mod mpeg_ts;
pub mod mxf;
pub mod nsv;
pub mod nut;
pub mod ogg;
pub mod p2_clip;
pub mod pmp;
pub mod ptx;
pub mod rm;
pub mod scte35;
pub mod sequence_info;
pub mod skm;
pub mod swf;
pub mod vbi;
pub mod wav;
pub mod wm;
pub mod wtv;
pub mod xdcam_clip;

pub use aaf::parse_aaf;
pub use aiff::parse_aiff;
pub use amv::parse_amv;
pub use avi::parse_avi;
pub use bdmv::parse_bdmv;
pub use cdxa::parse_cdxa;
pub use dash_mpd::parse_dash_mpd;
pub use dcp_am::parse_dcp_am;
pub use dcp_cpl::parse_dcp_cpl;
pub use dcp_pkl::parse_dcp_pkl;
pub use dpg::parse_dpg;
pub use dv_dif::parse_dv_dif;
pub use dvdv::parse_dvdv;
pub use dxw::parse_dxw;
pub use flv::parse_flv;
pub use gxf::parse_gxf;
pub use hds_f4m::parse_hds_f4m;
pub use hls::parse_hls;
pub use ibi::parse_ibi;
pub use ism::parse_ism;
pub use ivf::parse_ivf;
pub use lxf::parse_lxf;
pub use mi_xml::parse_mi_xml;
pub use mkv::parse_mkv;
pub use mp4::parse_mp4;
pub use mpeg_ps::parse_mpeg_ps;
pub use mpeg_ts::parse_mpeg_ts;
pub use mxf::parse_mxf;
pub use nsv::parse_nsv;
pub use nut::parse_nut;
pub use ogg::parse_ogg;
pub use p2_clip::parse_p2_clip;
pub use pmp::parse_pmp;
pub use ptx::parse_ptx;
pub use rm::parse_rm;
pub use scte35::parse_scte35;
pub use sequence_info::parse_sequence_info;
pub use skm::parse_skm;
pub use swf::parse_swf;
pub use vbi::parse_vbi;
pub use wav::parse_wav;
pub use wm::parse_wm;
pub use wtv::parse_wtv;
pub use xdcam_clip::parse_xdcam_clip;

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
    fn container_parsers_do_not_reintroduce_full_raw_scans() {
        assert_no_full_raw_scan_patterns(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("src").as_path(),
        );
    }
}
