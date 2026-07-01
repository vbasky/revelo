#!/usr/bin/env python3
"""Render a standalone benchmark comparison table from results JSON."""

from __future__ import annotations

import argparse
import html
import json
import re
import tempfile
from pathlib import Path
from typing import Any, TypeAlias, cast


JsonObject: TypeAlias = dict[str, Any]


DEFAULT_COLUMNS = [
    "revelo_0_4_6",
    "revelo_0_5_0",
    "revelo_0_5_1",
    "revelo_pr5",
    "revelo_branch",
    "revelo_cli_text",
    "mediainfo",
    "ffprobe",
]
DEFAULT_GROUPS = ["MP4", "MOV", "MKV", "WebM", "AVI", "MPEG-TS", "VOB", "WAV", "AIFF", "FLAC", "Ogg", "MP3"]
DEFAULT_TIERS = [
    {"name": "instant", "label": "under 10 ms", "max": 10, "class": "instant"},
    {"name": "very_fast", "label": "10-30 ms", "max": 30, "class": "very-fast"},
    {"name": "fast", "label": "30-80 ms", "max": 80, "class": "fast"},
    {"name": "medium", "label": "80-150 ms", "max": 150, "class": "medium"},
    {"name": "slow", "label": "150-500 ms", "max": 500, "class": "slow"},
    {"name": "very_slow", "label": "over 500 ms", "max": None, "class": "very-slow"},
]
DEFAULT_TEMPLATE = Path(__file__).resolve().parent / "templates" / "benchmark-table.html"
DEFAULT_STYLESHEET = Path(__file__).resolve().parent / "styles" / "benchmark-table.css"
TABLE_CONFIG_SCHEMA = "revelo_benchmark_table_config_v1"
TABLE_CONFIG_KEYS = {
    "schema",
    "title",
    "caption",
    "template_path",
    "stylesheet_path",
    "capture_id",
    "footer_note",
    "columns",
    "column_labels",
    "baseline",
    "align_sections",
    "show_size_column",
    "sections",
    "groups",
    "latency_tiers_ms",
}
SECTION_KEYS = {"id", "label", "match"}
TIER_KEYS = {"name", "label", "max", "class"}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--results", type=Path, help="results JSON produced by run_perf_investigation.py")
    parser.add_argument("--output", type=Path, help="output HTML path")
    parser.add_argument("--config", type=Path, help="optional table config JSON")
    parser.add_argument("--oracle-results", type=Path, help="optional oracle-parity JSON produced by run_oracle_parity.py")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    if args.self_test:
        return self_test()
    if args.results is None or args.output is None:
        parser.error("--results and --output are required unless --self-test is used")

    render_table(args.results, args.output, args.config, args.oracle_results)
    print(args.output)
    return 0


def render_table(results_path: Path, output_path: Path, config_path: Path | None, oracle_path: Path | None = None) -> None:
    results = json.loads(results_path.read_text(encoding="utf-8"))
    if oracle_path is not None:
        merge_oracle_results(results, json.loads(oracle_path.read_text(encoding="utf-8")))
    config = load_config(config_path)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(render_html(results, config), encoding="utf-8")


def merge_oracle_results(results: JsonObject, oracle_results: Any) -> None:
    if not isinstance(oracle_results, dict):
        raise SystemExit("oracle results must be an object")
    oracle_cases = oracle_results.get("cases", [])
    if not isinstance(oracle_cases, list):
        raise SystemExit("oracle results cases must be a list")
    status_by_id = {
        str(case.get("id")): str(case.get("status") or "n/a")
        for case in oracle_cases
        if isinstance(case, dict) and case.get("id")
    }
    for case in results.get("cases", []):
        if isinstance(case, dict):
            case["_oracle_status"] = normalize_oracle_status(status_by_id.get(str(case.get("id"))))


def normalize_oracle_status(status: str | None) -> str:
    if status in {"pass", "diff", "fail"}:
        return status
    return "na"


def load_config(path: Path | None) -> JsonObject:
    if path is None:
        return {}
    config = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(config, dict):
        raise SystemExit("table config must be an object")
    validate_config(cast(JsonObject, config))
    config["_config_dir"] = str(path.parent)
    return config


