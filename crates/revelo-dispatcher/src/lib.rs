//! Parser dispatch table — single source of truth.
//!
//! Both `revelo-cli` and `revelo-cdylib` depend on this crate.
//!
//! [`table`] returns the ordered array of parser function pointers.
//! [`detect`] races them in parallel via [`rayon`] and returns the first
//! (in table order) that recognizes a buffer. The caller then runs the winner
//! against a fresh [`FileAnalyze`] to extract full metadata.

#![deny(unsafe_code)]

use rayon::prelude::*;
use revelo_core::FileAnalyze;

use revelo_parsers_archive::{
    parse_7z, parse_ace, parse_bzip2, parse_elf, parse_gzip, parse_iso9660, parse_mach_o,
    parse_mz_exe, parse_rar, parse_tar, parse_zip,
};
use revelo_parsers_audio::{
    parse_aac, parse_aac_adts, parse_ac3, parse_ac4, parse_adm, parse_adpcm, parse_als, parse_amr,
    parse_ape, parse_aptx100, parse_au, parse_caf, parse_celt, parse_channel_grouping,
    parse_channel_splitting, parse_dat, parse_dolby_audio_metadata, parse_dolby_e, parse_dsdiff,
    parse_dsf, parse_dts, parse_dts_uhd, parse_extended_module, parse_flac, parse_iab, parse_iamf,
    parse_impulse_tracker, parse_la, parse_mga, parse_midi, parse_module, parse_mp3, parse_mpc,
    parse_mpc_sv8, parse_mpegh3da, parse_open_mg, parse_opus, parse_pcm, parse_pcm_m2ts,
    parse_pcm_vob, parse_ps2_audio, parse_rkau, parse_scream_tracker3, parse_smpte_st0302,
    parse_smpte_st0331, parse_smpte_st0337, parse_speex, parse_tak, parse_truehd, parse_tta,
    parse_twin_vq, parse_usac, parse_vorbis, parse_wvpk,
};
use revelo_parsers_container::{
    parse_aaf, parse_aiff, parse_amv, parse_avi, parse_bdmv, parse_cdxa, parse_dash_mpd,
    parse_dcp_am, parse_dcp_cpl, parse_dcp_pkl, parse_dpg, parse_dv_dif, parse_dvdv, parse_dxw,
    parse_flv, parse_gxf, parse_hds_f4m, parse_hls, parse_ibi, parse_ism, parse_ivf, parse_lxf,
    parse_mi_xml, parse_mkv, parse_mp4, parse_mpeg_ps, parse_mpeg_ts, parse_mxf, parse_nsv,
    parse_nut, parse_ogg, parse_p2_clip, parse_pmp, parse_ptx, parse_rm, parse_scte35,
    parse_sequence_info, parse_skm, parse_swf, parse_vbi, parse_wav, parse_wm, parse_wtv,
    parse_xdcam_clip,
};
use revelo_parsers_image::{
    parse_amiga_icon, parse_arriraw, parse_bmp, parse_bpg, parse_dds, parse_dpx, parse_exr,
    parse_gain_map, parse_gif, parse_ico, parse_jp2, parse_jpeg, parse_pcx, parse_png, parse_psd,
    parse_rle, parse_tga, parse_tiff, parse_webp,
};
use revelo_parsers_text::{
    parse_arib_std_b24_b37, parse_cdp, parse_cmml, parse_dtvcc_transport, parse_dvb_subtitle,
    parse_eia608, parse_eia708, parse_kate, parse_n19, parse_other_text, parse_pac, parse_pdf,
    parse_pgs, parse_scc, parse_scte20, parse_sdp, parse_sub_rip, parse_teletext, parse_timed_text,
    parse_ttml, parse_webvtt,
};
use revelo_parsers_video::{
    parse_afd_bar_data, parse_aic, parse_apv, parse_av1, parse_avc, parse_avs, parse_avs3,
    parse_canopus, parse_cineform, parse_dirac, parse_dolby_vision, parse_ffv1, parse_flic,
    parse_fraps, parse_h263, parse_hdr_vivid, parse_hevc, parse_huffyuv, parse_lagarith,
    parse_mpeg2, parse_mpeg4v, parse_prores, parse_theora, parse_vc1, parse_vc3, parse_vp8,
    parse_vp9, parse_vvc, parse_y4m,
};

