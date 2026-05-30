use std::env;
use std::fs;
use std::path::Path;
use std::process::{Command, ExitCode};
use std::time::UNIX_EPOCH;

use revelo_core::{FileAnalyze, FileLevelInfo, fill_file_level_fields};
use revelo_dispatcher::detect;
use revelo_export::to_xml;

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
        eprintln!("usage: revelo-diff [--rust-xml] [--strict] <media-file> [<media-file> ...]");
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
    Ok(Report { path: path.to_owned(), oracle_xml, rust_xml, strict })
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
    let xml = String::from_utf8_lossy(&output.stdout).into_owned();
    // The rust engine omits the `<creatingLibrary>` header (it identifies
    // the tool, not the file), so strip the oracle's copy to keep the
    // diff focused on real per-stream differences.
    Ok(strip_creating_library(&xml))
}

/// Drop the single `<creatingLibrary …>…</creatingLibrary>` line the
/// oracle emits, so its absence in the rust output isn't reported as a
/// difference.
fn strip_creating_library(xml: &str) -> String {
    let mut out = String::with_capacity(xml.len());
    for line in xml.lines() {
        if line.trim_start().starts_with("<creatingLibrary") {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

fn run_rust_engine(path: &str) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|e| format!("read failed: {e}"))?;
    let metadata = fs::metadata(path).map_err(|e| format!("stat failed: {e}"))?;

    let Some(parser) = detect(&bytes) else {
        return Err(format!("no rust parser matched ({} bytes)", bytes.len()));
    };

    let mut fa = FileAnalyze::new(&bytes);
    parser(&mut fa);

    // Shared with the CLI via revelo-core — single source of truth for
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

    Ok(to_xml(fa.streams(), path))
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
            dp[i][j] =
                if o[i] == r[j] { dp[i + 1][j + 1] + 1 } else { dp[i + 1][j].max(dp[i][j + 1]) };
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
