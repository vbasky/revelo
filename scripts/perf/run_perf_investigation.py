#!/usr/bin/env python3
"""Run Revelo-vs-reference performance probes from a local manifest.

The manifest may contain private local paths. This runner writes results under
target/perf-investigation and stores only case labels plus sanitized metadata in
the aggregate JSON.
"""

from __future__ import annotations

import argparse
import json
import os
import platform
import shlex
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from typing import Any, Literal, TypeAlias, cast


ROOT = Path(__file__).resolve().parents[2]
SCRIPT_DIR = Path(__file__).resolve().parent
DEFAULT_OUT_DIR = ROOT / "target" / "perf-investigation"
DEFAULT_FIXTURE_DIR = ROOT / "target" / "perf-fixtures"

if str(SCRIPT_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPT_DIR))

import generate_fixtures  # noqa: E402


JsonObject: TypeAlias = dict[str, Any]
SourceKind: TypeAlias = Literal["path", "synthetic"]

MANIFEST_SCHEMA = "revelo_perf_manifest_v2"
CASE_CLASSES = {"synthetic", "real"}
MANIFEST_KEYS = {"schema", "settings", "cases", "revelo_versions"}
MANIFEST_SETTINGS_KEYS = {"warmups", "runs", "render_png"}
REVELO_VERSION_KEYS = {"id", "label", "path"}
RESERVED_MEASUREMENT_IDS = {"mediainfo", "ffprobe", "revelo_perf_probe"}
CASE_KEYS = {
    "id",
    "label",
    "class",
    "format",
    "container",
    "codec",
    "layout",
    "source",
    "path",
    "synthetic",
}
SYNTHETIC_KEYS = {"kind", "size_bytes"}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, help="JSON manifest with local case paths")
    parser.add_argument("--out-dir", type=Path, default=DEFAULT_OUT_DIR)
    parser.add_argument("--fixture-dir", type=Path, default=DEFAULT_FIXTURE_DIR)
    parser.add_argument("--run-id", help="override run id; defaults to current timestamp")
    parser.add_argument("--warmups", type=int)
    parser.add_argument("--runs", type=int)
    parser.add_argument("--probe-export", choices=["none", "text", "json", "summary", "all"], default="all")
    parser.add_argument("--probe-output-target", choices=["sink", "file"], default="sink")
    parser.add_argument("--table-config", type=Path)
    parser.add_argument("--no-render-table", action="store_true")
    parser.add_argument("--render-png", action="store_true")
    parser.add_argument("--no-build", action="store_true")
    parser.add_argument("--skip-mediainfo", action="store_true")
    parser.add_argument("--skip-ffprobe", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    if args.self_test:
        return self_test()
    if args.manifest is None:
        parser.error("--manifest is required unless --self-test is used")

    manifest = load_manifest(args.manifest)
    validate_manifest(manifest)
    apply_manifest_settings(args, manifest)
    run_id = args.run_id or time.strftime("%Y%m%dT%H%M%S")
    run_dir = args.out_dir / run_id
    fixture_dir = args.fixture_dir / run_id
    run_dir.mkdir(parents=True, exist_ok=True)

    if not args.no_build:
        run_checked([
            "cargo",
            "build",
            "-p",
            "revelo-cli",
            "--bin",
            "revelo",
            "--example",
            "perf_probe",
            "--release",
        ])

    tools = resolve_tools(args)
    revelo_versions = resolve_revelo_versions(manifest)
    results: list[JsonObject] = []

    for case in manifest["cases"]:
        case_result = run_case(case, args, tools, revelo_versions, run_dir, fixture_dir)
        results.append(case_result)

    output = {
        "schema": "revelo_bench_compare_v1",
        "run_id": run_id,
        "repo": {
            "branch": git_text(["branch", "--show-current"]),
            "commit": git_text(["rev-parse", "--short", "HEAD"]),
        },
        "environment": environment_snapshot(),
        "tools": tool_versions(tools),
        "revelo_versions": sanitize_revelo_versions(revelo_versions),
        "measurement_backend": {
            "name": "hyperfine",
            "version": tool_version(tools.get("hyperfine"), ["--version"]),
            "required": True,
        },
        "settings": {
            "warmups": args.warmups,
            "runs": args.runs,
            "probe_export": args.probe_export,
            "probe_output_target": args.probe_output_target,
            "fixture_dir": "target/perf-fixtures",
        },
        "cases": results,
    }

    out_path = run_dir / "results.json"
    out_path.write_text(json.dumps(output, indent=2) + "\n")
    if not args.no_render_table:
        table_path = run_dir / "benchmark-table.html"
        run_checked([
            sys.executable,
            str(SCRIPT_DIR / "render_benchmark_table.py"),
            "--results",
            str(out_path),
            "--output",
            str(table_path),
            *(["--config", str(args.table_config)] if args.table_config else []),
        ])
        if args.render_png:
            selector = capture_selector(args.table_config)
            run_checked([
                sys.executable,
                str(SCRIPT_DIR / "capture_benchmark_table.py"),
                "--html",
                str(table_path),
                "--output",
                str(run_dir / "benchmark-table.png"),
                "--selector",
                selector,
            ])
    print(out_path)
    return 0


def run_case(
    case: JsonObject,
    args: argparse.Namespace,
    tools: dict[str, Path | None],
    revelo_versions: list[JsonObject],
    run_dir: Path,
    fixture_dir: Path,
) -> JsonObject:
    path = resolve_case_path(case, fixture_dir)
    if not path.exists():
        raise SystemExit(f"missing case path for {case['label']}: {path}")

    result: JsonObject = {
        "id": case.get("id") or safe_id(case["label"]),
        "label": case["label"],
        "class": case.get("class", ""),
        "format": case.get("format", case.get("container", "")),
        "container": case.get("container", case.get("format", "")),
        "codec": case.get("codec", ""),
        "layout": case.get("layout", ""),
        "source": case.get("source", ""),
        "size_bytes": path.stat().st_size,
        "measurements": {},
    }
    if case_source_kind(case) == "synthetic":
        result["synthetic_kind"] = case["synthetic"].get("kind", "")

    for version in revelo_versions:
        version_id = str(version["id"])
        revelo_cmd = [str(version["path"]), str(path)]
        result["measurements"][version_id] = measure_command(
            tools,
            version_id,
            revelo_cmd,
            args.warmups,
            args.runs,
            run_dir,
            case["label"],
        )

    probe_cmd = [
        str(tools["perf_probe"]),
        "--path",
        str(path),
        "--label",
        str(case["label"]),
        "--export",
        args.probe_export,
        "--output-target",
        args.probe_output_target,
    ]
    if args.probe_output_target == "file":
        probe_cmd.extend(["--output-file", str(run_dir / f"{safe_id(case['label'])}.probe-output")])
    result["measurements"]["revelo_perf_probe"] = run_probe(probe_cmd, args.warmups, args.runs, tools, run_dir, case["label"])

    if tools.get("mediainfo") is not None:
        result["measurements"]["mediainfo"] = measure_command(
            tools,
            "mediainfo",
            [str(tools["mediainfo"]), str(path)],
            args.warmups,
            args.runs,
            run_dir,
            case["label"],
        )
    if tools.get("ffprobe") is not None:
        result["measurements"]["ffprobe"] = measure_command(
            tools,
            "ffprobe",
            [str(tools["ffprobe"]), "-v", "error", "-show_format", "-show_streams", str(path)],
            args.warmups,
            args.runs,
            run_dir,
            case["label"],
        )

    return result


def resolve_case_path(case: JsonObject, fixture_dir: Path) -> Path:
    if case_source_kind(case) == "synthetic":
        return generate_fixtures.generate_case_fixture(case, fixture_dir)
    return Path(case["path"]).expanduser()


def case_source_kind(case: JsonObject) -> SourceKind:
    has_path = bool(case.get("path"))
    has_synthetic = isinstance(case.get("synthetic"), dict)
    if has_path == has_synthetic:
        raise SystemExit(f"case {case.get('id') or case.get('label')} must have exactly one of path or synthetic")
    return "path" if has_path else "synthetic"


def run_probe(
    command: list[str],
    warmups: int,
    runs: int,
    tools: dict[str, Path | None],
    run_dir: Path,
    label: str,
) -> JsonObject:
    measurement = measure_command(tools, "revelo_perf_probe", command, warmups, runs, run_dir, label)
    records = collect_probe_records(command, runs)
    return {"process_ms": measurement, "records": records, "records_diagnostic_only": True}


def collect_probe_records(command: list[str], runs: int) -> list[JsonObject]:
    records: list[JsonObject] = []
    for _ in range(runs):
        completed = subprocess.run(command, cwd=ROOT, text=True, capture_output=True, check=True)
        line = completed.stdout.strip().splitlines()[-1]
        records.append(sanitize_probe_record(json.loads(line)))
    return records


def sanitize_probe_record(record: JsonObject) -> JsonObject:
    sanitized = dict(record)
    sanitized.pop("path", None)
    return sanitized


def measure_command(
    tools: dict[str, Path | None],
    name: str,
    command: list[str],
    warmups: int,
    runs: int,
    run_dir: Path,
    label: str,
) -> JsonObject:
    hyperfine = require_tool(tools, "hyperfine")
    export_path = run_dir / f"{safe_id(label)}-{safe_id(name)}.hyperfine.json"
    hyperfine_command = [
        str(hyperfine),
        "--warmup",
        str(warmups),
        "--runs",
        str(runs),
        "--export-json",
        str(export_path),
        "--command-name",
        name,
        shlex.join(command),
    ]
    try:
        completed = subprocess.run(
            hyperfine_command,
            cwd=ROOT,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
            text=True,
        )
        if completed.returncode != 0:
            message = completed.stderr.strip() or f"exit code {completed.returncode}"
            raise SystemExit(f"hyperfine failed for {name}: {message}")
        data = json.loads(export_path.read_text(encoding="utf-8"))
    finally:
        export_path.unlink(missing_ok=True)
    results = data.get("results", [])
    if not results:
        raise SystemExit(f"hyperfine produced no results for {name}")
    return summarize_hyperfine_result(results[0])


def summarize_hyperfine_result(result: JsonObject) -> JsonObject:
    times = [seconds_to_ms(value) for value in result.get("times", [])]
    return {
        "runs": len(times),
        "mean_ms": seconds_to_ms(result["mean"]),
        "median_ms": seconds_to_ms(result["median"]),
        "min_ms": seconds_to_ms(result["min"]),
        "max_ms": seconds_to_ms(result["max"]),
        "stddev_ms": seconds_to_ms(result.get("stddev", 0.0)),
        "samples_ms": times,
    }


def seconds_to_ms(value: float | None) -> float | None:
    if value is None:
        return None
    return float(value) * 1000.0


def resolve_tools(args: argparse.Namespace) -> dict[str, Path | None]:
    tools: dict[str, Path | None] = {
        "hyperfine": which_path("hyperfine"),
        "revelo": ROOT / "target" / "release" / "revelo",
        "perf_probe": ROOT / "target" / "release" / "examples" / "perf_probe",
        "mediainfo": None if args.skip_mediainfo else which_path("mediainfo"),
        "ffprobe": None if args.skip_ffprobe else which_path("ffprobe"),
    }
    for name in ("perf_probe",):
        tool = tools[name]
        if tool is None or not tool.exists():
            raise SystemExit(f"missing built tool: {tool}")
    require_tool(tools, "hyperfine")
    if not args.skip_mediainfo:
        require_tool(tools, "mediainfo")
    if not args.skip_ffprobe:
        require_tool(tools, "ffprobe")
    return tools


def resolve_revelo_versions(manifest: JsonObject) -> list[JsonObject]:
    raw_versions = manifest.get("revelo_versions")
    if raw_versions is None:
        return [{"id": "revelo_cli_text", "label": "Revelo", "path": ROOT / "target" / "release" / "revelo"}]
    versions: list[JsonObject] = []
    for version in cast(list[JsonObject], raw_versions):
        path = resolve_tool_path(str(version["path"]))
        if not path.exists():
            raise SystemExit(f"missing Revelo version binary for {version['id']}: {path}")
        versions.append({"id": version["id"], "label": version["label"], "path": path})
    return versions


def resolve_tool_path(raw_path: str) -> Path:
    path = Path(raw_path).expanduser()
    if path.is_absolute():
        return path
    return ROOT / path


def sanitize_revelo_versions(versions: list[JsonObject]) -> list[JsonObject]:
    return [
        {
            "id": version["id"],
            "label": version["label"],
            "path": relative_tool_path(cast(Path, version["path"])),
        }
        for version in versions
    ]


def require_tool(tools: dict[str, Path | None], name: str) -> Path:
    tool = tools.get(name)
    if tool is None:
        if name == "hyperfine":
            raise SystemExit("hyperfine is required for bench-compare; install it with `brew install hyperfine`")
        raise SystemExit(f"{name} is required for bench-compare unless its --skip flag is used")
    return tool


def which_path(name: str) -> Path | None:
    resolved = shutil.which(name)
    return Path(resolved) if resolved else None


def environment_snapshot() -> JsonObject:
    return {
        "machine": platform.machine(),
        "system": platform.system(),
        "release": platform.release(),
        "platform": platform.platform(),
        "processor": command_text(["sysctl", "-n", "machdep.cpu.brand_string"]) or platform.processor(),
        "cpu_count": os.cpu_count(),
        "loadavg": list(os.getloadavg()) if hasattr(os, "getloadavg") else None,
        "sw_vers": command_text(["sw_vers"]),
        "rustc": command_text(["rustc", "--version", "--verbose"]),
        "cargo": command_text(["cargo", "--version", "--verbose"]),
    }


def tool_versions(tools: dict[str, Path | None]) -> JsonObject:
    return {
        "hyperfine": tool_version(tools.get("hyperfine"), ["--version"]),
        "revelo": {"available": tools.get("revelo") is not None, "path": relative_tool_path(tools.get("revelo"))},
        "perf_probe": {"available": tools.get("perf_probe") is not None, "path": relative_tool_path(tools.get("perf_probe"))},
        "mediainfo": tool_version(tools.get("mediainfo"), ["--Version"]),
        "ffprobe": tool_version(tools.get("ffprobe"), ["-version"]),
    }


def relative_tool_path(path: Path | None) -> str | None:
    if path is None:
        return None
    try:
        return str(path.relative_to(ROOT))
    except ValueError:
        return path.name


def tool_version(path: Path | None, args: list[str]) -> str | None:
    if path is None:
        return None
    text = command_text([str(path), *args])
    if text is None:
        return None
    return next((line for line in text.splitlines() if line.strip()), "")


def command_text(command: list[str]) -> str | None:
    try:
        completed = subprocess.run(command, cwd=ROOT, text=True, capture_output=True, check=True)
    except (OSError, subprocess.CalledProcessError):
        return None
    return completed.stdout.strip()


def git_text(args: list[str]) -> str | None:
    return command_text(["git", *args])


def run_checked(command: list[str]) -> None:
    subprocess.run(command, cwd=ROOT, check=True)


def load_manifest(path: Path) -> JsonObject:
    return json.loads(path.read_text())


def validate_manifest(manifest: JsonObject) -> None:
    reject_extra_keys("manifest", manifest, MANIFEST_KEYS, "unexpected manifest key")
    if manifest.get("schema") != MANIFEST_SCHEMA:
        raise SystemExit(f"manifest schema must be {MANIFEST_SCHEMA}")
    validate_manifest_settings(manifest.get("settings", {}))
    cases = manifest.get("cases")
    if not isinstance(cases, list) or not cases:
        raise SystemExit("manifest must contain non-empty cases list")
    for index, case in enumerate(cases):
        if not isinstance(case, dict):
            raise SystemExit(f"case {index} must be an object")
        validate_manifest_case(cast(JsonObject, case), index)
    validate_revelo_versions(manifest.get("revelo_versions"))


def validate_revelo_versions(value: Any) -> None:
    if value is None:
        return
    if not isinstance(value, list) or not value:
        raise SystemExit("manifest revelo_versions must be a non-empty list")
    seen: set[str] = set()
    for index, version in enumerate(value):
        if not isinstance(version, dict):
            raise SystemExit(f"revelo_versions {index} must be an object")
        version_object = cast(JsonObject, version)
        reject_extra_keys(f"revelo_versions {index}", version_object, REVELO_VERSION_KEYS, "unexpected revelo_versions key")
        for key in ("id", "label", "path"):
            if not isinstance(version_object.get(key), str) or not version_object[key]:
                raise SystemExit(f"revelo_versions {index} {key} must be a non-empty string")
        version_id = str(version_object["id"])
        if not is_measurement_id(version_id):
            raise SystemExit(f"revelo_versions {index} id must contain only letters, numbers, dash or underscore")
        if version_id in RESERVED_MEASUREMENT_IDS:
            raise SystemExit(f"revelo_versions {index} id {version_id} is reserved")
        if version_id in seen:
            raise SystemExit(f"duplicate revelo_versions id: {version_id}")
        seen.add(version_id)


def is_measurement_id(value: str) -> bool:
    return bool(value) and all(char.isalnum() or char in {"-", "_"} for char in value)


def validate_manifest_settings(settings: Any) -> None:
    if settings == {}:
        return
    if not isinstance(settings, dict):
        raise SystemExit("manifest settings must be an object")
    reject_extra_keys("manifest settings", cast(JsonObject, settings), MANIFEST_SETTINGS_KEYS, "unexpected manifest settings key")
    if "warmups" in settings:
        validate_int_field(settings, "warmups", minimum=0)
    if "runs" in settings:
        validate_int_field(settings, "runs", minimum=1)
    if "render_png" in settings and not isinstance(settings["render_png"], bool):
        raise SystemExit("manifest settings render_png must be boolean")


def validate_manifest_case(case: JsonObject, index: int) -> None:
    reject_extra_keys(f"case {index}", case, CASE_KEYS, "unexpected manifest case key")
    label = case.get("label")
    if not isinstance(label, str) or not label:
        raise SystemExit(f"case {index} missing label")
    for key in ("id", "format", "container", "codec", "layout", "source"):
        if key in case and not isinstance(case[key], str):
            raise SystemExit(f"case {label} field {key} must be a string")
    class_name = case.get("class")
    if class_name not in CASE_CLASSES:
        raise SystemExit(f"case {label} class must be one of: real, synthetic")

    source_kind = case_source_kind(case)
    if class_name == "real":
        validate_real_case(case, label, source_kind)
    else:
        validate_synthetic_case(case, label, source_kind)


def validate_real_case(case: JsonObject, label: str, source_kind: SourceKind) -> None:
    if source_kind != "path":
        raise SystemExit(f"case {label} real cases must use path, not synthetic")
    if not isinstance(case.get("path"), str) or not case["path"]:
        raise SystemExit(f"case {label} path must be a non-empty string")


def validate_synthetic_case(case: JsonObject, label: str, source_kind: SourceKind) -> None:
    if source_kind != "synthetic":
        raise SystemExit(f"case {label} synthetic cases must use synthetic, not path")
    synthetic = case["synthetic"]
    if not isinstance(synthetic, dict):
        raise SystemExit(f"case {label} synthetic must be an object")
    synthetic_object = cast(JsonObject, synthetic)
    reject_extra_keys(f"case {label} synthetic", synthetic_object, SYNTHETIC_KEYS, "unexpected synthetic key")
    kind = synthetic_object.get("kind")
    if not isinstance(kind, str) or not kind:
        raise SystemExit(f"case {label} missing synthetic kind")
    if kind not in generate_fixtures.GENERATORS:
        supported = ", ".join(sorted(generate_fixtures.GENERATORS))
        raise SystemExit(f"case {label} unsupported synthetic fixture kind {kind!r}; supported: {supported}")
    validate_int_field(synthetic_object, "size_bytes", minimum=generate_fixtures.MIN_SIZE)


def validate_int_field(data: JsonObject, key: str, *, minimum: int) -> None:
    value = data.get(key)
    if not isinstance(value, int) or isinstance(value, bool) or value < minimum:
        raise SystemExit(f"{key} must be an integer >= {minimum}")


def reject_extra_keys(where: str, data: JsonObject, allowed: set[str], prefix: str) -> None:
    extra = sorted(set(data) - allowed)
    if extra:
        raise SystemExit(f"{prefix} in {where}: {', '.join(extra)}")


def apply_manifest_settings(args: argparse.Namespace, manifest: JsonObject) -> None:
    raw_settings = manifest.get("settings")
    settings = cast(JsonObject, raw_settings) if isinstance(raw_settings, dict) else {}
    args.warmups = args.warmups if args.warmups is not None else int(settings.get("warmups", 2))
    args.runs = args.runs if args.runs is not None else int(settings.get("runs", 10))
    if bool(settings.get("render_png", False)):
        args.render_png = True


def safe_id(value: str) -> str:
    return "".join(char if char.isalnum() or char in ("-", "_") else "-" for char in value).strip("-") or "case"


def capture_selector(config_path: Path | None) -> str:
    if config_path is None:
        return "#benchmark-table-capture"
    try:
        config = json.loads(config_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return "#benchmark-table-capture"
    capture_id = config.get("capture_id") or "benchmark-table-capture"
    return f"#{capture_id}"


def self_test() -> int:
    with tempfile.TemporaryDirectory() as tmp:
        manifest_path = Path(tmp) / "manifest.json"
        manifest_path.write_text(
            json.dumps(
                {
                    "schema": "revelo_perf_manifest_v2",
                    "settings": {"warmups": 1, "runs": 2},
                    "cases": [
                        {
                            "label": "public-sample-label",
                            "class": "real",
                            "path": "relative/local-only.mp4",
                            "format": "MP4",
                            "source": "local",
                        }
                    ],
                }
            )
        )
        manifest = load_manifest(manifest_path)
        validate_manifest(manifest)
        assert manifest["cases"][0]["label"] == "public-sample-label"
        summary = summarize_hyperfine_result(
            {
                "mean": 0.003,
                "median": 0.002,
                "min": 0.001,
                "max": 0.004,
                "stddev": 0.001,
                "times": [0.003, 0.001, 0.002],
            }
        )
        assert summary["median_ms"] == 2.0
        sanitized = sanitize_probe_record({"label": "public", "path": "local-only/sample.mp4"})
        assert sanitized == {"label": "public"}
    print("self-test ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