def validate_config(config: JsonObject) -> None:
    reject_extra_keys("table config", config, TABLE_CONFIG_KEYS, "unexpected table config key")
    if config.get("schema", TABLE_CONFIG_SCHEMA) != TABLE_CONFIG_SCHEMA:
        raise SystemExit(f"table config schema must be {TABLE_CONFIG_SCHEMA}")
    for key in ("title", "caption", "template_path", "stylesheet_path", "capture_id", "footer_note", "baseline"):
        if key in config and not isinstance(config[key], str):
            raise SystemExit(f"table config {key} must be a string")
    if "align_sections" in config and not isinstance(config["align_sections"], bool):
        raise SystemExit("table config align_sections must be a boolean")
    if "show_size_column" in config and not isinstance(config["show_size_column"], bool):
        raise SystemExit("table config show_size_column must be a boolean")
    if "columns" in config:
        validate_string_list(config["columns"], "table config columns")
    if "groups" in config:
        validate_string_list(config["groups"], "table config groups")
    if "column_labels" in config:
        validate_string_map(config["column_labels"], "table config column_labels")
    if "sections" in config:
        validate_sections(config["sections"])
    if "latency_tiers_ms" in config:
        validate_latency_tiers(config["latency_tiers_ms"])


def validate_sections(value: Any) -> None:
    if not isinstance(value, list):
        raise SystemExit("table config sections must be a list")
    for index, section in enumerate(value):
        if not isinstance(section, dict):
            raise SystemExit(f"table config section {index} must be an object")
        section_object = cast(JsonObject, section)
        reject_extra_keys(f"table config section {index}", section_object, SECTION_KEYS, "unexpected section key")
        for key in ("id", "label"):
            if key in section_object and not isinstance(section_object[key], str):
                raise SystemExit(f"table config section {index} {key} must be a string")
        match = section_object.get("match", {})
        if not isinstance(match, dict):
            raise SystemExit(f"table config section {index} match must be an object")
        for match_key, expected in cast(JsonObject, match).items():
            if not isinstance(match_key, str):
                raise SystemExit(f"table config section {index} match keys must be strings")
            if isinstance(expected, list):
                validate_string_list(expected, f"table config section {index} match {match_key}")
            elif not isinstance(expected, str):
                raise SystemExit(f"table config section {index} match {match_key} must be a string or list of strings")


def validate_latency_tiers(value: Any) -> None:
    if not isinstance(value, list) or not value:
        raise SystemExit("table config latency_tiers_ms must be a non-empty list")
    for index, tier in enumerate(value):
        if not isinstance(tier, dict):
            raise SystemExit(f"table config latency tier {index} must be an object")
        tier_object = cast(JsonObject, tier)
        reject_extra_keys(f"table config latency tier {index}", tier_object, TIER_KEYS, "unexpected latency tier key")
        for key in ("name", "label", "class"):
            if not isinstance(tier_object.get(key), str) or not tier_object[key]:
                raise SystemExit(f"table config latency tier {index} {key} must be a non-empty string")
        maximum = tier_object.get("max")
        if maximum is not None and (not isinstance(maximum, (int, float)) or isinstance(maximum, bool)):
            raise SystemExit(f"table config latency tier {index} max must be a number or null")


def validate_string_list(value: Any, label: str) -> None:
    if not isinstance(value, list) or any(not isinstance(item, str) or not item for item in value):
        raise SystemExit(f"{label} must be a list of non-empty strings")


def validate_string_map(value: Any, label: str) -> None:
    if not isinstance(value, dict):
        raise SystemExit(f"{label} must be an object")
    for key, item in value.items():
        if not isinstance(key, str) or not isinstance(item, str):
            raise SystemExit(f"{label} must map strings to strings")


def reject_extra_keys(where: str, data: JsonObject, allowed: set[str], prefix: str) -> None:
    extra = sorted(set(data) - allowed)
    if extra:
        raise SystemExit(f"{prefix} in {where}: {', '.join(extra)}")


