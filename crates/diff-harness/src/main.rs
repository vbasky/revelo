use std::env;
use std::fs;
use std::process::{Command, ExitCode};

use mediainfo_core::{FileAnalyze, StreamKind};
use mediainfo_parsers_container::parse_wav;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: diff-harness <media-file> [<media-file> ...]");
        return ExitCode::from(2);
    }

    let mut any_failed = false;
    for path in &args {
        match (run_oracle(path), run_rust_engine(path)) {
            (Ok(oracle_xml), Ok(rust_summary)) => {
                println!("=== ORACLE XML  ({path}) ===");
                println!("{oracle_xml}");
                println!("=== RUST ENGINE ({path}) ===");
                println!("{rust_summary}");
            }
            (Err(o), _) => {
                eprintln!("oracle failed for {path}: {o}");
                any_failed = true;
            }
            (_, Err(r)) => {
                eprintln!("rust engine failed for {path}: {r}");
                any_failed = true;
            }
        }
    }

    if any_failed { ExitCode::from(1) } else { ExitCode::SUCCESS }
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
    let mut fa = FileAnalyze::new(&bytes);

    let parsed = parse_wav(&mut fa);
    if !parsed {
        return Ok(format!(
            "<no rust parser matched this file ({} bytes); supported: WAV>",
            bytes.len()
        ));
    }

    Ok(render_streams(&fa))
}

fn render_streams(fa: &FileAnalyze) -> String {
    let mut out = String::new();
    for kind in [
        StreamKind::General,
        StreamKind::Video,
        StreamKind::Audio,
        StreamKind::Text,
        StreamKind::Other,
        StreamKind::Image,
        StreamKind::Menu,
    ] {
        let count = fa.Count_Get(kind);
        for pos in 0..count {
            out.push_str(&format!("[{} #{}]\n", kind.name(), pos));
            if let Some(stream) = fa.streams().stream(kind, pos) {
                for (k, v) in stream.iter() {
                    out.push_str(&format!("  {k} = {v}\n"));
                }
            }
        }
    }
    out
}
