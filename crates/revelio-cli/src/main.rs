use std::env;
use std::fs;
use std::path::Path;
use std::process;
use std::time::UNIX_EPOCH;

use revelio_core::{fill_file_level_fields, FileAnalyze, FileLevelInfo};
use revelio_export::{to_xml, to_text, to_json};
use revelio_parsers_audio::{parse_aac_adts, parse_ac3, parse_ac4, parse_adpcm, parse_als, parse_amr,
    parse_ape, parse_aptx100, parse_au, parse_caf, parse_dat, parse_dsdiff, parse_dsf, parse_dts,
    parse_dts_uhd, parse_extended_module, parse_flac, parse_iab, parse_iamf, parse_impulse_tracker,
    parse_la, parse_midi, parse_module, parse_mp3, parse_mpc, parse_open_mg, parse_rkau,
    parse_scream_tracker3, parse_speex, parse_tak, parse_tta, parse_twin_vq, parse_wvpk, parse_opus, parse_vorbis, parse_usac};
use revelio_parsers_container::{parse_aaf, parse_aiff, parse_amv, parse_avi, parse_bdmv,
    parse_cdxa, parse_dash_mpd, parse_dcp_am, parse_dcp_cpl, parse_dpg, parse_dv_dif, parse_dvdv,
    parse_dxw, parse_flv, parse_gxf, parse_hds_f4m, parse_hls, parse_ibi, parse_ism, parse_ivf,
    parse_lxf, parse_mi_xml, parse_mkv, parse_mp4, parse_mpeg_ps, parse_mpeg_ts, parse_mxf,
    parse_nsv, parse_nut, parse_ogg, parse_p2_clip, parse_pmp, parse_ptx, parse_rm,
    parse_sequence_info, parse_skm, parse_swf, parse_vbi, parse_wav, parse_wm, parse_wtv,
    parse_xdcam_clip};
use revelio_parsers_image::{parse_amiga_icon, parse_arriraw, parse_bmp, parse_bpg, parse_dds,
    parse_dpx, parse_exr, parse_gain_map, parse_gif, parse_ico, parse_jpeg, parse_pcx, parse_png,
    parse_psd, parse_rle, parse_tga, parse_tiff, parse_webp};
use revelio_parsers_text::{parse_arib_std_b24_b37, parse_cdp, parse_cmml, parse_dvb_subtitle,
    parse_eia608, parse_eia708, parse_kate, parse_n19, parse_other_text, parse_pgs,
    parse_sub_rip, parse_ttml, parse_teletext, parse_scc, parse_timed_text};
use revelio_parsers_video::{parse_av1, parse_avc, parse_hevc, parse_theora, parse_vp8, parse_vp9,
    parse_y4m, parse_vc1, parse_mpeg2, parse_vvc, parse_prores, parse_vc3, parse_dolby_vision};

