use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::fixtures;
use crate::util::{Result, err};

const MANIFEST_SCHEMA: &str = "revelo_perf_manifest_v2";
const TABLE_CONFIG_SCHEMA: &str = "revelo_benchmark_table_config_v1";

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Manifest {
    pub(crate) schema: String,
    #[serde(default)]
    pub(crate) settings: ManifestSettings,
    #[serde(default)]
    pub(crate) revelo_versions: Vec<ReveloVersion>,
    pub(crate) cases: Vec<BenchCase>,
}

impl Manifest {
    pub(crate) fn load(path: &Path) -> Result<Self> {
        Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ManifestSettings {
    pub(crate) warmups: Option<u32>,
    pub(crate) runs: Option<u32>,
    pub(crate) render_png: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReveloVersion {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) path: PathBuf,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BenchCase {
    pub(crate) id: Option<String>,
    pub(crate) label: String,
    #[serde(rename = "class")]
    pub(crate) class_name: String,
    pub(crate) format: Option<String>,
    pub(crate) container: Option<String>,
    pub(crate) codec: Option<String>,
    pub(crate) layout: Option<String>,
    pub(crate) source: Option<String>,
    pub(crate) path: Option<PathBuf>,
    pub(crate) synthetic: Option<SyntheticCase>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SyntheticCase {
    pub(crate) kind: String,
    pub(crate) size_bytes: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TableConfig {
    pub(crate) schema: Option<String>,
    pub(crate) title: Option<String>,
    pub(crate) caption: Option<String>,
    pub(crate) capture_id: Option<String>,
    pub(crate) footer_note: Option<String>,
    pub(crate) columns: Option<Vec<String>>,
    #[serde(default)]
    pub(crate) column_labels: std::collections::BTreeMap<String, String>,
    pub(crate) show_size_column: Option<bool>,
    pub(crate) sections: Option<Vec<TableSection>>,
    pub(crate) latency_tiers_ms: Option<Vec<LatencyTier>>,
}

impl TableConfig {
    pub(crate) fn load(path: Option<&Path>) -> Result<Self> {
        match path {
            Some(path) => {
                let config: Self = serde_json::from_str(&fs::read_to_string(path)?)?;
                validate_table_config(&config)?;
                Ok(config)
            }
            None => Ok(Self::default()),
        }
    }

    pub(crate) fn capture_id(&self) -> &str {
        self.capture_id.as_deref().unwrap_or("benchmark-table-capture")
    }
}

impl Default for TableConfig {
    fn default() -> Self {
        Self {
            schema: Some(TABLE_CONFIG_SCHEMA.to_owned()),
            title: Some("Revelo benchmark comparison".to_owned()),
            caption: None,
            capture_id: Some("benchmark-table-capture".to_owned()),
            footer_note: Some(
                "Values are median milliseconds. Generated fixtures are parser-oriented sparse files; real rows come from the local manifest."
                    .to_owned(),
            ),
            columns: None,
            column_labels: Default::default(),
            show_size_column: Some(true),
            sections: Some(vec![
                TableSection::class("synthetic", "Generated fixtures"),
                TableSection::class("real", "Real media"),
            ]),
            latency_tiers_ms: Some(default_latency_tiers()),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TableSection {
    pub(crate) id: Option<String>,
    pub(crate) label: Option<String>,
    #[serde(rename = "match", default)]
    pub(crate) match_: std::collections::BTreeMap<String, serde_json::Value>,
}

impl TableSection {
    fn class(id: &str, label: &str) -> Self {
        Self {
            id: Some(id.to_owned()),
            label: Some(label.to_owned()),
            match_: [("class".to_owned(), serde_json::Value::String(id.to_owned()))].into(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LatencyTier {
    pub(crate) name: String,
    pub(crate) label: String,
    pub(crate) max: Option<f64>,
    #[serde(rename = "class")]
    pub(crate) class_name: String,
}

fn default_latency_tiers() -> Vec<LatencyTier> {
    [
        ("instant", "under 10 ms", Some(10.0), "instant"),
        ("very_fast", "10-30 ms", Some(30.0), "very-fast"),
        ("fast", "30-80 ms", Some(80.0), "fast"),
        ("medium", "80-150 ms", Some(150.0), "medium"),
        ("slow", "150-500 ms", Some(500.0), "slow"),
        ("very_slow", "over 500 ms", None, "very-slow"),
    ]
    .into_iter()
    .map(|(name, label, max, class_name)| LatencyTier {
        name: name.to_owned(),
        label: label.to_owned(),
        max,
        class_name: class_name.to_owned(),
    })
    .collect()
}

pub(crate) fn validate_manifest(manifest: &Manifest) -> Result<()> {
    if manifest.schema != MANIFEST_SCHEMA {
        return Err(err(format!("manifest schema must be {MANIFEST_SCHEMA}")));
    }
    if manifest.cases.is_empty() {
        return Err(err("manifest must contain non-empty cases list"));
    }
    let mut case_ids = HashSet::new();
    for case in &manifest.cases {
        if !matches!(case.class_name.as_str(), "synthetic" | "real") {
            return Err(err(format!("case {} class must be real or synthetic", case.label)));
        }
        match (&case.path, &case.synthetic) {
            (Some(_), None) if case.class_name == "real" => {}
            (None, Some(synthetic)) if case.class_name == "synthetic" => {
                if synthetic.size_bytes < fixtures::MIN_SIZE {
                    return Err(err(format!(
                        "case {} size_bytes must be >= {}",
                        case.label,
                        fixtures::MIN_SIZE
                    )));
                }
                if !fixtures::is_supported_kind(&synthetic.kind) {
                    return Err(err(format!(
                        "case {} unsupported synthetic kind {}; supported: {}",
                        case.label,
                        synthetic.kind,
                        fixtures::supported_kinds().join(", ")
                    )));
                }
            }
            _ => {
                return Err(err(format!(
                    "case {} must use exactly one of path or synthetic, matching class",
                    case.label
                )));
            }
        }
        if let Some(id) = &case.id
            && !case_ids.insert(id.clone())
        {
            return Err(err(format!("duplicate case id: {id}")));
        }
    }
    validate_revelo_versions(&manifest.revelo_versions)
}

fn validate_revelo_versions(versions: &[ReveloVersion]) -> Result<()> {
    let reserved = ["mediainfo", "ffprobe", "revelo_perf_probe"];
    let mut seen = HashSet::new();
    for version in versions {
        if version.id.is_empty()
            || !version
                .id
                .chars()
                .all(|char| char.is_ascii_alphanumeric() || matches!(char, '-' | '_'))
        {
            return Err(err(
                "revelo_versions id must contain only letters, numbers, dash or underscore",
            ));
        }
        if reserved.contains(&version.id.as_str()) {
            return Err(err(format!("revelo_versions id {} is reserved", version.id)));
        }
        if !seen.insert(version.id.clone()) {
            return Err(err(format!("duplicate revelo_versions id: {}", version.id)));
        }
    }
    Ok(())
}

fn validate_table_config(config: &TableConfig) -> Result<()> {
    if config.schema.as_deref().unwrap_or(TABLE_CONFIG_SCHEMA) != TABLE_CONFIG_SCHEMA {
        return Err(err(format!("table config schema must be {TABLE_CONFIG_SCHEMA}")));
    }
    if let Some(tiers) = &config.latency_tiers_ms {
        if tiers.is_empty() {
            return Err(err("table config latency_tiers_ms must be non-empty"));
        }
        for tier in tiers {
            if tier.name.is_empty() || tier.label.is_empty() || tier.class_name.is_empty() {
                return Err(err("latency tier name, label and class must be non-empty"));
            }
        }
    }
    Ok(())
}

pub(crate) fn self_test() -> Result<()> {
    let valid = r#"{
      "schema": "revelo_perf_manifest_v2",
      "settings": {"warmups": 1, "runs": 2},
      "cases": [{
        "id": "fixture",
        "label": "Fixture",
        "class": "synthetic",
        "container": "MP4",
        "synthetic": {"kind": "mp4_snv2_tail", "size_bytes": 4096}
      }]
    }"#;
    let manifest: Manifest = serde_json::from_str(valid)?;
    validate_manifest(&manifest)?;
    let invalid = r#"{
      "schema": "revelo_perf_manifest_v2",
      "cases": [{
        "label": "Bad",
        "class": "real",
        "path": "sample.mp4",
        "synthetic": {"kind": "mp4_snv2_tail", "size_bytes": 4096}
      }]
    }"#;
    let manifest: Manifest = serde_json::from_str(invalid)?;
    assert!(validate_manifest(&manifest).is_err());
    Ok(())
}
