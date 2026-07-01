#!/usr/bin/env python3
"""Compare Revelo JSON against MediaInfo JSON for local oracle-parity checks.

The manifest may contain private paths. This script writes only labels, sizes
and field-level differences to target/perf-investigation.
"""

from __future__ import annotations

import argparse
import json
import shutil
import subprocess
import sys
import time
from pathlib import Path
from typing import Any, TypeAlias, cast


ROOT = Path(__file__).resolve().parents[2]
SCRIPT_DIR = Path(__file__).resolve().parent
DEFAULT_OUT_DIR = ROOT / "target" / "perf-investigation"
DEFAULT_FIXTURE_DIR = ROOT / "target" / "perf-fixtures"

if str(SCRIPT_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPT_DIR))

import generate_fixtures  # noqa: E402
import run_perf_investigation as run_perf  # noqa: E402


JsonObject: TypeAlias = dict[str, Any]

CONFIG_SCHEMA = "revelo_oracle_parity_config_v1"
CONFIG_KEYS = {"schema", "ignore_fields", "required_fields", "case_overrides"}
REQUIRED_FIELD_KEYS = {"track_type", "field"}
DEFAULT_IGNORE_FIELDS = {
    "@type",
    "CompleteName",
    "FolderName",
    "FileName",
    "FileExtension",
    "File_Modified_Date",
    "File_Modified_Date_Local",
    "Encoded_Date",
    "Tagged_Date",
}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, help="manifest with real and/or generated cases")
    parser.add_argument("--config", type=Path, help="oracle field config JSON")
    parser.add_argument("--out-dir", type=Path, default=DEFAULT_OUT_DIR)
    parser.add_argument("--fixture-dir", type=Path, default=DEFAULT_FIXTURE_DIR)
    parser.add_argument("--run-id")
    parser.add_argument("--no-build", action="store_true")
    parser.add_argument("--fail-on-diff", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    if args.self_test:
        return self_test()
    if args.manifest is None:
        parser.error("--manifest is required unless --self-test is used")

    manifest = run_perf.load_manifest(args.manifest)
    run_perf.validate_manifest(manifest)
    config = load_config(args.config)

    if not args.no_build:
        run_checked(["cargo", "build", "-p", "revelo-cli", "--bin", "revelo", "--release"])

    revelo = ROOT / "target" / "release" / "revelo"
    mediainfo = which_required("mediainfo")
    run_id = args.run_id or time.strftime("%Y%m%dT%H%M%S")
    run_dir = args.out_dir / run_id
    fixture_dir = args.fixture_dir / run_id
    run_dir.mkdir(parents=True, exist_ok=True)

    cases = [
        run_case(case, config, revelo, mediainfo, fixture_dir)
        for case in cast(list[JsonObject], manifest["cases"])
    ]
    output = {
        "schema": "revelo_oracle_parity_results_v1",
        "run_id": run_id,
        "repo": {
            "branch": run_perf.git_text(["branch", "--show-current"]),
            "commit": run_perf.git_text(["rev-parse", "--short", "HEAD"]),
        },
        "tools": {
            "revelo": {"path": "target/release/revelo"},
            "mediainfo": run_perf.tool_version(mediainfo, ["--Version"]),
        },
        "cases": cases,
        "summary": summarize_cases(cases),
    }
    out_path = run_dir / "oracle-parity.json"
    out_path.write_text(json.dumps(output, indent=2) + "\n", encoding="utf-8")
    print(out_path)
    if args.fail_on_diff and any(case["status"] != "pass" for case in cases):
        return 1
    return 0


def run_case(
    case: JsonObject,
    config: JsonObject,
    revelo: Path,
    mediainfo: Path,
    fixture_dir: Path,
) -> JsonObject:
    path = resolve_case_path(case, fixture_dir)
    result = case_result_shell(case, path)
    revelo_json = run_json([str(revelo), "--json", str(path)])
    mediainfo_json = run_json([str(mediainfo), "--Output=JSON", str(path)])
    result.update(compare_documents(str(case.get("id") or case.get("label")), revelo_json, mediainfo_json, config))
    return result


def resolve_case_path(case: JsonObject, fixture_dir: Path) -> Path:
    if run_perf.case_source_kind(case) == "synthetic":
        return generate_fixtures.generate_case_fixture(case, fixture_dir)
    return Path(str(case["path"])).expanduser()


def case_result_shell(case: JsonObject, path: Path) -> JsonObject:
    return {
        "id": case.get("id") or run_perf.safe_id(str(case["label"])),
        "label": case["label"],
        "size_bytes": path.stat().st_size if path.exists() else 0,
    }


def compare_documents(case_id: str, revelo: JsonObject, mediainfo: JsonObject, config: JsonObject) -> JsonObject:
    ignore_fields = ignored_fields(config, case_id)
    required_fields = required_field_specs(config, case_id)
    revelo_tracks = tracks_by_key(revelo)
    mediainfo_tracks = tracks_by_key(mediainfo)

    required_failures: list[JsonObject] = []
    common_diffs: list[JsonObject] = []
    missing_tracks = sorted(set(mediainfo_tracks) - set(revelo_tracks))

    for spec in required_fields:
        track_key = first_track_key(mediainfo_tracks, spec["track_type"])
        if track_key is None:
            continue
        expected = mediainfo_tracks[track_key].get(spec["field"])
        actual = revelo_tracks.get(track_key, {}).get(spec["field"])
        if expected is not None and actual != expected:
            required_failures.append(
                {"track": track_key, "field": spec["field"], "expected": expected, "actual": actual}
            )

    for track_key in sorted(set(revelo_tracks) & set(mediainfo_tracks)):
        revelo_track = revelo_tracks[track_key]
        mediainfo_track = mediainfo_tracks[track_key]
        for field in sorted(set(revelo_track) & set(mediainfo_track) - ignore_fields):
            if revelo_track[field] != mediainfo_track[field]:
                common_diffs.append(
                    {
                        "track": track_key,
                        "field": field,
                        "revelo": revelo_track[field],
                        "mediainfo": mediainfo_track[field],
                    }
                )

    status = "fail" if required_failures else ("diff" if common_diffs or missing_tracks else "pass")
    return {
        "status": status,
        "required_failures": required_failures,
        "common_value_diffs": common_diffs,
        "missing_tracks_in_revelo": missing_tracks,
    }


def tracks_by_key(document: JsonObject) -> dict[str, JsonObject]:
    tracks = document.get("media", {}).get("track", []) if isinstance(document.get("media"), dict) else []
    if not isinstance(tracks, list):
        return {}
    counts: dict[str, int] = {}
    keyed: dict[str, JsonObject] = {}
    for raw_track in tracks:
        if not isinstance(raw_track, dict):
            continue
        track = cast(JsonObject, raw_track)
        track_type = str(track.get("@type") or "Unknown")
        index = counts.get(track_type, 0)
        counts[track_type] = index + 1
        keyed[f"{track_type}#{index}"] = track
    return keyed


def first_track_key(tracks: dict[str, JsonObject], track_type: str) -> str | None:
    prefix = f"{track_type}#"
    return next((key for key in sorted(tracks) if key.startswith(prefix)), None)


def ignored_fields(config: JsonObject, case_id: str) -> set[str]:
    fields = set(DEFAULT_IGNORE_FIELDS)
    fields.update(string_list(config.get("ignore_fields", []), "ignore_fields"))
    case_config = case_override(config, case_id)
    fields.update(string_list(case_config.get("ignore_fields", []), "case ignore_fields"))
    return fields


def required_field_specs(config: JsonObject, case_id: str) -> list[JsonObject]:
    specs = list(validate_required_fields(config.get("required_fields", []), "required_fields"))
    case_config = case_override(config, case_id)
    specs.extend(validate_required_fields(case_config.get("required_fields", []), "case required_fields"))
    return specs


def case_override(config: JsonObject, case_id: str) -> JsonObject:
    overrides = config.get("case_overrides", {})
    if not isinstance(overrides, dict):
        return {}
    value = overrides.get(case_id, {})
    return cast(JsonObject, value) if isinstance(value, dict) else {}


def load_config(path: Path | None) -> JsonObject:
    if path is None:
        return {"schema": CONFIG_SCHEMA, "ignore_fields": [], "required_fields": []}
    config = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(config, dict):
        raise SystemExit("oracle config must be an object")
    config_object = cast(JsonObject, config)
    validate_config(config_object)
    return config_object


def validate_config(config: JsonObject) -> None:
    reject_extra_keys("oracle config", config, CONFIG_KEYS, "unexpected oracle config key")
    if config.get("schema", CONFIG_SCHEMA) != CONFIG_SCHEMA:
        raise SystemExit(f"oracle config schema must be {CONFIG_SCHEMA}")
    string_list(config.get("ignore_fields", []), "ignore_fields")
    validate_required_fields(config.get("required_fields", []), "required_fields")
    overrides = config.get("case_overrides", {})
    if overrides and not isinstance(overrides, dict):
        raise SystemExit("case_overrides must be an object")


def validate_required_fields(value: Any, label: str) -> list[JsonObject]:
    if not isinstance(value, list):
        raise SystemExit(f"{label} must be a list")
    specs: list[JsonObject] = []
    for index, item in enumerate(value):
        if not isinstance(item, dict):
            raise SystemExit(f"{label} item {index} must be an object")
        spec = cast(JsonObject, item)
        reject_extra_keys(f"{label} item {index}", spec, REQUIRED_FIELD_KEYS, "unexpected required field key")
        if not isinstance(spec.get("track_type"), str) or not spec["track_type"]:
            raise SystemExit(f"{label} item {index} track_type must be a string")
        if not isinstance(spec.get("field"), str) or not spec["field"]:
            raise SystemExit(f"{label} item {index} field must be a string")
        specs.append(spec)
    return specs


def string_list(value: Any, label: str) -> list[str]:
    if not isinstance(value, list) or any(not isinstance(item, str) or not item for item in value):
        raise SystemExit(f"{label} must be a list of non-empty strings")
    return list(value)


def reject_extra_keys(where: str, data: JsonObject, allowed: set[str], prefix: str) -> None:
    extra = sorted(set(data) - allowed)
    if extra:
        raise SystemExit(f"{prefix} in {where}: {', '.join(extra)}")


def summarize_cases(cases: list[JsonObject]) -> JsonObject:
    return {
        "total": len(cases),
        "pass": sum(1 for case in cases if case["status"] == "pass"),
        "diff": sum(1 for case in cases if case["status"] == "diff"),
        "fail": sum(1 for case in cases if case["status"] == "fail"),
    }


def run_json(command: list[str]) -> JsonObject:
    completed = subprocess.run(command, cwd=ROOT, text=True, capture_output=True, check=True)
    return json.loads(completed.stdout)


def run_checked(command: list[str]) -> None:
    subprocess.run(command, cwd=ROOT, check=True)


def which_required(name: str) -> Path:
    resolved = shutil.which(name)
    if resolved is None:
        raise SystemExit(f"{name} is required for oracle parity checks")
    return Path(resolved)


def self_test() -> int:
    revelo = {"media": {"track": [{"@type": "General", "Format": "MPEG-4"}]}}
    mediainfo = {"media": {"track": [{"@type": "General", "Format": "MPEG-4", "CodecID": "isom"}]}}
    result = compare_documents(
        "case",
        revelo,
        mediainfo,
        {"required_fields": [{"track_type": "General", "field": "CodecID"}], "ignore_fields": []},
    )
    assert result["status"] == "fail"
    print("oracle parity self-test ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
