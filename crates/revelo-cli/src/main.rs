use clap::CommandFactory;
use std::fs;
use std::path::Path;
use std::process;
use std::time::UNIX_EPOCH;

use revelo_core::computed_fields::fill_computed_fields;
use revelo_core::multi_file::MultiFileLoader;
use revelo_core::multi_file::find_duplicate_streams;
use revelo_core::{FileAnalyze, FileLevelInfo, ReadBackend, StreamKind, fill_file_level_fields};
use revelo_dispatcher::detect;
use revelo_export::{to_csv, to_html, to_json, to_summary, to_text, to_xml, to_yaml};
use revelo_parsers_tag::parse_tags;

mod cli;
use cli::Cli;

fn main() -> process::ExitCode {
    let cli = <Cli as clap::Parser>::parse();

    // Expand glob patterns and collect the concrete input paths. A bare path
    // (no glob metacharacters) is kept verbatim so a genuinely missing file
    // still surfaces its own open error rather than silently vanishing.
    let inputs = expand_paths(&cli.paths);
    if inputs.is_empty() {
        if cli.paths.is_empty() {
            // No arguments at all: show help.
            let _ = Cli::command().print_help();
            println!();
            return process::ExitCode::SUCCESS;
        }
        // Patterns were supplied but matched nothing on disk.
        eprintln!("no files matched: {}", cli.paths.join(", "));
        return process::ExitCode::from(1);
    }

    let batch = inputs.len() > 1;
    let mut outputs: Vec<String> = Vec::new();
    let mut any_failed = false;

    for path in &inputs {
        match analyze_one(&cli, path) {
            Some(out) => outputs.push(out),
            None => any_failed = true,
        }
    }

    if outputs.is_empty() {
        return process::ExitCode::from(1);
    }

    let combined = combine_outputs(&cli, outputs, batch);
    if let Some(ref log_file) = cli.log_file {
        let _ = fs::write(log_file, &combined);
    } else {
        println!("{combined}");
    }

    if any_failed { process::ExitCode::from(1) } else { process::ExitCode::SUCCESS }
}

/// Expand each argument: glob patterns (containing `*`, `?`, or `[`) are
/// walked and their matching files collected in sorted order; plain paths
/// pass through unchanged. A pattern that matches nothing contributes no
/// inputs. A malformed pattern is treated as a literal path.
fn expand_paths(patterns: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for pat in patterns {
        if pat.contains(['*', '?', '[']) {
            match glob::glob(pat) {
                Ok(entries) => {
                    for entry in entries.flatten() {
                        if entry.is_file() {
                            out.push(entry.to_string_lossy().into_owned());
                        }
                    }
                }
                Err(_) => out.push(pat.clone()),
            }
        } else {
            out.push(pat.clone());
        }
    }
    out
}

/// Combine per-file outputs. For multi-file JSON the default is NDJSON (one
/// compact object per line); `--json-array` wraps every result in a single
/// JSON array (and applies even to a single file). Other formats are simply
/// concatenated. A single non-array result is returned verbatim, preserving
/// byte-for-byte single-file output.
fn combine_outputs(cli: &Cli, outputs: Vec<String>, batch: bool) -> String {
    if cli.json_array {
        return format!("[\n{}\n]", outputs.join(",\n"));
    }
    if batch && cli.json {
        // NDJSON: the JSON formatter uses newlines only as structural
        // separators (values escape their own newlines), so stripping them
        // yields a valid compact object per line.
        return outputs.iter().map(|o| o.replace('\n', "")).collect::<Vec<_>>().join("\n");
    }
    if batch {
        return outputs.join("\n");
    }
    outputs.into_iter().next().unwrap_or_default()
}