def render_html(results: JsonObject, config: JsonObject) -> str:
    columns = config.get("columns") or visible_columns(results)
    tiers = config.get("latency_tiers_ms") or DEFAULT_TIERS
    title = config.get("title") or "Revelo benchmark comparison"
    groups = config.get("groups") or DEFAULT_GROUPS
    capture_id = config.get("capture_id") or "benchmark-table-capture"
    cases = sorted(results.get("cases", []), key=lambda case: (section_index(case, config), group_index(case, groups), case.get("label", "")))
    caption = build_caption(results)
    show_oracle = any("_oracle_status" in case for case in cases)
    show_size = config.get("show_size_column") is True
    main_style = render_main_style(cases, config, show_size)
    sections = render_sections(cases, columns, tiers, config, show_oracle, show_size)
    legend = " ".join(
        f"<span class=\"{escape(tier.get('class', tier.get('name', 'tier')))}\">{escape(tier.get('label', tier.get('name', '')))}</span>"
        for tier in tiers
    )
    template = read_config_path(config, "template_path", DEFAULT_TEMPLATE).read_text(encoding="utf-8")
    css = read_config_path(config, "stylesheet_path", DEFAULT_STYLESHEET).read_text(encoding="utf-8")
    return render_template(
        template,
        {
            "title": escape(title),
            "caption": escape(config.get("caption") or caption),
            "capture_id": escape(capture_id),
            "main_style": main_style,
            "css": css,
            "sections": sections,
            "footer_note": escape(config.get("footer_note") or "Values are median milliseconds."),
            "legend": legend,
        },
    )


def visible_columns(results: JsonObject) -> list[str]:
    present: set[str] = set()
    for case in results.get("cases", []):
        present.update(case.get("measurements", {}).keys())
    ordered = [column for column in DEFAULT_COLUMNS if column in present]
    ordered.extend(sorted(present - set(ordered) - {"revelo_perf_probe"}))
    return ordered or DEFAULT_COLUMNS


def render_main_style(cases: list[JsonObject], config: JsonObject, show_size: bool) -> str:
    if config.get("align_sections") is not True:
        return ""
    width = case_column_width_ch(cases, show_size)
    return f" style=\"--case-col: {width}ch;\""


def case_column_width_ch(cases: list[JsonObject], show_size: bool) -> int:
    longest = 0
    for case in cases:
        label = case_label(case, show_size)
        subtitle = case_subtitle(case, label, not show_size)
        longest = max(longest, len(label), len(subtitle))
    return max(36, longest + 2)


def render_sections(
    cases: list[JsonObject],
    columns: list[str],
    tiers: list[JsonObject],
    config: JsonObject,
    show_oracle: bool,
    show_size: bool,
) -> str:
    section_defs = config.get("sections") or [{"id": "all", "label": "Benchmark cases", "match": {}}]
    rendered: list[str] = []
    used_ids: set[str] = set()
    for section_def in section_defs:
        section_cases = [case for case in cases if case_matches_section(case, section_def)]
        if not section_cases:
            continue
        used_ids.update(str(case.get("id") or case.get("label")) for case in section_cases)
        rendered.append(render_section(section_def, section_cases, columns, tiers, config, show_oracle, show_size))

    remaining = [case for case in cases if str(case.get("id") or case.get("label")) not in used_ids]
    if remaining:
        rendered.append(render_section({"label": "Other cases"}, remaining, columns, tiers, config, show_oracle, show_size))
    return "\n".join(rendered)


def render_section(
    section_def: JsonObject,
    cases: list[JsonObject],
    columns: list[str],
    tiers: list[JsonObject],
    config: JsonObject,
    show_oracle: bool,
    show_size: bool,
) -> str:
    headers = "\n".join(f"<th class=\"col-tool\">{escape(column_label(column, config))}</th>" for column in columns)
    size_header = "<th class=\"col-size\">Size</th>" if show_size else ""
    oracle_header = "<th class=\"col-oracle\">Oracle</th>" if show_oracle else ""
    rows = "\n".join(render_row(case, columns, tiers, show_oracle, show_size) for case in cases)
    label = section_def.get("label") or "Benchmark cases"
    count_label = f"{len(cases)} rows"
    return f"""<section class="panel">
  <div class="section-title">
    <h2>{escape(label)}</h2>
    <span>{escape(count_label)}</span>
  </div>
  <table>
    <thead>
      <tr>
        <th class="col-case">Case</th>
        {size_header}
        <th class="col-runs">Runs</th>
        {oracle_header}
        {headers}
      </tr>
    </thead>
    <tbody>
      {rows}
    </tbody>
  </table>
</section>"""


def case_matches_section(case: JsonObject, section_def: JsonObject) -> bool:
    match = section_def.get("match")
    if not match:
        return True
    for key, expected in match.items():
        actual = case.get(key)
        if isinstance(expected, list):
            if actual not in expected:
                return False
        elif actual != expected:
            return False
    return True


