use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use clap::Args;
use serde_json::{Value, json};

use crate::config::{BenchCase, Manifest, validate_manifest};
use crate::fixtures;
use crate::render;
use crate::util::{
    Result, command_text, err, relative_tool_path, repo_root, resolve_repo_path, safe_id, which,
};

#[derive(Debug, Args)]
pub(crate) struct CompareArgs {
    #[arg(long)]
    pub(crate) manifest: PathBuf,
    #[arg(long, default_value = "target/perf-investigation")]
    pub(crate) out_dir: PathBuf,
    #[arg(long, default_value = "target/perf-fixtures")]
    pub(crate) fixture_dir: PathBuf,
    #[arg(long)]
    pub(crate) run_id: Option<String>,
    #[arg(long)]
    pub(crate) warmups: Option<u32>,
    #[arg(long)]
    pub(crate) runs: Option<u32>,
    #[arg(long, default_value = "all")]
    pub(crate) probe_export: String,
    #[arg(long, default_value = "sink")]
    pub(crate) probe_output_target: String,
    #[arg(long)]
    pub(crate) table_config: Option<PathBuf>,
    #[arg(long)]
    pub(crate) no_render_table: bool,
    #[arg(long)]
    pub(crate) render_png: bool,
    #[arg(long)]
    pub(crate) chrome_path: Option<PathBuf>,
    #[arg(long)]
    pub(crate) no_build: bool,
    #[arg(long)]
    pub(crate) skip_mediainfo: bool,
    #[arg(long)]
    pub(crate) skip_ffprobe: bool,
}

#[derive(Debug)]
pub(crate) struct CompareRun {
    pub(crate) results_path: PathBuf,
    pub(crate) run_dir: PathBuf,
    pub(crate) run_id: String,
}

pub(crate) fn run(args: CompareArgs) -> Result<PathBuf> {
    Ok(run_compare(args)?.results_path)
}

pub(crate) fn run_compare(args: CompareArgs) -> Result<CompareRun> {
    let manifest = Manifest::load(&args.manifest)?;
    validate_manifest(&manifest)?;
    let run_id = args.run_id.clone().unwrap_or_else(timestamp_run_id);
    let run_dir = args.out_dir.join(&run_id);
    let fixture_dir = args.fixture_dir.join(&run_id);
    fs::create_dir_all(&run_dir)?;
    fs::create_dir_all(&fixture_dir)?;

    if !args.no_build {
        let mut build = Command::new("cargo");
        build.args([
            "build",
            "-p",
            "revelo-cli",
            "--bin",
            "revelo",
            "--example",
            "perf_probe",
            "--release",
        ]);
        crate::util::run_checked(&mut build)?;
    }

    let tools = Tools::resolve(&args)?;
    let revelo_versions = resolve_revelo_versions(&manifest)?;
    let warmups = args.warmups.or(manifest.settings.warmups).unwrap_or(2);
    let runs = args.runs.or(manifest.settings.runs).unwrap_or(10);
    let render_png = args.render_png || manifest.settings.render_png.unwrap_or(false);

    let context = RunContext {
        args: &args,
        tools: &tools,
        revelo_versions: &revelo_versions,
        warmups,
        runs,
        run_dir: &run_dir,
        fixture_dir: &fixture_dir,
    };
    let mut cases = Vec::new();
    for case in &manifest.cases {
        cases.push(run_case(case, &context)?);
    }

    let output = json!({
        "schema": "revelo_bench_compare_v1",
        "run_id": run_id,
        "repo": repo_metadata(),
        "environment": environment_snapshot(),
        "tools": tools.versions(),
        "revelo_versions": sanitize_revelo_versions(&revelo_versions),
        "measurement_backend": {
            "name": "hyperfine",
            "version": tool_version(tools.hyperfine.as_path(), &["--version"]),
            "required": true
        },
        "settings": {
            "warmups": warmups,
            "runs": runs,
            "probe_export": args.probe_export,
            "probe_output_target": args.probe_output_target,
            "fixture_dir": args.fixture_dir.display().to_string()
        },
        "cases": cases
    });

    let results_path = run_dir.join("results.json");
    fs::write(&results_path, serde_json::to_string_pretty(&output)? + "\n")?;
    if !args.no_render_table {
        let render_args = render::RenderArgs {
            results: results_path.clone(),
            output: Some(run_dir.join("benchmark-table.html")),
            config: args.table_config,
            render_png,
            chrome_path: args.chrome_path,
            selector: None,
        };
        let _ = render::run(render_args)?;
    }
    Ok(CompareRun { results_path, run_dir, run_id })
}