/// Read, detect, parse and format a single input path. Returns the formatted
/// output, or `None` (after printing a diagnostic to stderr) on any failure.
fn analyze_one(cli: &Cli, path: &str) -> Option<String> {
    let metadata = fs::metadata(path).ok();

    if cli.multi_file {
        let mut parse_buf = match fs::read(path) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("{path}: {e}");
                return None;
            }
        };

        let mut loader = MultiFileLoader::new();
        loader.scan_references(std::path::Path::new(path), &Default::default());
        let mut has_references = false;
        if let Some((data, _count)) = loader.load_all() {
            parse_buf.extend_from_slice(&data);
            has_references = true;
        }

        let source_len = parse_buf.len();
        if let Some(winner) = detect(&parse_buf) {
            let fa = FileAnalyze::new(parse_buf.as_slice());
            return format_one(
                cli,
                path,
                metadata.as_ref(),
                source_len,
                fa,
                winner,
                has_references,
            );
        }
        eprintln!("{path}: no parser matched ({source_len} bytes)");
        return None;
    }

    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(e) => {
            eprintln!("{path}: {e}");
            return None;
        }
    };

    // SAFETY: the file is opened read-only and is not mutated while mapped.
    match unsafe { memmap2::Mmap::map(&file) } {
        Ok(mmap) => {
            let bytes = mmap.as_ref();
            let source_len = bytes.len();
            if let Some(winner) = detect(bytes) {
                let fa = FileAnalyze::from_backend(ReadBackend::from(&mmap));
                return format_one(cli, path, metadata.as_ref(), source_len, fa, winner, false);
            }
            eprintln!("{path}: no parser matched ({source_len} bytes)");
            None
        }
        Err(_) => {
            let bytes = match fs::read(path) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("{path}: {e}");
                    return None;
                }
            };
            let source_len = bytes.len();
            if let Some(winner) = detect(&bytes) {
                let fa = FileAnalyze::new(bytes.as_slice());
                return format_one(cli, path, metadata.as_ref(), source_len, fa, winner, false);
            }
            eprintln!("{path}: no parser matched ({source_len} bytes)");
            None
        }
    }
}

fn format_one(
    cli: &Cli,
    path: &str,
    metadata: Option<&fs::Metadata>,
    source_len: usize,
    mut fa: FileAnalyze<'_>,
    winner: fn(&mut FileAnalyze) -> bool,
    has_references: bool,
) -> Option<String> {
    fa.set_option("demux", &cli.demux);
    fa.set_option("trace_level", &cli.trace);
    fa.set_option("multi_file", if cli.multi_file { "1" } else { "0" });
    if has_references {
        fa.reference_count = 1;
    }
    if !winner(&mut fa) {
        return None;
    }

    let modified_unix_secs = metadata
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64);
    let info = FileLevelInfo {
        file_size: metadata.map(|m| m.len()).unwrap_or(source_len as u64),
        extension: Path::new(path).extension().and_then(|s| s.to_str()),
        modified_unix_secs,
        local_offset_secs: local_offset_seconds(),
    };
    fill_file_level_fields(&mut fa, &info);
    fill_computed_fields(fa.streams_mut());
    fa.duplicate_indices = find_duplicate_streams(fa.streams());

    let _ = parse_tags(&mut fa);

    if cli.verify {
        let is_complete = if fa.truncated() { "No" } else { "Yes" };
        fa.force_field(StreamKind::General, 0, "IsComplete", is_complete);
        if fa.truncated() {
            fa.force_field(
                StreamKind::General,
                0,
                "Warning",
                "File appears truncated — parser was unable to read the full structure",
            );
        }
    }

    if cli.video_only || cli.audio_only || !cli.stream.is_empty() {
        let mut keep_kinds = vec![StreamKind::General];
        if cli.video_only {
            keep_kinds.push(StreamKind::Video);
        }
        if cli.audio_only {
            keep_kinds.push(StreamKind::Audio);
        }
        if cli.video_only && !cli.audio_only && cli.stream.is_empty() {
            // video-only: General + Video
        } else if !cli.video_only && cli.audio_only && cli.stream.is_empty() {
            // audio-only: General + Audio
        } else if !cli.video_only && !cli.audio_only && cli.stream.is_empty() {
            // Neither stream-kind flag set: keep all public stream kinds.
            keep_kinds.extend_from_slice(&[
                StreamKind::Video,
                StreamKind::Audio,
                StreamKind::Text,
                StreamKind::Other,
                StreamKind::Image,
                StreamKind::Menu,
                StreamKind::Exif,
                StreamKind::Iptc,
                StreamKind::Xmp,
                StreamKind::Icc,
                StreamKind::C2pa,
                StreamKind::MakerNotes,
            ]);
        }

        fa.streams_mut().filter_keep(&keep_kinds, &cli.stream);
    }

    let output = if cli.html {
        to_html(fa.streams(), path)
    } else if cli.yaml {
        to_yaml(fa.streams(), path)
    } else if cli.json || cli.json_array {
        to_json(fa.streams(), path)
    } else if cli.xml {
        to_xml(fa.streams(), path)
    } else if cli.csv {
        to_csv(fa.streams(), path)
    } else if cli.summary {
        to_summary(fa.streams(), path)
    } else {
        format_text_output(&to_text(fa.streams(), path), cli.inform_version, cli.inform_timestamp)
    };

    Some(output)
}

