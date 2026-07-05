mod cdp;
mod compare;
mod config;
mod evidence;
mod fixtures;
mod render;
mod util;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "revelo-bench")]
#[command(about = "Developer-only benchmark tooling for Revelo")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Generate synthetic benchmark fixtures from a manifest.
    GenerateFixtures {
        #[arg(long)]
        manifest: PathBuf,
        #[arg(long, default_value = "target/perf-fixtures")]
        out_dir: PathBuf,
    },
    /// Run Hyperfine comparisons from a manifest and write sanitized JSON.
    Compare(compare::CompareArgs),
    /// Render benchmark HTML and optionally capture a PNG through Chrome CDP.
    RenderTable(render::RenderArgs),
    /// Run compare plus table rendering with one shared run id.
    Evidence(evidence::EvidenceArgs),
    /// Run lightweight self-tests for benchmark tooling.
    SelfTest,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run() -> util::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::GenerateFixtures { manifest, out_dir } => {
            let manifest = config::Manifest::load(&manifest)?;
            config::validate_manifest(&manifest)?;
            let fixtures = fixtures::generate_manifest_fixtures(&manifest, &out_dir)?;
            println!("{}", serde_json::to_string_pretty(&fixtures)?);
        }
        Command::Compare(args) => {
            let path = compare::run(args)?;
            println!("{}", path.display());
        }
        Command::RenderTable(args) => {
            let output = render::run(args)?;
            println!("{}", output.html.display());
            if let Some(png) = output.png {
                println!("{}", png.display());
            }
        }
        Command::Evidence(args) => {
            let path = evidence::run(args)?;
            println!("{}", path.display());
        }
        Command::SelfTest => self_test()?,
    }
    Ok(())
}

fn self_test() -> util::Result<()> {
    config::self_test()?;
    fixtures::self_test()?;
    render::self_test()?;
    cdp::self_test()?;
    Ok(())
}