def build_caption(results: JsonObject) -> str:
    settings = results.get("settings", {})
    repo = results.get("repo", {})
    env = results.get("environment", {})
    warmups = settings.get("warmups", "?")
    runs = settings.get("runs", "?")
    machine = env.get("processor") or env.get("machine") or "unknown machine"
    commit = repo.get("commit") or "unknown commit"
    return f"{warmups} warmups + {runs} runs; {machine}; commit {commit}"


def render_row(case: JsonObject, columns: list[str], tiers: list[JsonObject], show_oracle: bool, show_size: bool) -> str:
    cells = "\n".join(render_measurement_cell(case.get("measurements", {}).get(column), tiers) for column in columns)
    label = case_label(case, show_size)
    metadata = case_subtitle(case, label, not show_size)
    subtitle = f"<span>{escape(metadata)}</span>" if metadata else ""
    size_cell = f"<td class=\"size\">{escape(format_bytes(case.get('size_bytes')) or 'n/a')}</td>" if show_size else ""
    runs = runs_label(case.get("measurements", {}))
    oracle_cell = render_oracle_cell(case) if show_oracle else ""
    return f"""<tr>
  <td class="case"><strong>{escape(label)}</strong>{subtitle}</td>
  {size_cell}
  <td class="runs split-runs">{runs}</td>
  {oracle_cell}
  {cells}
</tr>"""


def case_label(case: JsonObject, show_size: bool) -> str:
    label = str(case.get("label") or case.get("id") or "case")
    if not show_size:
        return label
    return remove_source_suffix(remove_size_suffix(label), case)


def remove_size_suffix(label: str) -> str:
    return re.sub(r"\s*/\s*\d+(?:\.\d+)?\s*(?:B|KiB|MiB|GiB)\s*$", "", label)


def remove_source_suffix(label: str, case: JsonObject) -> str:
    source = useful_source(case)
    if not source:
        return label
    return re.sub(rf"\s*/\s*real\s+{re.escape(source)}\s*$", "", label, flags=re.IGNORECASE)


def case_subtitle(case: JsonObject, label: Any, include_size: bool) -> str:
    label_text = str(label)
    label_normalized = normalize_case_text(label_text)
    label_tokens = set(label_normalized.split())
    parts: list[str] = []
    seen: set[str] = set()
    values = [*detail_fragments(case.get("layout")), useful_source(case)]
    if include_size:
        values.append(format_bytes(case.get("size_bytes")))
    for value in values:
        if not isinstance(value, str) or not value.strip():
            continue
        text = trim_repeated_phrases(value.strip(), label_text)
        normalized = normalize_case_text(text)
        if not normalized or normalized in seen or is_repeated_detail(normalized, label_normalized, label_tokens):
            continue
        parts.append(text)
        seen.add(normalized)
    return " | ".join(parts)


def detail_fragments(value: Any) -> list[str]:
    if not isinstance(value, str):
        return []
    return [fragment.strip() for fragment in value.split(",") if fragment.strip()]


def is_repeated_detail(detail: str, label: str, label_tokens: set[str]) -> bool:
    if detail in label:
        return True
    detail_tokens = {token for token in detail.split() if token not in {"and", "with"}}
    return bool(detail_tokens) and detail_tokens <= label_tokens


def trim_repeated_phrases(detail: str, label: str) -> str:
    cleaned = detail
    for phrase in label.split("/"):
        cleaned = remove_phrase(cleaned, phrase)
    if "mp4" in normalize_case_text(label).split():
        cleaned = remove_phrase(cleaned, "M4A")
    cleaned = re.sub(r"(^|\s)(with|and)($|\s)", " ", cleaned, flags=re.IGNORECASE)
    cleaned = re.sub(r"\s+", " ", cleaned)
    return cleaned.strip(" ,/+_-")


def remove_phrase(value: str, phrase: str) -> str:
    tokens = re.findall(r"[A-Za-z0-9.]+", phrase)
    if not tokens:
        return value
    pattern = r"[\s/+_,:()\-]*".join(re.escape(token) for token in tokens)
    return re.sub(pattern, " ", value, flags=re.IGNORECASE)


def useful_source(case: JsonObject) -> str:
    source = case.get("source")
    if not isinstance(source, str):
        return ""
    if source.lower().startswith("generated"):
        return ""
    return source


def normalize_case_text(value: str) -> str:
    value = re.sub(r"(?<=\d)\.0\b", "", value)
    value = value.replace("M4A", "MP4").replace("m4a", "mp4")
    for character in "/+-_,:()[]":
        value = value.replace(character, " ")
    return " ".join(value.casefold().split())