/// Add library version and/or timestamp header to text output if requested.
fn format_text_output(text: &str, add_version: bool, add_timestamp: bool) -> String {
    if !add_version && !add_timestamp {
        return text.to_owned();
    }
    let mut header = String::new();
    if add_timestamp {
        use std::time::{SystemTime, UNIX_EPOCH};
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        header.push_str(&format!("Report created: {}\n", now));
    }
    if add_version {
        header.push_str(&format!("Library version: revelo {}\n", env!("CARGO_PKG_VERSION")));
    }
    if !header.is_empty() {
        header.push('\n');
    }
    header + text
}

/// Local timezone offset in seconds east of UTC, for the `_Local` date variant.
fn local_offset_seconds() -> i64 {
    #[cfg(unix)]
    if let Some(offset) = local_offset_seconds_unix() {
        return offset;
    }
    local_offset_seconds_from_date()
}

fn local_offset_seconds_from_date() -> i64 {
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

#[cfg(unix)]
fn local_offset_seconds_unix() -> Option<i64> {
    use std::os::raw::{c_char, c_int, c_long};

    type TimeT = i64;

    #[repr(C)]
    struct Tm {
        tm_sec: c_int,
        tm_min: c_int,
        tm_hour: c_int,
        tm_mday: c_int,
        tm_mon: c_int,
        tm_year: c_int,
        tm_wday: c_int,
        tm_yday: c_int,
        tm_isdst: c_int,
        tm_gmtoff: c_long,
        tm_zone: *const c_char,
    }

    unsafe extern "C" {
        fn time(tloc: *mut TimeT) -> TimeT;
        fn localtime_r(timep: *const TimeT, result: *mut Tm) -> *mut Tm;
    }

    // SAFETY: `time` accepts a null pointer when the caller only needs the
    // returned timestamp. `localtime_r` writes into a stack-allocated `Tm`
    // with the platform C layout used by Unix targets supported here.
    unsafe {
        let now = time(std::ptr::null_mut());
        if now == -1 {
            return None;
        }
        let mut tm = std::mem::zeroed::<Tm>();
        if localtime_r(&now, &mut tm).is_null() {
            return None;
        }
        Some(tm.tm_gmtoff as i64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cli_json(array: bool) -> Cli {
        let mut c = <Cli as clap::Parser>::parse_from(["revelo", "x"]);
        c.json = true;
        c.json_array = array;
        c
    }

    #[test]
    fn single_output_returned_verbatim() {
        let cli = <Cli as clap::Parser>::parse_from(["revelo", "x"]);
        let out = combine_outputs(&cli, vec!["{\n\"a\":1}".to_string()], false);
        assert_eq!(out, "{\n\"a\":1}");
    }

    #[test]
    fn batch_json_defaults_to_ndjson() {
        let cli = cli_json(false);
        let out =
            combine_outputs(&cli, vec!["{\n\"a\":1}".to_string(), "{\n\"b\":2}".to_string()], true);
        // One compact object per line, newlines only between records.
        assert_eq!(out, "{\"a\":1}\n{\"b\":2}");
        assert_eq!(out.lines().count(), 2);
    }

    #[test]
    fn json_array_wraps_all_records() {
        let cli = cli_json(true);
        let out =
            combine_outputs(&cli, vec!["{\"a\":1}".to_string(), "{\"b\":2}".to_string()], true);
        assert_eq!(out, "[\n{\"a\":1},\n{\"b\":2}\n]");
    }

    #[test]
    fn batch_non_json_concatenates() {
        let cli = <Cli as clap::Parser>::parse_from(["revelo", "x"]);
        let out = combine_outputs(&cli, vec!["A".to_string(), "B".to_string()], true);
        assert_eq!(out, "A\nB");
    }

    #[test]
    fn expand_plain_paths_pass_through() {
        let got = expand_paths(&["a.mp4".to_string(), "dir/b.mkv".to_string()]);
        assert_eq!(got, vec!["a.mp4".to_string(), "dir/b.mkv".to_string()]);
    }

    #[test]
    fn expand_nonmatching_glob_yields_nothing() {
        // A glob (has a metacharacter) that matches no files contributes no
        // inputs — distinct from a plain path, which passes through.
        let got = expand_paths(&["/nonexistent_dir_zzz/*.mp4".to_string()]);
        assert!(got.is_empty(), "{got:?}");
    }
}