struct RunContext<'a> {
    args: &'a CompareArgs,
    tools: &'a Tools,
    revelo_versions: &'a [ResolvedReveloVersion],
    warmups: u32,
    runs: u32,
    run_dir: &'a Path,
    fixture_dir: &'a Path,
}

fn run_case(case: &BenchCase, context: &RunContext<'_>) -> Result<Value> {
    let path = resolve_case_path(case, context.fixture_dir)?;
    if !path.exists() {
        return Err(err(format!("missing case path for {}: {}", case.label, path.display())));
    }
    let mut measurements = serde_json::Map::new();
    for version in context.revelo_versions {
        let command = vec![version.path.display().to_string(), path.display().to_string()];
        measurements.insert(
            version.id.clone(),
            measure_command(
                context.tools,
                &version.id,
                &command,
                context.warmups,
                context.runs,
                context.run_dir,
                &case.label,
            )?,
        );
    }

    let mut probe_cmd = vec![
        context.tools.perf_probe.display().to_string(),
        "--path".to_owned(),
        path.display().to_string(),
        "--label".to_owned(),
        case.label.clone(),
        "--export".to_owned(),
        context.args.probe_export.clone(),
        "--output-target".to_owned(),
        context.args.probe_output_target.clone(),
    ];
    if context.args.probe_output_target == "file" {
        probe_cmd.extend([
            "--output-file".to_owned(),
            context
                .run_dir
                .join(format!("{}.probe-output", safe_id(&case.label)))
                .display()
                .to_string(),
        ]);
    }
    let probe_process = measure_command(
        context.tools,
        "revelo_perf_probe",
        &probe_cmd,
        context.warmups,
        context.runs,
        context.run_dir,
        &case.label,
    )?;
    measurements.insert(
        "revelo_perf_probe".to_owned(),
        json!({
            "process_ms": probe_process,
            "records": collect_probe_records(&probe_cmd, context.runs)?,
            "records_diagnostic_only": true
        }),
    );

    if let Some(mediainfo) = &context.tools.mediainfo {
        let command = vec![mediainfo.display().to_string(), path.display().to_string()];
        measurements.insert(
            "mediainfo".to_owned(),
            measure_command(
                context.tools,
                "mediainfo",
                &command,
                context.warmups,
                context.runs,
                context.run_dir,
                &case.label,
            )?,
        );
    }
    if let Some(ffprobe) = &context.tools.ffprobe {
        let command = vec![
            ffprobe.display().to_string(),
            "-v".to_owned(),
            "error".to_owned(),
            "-show_format".to_owned(),
            "-show_streams".to_owned(),
            path.display().to_string(),
        ];
        measurements.insert(
            "ffprobe".to_owned(),
            measure_command(
                context.tools,
                "ffprobe",
                &command,
                context.warmups,
                context.runs,
                context.run_dir,
                &case.label,
            )?,
        );
    }

    Ok(json!({
        "id": case.id.clone().unwrap_or_else(|| safe_id(&case.label)),
        "label": case.label,
        "class": case.class_name,
        "format": case.format.clone().or_else(|| case.container.clone()).unwrap_or_default(),
        "container": case.container.clone().or_else(|| case.format.clone()).unwrap_or_default(),
        "codec": case.codec.clone().unwrap_or_default(),
        "layout": case.layout.clone().unwrap_or_default(),
        "source": case.source.clone().unwrap_or_default(),
        "size_bytes": path.metadata()?.len(),
        "synthetic_kind": case.synthetic.as_ref().map(|synthetic| synthetic.kind.clone()).unwrap_or_default(),
        "measurements": measurements
    }))
}

