use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use clap::Args;
use serde_json::Value;

use crate::cdp;
use crate::config::{LatencyTier, TableConfig};
use crate::util::{Result, err};

#[derive(Debug, Args)]
pub(crate) struct RenderArgs {
    #[arg(long)]
    pub(crate) results: PathBuf,
    #[arg(long)]
    pub(crate) output: Option<PathBuf>,
    #[arg(long)]
    pub(crate) config: Option<PathBuf>,
    #[arg(long)]
    pub(crate) render_png: bool,
    #[arg(long)]
    pub(crate) chrome_path: Option<PathBuf>,
    #[arg(long)]
    pub(crate) selector: Option<String>,
}

#[derive(Debug)]
pub(crate) struct RenderOutput {
    pub(crate) html: PathBuf,
    pub(crate) png: Option<PathBuf>,
}

pub(crate) fn run(args: RenderArgs) -> Result<RenderOutput> {
    let results: Value = serde_json::from_str(&fs::read_to_string(&args.results)?)?;
    let config = TableConfig::load(args.config.as_deref())?;
    let output = args.output.unwrap_or_else(|| args.results.with_file_name("benchmark-table.html"));
    let css_path = output.with_file_name("benchmark-table.css");
    fs::create_dir_all(output.parent().ok_or_else(|| err("output path has no parent"))?)?;
    fs::write(&css_path, stylesheet())?;
    fs::write(&output, render_html(&results, &config)?)?;

    let mut png = None;
    if args.render_png {
        let png_path = output.with_extension("png");
        let selector = args.selector.unwrap_or_else(|| format!("#{}", config.capture_id()));
        match cdp::capture_html(&output, &png_path, &selector, args.chrome_path.as_deref()) {
            Ok(()) => {
                write_png_status(&args.results, "generated", None)?;
                png = Some(png_path);
            }
            Err(error) => {
                write_png_status(&args.results, "skipped", Some(&error.to_string()))?;
                eprintln!("warning: PNG capture skipped: {error}");
            }
        }
    }
    Ok(RenderOutput { html: output, png })
}

fn write_png_status(
    results_path: &std::path::Path,
    status: &str,
    reason: Option<&str>,
) -> Result<()> {
    let mut results: Value = serde_json::from_str(&fs::read_to_string(results_path)?)?;
    if let Some(object) = results.as_object_mut() {
        object.insert(
            "table_png".to_owned(),
            serde_json::json!({
                "status": status,
                "reason": reason
            }),
        );
    }
    fs::write(results_path, serde_json::to_string_pretty(&results)? + "\n")?;
    Ok(())
}

fn render_html(results: &Value, config: &TableConfig) -> Result<String> {
    let cases = results
        .get("cases")
        .and_then(Value::as_array)
        .ok_or_else(|| err("results JSON missing cases array"))?;
    let columns = columns(results, config);
    let sections = config.sections.clone().unwrap_or_default();
    let title = config.title.as_deref().unwrap_or("Revelo benchmark comparison");
    let caption = config.caption.clone().unwrap_or_else(|| {
        let run_id = results.get("run_id").and_then(Value::as_str).unwrap_or("unknown");
        let commit = results.pointer("/repo/commit").and_then(Value::as_str).unwrap_or("unknown");
        format!("Run {run_id}; commit {commit}")
    });

    let mut html = String::new();
    html.push_str("<!doctype html><html><head><meta charset=\"utf-8\">");
    html.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">");
    html.push_str(&format!("<title>{}</title>", escape(title)));
    html.push_str("<link rel=\"stylesheet\" href=\"benchmark-table.css\">");
    html.push_str("</head><body>");
    html.push_str(&format!(
        "<main id=\"{}\" class=\"benchmark-shell\"><header><h1>{}</h1><p>{}</p></header>",
        escape(config.capture_id()),
        escape(title),
        escape(&caption)
    ));

    if sections.is_empty() {
        html.push_str(&render_table(
            "Results",
            cases.iter().collect::<Vec<_>>().as_slice(),
            &columns,
            config,
        ));
    } else {
        let mut matched_count = 0usize;
        for section in sections {
            let label = section.label.as_deref().unwrap_or("Results");
            let matched = cases
                .iter()
                .filter(|case| matches_section(case, &section.match_))
                .collect::<Vec<_>>();
            if !matched.is_empty() {
                matched_count += matched.len();
                html.push_str(&render_table(label, &matched, &columns, config));
            }
        }
        if matched_count != cases.len() {
            return Err(err(format!(
                "table config sections matched {matched_count} of {} cases",
                cases.len()
            )));
        }
    }
    if let Some(note) = &config.footer_note {
        html.push_str(&format!("<footer>{}</footer>", escape(note)));
    }
    html.push_str("</main></body></html>");
    Ok(html)
}

