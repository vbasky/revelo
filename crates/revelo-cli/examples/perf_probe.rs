use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process;
use std::time::{Duration, Instant, UNIX_EPOCH};

use revelo_core::computed_fields::fill_computed_fields;
use revelo_core::multi_file::find_duplicate_streams;
use revelo_core::{
    AccessStats, FileAnalyze, FileLevelInfo, ReadBackend, StreamKind, fill_file_level_fields,
};
use revelo_dispatcher::table;
use revelo_export::{to_json, to_summary, to_text};
use revelo_parsers_tag::parse_tags;

const LARGE_INPUT_RAW_READ_LIMIT: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExportMode {
    None,
    Text,
    Json,
    Summary,
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputTarget {
    Sink,
    File,
    Stdout,
}

#[derive(Debug)]
struct Config {
    path: PathBuf,
    label: String,
    export_mode: ExportMode,
    output_target: OutputTarget,
    output_file: Option<PathBuf>,
    demux: String,
    trace: String,
    skip_tags: bool,
    include_path: bool,
    fixed_local_offset_secs: Option<i64>,
}

#[derive(Debug)]
struct Phase {
    name: &'static str,
    duration: Duration,
    access_stats: Option<AccessStats>,
    bytes: Option<usize>,
}

impl Phase {
    fn new(name: &'static str, duration: Duration) -> Self {
        Self { name, duration, access_stats: None, bytes: None }
    }

    fn with_access(name: &'static str, duration: Duration, access_stats: AccessStats) -> Self {
        Self { name, duration, access_stats: Some(access_stats), bytes: None }
    }

    fn with_bytes(name: &'static str, duration: Duration, bytes: usize) -> Self {
        Self { name, duration, access_stats: None, bytes: Some(bytes) }
    }
}

#[derive(Debug)]
struct DetectionProbe {
    parser: Option<fn(&mut FileAnalyze) -> bool>,
    parser_index: Option<usize>,
    candidates_run: usize,
    access_stats: AccessStats,
}

fn main() -> process::ExitCode {
    let config = match parse_args(env::args().skip(1)) {
        Ok(config) => config,
        Err(message) => {
            eprintln!("{message}");
            print_usage();
            return process::ExitCode::from(2);
        }
    };

    match run_probe(&config) {
        Ok(line) => {
            println!("{line}");
            process::ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("{message}");
            process::ExitCode::from(1)
        }
    }
}

fn run_probe(config: &Config) -> Result<String, String> {
    let mut phases = Vec::new();
    let start_total = Instant::now();

    let start = Instant::now();
    let metadata = fs::metadata(&config.path).map_err(|err| format!("metadata: {err}"))?;
    let file = fs::File::open(&config.path).map_err(|err| format!("open: {err}"))?;
    // SAFETY: the file is opened read-only and this probe never mutates it.
    let mmap = unsafe { memmap2::Mmap::map(&file) }.map_err(|err| format!("mmap: {err}"))?;
    phases.push(Phase::new("open_mmap", start.elapsed()));

    let bytes = mmap.as_ref();
    let source_len = bytes.len();

    let start = Instant::now();
    let detection = detect_with_probe(bytes);
    phases.push(Phase::with_access("detect", start.elapsed(), detection.access_stats));
    let Some(parser) = detection.parser else {
        return Err(format!("{}: no parser matched ({source_len} bytes)", config.path.display()));
    };

    let mut fa = FileAnalyze::from_backend(ReadBackend::from(&mmap));
    fa.set_option("demux", &config.demux);
    fa.set_option("trace_level", &config.trace);

    let start = Instant::now();
    if !parser(&mut fa) {
        return Err(format!("{}: selected parser failed", config.path.display()));
    }
    phases.push(Phase::with_access("parse_container", start.elapsed(), fa.access_stats()));

    let start = Instant::now();
    let local_offset_secs = match config.fixed_local_offset_secs {
        Some(offset) => offset,
        None => local_offset_seconds(),
    };
    phases.push(Phase::new("local_offset", start.elapsed()));

    let modified_unix_secs = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64);
    let info = FileLevelInfo {
        file_size: metadata.len(),
        extension: config.path.extension().and_then(|s| s.to_str()),
        modified_unix_secs,
        local_offset_secs,
    };

    let start = Instant::now();
    fill_file_level_fields(&mut fa, &info);
    phases.push(Phase::new("file_level_fields", start.elapsed()));

    let start = Instant::now();
    fill_computed_fields(fa.streams_mut());
    phases.push(Phase::new("computed_fields", start.elapsed()));

    let start = Instant::now();
    fa.duplicate_indices = find_duplicate_streams(fa.streams());
    phases.push(Phase::new("duplicate_streams", start.elapsed()));

    if !config.skip_tags {
        let before = fa.access_stats();
        let start = Instant::now();
        let _ = parse_tags(&mut fa);
        let tag_stats = delta_stats(before, fa.access_stats());
        phases.push(Phase::with_access("parse_tags", start.elapsed(), tag_stats));
    }

    let mut export_results = Vec::new();
    for mode in selected_export_modes(config.export_mode) {
        let start = Instant::now();
        let output = render_export(mode, fa.streams(), &config.path);
        phases.push(Phase::with_bytes(export_phase_name(mode), start.elapsed(), output.len()));

        let start = Instant::now();
        emit_output(config, mode, output.as_bytes())?;
        phases.push(Phase::with_bytes(
            output_phase_name(config.output_target),
            start.elapsed(),
            output.len(),
        ));
        export_results.push((export_mode_name(mode), output.len()));
    }

    let total_duration = start_total.elapsed();
    let format = fa
        .retrieve(StreamKind::General, 0, "Format")
        .map(|z| z.as_str().to_owned())
        .unwrap_or_default();
    let record = ProbeRecord {
        label: &config.label,
        path: &config.path,
        include_path: config.include_path,
        source_len,
        parser_index: detection.parser_index,
        candidates_run: detection.candidates_run,
        format: &format,
        total_duration,
        phases: &phases,
        export_results: &export_results,
    };
    Ok(record.to_json_line())
}

