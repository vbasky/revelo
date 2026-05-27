use std::env;
use std::fs;
use std::path::Path;
use std::process::{Command, ExitCode};
use std::time::{SystemTime, UNIX_EPOCH};

use mediainfo_core::{FileAnalyze, StreamKind};
use mediainfo_export::to_xml;
use mediainfo_parsers_audio::parse_flac;
use mediainfo_parsers_container::{parse_aiff, parse_wav};

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

    if args.is_empty() {
        eprintln!("usage: diff-harness [--rust-xml] <media-file> [<media-file> ...]");
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
        match diff_one(path) {
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
}

impl std::fmt::Display for Report {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "=== {} ===", self.path)?;
        let diffs = diff_lines(&self.oracle_xml, &self.rust_xml);
        let only_oracle = diffs.iter().filter(|d| matches!(d, LineDiff::OnlyOracle(_))).count();
        let only_rust = diffs.iter().filter(|d| matches!(d, LineDiff::OnlyRust(_))).count();
        let common = diffs.iter().filter(|d| matches!(d, LineDiff::Common)).count();
        writeln!(
            f,
            "  {common} lines match, {only_oracle} only in oracle, {only_rust} only in rust",
        )?;
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

fn diff_one(path: &str) -> Result<Report, String> {
    let oracle_xml = run_oracle(path)?;
    let rust_xml = run_rust_engine(path)?;
    Ok(Report {
        path: path.to_owned(),
        oracle_xml,
        rust_xml,
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
    String::from_utf8(output.stdout).map_err(|e| format!("non-UTF8 output: {e}"))
}

fn run_rust_engine(path: &str) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|e| format!("read failed: {e}"))?;
    let metadata = fs::metadata(path).map_err(|e| format!("stat failed: {e}"))?;
    let mut fa = FileAnalyze::new(&bytes);

    let parsers: [(&str, fn(&mut FileAnalyze) -> bool); 3] =
        [("WAV", parse_wav), ("AIFF", parse_aiff), ("FLAC", parse_flac)];
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

    fill_file_level_fields(&mut fa, path, &metadata);

    // Library version pulled from the oracle's banner output so the
    // diff isolates real semantic differences, not version-string noise.
    let library_version = detect_library_version().unwrap_or_else(|| "0.0.0".into());
    Ok(to_xml(fa.streams(), path, &library_version))
}

/// Fill fields that aren't derivable from the parsed bytes alone —
/// they come from the path / fstat / cross-stream arithmetic. In the
/// C++ side these are filled by `MediaInfo_Internal` (the wrapper that
/// drives the parser). For now the harness plays that role.
fn fill_file_level_fields(fa: &mut FileAnalyze, path: &str, metadata: &fs::Metadata) {
    let file_size = metadata.len();
    fa.Fill(StreamKind::General, 0, "FileSize", file_size.to_string(), false);

    if let Some(ext) = Path::new(path).extension().and_then(|s| s.to_str()) {
        fa.Fill(StreamKind::General, 0, "FileExtension", ext.to_owned(), false);
    }

    let audio_stream_size: Option<u64> = fa
        .Retrieve(StreamKind::Audio, 0, "StreamSize")
        .and_then(|z| z.as_str().parse().ok());
    let duration_ms: Option<u64> = fa
        .Retrieve(StreamKind::Audio, 0, "Duration")
        .and_then(|z| z.as_str().parse().ok());

    // General Duration is the same as the audio stream's duration when
    // there is exactly one audio stream and no other streams.
    if let Some(ms) = duration_ms {
        fa.Fill(StreamKind::General, 0, "Duration", ms.to_string(), false);
    }

    // OverallBitRate = FileSize * 8 * 1000 / Duration_ms, rounded to
    // nearest (the C++ side fills with AfterComma=0 which renders via
    // `%.0f` — round-half-to-even).
    if let Some(ms) = duration_ms {
        if ms > 0 {
            let overall = ((file_size as f64) * 8.0 * 1000.0 / (ms as f64)).round() as u64;
            // Mirror the audio stream's BitRate_Mode rather than hardcoding —
            // a parser may have flagged the audio as VBR (e.g. FLAC).
            let bitrate_mode = fa
                .Retrieve(StreamKind::Audio, 0, "BitRate_Mode")
                .map(|z| z.as_str().to_owned())
                .unwrap_or_else(|| "CBR".to_owned());
            fa.Fill(StreamKind::General, 0, "OverallBitRate_Mode", bitrate_mode, false);
            fa.Fill(
                StreamKind::General,
                0,
                "OverallBitRate",
                overall.to_string(),
                false,
            );
        }
    }

    // General StreamSize = file overhead = FileSize - audio data.
    if let Some(audio_size) = audio_stream_size {
        let overhead = file_size.saturating_sub(audio_size);
        fa.Fill(StreamKind::General, 0, "StreamSize", overhead.to_string(), false);
    }

    if let Ok(modified) = metadata.modified() {
        if let Ok(d) = modified.duration_since(UNIX_EPOCH) {
            let unix_secs = d.as_secs() as i64;
            fa.Fill(
                StreamKind::General,
                0,
                "File_Modified_Date",
                format_utc(unix_secs),
                false,
            );
            fa.Fill(
                StreamKind::General,
                0,
                "File_Modified_Date_Local",
                format_local(modified),
                false,
            );
        }
    }
}

fn format_utc(unix_secs: i64) -> String {
    let (y, m, d, hh, mm, ss) = civil_from_unix(unix_secs);
    format!("{y:04}-{m:02}-{d:02} {hh:02}:{mm:02}:{ss:02} UTC")
}

fn format_local(t: SystemTime) -> String {
    let secs = match t.duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_secs() as i64,
        Err(e) => -(e.duration().as_secs() as i64),
    };
    let local_offset = local_offset_seconds();
    let (y, m, d, hh, mm, ss) = civil_from_unix(secs + local_offset);
    format!("{y:04}-{m:02}-{d:02} {hh:02}:{mm:02}:{ss:02}")
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

/// Convert a unix timestamp to (year, month, day, hour, min, sec) using
/// Howard Hinnant's `days_from_civil` algorithm — proleptic Gregorian,
/// matches `chrono`'s behavior to the second within representable range.
fn civil_from_unix(unix_secs: i64) -> (i32, u8, u8, u8, u8, u8) {
    let days = unix_secs.div_euclid(86400);
    let rem = unix_secs.rem_euclid(86400);
    let hh = (rem / 3600) as u8;
    let mm = ((rem % 3600) / 60) as u8;
    let ss = (rem % 60) as u8;

    let z = days + 719_468;
    let era = if z >= 0 { z / 146_097 } else { (z - 146_096) / 146_097 };
    let doe = (z - era * 146_097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u8;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u8;
    let year = if m <= 2 { y + 1 } else { y } as i32;
    (year, m, d, hh, mm, ss)
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