fn main() -> process::ExitCode {
    let mut args: Vec<String> = env::args().skip(1).collect();
    let mut text_mode = false;
    let mut json_mode = false;

    args.retain(|a| {
        if a == "--text" { text_mode = true; false }
        else if a == "--json" { json_mode = true; false }
        else { true }
    });

    if args.is_empty() {
        eprintln!("Usage: revelio [--text|--json] <file-path>");
        return process::ExitCode::from(2);
    }

    let path = &args[0];
    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(e) => { eprintln!("{path}: {e}"); return process::ExitCode::from(1); }
    };

    let parsers: [fn(&mut FileAnalyze) -> bool; 124] = [
        parse_wav, parse_avi, parse_cdxa, parse_amv, parse_webp, parse_aiff, parse_flac,
        parse_dsdiff, parse_caf, parse_mp4, parse_mkv, parse_ogg, parse_mpeg_ts, parse_mpeg_ps,
        parse_swf, parse_skm, parse_dpg, parse_hds_f4m, parse_hls, parse_dash_mpd, parse_dcp_am,
        parse_dcp_cpl, parse_ibi, parse_dxw, parse_aaf, parse_mxf, parse_bdmv, parse_dvdv,
        parse_dv_dif, parse_flv, parse_lxf, parse_nut, parse_wm, parse_wtv, parse_rm, parse_ivf,
        parse_ism, parse_mi_xml, parse_p2_clip, parse_xdcam_clip, parse_sequence_info, parse_ptx,
        parse_nsv, parse_pmp, parse_gxf, parse_cdp, parse_pgs, parse_dvb_subtitle,
        parse_arib_std_b24_b37, parse_kate, parse_cmml, parse_ttml, parse_n19, parse_sub_rip,
        parse_other_text, parse_dsf, parse_png, parse_jpeg, parse_bmp, parse_gif, parse_tiff,
        parse_ico, parse_psd, parse_dpx, parse_dds, parse_exr, parse_bpg, parse_pcx,
        parse_arriraw, parse_amiga_icon, parse_y4m, parse_vc1, parse_mpeg2, parse_av1, parse_avc,
        parse_hevc, parse_vp8, parse_vp9, parse_theora, parse_ac3, parse_ac4, parse_dts,
        parse_dts_uhd, parse_aac_adts, parse_iab, parse_iamf, parse_als, parse_ape, parse_au,
        parse_amr, parse_speex, parse_mpc, parse_la, parse_tak, parse_tta, parse_wvpk,
        parse_twin_vq, parse_extended_module, parse_dat, parse_rkau, parse_aptx100, parse_open_mg,
        parse_midi, parse_module, parse_impulse_tracker, parse_scream_tracker3, parse_mp3,
        parse_tga, parse_gain_map, parse_rle, parse_adpcm, parse_eia608, parse_eia708, parse_vbi,
        parse_vvc, parse_prores, parse_vc3, parse_dolby_vision,
        parse_opus, parse_vorbis, parse_usac,
        parse_teletext, parse_scc, parse_timed_text,
    ];

    let metadata = fs::metadata(path).ok();
    let mut parsed = false;
    for parser in parsers {
        let mut fa = FileAnalyze::new(&bytes);
        if parser(&mut fa) {
            parsed = true;
            // Fill the derived General-stream fields (FileSize,
            // OverallBitRate, Duration, FileExtension, dates, container
            // StreamSize overhead) that aren't readable from the media
            // bytes alone — shared with the diff harness via core.
            let modified_unix_secs = metadata
                .as_ref()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64);
            let info = FileLevelInfo {
                file_size: metadata.as_ref().map(|m| m.len()).unwrap_or(bytes.len() as u64),
                extension: Path::new(path).extension().and_then(|s| s.to_str()),
                modified_unix_secs,
                local_offset_secs: local_offset_seconds(),
            };
            fill_file_level_fields(&mut fa, &info);

            let output = if json_mode {
                to_json(fa.streams(), path)
            } else if text_mode {
                to_text(fa.streams(), path)
            } else {
                to_xml(fa.streams(), path, env!("CARGO_PKG_VERSION"))
            };
            println!("{output}");
            break;
        }
    }

    if !parsed {
        eprintln!("{path}: no parser matched ({} bytes)", bytes.len());
        return process::ExitCode::from(1);
    }

    process::ExitCode::SUCCESS
}

/// Local timezone offset in seconds east of UTC, via `date +%z`
/// (e.g. "+1000" → 36000). Used for the `_Local` date variant.
fn local_offset_seconds() -> i64 {
    let Ok(out) = process::Command::new("date").arg("+%z").output() else {
        return 0;
    };
    let s = String::from_utf8_lossy(&out.stdout);
    let s = s.trim();
    if s.len() < 5 {
        return 0;
    }
    let sign = if s.starts_with('-') { -1 } else { 1 };
    let hh: i64 = s[1..3].parse().unwrap_or(0);
    let mm: i64 = s[3..5].parse().unwrap_or(0);
    sign * (hh * 3600 + mm * 60)
}
