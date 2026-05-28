use std::env;
use std::fs;
use std::path::Path;
use std::process::{Command, ExitCode};
use std::time::UNIX_EPOCH;

use revelio_core::{fill_file_level_fields, FileAnalyze, FileLevelInfo};
use revelio_export::to_xml;
use revelio_parsers_audio::{parse_aac_adts, parse_ac3, parse_ac4, parse_adpcm, parse_als, parse_amr, parse_ape, parse_aptx100, parse_au, parse_caf, parse_dat, parse_dsdiff, parse_dsf, parse_dts, parse_dts_uhd, parse_extended_module, parse_flac, parse_iab, parse_iamf, parse_impulse_tracker, parse_la, parse_midi, parse_module, parse_mp3, parse_mpc, parse_open_mg, parse_rkau, parse_scream_tracker3, parse_speex, parse_tak, parse_tta, parse_twin_vq, parse_wvpk, parse_opus, parse_vorbis, parse_usac};
use revelio_parsers_container::{parse_aaf, parse_aiff, parse_amv, parse_avi, parse_bdmv, parse_cdxa, parse_dash_mpd, parse_dcp_am, parse_dcp_cpl, parse_dpg, parse_dv_dif, parse_dvdv, parse_dxw, parse_flv, parse_gxf, parse_hds_f4m, parse_hls, parse_ibi, parse_ism, parse_ivf, parse_lxf, parse_mi_xml, parse_mkv, parse_mp4, parse_mpeg_ps, parse_mpeg_ts, parse_mxf, parse_nsv, parse_nut, parse_ogg, parse_p2_clip, parse_pmp, parse_ptx, parse_rm, parse_sequence_info, parse_skm, parse_swf, parse_vbi, parse_wav, parse_wm, parse_wtv, parse_xdcam_clip};
use revelio_parsers_text::{parse_arib_std_b24_b37, parse_cdp, parse_cmml, parse_dvb_subtitle, parse_eia608, parse_eia708, parse_kate, parse_n19, parse_other_text, parse_pgs, parse_sub_rip, parse_ttml, parse_teletext, parse_scc, parse_timed_text};
use revelio_parsers_image::{parse_amiga_icon, parse_arriraw, parse_bmp, parse_bpg, parse_dds, parse_dpx, parse_exr, parse_gain_map, parse_gif, parse_ico, parse_jpeg, parse_pcx, parse_png, parse_psd, parse_rle, parse_tga, parse_tiff, parse_webp};
use revelio_parsers_video::{parse_av1, parse_avc, parse_hevc, parse_theora, parse_vp8, parse_vp9, parse_y4m, parse_vc1, parse_mpeg2, parse_vvc, parse_prores, parse_vc3, parse_dolby_vision};

fn main() -> ExitCode {
    let mut args: Vec<String> = env::args().skip(1).collect();
    let print_xml = args
        .iter()
        .position(|a| a == "--rust-xml")
        .map(|i| {
            args.remove(i);
            true
        })
        .unwrap_or(false);
    // --strict swaps the default order-insensitive line-set diff for an
    // order-sensitive LCS diff, so "0 only in oracle, 0 only in rust"
    // means the two XML outputs are line-for-line identical (true byte
    // fidelity), not merely the same set of lines.
    let strict = args
        .iter()
        .position(|a| a == "--strict")
        .map(|i| {
            args.remove(i);
            true
        })
        .unwrap_or(false);

    if args.is_empty() {
        eprintln!("usage: diff-harness [--rust-xml] [--strict] <media-file> [<media-file> ...]");
        return ExitCode::from(2);
    }

    let mut any_failed = false;
    for path in &args {
        if print_xml {
            match run_rust_engine(path) {
                Ok(xml) => print!("{xml}"),
                Err(msg) => {
                    eprintln!("{path}: {msg}");
                    any_failed = true;
                }
            }
            continue;
        }
        match diff_one(path, strict) {
            Ok(report) => println!("{report}"),
            Err(msg) => {
                eprintln!("{path}: {msg}");
                any_failed = true;
            }
        }
    }

    if any_failed { ExitCode::from(1) } else { ExitCode::SUCCESS }
}