def render_oracle_cell(case: JsonObject) -> str:
    status = normalize_oracle_status(case.get("_oracle_status"))
    label = "n/a" if status == "na" else status
    return f"<td class=\"oracle-cell oracle-{escape(status)}\">{escape(label)}</td>"


def render_measurement_cell(measurement: JsonObject | None, tiers: list[JsonObject]) -> str:
    if not measurement:
        return "<td class=\"measure missing\">n/a</td>"
    median = measurement.get("median_ms")
    if median is None and isinstance(measurement.get("process_ms"), dict):
        median = measurement["process_ms"].get("median_ms")
    if median is None:
        return "<td class=\"measure missing\">n/a</td>"
    tier_class = tier_for(float(median), tiers)
    return f"<td class=\"measure {escape(tier_class)}\"><span class=\"ms-value\">{float(median):.1f}</span></td>"


def runs_label(measurements: JsonObject) -> str:
    runs = sorted(
        {
            run
            for measurement in measurements.values()
            if isinstance(measurement, dict)
            for run in [measurement.get("runs")]
            if isinstance(run, int)
        }
    )
    if not runs:
        return "<span>n/a</span>"
    if len(runs) == 1:
        return f"<span>{escape(runs[0])} runs</span>"
    return "".join(f"<span>{escape(run)} runs</span>" for run in runs)


def tier_for(value: float, tiers: list[JsonObject]) -> str:
    for tier in tiers:
        maximum = tier.get("max")
        if maximum is None or value < float(maximum):
            return str(tier.get("class") or f"tier-{tier.get('name', 'default')}")
    return "very-slow"


def section_index(case: JsonObject, config: JsonObject) -> int:
    sections = config.get("sections") or []
    for index, section_def in enumerate(sections):
        if case_matches_section(case, section_def):
            return index
    return len(sections)


def group_index(case: JsonObject, groups: list[str]) -> tuple[int, str]:
    container = str(case.get("container") or case.get("format") or "")
    try:
        return (groups.index(container), container)
    except ValueError:
        return (len(groups), container)


def column_label(column: str, config: JsonObject) -> str:
    labels = config.get("column_labels") or {}
    return labels.get(column, column.replace("_", " "))


def format_bytes(value: Any) -> str:
    if not isinstance(value, int) or value < 0:
        return ""
    units = ["B", "KiB", "MiB", "GiB"]
    amount = float(value)
    for unit in units:
        if amount < 1024 or unit == units[-1]:
            return f"{amount:.0f} {unit}" if unit == "B" else f"{amount:.1f} {unit}"
        amount /= 1024
    return f"{value} B"


def escape(value: Any) -> str:
    return html.escape(str(value), quote=True)


def read_config_path(config: JsonObject, key: str, default: Path) -> Path:
    raw = config.get(key)
    if not raw:
        return default
    path = Path(str(raw)).expanduser()
    if path.is_absolute():
        return path
    config_dir = config.get("_config_dir")
    if config_dir:
        candidate = Path(str(config_dir)) / path
        if candidate.exists():
            return candidate
    return Path.cwd() / path


def render_template(template: str, values: dict[str, str]) -> str:
    rendered = template
    for key, value in values.items():
        rendered = rendered.replace("{{ " + key + " }}", value)
    return rendered


def self_test() -> int:
    with tempfile.TemporaryDirectory() as tmp:
        results = {
            "schema": "revelo_bench_compare_v1",
            "run_id": "self-test",
            "repo": {"commit": "abc1234"},
            "environment": {"processor": "Test CPU"},
            "settings": {"warmups": 1, "runs": 2},
            "cases": [
                {
                    "id": "case",
                    "label": "MP4 / synthetic",
                    "container": "MP4",
                    "codec": "H.264",
                    "layout": "moov tail",
                    "size_bytes": 1048576,
                    "measurements": {"revelo_cli_text": {"median_ms": 7.5, "runs": 2}},
                }
            ],
        }
        results_path = Path(tmp) / "results.json"
        output_path = Path(tmp) / "table.html"
        results_path.write_text(json.dumps(results), encoding="utf-8")
        render_table(results_path, output_path, None)
        html_text = output_path.read_text(encoding="utf-8")
        assert "benchmark-table-capture" in html_text
        assert "7.5" in html_text
        assert "instant" in html_text
        assert "Benchmark cases" in html_text
    print("benchmark table renderer self-test ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
