use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use clap::Args;
use serde_json::json;

use crate::compare::{self, CompareArgs};
use crate::util::Result;

#[derive(Debug, Args)]
pub(crate) struct EvidenceArgs {
    #[arg(long)]
    pub(crate) manifest: PathBuf,
    #[arg(long, default_value = "target/perf-investigation")]
    pub(crate) out_dir: PathBuf,
    #[arg(long, default_value = "target/perf-fixtures")]
    pub(crate) fixture_dir: PathBuf,
    #[arg(long)]
    pub(crate) run_id: Option<String>,
    #[arg(long, default_value = "scripts/perf/table.config.example.json")]
    pub(crate) table_config: PathBuf,
    #[arg(long)]
    pub(crate) warmups: Option<u32>,
    #[arg(long)]
    pub(crate) runs: Option<u32>,
    #[arg(long)]
    pub(crate) chrome_path: Option<PathBuf>,
}

pub(crate) fn run(args: EvidenceArgs) -> Result<PathBuf> {
    let run_id = args.run_id.unwrap_or_else(timestamp_run_id);
    let compare = compare::run_compare(CompareArgs {
        manifest: args.manifest.clone(),
        out_dir: args.out_dir.clone(),
        fixture_dir: args.fixture_dir,
        run_id: Some(run_id.clone()),
        warmups: args.warmups,
        runs: args.runs,
        probe_export: "all".to_owned(),
        probe_output_target: "sink".to_owned(),
        table_config: Some(args.table_config),
        no_render_table: false,
        render_png: true,
        chrome_path: args.chrome_path,
        no_build: false,
        skip_mediainfo: false,
        skip_ffprobe: false,
    })?;
    let summary_path = compare.run_dir.join("evidence-summary.json");
    fs::write(
        &summary_path,
        serde_json::to_string_pretty(&json!({
            "schema": "revelo_benchmark_evidence_v1",
            "run_id": compare.run_id,
            "manifest": args.manifest,
            "outputs": [
                {"name": "bench_compare", "output": compare.results_path},
                {"name": "render_table", "output": compare.run_dir.join("benchmark-table.html")},
                {"name": "capture_table", "output": compare.run_dir.join("benchmark-table.png")}
            ]
        }))? + "\n",
    )?;
    Ok(summary_path)
}

fn timestamp_run_id() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default();
    format!("run-{seconds}")
}
