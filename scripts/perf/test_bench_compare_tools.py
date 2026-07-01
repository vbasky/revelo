#!/usr/bin/env python3
"""Self-tests for the local benchmark comparison tooling."""

from __future__ import annotations

import importlib.util
import json
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]


def load_module(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None:
        raise AssertionError(f"cannot load module at {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


run_perf = load_module("run_perf_investigation", ROOT / "scripts" / "perf" / "run_perf_investigation.py")
fixtures = load_module("generate_fixtures", ROOT / "scripts" / "perf" / "generate_fixtures.py")
renderer = load_module("render_benchmark_table", ROOT / "scripts" / "perf" / "render_benchmark_table.py")
oracle = load_module("run_oracle_parity", ROOT / "scripts" / "perf" / "run_oracle_parity.py")
wasm_probe = load_module("run_wasm_probe", ROOT / "scripts" / "perf" / "run_wasm_probe.py")
evidence = load_module("run_benchmark_evidence", ROOT / "scripts" / "perf" / "run_benchmark_evidence.py")


def test_manifest_v2_accepts_generated_and_real_cases() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        real = Path(tmp) / "real.mp4"
        real.write_bytes(b"real")
        manifest = {
            "schema": "revelo_perf_manifest_v2",
            "settings": {"warmups": 1, "runs": 2},
            "cases": [
                {
                    "id": "generated-mp4",
                    "label": "MP4 / generated",
                    "class": "synthetic",
                    "container": "MP4",
                    "codec": "H.264",
                    "layout": "moov tail",
                    "source": "generated",
                    "synthetic": {"kind": "mp4_snv2_tail", "size_bytes": 1048576},
                },
                {
                    "id": "real-mp4",
                    "label": "MP4 / real",
                    "class": "real",
                    "container": "MP4",
                    "codec": "H.264",
                    "layout": "real media",
                    "source": "private local corpus",
                    "path": str(real),
                },
            ],
        }
        run_perf.validate_manifest(manifest)
        assert run_perf.case_source_kind(manifest["cases"][0]) == "synthetic"
        assert run_perf.case_source_kind(manifest["cases"][1]) == "path"


def test_manifest_v2_accepts_configured_revelo_versions() -> None:
    manifest = {
        "schema": "revelo_perf_manifest_v2",
        "revelo_versions": [
            {"id": "revelo_0_4_6", "label": "Revelo 0.4.6", "path": "target/perf-tools/revelo-0.4.6"},
            {"id": "revelo_branch", "label": "Revelo branch", "path": "target/release/revelo"},
        ],
        "cases": [
            {
                "id": "generated-mp4",
                "label": "MP4 / generated",
                "class": "synthetic",
                "container": "MP4",
                "synthetic": {"kind": "mp4_snv2_tail", "size_bytes": 1048576},
            }
        ],
    }
    run_perf.validate_manifest(manifest)


def test_manifest_v2_rejects_invalid_revelo_version_ids() -> None:
    manifest = {
        "schema": "revelo_perf_manifest_v2",
        "revelo_versions": [{"id": "mediainfo", "label": "bad", "path": "bin/revelo"}],
        "cases": [
            {
                "id": "generated-mp4",
                "label": "MP4 / generated",
                "class": "synthetic",
                "container": "MP4",
                "synthetic": {"kind": "mp4_snv2_tail", "size_bytes": 1048576},
            }
        ],
    }
    try:
        run_perf.validate_manifest(manifest)
    except SystemExit as error:
        assert "reserved" in str(error)
    else:
        raise AssertionError("manifest with reserved Revelo version id should fail")


def test_manifest_v2_rejects_cases_with_both_path_and_synthetic() -> None:
    manifest = {
        "schema": "revelo_perf_manifest_v2",
        "cases": [
            {
                "id": "bad",
                "label": "bad",
                "class": "real",
                "path": "local.mp4",
                "synthetic": {"kind": "mp4_moov_front", "size_bytes": 1024},
            }
        ],
    }
    try:
        run_perf.validate_manifest(manifest)
    except SystemExit as error:
        assert "exactly one" in str(error)
    else:
        raise AssertionError("manifest case with both path and synthetic should fail")


def test_manifest_v2_rejects_extra_top_level_keys() -> None:
    manifest = {
        "schema": "revelo_perf_manifest_v2",
        "unexpected": True,
        "cases": [
            {
                "id": "generated-mp4",
                "label": "MP4 / generated",
                "class": "synthetic",
                "container": "MP4",
                "synthetic": {"kind": "mp4_snv2_tail", "size_bytes": 1048576},
            }
        ],
    }
    try:
        run_perf.validate_manifest(manifest)
    except SystemExit as error:
        assert "unexpected manifest key" in str(error)
    else:
        raise AssertionError("manifest with extra top-level key should fail")


def test_manifest_v2_rejects_invalid_class_and_source_mismatch() -> None:
    manifest = {
        "schema": "revelo_perf_manifest_v2",
        "cases": [
            {
                "id": "bad-class",
                "label": "bad",
                "class": "generated",
                "container": "MP4",
                "synthetic": {"kind": "mp4_snv2_tail", "size_bytes": 1048576},
            }
        ],
    }
    try:
        run_perf.validate_manifest(manifest)
    except SystemExit as error:
        assert "class must be one of" in str(error)
    else:
        raise AssertionError("manifest with invalid class should fail")

    manifest["cases"][0]["class"] = "real"
    try:
        run_perf.validate_manifest(manifest)
    except SystemExit as error:
        assert "real cases must use path" in str(error)
    else:
        raise AssertionError("real case with synthetic source should fail")


def test_manifest_v2_rejects_unknown_synthetic_kind() -> None:
    manifest = {
        "schema": "revelo_perf_manifest_v2",
        "cases": [
            {
                "id": "bad-kind",
                "label": "bad kind",
                "class": "synthetic",
                "container": "MP4",
                "synthetic": {"kind": "not_a_fixture", "size_bytes": 1048576},
            }
        ],
    }
    try:
        run_perf.validate_manifest(manifest)
    except SystemExit as error:
        assert "unsupported synthetic fixture kind" in str(error)
    else:
        raise AssertionError("manifest with unknown synthetic kind should fail")


def test_table_config_rejects_unknown_keys() -> None:
    try:
        renderer.validate_config({"schema": "revelo_benchmark_table_config_v1", "surprise": True})
    except SystemExit as error:
        assert "unexpected table config key" in str(error)
    else:
        raise AssertionError("table config with extra key should fail")


def test_oracle_parity_detects_required_field_truncation() -> None:
    revelo_json = {
        "media": {
            "track": [
                {"@type": "General", "Format": "MPEG-4"},
                {"@type": "Video", "Format": "AVC"},
            ]
        }
    }
    mediainfo_json = {
        "media": {
            "track": [
                {"@type": "General", "Format": "MPEG-4", "CodecID": "isom"},
                {"@type": "Video", "Format": "AVC"},
            ]
        }
    }
    config = {
        "required_fields": [
            {"track_type": "General", "field": "CodecID"},
            {"track_type": "Video", "field": "Format"},
        ],
        "ignore_fields": [],
    }
    result = oracle.compare_documents("case", revelo_json, mediainfo_json, config)
    assert result["required_failures"] == [
        {
            "track": "General#0",
            "field": "CodecID",
            "expected": "isom",
            "actual": None,
        }
    ]
    assert result["status"] == "fail"


def test_oracle_parity_omits_private_paths_from_case_result() -> None:
    case = {"id": "real-mp4", "label": "Real MP4", "path": "/Users/private/sample.mp4"}
    result = oracle.case_result_shell(case, Path("/Users/private/sample.mp4"))
    assert result == {"id": "real-mp4", "label": "Real MP4", "size_bytes": 0}


def test_wasm_probe_summary_uses_median_samples() -> None:
    summary = wasm_probe.summarize_samples([3.0, 1.0, 2.0])
    assert summary == {
        "runs": 3,
        "mean_ms": 2.0,
        "median_ms": 2.0,
        "min_ms": 1.0,
        "max_ms": 3.0,
        "samples_ms": [3.0, 1.0, 2.0],
    }


def test_evidence_plan_reuses_run_id_and_fixture_dir() -> None:
    plan = evidence.build_plan(
        manifest=Path("local-manifest.json"),
        run_id="run-1",
        out_dir=Path("target/perf-investigation"),
        fixture_dir=Path("target/perf-fixtures"),
        table_config=Path("scripts/perf/table.config.example.json"),
        oracle_config=Path("scripts/perf/oracle.config.example.json"),
        include_oracle_in_table=False,
        include_wasm=False,
        warmups=None,
        runs=None,
    )
    assert [step.name for step in plan] == ["bench_compare", "oracle_parity", "render_table", "capture_table"]
    for step in plan:
        command = [str(part) for part in step.command]
        if step.name in {"bench_compare", "oracle_parity"}:
            assert "--run-id" in command
            assert "run-1" in command
            assert "--fixture-dir" in command
            assert "target/perf-fixtures" in command
    render_command = [str(part) for part in plan[2].command]
    assert "--oracle-results" not in render_command


def test_evidence_plan_can_render_oracle_when_requested() -> None:
    plan = evidence.build_plan(
        manifest=Path("local-manifest.json"),
        run_id="run-1",
        out_dir=Path("target/perf-investigation"),
        fixture_dir=Path("target/perf-fixtures"),
        table_config=Path("scripts/perf/table.config.example.json"),
        oracle_config=Path("scripts/perf/oracle.config.example.json"),
        include_oracle_in_table=True,
        include_wasm=False,
        warmups=None,
        runs=None,
    )
    render_command = [str(part) for part in plan[2].command]
    assert "--oracle-results" in render_command


def test_evidence_plan_can_include_wasm() -> None:
    plan = evidence.build_plan(
        manifest=Path("local-manifest.json"),
        run_id="run-1",
        out_dir=Path("target/perf-investigation"),
        fixture_dir=Path("target/perf-fixtures"),
        table_config=Path("scripts/perf/table.config.example.json"),
        oracle_config=Path("scripts/perf/oracle.config.example.json"),
        include_oracle_in_table=False,
        include_wasm=True,
        warmups=1,
        runs=2,
    )
    assert [step.name for step in plan] == ["bench_compare", "oracle_parity", "render_table", "capture_table", "wasm_probe"]
    wasm_command = [str(part) for part in plan[-1].command]
    assert "--warmups" in wasm_command
    assert "--runs" in wasm_command


def test_fixture_generator_creates_sparse_mp4() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        case = {
            "id": "generated-mp4",
            "label": "MP4 / generated",
            "synthetic": {"kind": "mp4_snv2_tail", "size_bytes": 1048576},
        }
        path = fixtures.generate_case_fixture(case, Path(tmp))
        assert path.exists()
        assert path.stat().st_size == 1048576
        assert path.read_bytes()[4:8] == b"ftyp"


def test_hyperfine_summary_converts_seconds_to_milliseconds() -> None:
    summary = run_perf.summarize_hyperfine_result(
        {
            "command": "revelo sample.mp4",
            "mean": 0.0125,
            "median": 0.011,
            "min": 0.010,
            "max": 0.019,
            "stddev": 0.002,
            "times": [0.010, 0.011, 0.019],
        }
    )
    assert summary == {
        "runs": 3,
        "mean_ms": 12.5,
        "median_ms": 11.0,
        "min_ms": 10.0,
        "max_ms": 19.0,
        "stddev_ms": 2.0,
        "samples_ms": [10.0, 11.0, 19.0],
    }


def test_hyperfine_is_required_for_process_measurements() -> None:
    try:
        run_perf.require_tool({"hyperfine": None}, "hyperfine")
    except SystemExit as error:
        assert "hyperfine is required" in str(error)
    else:
        raise AssertionError("missing hyperfine must fail")


def test_resolve_revelo_versions_defaults_to_current_release_binary() -> None:
    versions = run_perf.resolve_revelo_versions({"schema": "revelo_perf_manifest_v2"})
    assert versions == [
        {
            "id": "revelo_cli_text",
            "label": "Revelo",
            "path": ROOT / "target" / "release" / "revelo",
        }
    ]


def test_renderer_auto_orders_common_revelo_version_columns() -> None:
    results = {
        "cases": [
            {
                "measurements": {
                    "ffprobe": {"median_ms": 1.0},
                    "revelo_branch": {"median_ms": 1.0},
                    "revelo_0_4_6": {"median_ms": 1.0},
                    "mediainfo": {"median_ms": 1.0},
                }
            }
        ]
    }
    assert renderer.visible_columns(results) == ["revelo_0_4_6", "revelo_branch", "mediainfo", "ffprobe"]


def test_renderer_omits_private_paths() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        results = {
            "schema": "revelo_bench_compare_v1",
            "run_id": "test-run",
            "repo": {"branch": "test", "commit": "abc1234"},
            "environment": {"system": "Darwin", "processor": "Test CPU"},
            "settings": {"warmups": 1, "runs": 2},
            "tools": {},
            "cases": [
                {
                    "id": "real-mp4",
                    "label": "MP4 / real",
                    "class": "real",
                    "container": "MP4",
                    "codec": "H.264",
                    "layout": "real media",
                    "source": "private local corpus",
                    "size_bytes": 1024,
                    "measurements": {
                        "revelo_cli_text": {"median_ms": 7.5, "runs": 2},
                        "mediainfo": {"median_ms": 12.0, "runs": 2},
                    },
                }
            ],
        }
        input_path = Path(tmp) / "results.json"
        output_path = Path(tmp) / "table.html"
        input_path.write_text(json.dumps(results), encoding="utf-8")
        renderer.render_table(input_path, output_path, None)
        html = output_path.read_text(encoding="utf-8")
        assert "MP4 / real" in html
        assert "7.5" in html
        assert "/Users/" not in html
        assert "private/path" not in html


def test_renderer_uses_external_template_and_stylesheet_config() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        template_path = Path(tmp) / "custom-template.html"
        stylesheet_path = Path(tmp) / "custom.css"
        config_path = Path(tmp) / "table.json"
        results_path = Path(tmp) / "results.json"
        output_path = Path(tmp) / "table.html"
        template_path.write_text(
            "<main id=\"{{ capture_id }}\"><style>{{ css }}</style>{{ sections }}{{ legend }}</main>",
            encoding="utf-8",
        )
        stylesheet_path.write_text(".custom-token { color: white; }", encoding="utf-8")
        config_path.write_text(
            json.dumps(
                {
                    "schema": "revelo_benchmark_table_config_v1",
                    "template_path": str(template_path),
                    "stylesheet_path": str(stylesheet_path),
                    "capture_id": "custom-capture",
                    "columns": ["revelo_cli_text"],
                    "latency_tiers_ms": [{"name": "instant", "label": "under 10 ms", "max": None, "class": "instant"}],
                }
            ),
            encoding="utf-8",
        )
        results_path.write_text(
            json.dumps(
                {
                    "schema": "revelo_bench_compare_v1",
                    "run_id": "test-run",
                    "repo": {"commit": "abc1234"},
                    "environment": {"processor": "Test CPU"},
                    "settings": {"warmups": 1, "runs": 2},
                    "cases": [
                        {
                            "id": "synthetic",
                            "label": "Synthetic row",
                            "class": "synthetic",
                            "container": "MP4",
                            "size_bytes": 1024,
                            "measurements": {"revelo_cli_text": {"median_ms": 4.0, "runs": 2}},
                        }
                    ],
                }
            ),
            encoding="utf-8",
        )
        renderer.render_table(results_path, output_path, config_path)
        html = output_path.read_text(encoding="utf-8")
        assert "custom-capture" in html
        assert ".custom-token" in html
        assert "Synthetic row" in html


def test_renderer_can_include_oracle_status_column() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        results_path = Path(tmp) / "results.json"
        oracle_path = Path(tmp) / "oracle-parity.json"
        output_path = Path(tmp) / "table.html"
        results_path.write_text(
            json.dumps(
                {
                    "schema": "revelo_bench_compare_v1",
                    "run_id": "test-run",
                    "repo": {"commit": "abc1234"},
                    "environment": {"processor": "Test CPU"},
                    "settings": {"warmups": 1, "runs": 2},
                    "cases": [
                        {
                            "id": "synthetic",
                            "label": "Synthetic row",
                            "class": "synthetic",
                            "container": "MP4",
                            "size_bytes": 1024,
                            "measurements": {"revelo_cli_text": {"median_ms": 4.0, "runs": 2}},
                        }
                    ],
                }
            ),
            encoding="utf-8",
        )
        oracle_path.write_text(
            json.dumps(
                {
                    "schema": "revelo_oracle_parity_v1",
                    "run_id": "test-run",
                    "cases": [{"id": "synthetic", "label": "Synthetic row", "status": "pass"}],
                }
            ),
            encoding="utf-8",
        )
        renderer.render_table(results_path, output_path, None, oracle_path)
        html = output_path.read_text(encoding="utf-8")
        assert "Oracle" in html
        assert "oracle-pass" in html
        assert ">pass<" in html


def main() -> int:
    for name, value in sorted(globals().items()):
        if name.startswith("test_") and callable(value):
            value()
    print("bench compare tooling self-tests ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