fn detect_with_probe(bytes: &[u8]) -> DetectionProbe {
    let mut combined = AccessStats::default();
    let mut candidates_run = 0;
    let limit = bytes.len().gt(&LARGE_INPUT_RAW_READ_LIMIT).then_some(LARGE_INPUT_RAW_READ_LIMIT);

    for (index, parser) in table().into_iter().enumerate() {
        let mut fa = if let Some(limit) = limit {
            FileAnalyze::with_raw_read_limit(bytes, limit)
        } else {
            FileAnalyze::new(bytes)
        };
        let matched = parser(&mut fa);
        candidates_run += 1;
        combined = add_stats(combined, fa.access_stats());
        if matched {
            return DetectionProbe {
                parser: Some(parser),
                parser_index: Some(index),
                candidates_run,
                access_stats: combined,
            };
        }
    }

    DetectionProbe { parser: None, parser_index: None, candidates_run, access_stats: combined }
}

fn selected_export_modes(mode: ExportMode) -> Vec<ExportMode> {
    match mode {
        ExportMode::None => Vec::new(),
        ExportMode::Text => vec![ExportMode::Text],
        ExportMode::Json => vec![ExportMode::Json],
        ExportMode::Summary => vec![ExportMode::Summary],
        ExportMode::All => vec![ExportMode::Text, ExportMode::Json, ExportMode::Summary],
    }
}

fn render_export(mode: ExportMode, streams: &revelo_core::StreamCollection, path: &Path) -> String {
    let path = path.to_string_lossy();
    match mode {
        ExportMode::Text => to_text(streams, &path),
        ExportMode::Json => to_json(streams, &path),
        ExportMode::Summary => to_summary(streams, &path),
        ExportMode::None | ExportMode::All => String::new(),
    }
}

fn emit_output(config: &Config, mode: ExportMode, bytes: &[u8]) -> Result<(), String> {
    match config.output_target {
        OutputTarget::Sink => {
            io::sink().write_all(bytes).map_err(|err| format!("sink output: {err}"))?;
        }
        OutputTarget::Stdout => {
            io::stdout().write_all(bytes).map_err(|err| format!("stdout output: {err}"))?;
        }
        OutputTarget::File => {
            let path = output_file_for_mode(config, mode)?;
            fs::write(&path, bytes).map_err(|err| format!("write {}: {err}", path.display()))?;
        }
    }
    Ok(())
}