fn resolve_case_path(case: &BenchCase, fixture_dir: &Path) -> Result<PathBuf> {
    if case.synthetic.is_some() {
        fixtures::generate_case_fixture(case, fixture_dir)
    } else {
        let path =
            case.path.as_ref().ok_or_else(|| err(format!("case {} missing path", case.label)))?;
        if path.is_absolute() { Ok(path.clone()) } else { Ok(repo_root()?.join(path)) }
    }
}

fn measure_command(
    tools: &Tools,
    name: &str,
    command: &[String],
    warmups: u32,
    runs: u32,
    run_dir: &Path,
    label: &str,
) -> Result<Value> {
    let export_path = run_dir.join(format!("{}-{}.hyperfine.json", safe_id(label), safe_id(name)));
    let shell_command = shell_join(command);
    let output = Command::new(&tools.hyperfine)
        .args(["--warmup", &warmups.to_string(), "--runs", &runs.to_string(), "--export-json"])
        .arg(&export_path)
        .args(["--command-name", name])
        .arg(&shell_command)
        .output()?;
    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr);
        return Err(err(format!("hyperfine failed for {name}: {}", message.trim())));
    }
    let data: Value = serde_json::from_str(&fs::read_to_string(&export_path)?)?;
    let _ = fs::remove_file(&export_path);
    let result = data
        .get("results")
        .and_then(Value::as_array)
        .and_then(|results| results.first())
        .ok_or_else(|| err(format!("hyperfine produced no results for {name}")))?;
    Ok(summarize_hyperfine_result(result))
}

fn summarize_hyperfine_result(result: &Value) -> Value {
    let times = result
        .get("times")
        .and_then(Value::as_array)
        .map(|times| {
            times.iter().filter_map(Value::as_f64).map(|value| value * 1000.0).collect::<Vec<_>>()
        })
        .unwrap_or_default();
    json!({
        "runs": times.len(),
        "mean_ms": seconds_to_ms(result.get("mean")),
        "median_ms": seconds_to_ms(result.get("median")),
        "min_ms": seconds_to_ms(result.get("min")),
        "max_ms": seconds_to_ms(result.get("max")),
        "stddev_ms": seconds_to_ms(result.get("stddev")),
        "samples_ms": times
    })
}

fn seconds_to_ms(value: Option<&Value>) -> Option<f64> {
    value.and_then(Value::as_f64).map(|value| value * 1000.0)
}