fn render_table(label: &str, cases: &[&Value], columns: &[String], config: &TableConfig) -> String {
    let mut html = String::new();
    html.push_str(&format!("<section class=\"bench-section\"><h2>{}</h2><table>", escape(label)));
    html.push_str("<thead><tr><th>Case</th>");
    if config.show_size_column.unwrap_or(true) {
        html.push_str("<th class=\"size-col\">Size</th>");
    }
    for column in columns {
        html.push_str(&format!(
            "<th class=\"metric-col\">{}</th>",
            escape(config.column_labels.get(column).map_or(column, String::as_str))
        ));
    }
    html.push_str("</tr></thead><tbody>");
    for case in cases {
        html.push_str("<tr>");
        let label = case.get("label").and_then(Value::as_str).unwrap_or("case");
        html.push_str("<td class=\"case-cell\">");
        html.push_str(&format!("<strong>{}</strong>", escape(label)));
        let subtitle = subtitle(case);
        if !subtitle.is_empty() {
            html.push_str(&format!("<span>{}</span>", escape(&subtitle)));
        }
        html.push_str("</td>");
        if config.show_size_column.unwrap_or(true) {
            html.push_str(&format!(
                "<td class=\"size-col\">{}</td>",
                escape(&format_bytes(case.get("size_bytes").and_then(Value::as_u64).unwrap_or(0)))
            ));
        }
        for column in columns {
            let measurement =
                case.get("measurements").and_then(|measurements| measurements.get(column));
            html.push_str(&render_measurement(measurement, config));
        }
        html.push_str("</tr>");
    }
    html.push_str("</tbody></table></section>");
    html
}

fn render_measurement(measurement: Option<&Value>, config: &TableConfig) -> String {
    let median = measurement
        .and_then(|value| value.get("median_ms").or_else(|| value.pointer("/process_ms/median_ms")))
        .and_then(Value::as_f64);
    match median {
        Some(value) => format!(
            "<td class=\"metric {}\"><span>{:.1}</span></td>",
            tier_class(value, config),
            value
        ),
        None => "<td class=\"metric missing\"><span>n/a</span></td>".to_owned(),
    }
}

fn tier_class(value: f64, config: &TableConfig) -> String {
    let tiers = config.latency_tiers_ms.clone().unwrap_or_default();
    for LatencyTier { max, class_name, .. } in tiers {
        if max.is_none_or(|limit| value < limit) {
            return class_name;
        }
    }
    "missing".to_owned()
}

fn columns(results: &Value, config: &TableConfig) -> Vec<String> {
    if let Some(columns) = &config.columns {
        return columns.clone();
    }
    let preferred = [
        "revelo_0_4_6",
        "revelo_0_5_0",
        "revelo_0_5_1",
        "revelo_pr5",
        "revelo_branch",
        "revelo_cli_text",
        "mediainfo",
        "ffprobe",
    ];
    let mut observed = BTreeSet::new();
    if let Some(cases) = results.get("cases").and_then(Value::as_array) {
        for case in cases {
            if let Some(measurements) = case.get("measurements").and_then(Value::as_object) {
                observed.extend(measurements.keys().cloned());
            }
        }
    }
    let mut columns = preferred
        .iter()
        .filter(|column| observed.contains(**column))
        .map(|column| (*column).to_owned())
        .collect::<Vec<_>>();
    for column in observed {
        if !columns.contains(&column) && column != "revelo_perf_probe" {
            columns.push(column);
        }
    }
    columns
}

fn matches_section(case: &Value, expected: &std::collections::BTreeMap<String, Value>) -> bool {
    expected.iter().all(|(key, expected_value)| match expected_value {
        Value::String(expected) => case.get(key).and_then(Value::as_str) == Some(expected.as_str()),
        Value::Array(values) => values
            .iter()
            .filter_map(Value::as_str)
            .any(|expected| case.get(key).and_then(Value::as_str) == Some(expected)),
        _ => false,
    })
}

