use std::env;
use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: diff-harness <media-file> [<media-file> ...]");
        return ExitCode::from(2);
    }

    let mut any_failed = false;
    for path in &args {
        match run_oracle(path) {
            Ok(xml) => {
                println!("=== ORACLE ({path}) ===");
                println!("{xml}");
                println!("=== RUST    ({path}) ===");
                println!("<not implemented yet>");
                println!("=== DIFF    ({path}) ===");
                println!("<rust engine missing; full diff pending>");
            }
            Err(msg) => {
                eprintln!("oracle failed for {path}: {msg}");
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