fn output_file_for_mode(config: &Config, mode: ExportMode) -> Result<PathBuf, String> {
    let Some(path) = &config.output_file else {
        return Err("--output-target file requires --output-file".to_string());
    };
    if config.export_mode == ExportMode::All {
        let mut path = path.clone();
        let extension = export_mode_name(mode);
        path.set_extension(extension);
        Ok(path)
    } else {
        Ok(path.clone())
    }
}

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

    // SAFETY: `time` accepts null for return-only use. `localtime_r` writes
    // into a stack `Tm` matching the Unix C layout used by this diagnostic.
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

fn add_stats(a: AccessStats, b: AccessStats) -> AccessStats {
    AccessStats {
        peek_raw_calls: a.peek_raw_calls.saturating_add(b.peek_raw_calls),
        read_raw_calls: a.read_raw_calls.saturating_add(b.read_raw_calls),
        peek_raw_at_calls: a.peek_raw_at_calls.saturating_add(b.peek_raw_at_calls),
        bytes_requested: a.bytes_requested.saturating_add(b.bytes_requested),
        bytes_returned: a.bytes_returned.saturating_add(b.bytes_returned),
        max_request_len: a.max_request_len.max(b.max_request_len),
    }
}

fn delta_stats(before: AccessStats, after: AccessStats) -> AccessStats {
    AccessStats {
        peek_raw_calls: after.peek_raw_calls.saturating_sub(before.peek_raw_calls),
        read_raw_calls: after.read_raw_calls.saturating_sub(before.read_raw_calls),
        peek_raw_at_calls: after.peek_raw_at_calls.saturating_sub(before.peek_raw_at_calls),
        bytes_requested: after.bytes_requested.saturating_sub(before.bytes_requested),
        bytes_returned: after.bytes_returned.saturating_sub(before.bytes_returned),
        max_request_len: after.max_request_len.max(before.max_request_len),
    }
}

struct ProbeRecord<'a> {
    label: &'a str,
    path: &'a Path,
    include_path: bool,
    source_len: usize,
    parser_index: Option<usize>,
    candidates_run: usize,
    format: &'a str,
    total_duration: Duration,
    phases: &'a [Phase],
    export_results: &'a [(&'static str, usize)],
}

impl ProbeRecord<'_> {
    fn to_json_line(&self) -> String {
        let mut out = String::new();
        out.push('{');
        push_json_string_field(&mut out, "schema", "revelo_perf_probe_v1", true);
        push_json_string_field(&mut out, "label", self.label, false);
        if self.include_path {
            push_json_string_field(&mut out, "path", &self.path.to_string_lossy(), false);
        }
        push_json_u64_field(&mut out, "source_len", self.source_len as u64, false);
        match self.parser_index {
            Some(index) => push_json_u64_field(&mut out, "parser_index", index as u64, false),
            None => out.push_str(",\"parser_index\":null"),
        }
        push_json_u64_field(&mut out, "candidates_run", self.candidates_run as u64, false);
        push_json_string_field(&mut out, "format", self.format, false);
        push_json_f64_field(&mut out, "total_ms", duration_ms(self.total_duration), false);
        out.push_str(",\"phases\":[");
        for (index, phase) in self.phases.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            push_phase_json(&mut out, phase);
        }
        out.push(']');
        out.push_str(",\"exports\":[");
        for (index, (mode, bytes)) in self.export_results.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            out.push('{');
            push_json_string_field(&mut out, "mode", mode, true);
            push_json_u64_field(&mut out, "bytes", *bytes as u64, false);
            out.push('}');
        }
        out.push_str("]}");
        out
    }
}

fn push_phase_json(out: &mut String, phase: &Phase) {
    out.push('{');
    push_json_string_field(out, "name", phase.name, true);
    push_json_f64_field(out, "ms", duration_ms(phase.duration), false);
    if let Some(bytes) = phase.bytes {
        push_json_u64_field(out, "bytes", bytes as u64, false);
    }
    if let Some(stats) = phase.access_stats {
        out.push_str(",\"access_stats\":{");
        push_json_u64_field(out, "peek_raw_calls", stats.peek_raw_calls, true);
        push_json_u64_field(out, "read_raw_calls", stats.read_raw_calls, false);
        push_json_u64_field(out, "peek_raw_at_calls", stats.peek_raw_at_calls, false);
        push_json_u64_field(out, "bytes_requested", stats.bytes_requested, false);
        push_json_u64_field(out, "bytes_returned", stats.bytes_returned, false);
        push_json_u64_field(out, "max_request_len", stats.max_request_len as u64, false);
        out.push('}');
    }
    out.push('}');
}

