#!/usr/bin/env python3
"""Measure revelo-wasm parsing with already-loaded byte buffers.

This complements CLI benchmarks: it measures the wasm-bindgen parse call on a
Uint8Array, so mmap, filesystem page faults and CLI export costs are excluded.
"""

from __future__ import annotations

import argparse
import json
import platform
import shutil
import statistics
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from typing import Any, TypeAlias, cast


ROOT = Path(__file__).resolve().parents[2]
SCRIPT_DIR = Path(__file__).resolve().parent
DEFAULT_OUT_DIR = ROOT / "target" / "perf-investigation"
DEFAULT_FIXTURE_DIR = ROOT / "target" / "perf-fixtures"
DEFAULT_WASM_PKG_DIR = ROOT / "target" / "perf-investigation" / "wasm-pkg"

if str(SCRIPT_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPT_DIR))

import generate_fixtures  # noqa: E402
import run_perf_investigation as run_perf  # noqa: E402


JsonObject: TypeAlias = dict[str, Any]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, help="manifest with real and/or generated cases")
    parser.add_argument("--out-dir", type=Path, default=DEFAULT_OUT_DIR)
    parser.add_argument("--fixture-dir", type=Path, default=DEFAULT_FIXTURE_DIR)
    parser.add_argument("--wasm-pkg-dir", type=Path, default=DEFAULT_WASM_PKG_DIR)
    parser.add_argument("--run-id")
    parser.add_argument("--warmups", type=int)
    parser.add_argument("--runs", type=int)
    parser.add_argument("--no-build", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    if args.self_test:
        return self_test()
    if args.manifest is None:
        parser.error("--manifest is required unless --self-test is used")

    manifest = run_perf.load_manifest(args.manifest)
    run_perf.validate_manifest(manifest)
    run_perf.apply_manifest_settings(args, manifest)

    if not args.no_build:
        build_wasm_package(args.wasm_pkg_dir)
    require_wasm_package(args.wasm_pkg_dir)
    node = which_required("node")

    run_id = args.run_id or time.strftime("%Y%m%dT%H%M%S")
    run_dir = args.out_dir / run_id
    fixture_dir = args.fixture_dir / run_id
    run_dir.mkdir(parents=True, exist_ok=True)

    cases = [
        run_case(case, args, node, args.wasm_pkg_dir, fixture_dir)
        for case in cast(list[JsonObject], manifest["cases"])
    ]
    output = {
        "schema": "revelo_wasm_probe_results_v1",
        "run_id": run_id,
        "repo": {
            "branch": run_perf.git_text(["branch", "--show-current"]),
            "commit": run_perf.git_text(["rev-parse", "--short", "HEAD"]),
        },
        "environment": environment_snapshot(),
        "settings": {"warmups": args.warmups, "runs": args.runs},
        "measurement_note": "WASM parse is timed after Node has read the file into memory.",
        "cases": cases,
    }
    out_path = run_dir / "wasm-probe.json"
    out_path.write_text(json.dumps(output, indent=2) + "\n", encoding="utf-8")
    print(out_path)
    return 0


def run_case(
    case: JsonObject,
    args: argparse.Namespace,
    node: Path,
    wasm_pkg_dir: Path,
    fixture_dir: Path,
) -> JsonObject:
    path = resolve_case_path(case, fixture_dir)
    raw = run_node_probe(node, wasm_pkg_dir, path, args.warmups, args.runs)
    return {
        "id": case.get("id") or run_perf.safe_id(str(case["label"])),
        "label": case["label"],
        "class": case.get("class", ""),
        "container": case.get("container", case.get("format", "")),
        "codec": case.get("codec", ""),
        "layout": case.get("layout", ""),
        "size_bytes": path.stat().st_size,
        "wasm_parse_ms": summarize_samples(raw["samples_ms"]),
        "recognized": raw["recognized"],
        "json_bytes": raw["json_bytes"],
    }


def resolve_case_path(case: JsonObject, fixture_dir: Path) -> Path:
    if run_perf.case_source_kind(case) == "synthetic":
        return generate_fixtures.generate_case_fixture(case, fixture_dir)
    return Path(str(case["path"])).expanduser()


def run_node_probe(node: Path, wasm_pkg_dir: Path, path: Path, warmups: int, runs: int) -> JsonObject:
    with tempfile.TemporaryDirectory() as tmp:
        runner = Path(tmp) / "wasm_probe_runner.cjs"
        runner.write_text(node_runner_source(), encoding="utf-8")
        completed = subprocess.run(
            [
                str(node),
                str(runner),
                str(wasm_pkg_dir / "revelo_wasm.js"),
                str(path),
                str(warmups),
                str(runs),
            ],
            cwd=ROOT,
            text=True,
            capture_output=True,
            check=True,
        )
    data = json.loads(completed.stdout)
    if not isinstance(data, dict):
        raise SystemExit("wasm probe runner returned invalid JSON")
    return cast(JsonObject, data)


def node_runner_source() -> str:
    return r"""
const fs = require("fs");
const { performance } = require("perf_hooks");

const [pkgPath, filePath, warmupsRaw, runsRaw] = process.argv.slice(2);
const wasm = require(pkgPath);
const data = fs.readFileSync(filePath);
const warmups = Number.parseInt(warmupsRaw, 10);
const runs = Number.parseInt(runsRaw, 10);

let last = null;
for (let i = 0; i < warmups; i += 1) {
  last = wasm.parse(data);
}

const samples = [];
for (let i = 0; i < runs; i += 1) {
  const start = performance.now();
  last = wasm.parse(data);
  samples.push(performance.now() - start);
}

process.stdout.write(JSON.stringify({
  recognized: last !== null && last !== undefined,
  json_bytes: last ? Buffer.byteLength(last, "utf8") : 0,
  samples_ms: samples
}));
"""


def summarize_samples(samples: list[float]) -> JsonObject:
    if not samples:
        raise SystemExit("cannot summarize empty sample list")
    return {
        "runs": len(samples),
        "mean_ms": statistics.fmean(samples),
        "median_ms": statistics.median(samples),
        "min_ms": min(samples),
        "max_ms": max(samples),
        "samples_ms": samples,
    }


def build_wasm_package(out_dir: Path) -> None:
    wasm_pack = which_required("wasm-pack")
    out_dir.mkdir(parents=True, exist_ok=True)
    subprocess.run(
        [
            str(wasm_pack),
            "build",
            "crates/revelo-wasm",
            "--release",
            "--target",
            "nodejs",
            "--out-dir",
            str(out_dir),
        ],
        cwd=ROOT,
        check=True,
    )


def require_wasm_package(out_dir: Path) -> None:
    if not (out_dir / "revelo_wasm.js").exists():
        raise SystemExit(f"missing wasm package at {out_dir}; run without --no-build or run `just bench-wasm`")


def which_required(name: str) -> Path:
    resolved = shutil.which(name)
    if resolved is None:
        raise SystemExit(f"{name} is required for WASM probe")
    return Path(resolved)


def environment_snapshot() -> JsonObject:
    return {
        "system": platform.system(),
        "machine": platform.machine(),
        "platform": platform.platform(),
        "node": command_text(["node", "--version"]),
        "wasm_pack": command_text(["wasm-pack", "--version"]),
        "rustc": command_text(["rustc", "--version", "--verbose"]),
    }


def command_text(command: list[str]) -> str | None:
    try:
        completed = subprocess.run(command, cwd=ROOT, text=True, capture_output=True, check=True)
    except (OSError, subprocess.CalledProcessError):
        return None
    return completed.stdout.strip()


def self_test() -> int:
    summary = summarize_samples([3.0, 1.0, 2.0])
    assert summary["median_ms"] == 2.0
    assert "wasm.parse" in node_runner_source()
    print("wasm probe self-test ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