fn collect_probe_records(command: &[String], runs: u32) -> Result<Vec<Value>> {
    let mut records = Vec::new();
    for _ in 0..runs {
        let output = Command::new(&command[0]).args(&command[1..]).output()?;
        if !output.status.success() {
            return Err(err("perf_probe command failed"));
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let line = stdout.lines().last().ok_or_else(|| err("perf_probe produced no output"))?;
        let mut record: Value = serde_json::from_str(line)?;
        if let Some(object) = record.as_object_mut() {
            object.remove("path");
        }
        records.push(record);
    }
    Ok(records)
}

#[derive(Debug)]
struct Tools {
    hyperfine: PathBuf,
    revelo: PathBuf,
    perf_probe: PathBuf,
    mediainfo: Option<PathBuf>,
    ffprobe: Option<PathBuf>,
}

impl Tools {
    fn resolve(args: &CompareArgs) -> Result<Self> {
        let root = repo_root()?;
        let hyperfine = which("hyperfine").ok_or_else(|| {
            err("hyperfine is required for bench-compare; install it with `brew install hyperfine`")
        })?;
        let revelo = root.join("target/release/revelo");
        let perf_probe = root.join("target/release/examples/perf_probe");
        if !perf_probe.exists() {
            return Err(err(format!("missing built tool: {}", perf_probe.display())));
        }
        let mediainfo = if args.skip_mediainfo {
            None
        } else {
            Some(
                which("mediainfo")
                    .ok_or_else(|| err("mediainfo is required unless --skip-mediainfo is used"))?,
            )
        };
        let ffprobe = if args.skip_ffprobe {
            None
        } else {
            Some(
                which("ffprobe")
                    .ok_or_else(|| err("ffprobe is required unless --skip-ffprobe is used"))?,
            )
        };
        Ok(Self { hyperfine, revelo, perf_probe, mediainfo, ffprobe })
    }

    fn versions(&self) -> Value {
        json!({
            "hyperfine": tool_version(self.hyperfine.as_path(), &["--version"]),
            "revelo": {"available": self.revelo.exists(), "path": relative_tool_path(&self.revelo)},
            "perf_probe": {"available": self.perf_probe.exists(), "path": relative_tool_path(&self.perf_probe)},
            "mediainfo": self.mediainfo.as_ref().and_then(|path| tool_version(path, &["--Version"])),
            "ffprobe": self.ffprobe.as_ref().and_then(|path| tool_version(path, &["-version"]))
        })
    }
}

#[derive(Debug, Clone)]
struct ResolvedReveloVersion {
    id: String,
    label: String,
    path: PathBuf,
}

fn resolve_revelo_versions(manifest: &Manifest) -> Result<Vec<ResolvedReveloVersion>> {
    if manifest.revelo_versions.is_empty() {
        return Ok(vec![ResolvedReveloVersion {
            id: "revelo_cli_text".to_owned(),
            label: "Revelo".to_owned(),
            path: repo_root()?.join("target/release/revelo"),
        }]);
    }
    manifest
        .revelo_versions
        .iter()
        .map(|version| {
            let path = resolve_repo_path(&version.path)?;
            if !path.exists() {
                return Err(err(format!(
                    "missing Revelo version binary for {}: {}",
                    version.id,
                    path.display()
                )));
            }
            Ok(ResolvedReveloVersion { id: version.id.clone(), label: version.label.clone(), path })
        })
        .collect()
}

fn sanitize_revelo_versions(versions: &[ResolvedReveloVersion]) -> Vec<Value> {
    versions
        .iter()
        .map(|version| {
            json!({
                "id": version.id,
                "label": version.label,
                "path": relative_tool_path(&version.path)
            })
        })
        .collect()
}

fn repo_metadata() -> Value {
    json!({
        "branch": command_text("git", &["branch", "--show-current"]),
        "commit": command_text("git", &["rev-parse", "--short", "HEAD"])
    })
}

fn environment_snapshot() -> Value {
    json!({
        "machine": command_text("uname", &["-m"]),
        "system": command_text("uname", &["-s"]),
        "release": command_text("uname", &["-r"]),
        "processor": command_text("sysctl", &["-n", "machdep.cpu.brand_string"]),
        "cpu_count": std::thread::available_parallelism().ok().map(usize::from),
        "sw_vers": command_text("sw_vers", &[]),
        "rustc": command_text("rustc", &["--version", "--verbose"]),
        "cargo": command_text("cargo", &["--version", "--verbose"])
    })
}

fn tool_version(path: &Path, args: &[&str]) -> Option<String> {
    command_text(path.as_os_str(), args)
        .and_then(|text| text.lines().find(|line| !line.trim().is_empty()).map(ToOwned::to_owned))
}

fn timestamp_run_id() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default();
    format!("run-{seconds}")
}

fn shell_join(command: &[String]) -> String {
    command
        .iter()
        .map(|part| {
            if part.chars().all(|char| char.is_ascii_alphanumeric() || "-_./:=+".contains(char)) {
                part.clone()
            } else {
                format!("'{}'", part.replace('\'', "'\\''"))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