fn duration_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn push_json_string_field(out: &mut String, key: &str, value: &str, first: bool) {
    if !first {
        out.push(',');
    }
    out.push('"');
    out.push_str(key);
    out.push_str("\":\"");
    out.push_str(&json_escape(value));
    out.push('"');
}

fn push_json_u64_field(out: &mut String, key: &str, value: u64, first: bool) {
    if !first {
        out.push(',');
    }
    out.push('"');
    out.push_str(key);
    out.push_str("\":");
    out.push_str(&value.to_string());
}

fn push_json_f64_field(out: &mut String, key: &str, value: f64, first: bool) {
    if !first {
        out.push(',');
    }
    out.push('"');
    out.push_str(key);
    out.push_str("\":");
    out.push_str(&format!("{value:.6}"));
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str(r"\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Config, String> {
    let mut path = None;
    let mut label = None;
    let mut export_mode = ExportMode::Text;
    let mut output_target = OutputTarget::Sink;
    let mut output_file = None;
    let mut demux = "frame".to_string();
    let mut trace = "0".to_string();
    let mut skip_tags = false;
    let mut include_path = false;
    let mut fixed_local_offset_secs = None;

    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--path" => path = Some(next_arg(&mut args, "--path")?.into()),
            "--label" => label = Some(next_arg(&mut args, "--label")?),
            "--export" => export_mode = parse_export_mode(&next_arg(&mut args, "--export")?)?,
            "--output-target" => {
                output_target = parse_output_target(&next_arg(&mut args, "--output-target")?)?;
            }
            "--output-file" => output_file = Some(next_arg(&mut args, "--output-file")?.into()),
            "--demux" => demux = next_arg(&mut args, "--demux")?,
            "--trace" => trace = next_arg(&mut args, "--trace")?,
            "--skip-tags" => skip_tags = true,
            "--include-path" => include_path = true,
            "--fixed-local-offset-secs" => {
                let raw = next_arg(&mut args, "--fixed-local-offset-secs")?;
                fixed_local_offset_secs =
                    Some(raw.parse().map_err(|_| format!("invalid offset: {raw}"))?);
            }
            "--help" | "-h" => return Err(String::new()),
            _ if arg.starts_with("--") => return Err(format!("unknown argument: {arg}")),
            _ if path.is_none() => path = Some(arg.into()),
            _ => return Err(format!("unexpected positional argument: {arg}")),
        }
    }

    let path: PathBuf = path.ok_or_else(|| "--path is required".to_string())?;
    let label = label.unwrap_or_else(|| {
        path.file_name().and_then(|s| s.to_str()).unwrap_or("input").to_string()
    });

    Ok(Config {
        path,
        label,
        export_mode,
        output_target,
        output_file,
        demux,
        trace,
        skip_tags,
        include_path,
        fixed_local_offset_secs,
    })
}

fn next_arg(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, String> {
    args.next().ok_or_else(|| format!("{flag} requires a value"))
}

fn parse_export_mode(value: &str) -> Result<ExportMode, String> {
    match value {
        "none" => Ok(ExportMode::None),
        "text" => Ok(ExportMode::Text),
        "json" => Ok(ExportMode::Json),
        "summary" => Ok(ExportMode::Summary),
        "all" => Ok(ExportMode::All),
        _ => Err(format!("invalid --export value: {value}")),
    }
}

fn parse_output_target(value: &str) -> Result<OutputTarget, String> {
    match value {
        "sink" => Ok(OutputTarget::Sink),
        "file" => Ok(OutputTarget::File),
        "stdout" => Ok(OutputTarget::Stdout),
        _ => Err(format!("invalid --output-target value: {value}")),
    }
}

fn export_phase_name(mode: ExportMode) -> &'static str {
    match mode {
        ExportMode::Text => "export_text",
        ExportMode::Json => "export_json",
        ExportMode::Summary => "export_summary",
        ExportMode::None | ExportMode::All => "export_none",
    }
}

fn output_phase_name(target: OutputTarget) -> &'static str {
    match target {
        OutputTarget::Sink => "output_sink",
        OutputTarget::File => "output_file",
        OutputTarget::Stdout => "output_stdout",
    }
}

