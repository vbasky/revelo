use std::env;
use std::fs;
use std::path::Path;
use std::process;
use std::time::UNIX_EPOCH;

use revelio_core::{fill_file_level_fields, FileAnalyze, FileLevelInfo};
use revelio_core::multi_file::MultiFileLoader;
use revelio_core::multi_file::find_duplicate_streams;
use revelio_core::computed_fields::fill_computed_fields;
use revelio_export::{to_xml, to_text, to_json};
use revelio_dispatcher::table;


fn main() -> process::ExitCode {
    let mut args: Vec<String> = env::args().skip(1).collect();
    let mut xml_mode = false;
    let mut json_mode = false;
    let mut demux_level = String::from("frame");
    let mut trace_level = String::from("0");
    let mut multi_file = false;

    args.retain(|a| {
        if a == "--xml" { xml_mode = true; false }
        else if a == "--json" { json_mode = true; false }
        else if a.starts_with("--demux=") { demux_level = a[8..].to_string(); false }
        else if a.starts_with("--trace=") { trace_level = a[8..].to_string(); false }
        else if a == "--multi-file" { multi_file = true; false }
        else { true }
    });

    if args.is_empty() {
        eprintln!(r#"    ✦
     \
      \
       |  revelio — media metadata extractor
       |
      ╱ ╲
     ✦   ✦
  Usage: revelio [--xml|--json] <file-path>
  Options:
    --xml         XML output
    --json        JSON output
    --multi-file  scan companion files (BDMV M2TS, sidecar subtitles)
    --demux=N     demux level: frame (default), container, elementary
    --trace=N     trace verbosity (0-9)
"#);
        return process::ExitCode::from(2);
    }

    let path = &args[0];
    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(e) => { eprintln!("{path}: {e}"); return process::ExitCode::from(1); }
    };

    let parsers = table();

    let metadata = fs::metadata(path).ok();
    let mut parsed = false;

    // Multi-file: scan for companion files (BDMV M2TS, SRT/SST sidecars)
    let mut extra_data: Option<Vec<u8>> = None;
    if multi_file {
        let mut loader = MultiFileLoader::new();
        loader.scan_references(std::path::Path::new(path), &Default::default());
        if let Some((data, _count)) = loader.load_all() {
            extra_data = Some(data);
        }
    }

    // Prepare parse buffer: primary file + optional companion data
    let parse_buf: Vec<u8> = if let Some(ref extra) = extra_data {
        let mut combined = bytes.clone();
        combined.extend_from_slice(extra);
        combined
    } else {
        bytes.clone()
    };

    for parser in parsers {
        let mut fa = FileAnalyze::new(&parse_buf);
        // Apply config from CLI options
        fa.set_option("demux", &demux_level);
        fa.set_option("trace_level", &trace_level);
        fa.set_option("multi_file", if multi_file { "1" } else { "0" });
        if let Some(ref _extra) = extra_data {
            fa.reference_count = 1;
        }
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
            fill_computed_fields(fa.streams_mut());
            fa.duplicate_indices = find_duplicate_streams(fa.streams());

            let output = if json_mode {
                to_json(fa.streams(), path, env!("CARGO_PKG_VERSION"))
            } else if xml_mode {
                to_xml(fa.streams(), path, env!("CARGO_PKG_VERSION"))
            } else {
                to_text(fa.streams(), path)
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