struct Report {
    path: String,
    oracle_xml: String,
    rust_xml: String,
    strict: bool,
}

impl std::fmt::Display for Report {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "=== {} ===", self.path)?;
        let diffs = if self.strict {
            diff_lines_ordered(&self.oracle_xml, &self.rust_xml)
        } else {
            diff_lines(&self.oracle_xml, &self.rust_xml)
        };
        let only_oracle = diffs.iter().filter(|d| matches!(d, LineDiff::OnlyOracle(_))).count();
        let only_rust = diffs.iter().filter(|d| matches!(d, LineDiff::OnlyRust(_))).count();
        let common = diffs.iter().filter(|d| matches!(d, LineDiff::Common)).count();
        let mode = if self.strict { " (order-sensitive)" } else { "" };
        writeln!(
            f,
            "  {common} lines match, {only_oracle} only in oracle, {only_rust} only in rust{mode}",
        )?;
        if self.strict && only_oracle == 0 && only_rust == 0 {
            writeln!(f, "  BYTE-EQUAL {common}/{common}")?;
        }
        for d in &diffs {
            match d {
                LineDiff::Common => {}
                LineDiff::OnlyOracle(line) => writeln!(f, "  - oracle: {line}")?,
                LineDiff::OnlyRust(line) => writeln!(f, "  + rust:   {line}")?,
            }
        }
        Ok(())
    }
}

fn diff_one(path: &str, strict: bool) -> Result<Report, String> {
    let oracle_xml = run_oracle(path)?;
    let rust_xml = run_rust_engine(path)?;
    Ok(Report {
        path: path.to_owned(),
        oracle_xml,
        rust_xml,
        strict,
    })
}

