use std::env;
use std::fs;
use std::path::Path;
use std::process;

use mediainfo_core::FileAnalyze;
use mediainfo_export::to_xml;
use mediainfo_parsers_audio::{parse_aac_adts, parse_ac3, parse_ac4, parse_adpcm, parse_als, parse_amr,
    parse_ape, parse_aptx100, parse_au, parse_caf, parse_dat, parse_dsdiff, parse_dsf, parse_dts,
    parse_dts_uhd, parse_extended_module, parse_flac, parse_iab, parse_iamf, parse_impulse_tracker,
    parse_la, parse_midi, parse_module, parse_mp3, parse_mpc, parse_open_mg, parse_rkau,
    parse_scream_tracker3, parse_speex, parse_tak, parse_tta, parse_twin_vq, parse_wvpk};
use mediainfo_parsers_container::{parse_aaf, parse_aiff, parse_amv, parse_avi, parse_bdmv,
    parse_cdxa, parse_dash_mpd, parse_dcp_am, parse_dcp_cpl, parse_dpg, parse_dv_dif, parse_dvdv,
    parse_dxw, parse_flv, parse_gxf, parse_hds_f4m, parse_hls, parse_ibi, parse_ism, parse_ivf,
    parse_lxf, parse_mi_xml, parse_mkv, parse_mp4, parse_mpeg_ps, parse_mpeg_ts, parse_mxf,
    parse_nsv, parse_nut, parse_ogg, parse_p2_clip, parse_pmp, parse_ptx, parse_rm,
    parse_sequence_info, parse_skm, parse_swf, parse_vbi, parse_wav, parse_wm, parse_wtv,
    parse_xdcam_clip};
use mediainfo_parsers_image::{parse_amiga_icon, parse_arriraw, parse_bmp, parse_bpg, parse_dds,
    parse_dpx, parse_exr, parse_gain_map, parse_gif, parse_ico, parse_jpeg, parse_pcx, parse_png,
    parse_psd, parse_rle, parse_tga, parse_tiff, parse_webp};
use mediainfo_parsers_text::{parse_arib_std_b24_b37, parse_cdp, parse_cmml, parse_dvb_subtitle,
    parse_eia608, parse_eia708, parse_kate, parse_n19, parse_other_text, parse_pgs,
    parse_sub_rip, parse_ttml};
use mediainfo_parsers_video::{parse_av1, parse_avc, parse_hevc, parse_theora, parse_vp8, parse_vp9,
    parse_y4m, parse_vc1, parse_mpeg2};

fn main() -> process::ExitCode {
    let mut args: Vec<String> = env::args().skip(1).collect();
    let mut use_text = false;
    let mut use_json = false;

    args.retain(|a| {
        if a == "--text" { use_text = true; false }
        else if a == "--json" { use_json = true; false }
        else { true }
    });

    if args.is_empty() {
        eprintln!("Usage: mediainfo-rs [--text|--json] <file-path>");
        return process::ExitCode::from(2);
    }

    let path = &args[0];
    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(e) => { eprintln!("{}: {e}", path); return process::ExitCode::from(1); }
    };

    let parsers: [fn(&mut FileAnalyze) -> bool; 114] = [
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
    ];

    let mut parsed = false;
    for parser in parsers {
        let mut fa = FileAnalyze::new(&bytes);
        if parser(&mut fa) {
            parsed = true;

            if use_text {
                print_text_output(&fa);
            } else if use_json {
                print_json_output(&fa);
            } else {
                println!("{}", to_xml(fa.streams(), path, env!("CARGO_PKG_VERSION")));
            }
            break;
        }
    }

    if !parsed {
        eprintln!("{}: no parser matched ({} bytes)", path, bytes.len());
        return process::ExitCode::from(1);
    }

    process::ExitCode::SUCCESS
}

fn print_text_output(fa: &FileAnalyze) {
    use mediainfo_core::StreamKind;
    let kinds = [StreamKind::General, StreamKind::Video, StreamKind::Audio, StreamKind::Text,
        StreamKind::Other, StreamKind::Image, StreamKind::Menu];

    for kind in kinds {
        let count = fa.Count_Get(kind);
        for pos in 0..count {
            println!("{}", kind.name());
            if count > 1 { println!("#{}", pos + 1); }
            if let Some(s) = fa.streams().stream(kind, pos) {
                for (k, v) in s.iter() {
                    println!("{} : {}", k, v.as_str());
                }
                for (k, v) in s.extras_iter() {
                    println!("{} : {}", k, v.as_str());
                }
            }
            println!();
        }
    }
}

fn print_json_output(fa: &FileAnalyze) {
    use mediainfo_core::StreamKind;
    let kinds = [StreamKind::General, StreamKind::Video, StreamKind::Audio, StreamKind::Text,
        StreamKind::Other, StreamKind::Image, StreamKind::Menu];

    print!("{{\"media\":{{\"@ref\":\"\",\"track\":[");
    let mut first = true;
    for kind in kinds {
        let count = fa.Count_Get(kind);
        for pos in 0..count {
            if let Some(s) = fa.streams().stream(kind, pos) {
                if !first { print!(",") } else { first = false; }
                print!("{{\"@type\":\"{}\"", kind.name());
                for (k, v) in s.iter() {
                    let val = json_escape(v.as_str());
                    print!(",\"{}\":\"{}\"", k, val);
                }
                for (k, v) in s.extras_iter() {
                    let val = json_escape(v.as_str());
                    print!(",\"{}\":\"{}\"", k, val);
                }
                print!("}}");
            }
        }
    }
    println!("]}}}}");
}

fn json_escape(s: &str) -> String {
    s.replace('\\', r"\\").replace('"', "\\\"").replace('\n', "\\n").replace('\t', "\\t")
}