/// Returns the complete parser dispatch table (178 entries).
///
/// Ordering: containers first (header peek → sub-parser delegation),
/// then video codecs, audio codecs, images, text, and archives.
/// Container-vs-elementary ordering matters — a raw codec parser
/// running before a container could false-match on random bytes.
pub fn table() -> [fn(&mut FileAnalyze) -> bool; 178] {
    [
        // ── Containers ──────────────────────────────────────────
        parse_wav,           // WAV
        parse_avi,           // AVI
        parse_cdxa,          // CD-XA
        parse_amv,           // AMV
        parse_webp,          // WebP
        parse_jp2,           // JPEG 2000
        parse_aiff,          // AIFF
        parse_flac,          // FLAC
        parse_dsdiff,        // DSDIFF
        parse_caf,           // CAF
        parse_mp4,           // MP4
        parse_mkv,           // Matroska
        parse_ogg,           // Ogg
        parse_mpeg_ts,       // MPEG-TS
        parse_mpeg_ps,       // MPEG-PS
        parse_swf,           // SWF
        parse_skm,           // SKM
        parse_dpg,           // DPG
        parse_hds_f4m,       // HDS F4M
        parse_hls,           // HLS
        parse_dash_mpd,      // DASH MPD
        parse_dcp_am,        // DCP AM
        parse_dcp_cpl,       // DCP CPL
        parse_dcp_pkl,       // DCP PKL
        parse_ibi,           // IBI
        parse_dxw,           // DXW
        parse_aaf,           // AAF
        parse_mxf,           // MXF
        parse_bdmv,          // BDMV
        parse_dvdv,          // DVD-Video
        parse_dv_dif,        // DV
        parse_flv,           // FLV
        parse_lxf,           // LXF
        parse_nut,           // NUT
        parse_wm,            // Windows Media
        parse_wtv,           // WTV
        parse_rm,            // RealMedia
        parse_ivf,           // IVF
        parse_ism,           // ISM
        parse_mi_xml,        // MI XML
        parse_p2_clip,       // P2 Clip
        parse_xdcam_clip,    // XDCAM Clip
        parse_sequence_info, // Sequence Info
        parse_ptx,           // PTX
        parse_nsv,           // NSV
        parse_pmp,           // PMP
        parse_gxf,           // GXF
        // ── Subtitles / Text ────────────────────────────────────
        parse_scte35,           // SCTE 35
        parse_cdp,              // CDP
        parse_pgs,              // PGS
        parse_dvb_subtitle,     // DVB Subtitle
        parse_arib_std_b24_b37, // ARIB B24/B37
        parse_kate,             // Kate
        parse_cmml,             // CMML
        parse_ttml,             // TTML
        parse_n19,              // N19
        parse_sub_rip,          // SubRip
        parse_other_text,       // Other Text
        // ── Audio ───────────────────────────────────────────────
        parse_dsf, // DSF
        // ── Images ──────────────────────────────────────────────
        parse_png,        // PNG
        parse_jpeg,       // JPEG
        parse_bmp,        // BMP
        parse_gif,        // GIF
        parse_tiff,       // TIFF
        parse_ico,        // ICO
        parse_psd,        // PSD
        parse_dpx,        // DPX
        parse_dds,        // DDS
        parse_exr,        // OpenEXR
        parse_bpg,        // BPG
        parse_pcx,        // PCX
        parse_arriraw,    // ARRIRAW
        parse_amiga_icon, // Amiga Icon
        // ── Video ───────────────────────────────────────────────
        parse_y4m,      // Y4M
        parse_vc1,      // VC-1
        parse_mpeg2,    // MPEG-2 Video
        parse_av1,      // AV1
        parse_apv,      // APV (Advanced Professional Video)
        parse_avc,      // AVC / H.264
        parse_hevc,     // HEVC / H.265
        parse_vp8,      // VP8
        parse_vp9,      // VP9
        parse_theora,   // Theora
        parse_ffv1,     // FFV1
        parse_h263,     // H.263
        parse_mpeg4v,   // MPEG-4 Visual
        parse_aic,      // Apple Intermediate Codec
        parse_avs,      // AVS
        parse_avs3,     // AVS3
        parse_canopus,  // Canopus HQ
        parse_cineform, // CineForm
        parse_dirac,    // Dirac
        parse_flic,     // FLIC
        parse_fraps,    // Fraps
        parse_huffyuv,  // HuffYUV
        parse_lagarith, // Lagarith
        // ── Audio codecs ────────────────────────────────────────
        parse_ac3,               // AC-3
        parse_ac4,               // AC-4
        parse_dts,               // DTS
        parse_dts_uhd,           // DTS-UHD
        parse_aac_adts,          // AAC ADTS
        parse_iab,               // IAB
        parse_iamf,              // IAMF
        parse_als,               // ALS
        parse_ape,               // Monkey's Audio
        parse_au,                // AU
        parse_amr,               // AMR
        parse_speex,             // Speex
        parse_mpc,               // Musepack
        parse_la,                // LA
        parse_tak,               // TAK
        parse_tta,               // True Audio
        parse_wvpk,              // WavPack
        parse_twin_vq,           // TwinVQ
        parse_extended_module,   // Extended Module
        parse_dat,               // DAT
        parse_rkau,              // Rkau
        parse_aptx100,           // aptX 100
        parse_open_mg,           // OpenMG
        parse_midi,              // MIDI
        parse_module,            // Module
        parse_impulse_tracker,   // Impulse Tracker
        parse_scream_tracker3,   // Scream Tracker 3
        parse_mp3,               // MP3
        parse_celt,              // CELT
        parse_dolby_e,           // Dolby E
        parse_mpegh3da,          // MPEG-H 3D Audio
        parse_mpc_sv8,           // Musepack SV8
        parse_ps2_audio,         // PS2 Audio
        parse_truehd,            // TrueHD
        parse_smpte_st0302,      // SMPTE ST 302
        parse_smpte_st0331,      // SMPTE ST 331
        parse_smpte_st0337,      // SMPTE ST 337
        parse_channel_grouping,  // Channel Grouping
        parse_channel_splitting, // Channel Splitting
        // ── Images (fallback / extension) ───────────────────────
        parse_tga,      // TGA
        parse_gain_map, // Gain Map
        parse_rle,      // RLE
        // ── Audio (fallback / extension) ────────────────────────
        parse_adpcm,                // ADPCM
        parse_adm,                  // ADM
        parse_dolby_audio_metadata, // Dolby Audio Metadata
        parse_pcm,                  // PCM
        parse_pcm_vob,              // PCM VOB
        parse_pcm_m2ts,             // PCM M2TS
        parse_mga,                  // MGA
        parse_aac,                  // AAC
        // ── Text (fallback / extension) ─────────────────────────
        parse_pdf,             // PDF
        parse_sdp,             // SDP
        parse_pac,             // PAC
        parse_dtvcc_transport, // DTVCC Transport
        parse_scte20,          // SCTE-20
        parse_eia608,          // EIA-608
        parse_eia708,          // EIA-708
        parse_vbi,             // VBI
        parse_webvtt,          // WebVTT
        // ── Video (fallback / extension) ────────────────────────
        parse_vvc,          // VVC / H.266
        parse_prores,       // ProRes
        parse_vc3,          // VC-3
        parse_dolby_vision, // Dolby Vision
        parse_afd_bar_data, // AFD / Bar Data
        parse_hdr_vivid,    // HDR Vivid
        // ── Audio (late matching) ───────────────────────────────
        parse_opus,   // Opus
        parse_vorbis, // Vorbis
        parse_usac,   // USAC
        // ── Text (late matching) ────────────────────────────────
        parse_teletext,   // Teletext
        parse_scc,        // SCC
        parse_timed_text, // Timed Text
        // ── Archives ────────────────────────────────────────────
        parse_zip,     // ZIP
        parse_rar,     // RAR
        parse_7z,      // 7-Zip
        parse_tar,     // TAR
        parse_gzip,    // Gzip
        parse_bzip2,   // Bzip2
        parse_iso9660, // ISO 9660
        parse_elf,     // ELF
        parse_mach_o,  // Mach-O
        parse_mz_exe,  // MZ EXE
        parse_ace,     // ACE
    ]
}

/// Race every parser across cores and return the first one (in table order)
/// that recognizes `bytes`, or `None` if none match.
///
/// Each candidate runs against a fresh [`FileAnalyze`] over the same buffer —
/// parsers only peek, so this is a cheap detection pass; the caller re-runs the
/// winner to extract full metadata. `find_first` preserves table-order priority
/// (containers before elementary streams; see [`table`]) while still evaluating
/// candidates in parallel.
pub fn detect(bytes: &[u8]) -> Option<fn(&mut FileAnalyze) -> bool> {
    table()
        .par_iter()
        .find_first(|&&parser| {
            let mut fa = FileAnalyze::new(bytes);
            parser(&mut fa)
        })
        .copied()
}