fn export_mode_name(mode: ExportMode) -> &'static str {
    match mode {
        ExportMode::None => "none",
        ExportMode::Text => "text",
        ExportMode::Json => "json",
        ExportMode::Summary => "summary",
        ExportMode::All => "all",
    }
}

fn print_usage() {
    eprintln!(
        "usage: perf_probe --path FILE [--label LABEL] [--export none|text|json|summary|all] \\
         [--output-target sink|file|stdout] [--output-file FILE] [--skip-tags] \\
         [--include-path] [--fixed-local-offset-secs N]"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_escape_handles_control_characters() {
        assert_eq!(json_escape("a\"b\\c\n"), "a\\\"b\\\\c\\n");
    }

    #[test]
    fn record_json_contains_stable_top_level_fields() {
        let phase = Phase::with_access(
            "detect",
            Duration::from_micros(1500),
            AccessStats {
                peek_raw_calls: 1,
                read_raw_calls: 2,
                peek_raw_at_calls: 3,
                bytes_requested: 4,
                bytes_returned: 5,
                max_request_len: 6,
            },
        );
        let record = ProbeRecord {
            label: "fixture",
            path: Path::new("fixture.mp4"),
            include_path: false,
            source_len: 42,
            parser_index: Some(10),
            candidates_run: 11,
            format: "MPEG-4",
            total_duration: Duration::from_millis(2),
            phases: &[phase],
            export_results: &[("json", 123)],
        };
        let line = record.to_json_line();
        assert!(line.contains("\"schema\":\"revelo_perf_probe_v1\""), "{line}");
        assert!(line.contains("\"parser_index\":10"), "{line}");
        assert!(line.contains("\"bytes_requested\":4"), "{line}");
    }

    #[test]
    fn record_json_omits_private_path_by_default() {
        let record = ProbeRecord {
            label: "public-label",
            path: Path::new("local-only-fixture/sample.mp4"),
            include_path: false,
            source_len: 42,
            parser_index: Some(10),
            candidates_run: 11,
            format: "MPEG-4",
            total_duration: Duration::from_millis(2),
            phases: &[],
            export_results: &[],
        };
        let line = record.to_json_line();
        assert!(!line.contains("\"path\""), "{line}");
        assert!(!line.contains("local-only-fixture/sample.mp4"), "{line}");
    }

    #[test]
    fn record_json_includes_path_only_when_requested() {
        let record = ProbeRecord {
            label: "public-label",
            path: Path::new("local-only-fixture/sample.mp4"),
            include_path: true,
            source_len: 42,
            parser_index: Some(10),
            candidates_run: 11,
            format: "MPEG-4",
            total_duration: Duration::from_millis(2),
            phases: &[],
            export_results: &[],
        };
        let line = record.to_json_line();
        assert!(line.contains("\"path\":\"local-only-fixture/sample.mp4\""), "{line}");
    }

    #[test]
    fn access_stats_delta_is_saturating() {
        let before = AccessStats {
            peek_raw_calls: 1,
            read_raw_calls: 1,
            peek_raw_at_calls: 1,
            bytes_requested: 10,
            bytes_returned: 10,
            max_request_len: 100,
        };
        let after = AccessStats {
            peek_raw_calls: 3,
            read_raw_calls: 1,
            peek_raw_at_calls: 4,
            bytes_requested: 40,
            bytes_returned: 35,
            max_request_len: 80,
        };
        let delta = delta_stats(before, after);
        assert_eq!(delta.peek_raw_calls, 2);
        assert_eq!(delta.bytes_requested, 30);
        assert_eq!(delta.max_request_len, 100);
    }

    #[test]
    fn args_do_not_require_private_paths() {
        let config = parse_args([
            "--path".to_string(),
            "sample.mp4".to_string(),
            "--label".to_string(),
            "public-label".to_string(),
            "--export".to_string(),
            "all".to_string(),
        ])
        .expect("args");
        assert_eq!(config.path, PathBuf::from("sample.mp4"));
        assert_eq!(config.label, "public-label");
        assert_eq!(config.export_mode, ExportMode::All);
        assert!(!config.include_path);
    }

    #[test]
    fn include_path_arg_is_explicit_opt_in() {
        let config = parse_args([
            "--path".to_string(),
            "sample.mp4".to_string(),
            "--include-path".to_string(),
        ])
        .expect("args");
        assert!(config.include_path);
    }
}