fn subtitle(case: &Value) -> String {
    let mut parts = Vec::new();
    for key in ["source", "layout", "codec"] {
        if let Some(value) = case.get(key).and_then(Value::as_str)
            && !value.is_empty()
            && !parts.iter().any(|part| part == value)
        {
            parts.push(value.to_owned());
        }
    }
    parts.join(" · ")
}

fn format_bytes(bytes: u64) -> String {
    const MIB: f64 = 1024.0 * 1024.0;
    if bytes == 0 { "n/a".to_owned() } else { format!("{:.1} MiB", bytes as f64 / MIB) }
}

fn escape(value: &str) -> String {
    value.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn stylesheet() -> &'static str {
    r#"
:root {
  color-scheme: dark;
  --bg: #050506;
  --panel: #0b0d10;
  --line: #20252b;
  --text: #f4f6f8;
  --muted: #9ba7b4;
}
* { box-sizing: border-box; }
body {
  margin: 0;
  background: var(--bg);
  color: var(--text);
  font: 15px/1.35 ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
}
.benchmark-shell {
  width: max-content;
  min-width: 1200px;
  padding: 24px;
  background: var(--bg);
}
header { margin-bottom: 18px; }
h1 { margin: 0 0 6px; font-size: 26px; font-weight: 720; letter-spacing: 0; }
p, footer { color: var(--muted); margin: 0; }
.bench-section { margin-top: 18px; }
h2 { margin: 0 0 8px; font-size: 18px; }
table {
  border-collapse: separate;
  border-spacing: 0;
  background: var(--panel);
  table-layout: auto;
}
th, td {
  padding: 8px 10px;
  border-bottom: 1px solid var(--line);
  text-align: left;
  vertical-align: top;
  white-space: nowrap;
}
th {
  color: #d9e0e7;
  font-size: 12px;
  text-transform: uppercase;
  letter-spacing: .04em;
  background: #11151a;
}
.case-cell { min-width: 440px; max-width: 620px; }
.case-cell strong { display: block; font-size: 15px; }
.case-cell span { display: block; margin-top: 3px; color: var(--muted); font-size: 12px; }
.size-col { min-width: 86px; color: #c6d0da; }
.metric-col { min-width: 112px; text-align: right; }
.metric { text-align: right; font-variant-numeric: tabular-nums; font-size: 17px; font-weight: 700; }
.metric span { display: block; }
.instant { background: #123b2a; color: #d7ffe8; }
.very-fast { background: #17492d; color: #e4ffdc; }
.fast { background: #4b4617; color: #fff5c4; }
.medium { background: #5b3516; color: #ffe1c0; }
.slow { background: #5e231d; color: #ffd0c8; }
.very-slow { background: #581d2c; color: #ffd0dd; }
.missing { background: #161a1f; color: #708090; }
footer { margin-top: 16px; font-size: 12px; }
"#
}

pub(crate) fn self_test() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let results = temp.path().join("results.json");
    fs::write(
        &results,
        r#"{"run_id":"self","repo":{"commit":"abc"},"cases":[{"label":"MP4 / AVC","class":"synthetic","size_bytes":1048576,"source":"generated","measurements":{"revelo_cli_text":{"median_ms":3.2},"mediainfo":{"median_ms":11.0}}}]}"#,
    )?;
    let output = temp.path().join("table.html");
    let rendered = run(RenderArgs {
        results,
        output: Some(output.clone()),
        config: None,
        render_png: false,
        chrome_path: None,
        selector: None,
    })?;
    assert_eq!(rendered.html, output);
    assert!(output.read_to_string()?.contains("MP4 / AVC"));
    let unmatched = serde_json::json!({
        "cases": [{"label": "Other", "class": "unknown", "measurements": {}}]
    });
    assert!(render_html(&unmatched, &TableConfig::default()).is_err());
    Ok(())
}

trait ReadToString {
    fn read_to_string(&self) -> Result<String>;
}

impl ReadToString for std::path::Path {
    fn read_to_string(&self) -> Result<String> {
        Ok(fs::read_to_string(self)?)
    }
}

impl ReadToString for PathBuf {
    fn read_to_string(&self) -> Result<String> {
        self.as_path().read_to_string()
    }
}
