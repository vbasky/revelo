use clap::CommandFactory;
use std::fs;
use std::path::Path;
use std::process;
use std::time::UNIX_EPOCH;

use revelo_core::computed_fields::fill_computed_fields;
use revelo_core::multi_file::MultiFileLoader;
use revelo_core::multi_file::find_duplicate_streams;
use revelo_core::{FileAnalyze, FileLevelInfo, StreamKind, fill_file_level_fields};
use revelo_dispatcher::detect;
use revelo_export::{to_csv, to_json, to_summary, to_text, to_xml};

mod cli;
use cli::Cli;

fn main() -> process::ExitCode {
    let cli = <Cli as clap::Parser>::parse();

    let path = match cli.path {
        Some(p) => p,
        None => {
            let _ = Cli::command().print_help();
            println!();
            return process::ExitCode::SUCCESS;
        }
    };

    let bytes = match fs::read(&path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("{path}: {e}");
            return process::ExitCode::from(1);
        }
    };

    let metadata = fs::metadata(&path).ok();
    let mut parsed = false;

    // Multi-file: scan for companion files (BDMV M2TS, SRT/SST sidecars)
    let mut extra_data: Option<Vec<u8>> = None;
    if cli.multi_file {
        let mut loader = MultiFileLoader::new();
        loader.scan_references(std::path::Path::new(&path), &Default::default());
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

    // Phase 1: race all parsers across cores to find the one that
    // recognizes the buffer, then re-run that winner synchronously below
    // for full metadata extraction.
    let winner = detect(&parse_buf);

    if let Some(winner) = winner {
        let mut fa = FileAnalyze::new(&parse_buf);
        fa.set_option("demux", &cli.demux);
        fa.set_option("trace_level", &cli.trace);
        fa.set_option("multi_file", if cli.multi_file { "1" } else { "0" });
        if extra_data.is_some() {
            fa.reference_count = 1;
        }
        if winner(&mut fa) {
            parsed = true;
            // Fill the derived General-stream fields (FileSize,
            // OverallBitRate, Duration, FileExtension, dates, container
            // StreamSize overhead) that aren't readable from the media
            // bytes alone — shared with revelo-diff via core.
            let modified_unix_secs = metadata
                .as_ref()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64);
            let info = FileLevelInfo {
                file_size: metadata.as_ref().map(|m| m.len()).unwrap_or(bytes.len() as u64),
                extension: Path::new(&path).extension().and_then(|s| s.to_str()),
                modified_unix_secs,
                local_offset_secs: local_offset_seconds(),
            };
            fill_file_level_fields(&mut fa, &info);
            fill_computed_fields(fa.streams_mut());
            fa.duplicate_indices = find_duplicate_streams(fa.streams());

            // --verify: report structural integrity
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

            // Stream filtering: apply before output
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
                    // Neither flag set but --stream was used — keep all kinds
                    keep_kinds.extend_from_slice(&[
                        StreamKind::Video,
                        StreamKind::Audio,
                        StreamKind::Text,
                        StreamKind::Other,
                        StreamKind::Image,
                        StreamKind::Menu,
                    ]);
                }

                fa.streams_mut().filter_keep(&keep_kinds, &cli.stream);
            }

            let output = if cli.json {
                to_json(fa.streams(), &path, env!("CARGO_PKG_VERSION"))
            } else if cli.xml {
                to_xml(fa.streams(), &path, env!("CARGO_PKG_VERSION"))
            } else if cli.csv {
                to_csv(fa.streams(), &path)
            } else if cli.summary {
                to_summary(fa.streams(), &path)
            } else {
                format_text_output(
                    &to_text(fa.streams(), &path),
                    cli.inform_version,
                    cli.inform_timestamp,
                )
            };

            if let Some(ref log_file) = cli.log_file {
                let _ = fs::write(log_file, &output);
            } else {
                println!("{output}");
            }
        }
    }

    if !parsed {
        eprintln!("{path}: no parser matched ({} bytes)", bytes.len());
        return process::ExitCode::from(1);
    }

    process::ExitCode::SUCCESS
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
