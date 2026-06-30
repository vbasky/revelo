#!/usr/bin/env python3
"""Run the reproducible benchmark evidence pipeline with one shared run id."""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import time
from pathlib import Path
from typing import NamedTuple


ROOT = Path(__file__).resolve().parents[2]
SCRIPT_DIR = Path(__file__).resolve().parent
DEFAULT_OUT_DIR = ROOT / "target" / "perf-investigation"
DEFAULT_FIXTURE_DIR = ROOT / "target" / "perf-fixtures"
DEFAULT_TABLE_CONFIG = SCRIPT_DIR / "table.config.example.json"
DEFAULT_ORACLE_CONFIG = SCRIPT_DIR / "oracle.config.example.json"


class EvidenceStep(NamedTuple):
    name: str
    command: list[str | Path]
    output: Path


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, help="manifest shared by compare/oracle/WASM")
    parser.add_argument("--out-dir", type=Path, default=DEFAULT_OUT_DIR)
    parser.add_argument("--fixture-dir", type=Path, default=DEFAULT_FIXTURE_DIR)
    parser.add_argument("--run-id", help="override run id; defaults to current timestamp")
    parser.add_argument("--table-config", type=Path, default=DEFAULT_TABLE_CONFIG)
    parser.add_argument("--oracle-config", type=Path, default=DEFAULT_ORACLE_CONFIG)
    parser.add_argument("--include-wasm", action="store_true")
    parser.add_argument("--warmups", type=int)
    parser.add_argument("--runs", type=int)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    if args.self_test:
        return self_test()
    if args.manifest is None:
        parser.error("--manifest is required unless --self-test is used")

    run_id = args.run_id or time.strftime("%Y%m%dT%H%M%S")
    run_dir = args.out_dir / run_id
    run_dir.mkdir(parents=True, exist_ok=True)

    plan = build_plan(
        manifest=args.manifest,
        run_id=run_id,
        out_dir=args.out_dir,
        fixture_dir=args.fixture_dir,
        table_config=args.table_config,
        oracle_config=args.oracle_config,
        include_wasm=args.include_wasm,
        warmups=args.warmups,
        runs=args.runs,
    )
    completed: list[dict[str, str]] = []
    for step in plan:
        run_checked(step.command)
        completed.append({"name": step.name, "output": str(step.output)})

    summary = {
        "schema": "revelo_benchmark_evidence_v1",
        "run_id": run_id,
        "manifest": str(args.manifest),
        "outputs": completed,
    }
    summary_path = run_dir / "evidence-summary.json"
    summary_path.write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    print(summary_path)
    return 0


def build_plan(
    *,
    manifest: Path,
    run_id: str,
    out_dir: Path,
    fixture_dir: Path,
    table_config: Path,
    oracle_config: Path,
    include_wasm: bool,
    warmups: int | None,
    runs: int | None,
) -> list[EvidenceStep]:
    run_dir = out_dir / run_id
    shared_args: list[str | Path] = [
        "--manifest",
        manifest,
        "--out-dir",
        out_dir,
        "--fixture-dir",
        fixture_dir,
        "--run-id",
        run_id,
    ]
    timing_args = optional_timing_args(warmups, runs)
    steps = [
        EvidenceStep(
            "bench_compare",
            [
                sys.executable,
                SCRIPT_DIR / "run_perf_investigation.py",
                *shared_args,
                *timing_args,
                "--no-render-table",
            ],
            run_dir / "results.json",
        ),
        EvidenceStep(
            "oracle_parity",
            [
                sys.executable,
                SCRIPT_DIR / "run_oracle_parity.py",
                *shared_args,
                "--config",
                oracle_config,
                "--no-build",
            ],
            run_dir / "oracle-parity.json",
        ),
        EvidenceStep(
            "render_table",
            [
                sys.executable,
                SCRIPT_DIR / "render_benchmark_table.py",
                "--results",
                run_dir / "results.json",
                "--oracle-results",
                run_dir / "oracle-parity.json",
                "--output",
                run_dir / "benchmark-table.html",
                "--config",
                table_config,
            ],
            run_dir / "benchmark-table.html",
        ),
        EvidenceStep(
            "capture_table",
            [
                sys.executable,
                SCRIPT_DIR / "capture_benchmark_table.py",
                "--html",
                run_dir / "benchmark-table.html",
                "--output",
                run_dir / "benchmark-table.png",
                "--selector",
                capture_selector(table_config),
            ],
            run_dir / "benchmark-table.png",
        ),
    ]
    if include_wasm:
        steps.append(
            EvidenceStep(
                "wasm_probe",
                [
                    sys.executable,
                    SCRIPT_DIR / "run_wasm_probe.py",
                    *shared_args,
                    *timing_args,
                ],
                run_dir / "wasm-probe.json",
            )
        )
    return steps


def optional_timing_args(warmups: int | None, runs: int | None) -> list[str]:
    args: list[str] = []
    if warmups is not None:
        args.extend(["--warmups", str(warmups)])
    if runs is not None:
        args.extend(["--runs", str(runs)])
    return args


def run_checked(command: list[str | Path]) -> None:
    subprocess.run([str(part) for part in command], cwd=ROOT, check=True)


def capture_selector(config_path: Path) -> str:
    try:
        config = json.loads(config_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return "#benchmark-table-capture"
    capture_id = config.get("capture_id") or "benchmark-table-capture"
    return f"#{capture_id}"


def self_test() -> int:
    plan = build_plan(
        manifest=Path("manifest.json"),
        run_id="self-test",
        out_dir=Path("target/perf-investigation"),
        fixture_dir=Path("target/perf-fixtures"),
        table_config=DEFAULT_TABLE_CONFIG,
        oracle_config=DEFAULT_ORACLE_CONFIG,
        include_wasm=True,
        warmups=1,
        runs=2,
    )
    assert [step.name for step in plan] == ["bench_compare", "oracle_parity", "render_table", "capture_table", "wasm_probe"]
    assert all(
        "--run-id" in [str(part) for part in step.command]
        for step in plan
        if step.name in {"bench_compare", "oracle_parity", "wasm_probe"}
    )
    render_command = [str(part) for part in plan[2].command]
    assert "--oracle-results" in render_command
    print("benchmark evidence self-test ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