fn run_oracle(path: &str) -> Result<String, String> {
    let output = Command::new("mediainfo")
        .arg("--Output=XML")
        .arg(path)
        .output()
        .map_err(|e| format!("failed to spawn `mediainfo`: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "`mediainfo` exited with status {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    // Oracle occasionally emits Latin-1 bytes (e.g. 0xB0 for "°" in
    // GPS-derived Recorded_Location). Use lossy conversion so a single
    // non-UTF-8 byte doesn't poison the entire diff.
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn run_rust_engine(path: &str) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|e| format!("read failed: {e}"))?;
    let metadata = fs::metadata(path).map_err(|e| format!("stat failed: {e}"))?;
    let mut fa = FileAnalyze::new(&bytes);

    // Structured/magic-based parsers first; sync-based MP3 last so it
    // only fires when nothing else claimed the file.
    let parsers: [(&str, fn(&mut FileAnalyze) -> bool); 124] = [
        ("WAV", parse_wav),
        ("AVI", parse_avi),
        ("CDXA", parse_cdxa),
        ("AMV", parse_amv),
        ("WebP", parse_webp),
        ("AIFF", parse_aiff),
        ("FLAC", parse_flac),
        ("DSDIFF", parse_dsdiff),
        ("CAF", parse_caf),
        ("MP4", parse_mp4),
        ("MKV", parse_mkv),
        ("Ogg", parse_ogg),
        ("MPEG-TS", parse_mpeg_ts),
        ("MPEG-PS", parse_mpeg_ps),
        ("SWF", parse_swf),
        ("SKM", parse_skm),
        ("DPG", parse_dpg),
        ("HDS-F4M", parse_hds_f4m),
        ("HLS", parse_hls),
        ("DASH-MPD", parse_dash_mpd),
        ("DCP-AM", parse_dcp_am),
        ("DCP-CPL", parse_dcp_cpl),
        ("Ibi", parse_ibi),
        ("DXW", parse_dxw),
        ("AAF", parse_aaf),
        ("MXF", parse_mxf),
        ("BDMV", parse_bdmv),
        ("DVDV", parse_dvdv),
        ("DV-DIF", parse_dv_dif),
        ("FLV", parse_flv),
        ("LXF", parse_lxf),
        ("Nut", parse_nut),
        ("WM", parse_wm),
        ("WTV", parse_wtv),
        ("RM", parse_rm),
        ("IVF", parse_ivf),
        ("ISM", parse_ism),
        ("MiXml", parse_mi_xml),
        ("P2-Clip", parse_p2_clip),
        ("XDCAM-Clip", parse_xdcam_clip),
        ("SequenceInfo", parse_sequence_info),
        ("Ptx", parse_ptx),
        ("NSV", parse_nsv),
        ("PMP", parse_pmp),
        ("GXF", parse_gxf),
        ("CDP", parse_cdp),
        ("PGS", parse_pgs),
        ("DVB-Sub", parse_dvb_subtitle),
        ("ARIB-B24", parse_arib_std_b24_b37),
        ("Kate", parse_kate),
        ("CMML", parse_cmml),
        ("TTML", parse_ttml),
        ("N19", parse_n19),
        ("SubRip", parse_sub_rip),
        ("OtherText", parse_other_text),
        ("DSF", parse_dsf),
        ("PNG", parse_png),
        ("JPEG", parse_jpeg),
        ("BMP", parse_bmp),
        ("GIF", parse_gif),
        ("TIFF", parse_tiff),
        ("ICO", parse_ico),
        ("PSD", parse_psd),
        ("DPX", parse_dpx),
        ("DDS", parse_dds),
        ("EXR", parse_exr),
        ("BPG", parse_bpg),
        ("PCX", parse_pcx),
        ("ArriRaw", parse_arriraw),
        ("AmigaIcon", parse_amiga_icon),
        ("Y4M", parse_y4m),
        ("VC1", parse_vc1),
        ("MPEG-2", parse_mpeg2),
        ("AV1", parse_av1),
        ("AVC", parse_avc),
        ("HEVC", parse_hevc),
        ("VP8", parse_vp8),
        ("VP9", parse_vp9),
        ("Theora", parse_theora),
        ("AC3", parse_ac3),
        ("AC4", parse_ac4),
        ("DTS", parse_dts),
        ("DTS-UHD", parse_dts_uhd),
        ("AAC-ADTS", parse_aac_adts),
        ("IAB", parse_iab),
        ("IAMF", parse_iamf),
        ("ALS", parse_als),
        ("APE", parse_ape),
        ("AU", parse_au),
        ("AMR", parse_amr),
        ("Speex", parse_speex),
        ("MPC", parse_mpc),
        ("LA", parse_la),
        ("TAK", parse_tak),
        ("TTA", parse_tta),
        ("WavPack", parse_wvpk),
        ("TwinVQ", parse_twin_vq),
        ("XM", parse_extended_module),
        ("DAT", parse_dat),
        ("RKAU", parse_rkau),
        ("aptX100", parse_aptx100),
        ("OpenMG", parse_open_mg),
        ("Midi", parse_midi),
        ("Module", parse_module),
        ("IT", parse_impulse_tracker),
        ("S3M", parse_scream_tracker3),
        ("MP3", parse_mp3),
        ("TGA", parse_tga),
        ("GainMap", parse_gain_map),
        ("RLE", parse_rle),
        ("ADPCM", parse_adpcm),
        ("EIA-608", parse_eia608),
        ("EIA-708", parse_eia708),
        ("VBI", parse_vbi),
        ("VVC", parse_vvc),
        ("ProRes", parse_prores),
        ("VC-3", parse_vc3),
        ("DolbyVision", parse_dolby_vision),
        ("Opus", parse_opus),
        ("Vorbis", parse_vorbis),
        ("USAC", parse_usac),
        ("Teletext", parse_teletext),
        ("SCC", parse_scc),
        ("TimedText", parse_timed_text),
    ];
    let mut parsed = false;
    for (_name, parser) in parsers {
        fa = FileAnalyze::new(&bytes);
        if parser(&mut fa) {
            parsed = true;
            break;
        }
    }
    if !parsed {
        return Err(format!(
            "no rust parser matched ({} bytes)",
            bytes.len()
        ));
    }

    // Shared with the CLI via revelio-core — single source of truth for
    // the derived General-stream fields.
    let modified_unix_secs = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64);
    let info = FileLevelInfo {
        file_size: metadata.len(),
        extension: Path::new(path).extension().and_then(|s| s.to_str()),
        modified_unix_secs,
        local_offset_secs: local_offset_seconds(),
    };
    fill_file_level_fields(&mut fa, &info);

    // Library version pulled from the oracle's banner output so the
    // diff isolates real semantic differences, not version-string noise.
    let library_version = detect_library_version().unwrap_or_else(|| "0.0.0".into());
    Ok(to_xml(fa.streams(), path, &library_version))
}

/// Detect the local timezone offset in seconds via shelling out to
/// `date +%z` (e.g. "+1000" → 36000). Cheap, macOS/Linux compatible,
/// and the harness is a dev tool so the shell-out is acceptable.
fn local_offset_seconds() -> i64 {
    let Ok(out) = Command::new("date").arg("+%z").output() else {
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

fn detect_library_version() -> Option<String> {
    let out = Command::new("mediainfo").arg("--Version").output().ok()?;
    let s = String::from_utf8_lossy(&out.stdout);
    for line in s.lines() {
        if let Some(rest) = line.strip_prefix("MediaInfoLib - v") {
            return Some(rest.trim().to_owned());
        }
    }
    None
}

enum LineDiff<'a> {
    Common,
    OnlyOracle(&'a str),
    OnlyRust(&'a str),
}

/// Naive line-set diff. Cheap, asymmetric: collects lines only in one
/// side. Order-insensitive — enough to flag missing/extra fields
/// without dragging in a full LCS implementation.
fn diff_lines<'a>(oracle: &'a str, rust: &'a str) -> Vec<LineDiff<'a>> {
    use std::collections::HashSet;
    let o: HashSet<&str> = oracle.lines().collect();
    let r: HashSet<&str> = rust.lines().collect();
    let mut diffs = Vec::new();
    for line in oracle.lines() {
        if r.contains(line) {
            diffs.push(LineDiff::Common);
        } else {
            diffs.push(LineDiff::OnlyOracle(line));
        }
    }
    for line in rust.lines() {
        if !o.contains(line) {
            diffs.push(LineDiff::OnlyRust(line));
        }
    }
    diffs
}

/// Order-sensitive diff via longest-common-subsequence. Lines kept in
/// sequence are Common; the rest become deletions (only in oracle) and
/// insertions (only in rust). Unlike the set diff this respects both
/// ordering and duplicate multiplicity, so a reordered field shows up as
/// a delete+insert pair and "0 only in oracle, 0 only in rust" means the
/// two outputs are line-for-line identical. XML here is at most a few
/// hundred lines, so the O(n·m) table is cheap.
fn diff_lines_ordered<'a>(oracle: &'a str, rust: &'a str) -> Vec<LineDiff<'a>> {
    let o: Vec<&str> = oracle.lines().collect();
    let r: Vec<&str> = rust.lines().collect();
    let (n, m) = (o.len(), r.len());

    // dp[i][j] = LCS length of o[i..] and r[j..].
    let mut dp = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i][j] = if o[i] == r[j] {
                dp[i + 1][j + 1] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }

    let mut diffs = Vec::new();
    let (mut i, mut j) = (0usize, 0usize);
    while i < n && j < m {
        if o[i] == r[j] {
            diffs.push(LineDiff::Common);
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            diffs.push(LineDiff::OnlyOracle(o[i]));
            i += 1;
        } else {
            diffs.push(LineDiff::OnlyRust(r[j]));
            j += 1;
        }
    }
    while i < n {
        diffs.push(LineDiff::OnlyOracle(o[i]));
        i += 1;
    }
    while j < m {
        diffs.push(LineDiff::OnlyRust(r[j]));
        j += 1;
    }
    diffs
}
