//! revelo-exif-diff — differential testing of revelo's EXIF / maker-note
//! extraction against the `exiftool` oracle.
//!
//! For each file it runs the revelo engine (with the `exiftool-tables`
//! feature on) and the real `exiftool -G1 -s`, then reports how many of
//! exiftool's EXIF/maker-note tags revelo also extracts (matched by tag
//! name, case-insensitive). This is the validation gate for growing the
//! ExifTool-derived tables: every addition can be measured against the
//! binary it is derived from.
//!
//! Usage: revelo-exif-diff [--verbose] <file> [<file> ...]
//!
//! Limitations: name-level coverage only (values aren't compared yet),
//! and revelo currently routes its EXIF/maker-note tags into the `Exif`
//! stream, so that is the set compared.

use std::collections::BTreeSet;
use std::env;
use std::process::{Command, ExitCode};

use revelo_core::{FileAnalyze, StreamKind};
use revelo_dispatcher::detect;
use revelo_parsers_tag::parse_tags;

/// exiftool family-1 groups that are NOT EXIF/maker-note metadata, so are
/// excluded from the parity comparison.
const NON_EXIF_GROUPS: &[&str] =
    &["ExifTool", "System", "File", "Composite", "ICC_Profile", "ICC-header", "Photoshop", "IPTC"];

fn main() -> ExitCode {
    let mut args: Vec<String> = env::args().skip(1).collect();
    let verbose = take_flag(&mut args, "--verbose");
    if args.is_empty() {
        eprintln!("usage: revelo-exif-diff [--verbose] <file> [<file> ...]");
        return ExitCode::from(2);
    }

    let mut any_failed = false;
    let (mut grand_total, mut grand_matched) = (0usize, 0usize);

    for path in &args {
        match diff_one(path, verbose) {
            Ok((total, matched)) => {
                grand_total += total;
                grand_matched += matched;
            }
            Err(msg) => {
                eprintln!("{path}: {msg}");
                any_failed = true;
            }
        }
    }

    if grand_total > 0 {
        println!(
            "\nTOTAL: {grand_matched}/{grand_total} exiftool EXIF/maker-note tags matched ({}%)",
            pct(grand_matched, grand_total)
        );
    }
    if any_failed { ExitCode::from(1) } else { ExitCode::SUCCESS }
}

fn diff_one(path: &str, verbose: bool) -> Result<(usize, usize), String> {
    let revelo_tags = run_revelo(path)?;
    let oracle = run_exiftool(path)?;

    // Compare by lower-cased tag name (revelo's field names and exiftool's
    // short tag names use the same ExifTool vocabulary under the feature).
    let revelo_lc: BTreeSet<String> = revelo_tags.iter().map(|t| t.to_lowercase()).collect();

    let mut total = 0usize;
    let mut matched = 0usize;
    let mut missing: Vec<(String, String)> = Vec::new();
    for (group, tag) in &oracle {
        // Skip non-EXIF/maker-note groups: XMP, and ICC profile internals
        // (ICC-header/-view/-meas/-chrm/…), which aren't maker-note parity.
        if NON_EXIF_GROUPS.contains(&group.as_str())
            || group.starts_with("XMP")
            || group.starts_with("ICC")
        {
            continue;
        }
        total += 1;
        if revelo_lc.contains(&tag.to_lowercase()) {
            matched += 1;
        } else {
            missing.push((group.clone(), tag.clone()));
        }
    }

    println!("=== {path} ===");
    println!(
        "  exiftool EXIF/maker-note tags: {total}\n  matched by revelo:             {matched} ({}%)\n  missing:                       {}",
        pct(matched, total),
        missing.len()
    );
    if verbose {
        for (group, tag) in missing.iter().take(40) {
            println!("    - [{group}] {tag}");
        }
        if missing.len() > 40 {
            println!("    … and {} more", missing.len() - 40);
        }
    }
    Ok((total, matched))
}

/// Run the revelo engine and collect the tag (field) names it extracted
/// into the `Exif` stream.
fn run_revelo(path: &str) -> Result<Vec<String>, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("read failed: {e}"))?;
    let Some(parser) = detect(&bytes) else {
        return Err(format!("no rust parser matched ({} bytes)", bytes.len()));
    };
    let mut fa = FileAnalyze::new(&bytes);
    parser(&mut fa);
    parse_tags(&mut fa);

    let mut names = Vec::new();
    let streams = fa.streams();
    for kind in [StreamKind::Exif, StreamKind::Iptc, StreamKind::General] {
        for pos in 0..streams.stream_count(kind) {
            if let Some(stream) = streams.stream(kind, pos) {
                for (k, _) in stream.iter() {
                    names.push(k.to_string());
                }
            }
        }
    }
    Ok(names)
}

/// Run `exiftool -G1 -s` and parse `[Group] TagName : Value` lines into
/// (group, tag) pairs.
fn run_exiftool(path: &str) -> Result<Vec<(String, String)>, String> {
    let output = Command::new("exiftool")
        .args(["-G1", "-s", "-e"]) // -e: don't compute Composite tags
        .arg(path)
        .output()
        .map_err(|e| format!("failed to spawn `exiftool`: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "`exiftool` exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim_start();
        if !line.starts_with('[') {
            continue;
        }
        let Some(close) = line.find(']') else { continue };
        let group = line[1..close].to_string();
        let rest = line[close + 1..].trim_start();
        let Some(colon) = rest.find(" : ") else { continue };
        let tag = rest[..colon].trim().to_string();
        if !tag.is_empty() {
            out.push((group, tag));
        }
    }
    Ok(out)
}

fn take_flag(args: &mut Vec<String>, flag: &str) -> bool {
    if let Some(i) = args.iter().position(|a| a == flag) {
        args.remove(i);
        true
    } else {
        false
    }
}

fn pct(num: usize, den: usize) -> u32 {
    if den == 0 { 0 } else { ((num as f64 / den as f64) * 100.0).round() as u32 }
}
