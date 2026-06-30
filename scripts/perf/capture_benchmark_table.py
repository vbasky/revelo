#!/usr/bin/env python3
"""Capture the standalone benchmark table HTML as a PNG when Playwright exists."""

from __future__ import annotations

import argparse
import shutil
import subprocess
import tempfile
from pathlib import Path


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--html", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--selector", default="#benchmark-table-capture")
    args = parser.parse_args()

    if shutil.which("node") is None:
        print("node not found; skipping PNG capture")
        return 0

    args.output.parent.mkdir(parents=True, exist_ok=True)
    script = build_script(args.html.resolve(), args.output.resolve(), args.selector)
    with tempfile.TemporaryDirectory() as tmp:
        script_path = Path(tmp) / "capture.mjs"
        script_path.write_text(script, encoding="utf-8")
        completed = subprocess.run(["node", str(script_path)], text=True, capture_output=True)
        if completed.returncode != 0:
            print("Playwright capture unavailable; HTML table is still available")
            return 0
    print(args.output)
    return 0


def build_script(html_path: Path, output_path: Path, selector: str) -> str:
    return f"""
import {{ chromium }} from 'playwright';

const browser = await chromium.launch();
const page = await browser.newPage({{ viewport: {{ width: 1400, height: 1000 }}, deviceScaleFactor: 2 }});
await page.goto({html_path.as_uri()!r});
const element = await page.locator({selector!r});
const box = await element.boundingBox();
if (!box) {{
  throw new Error('missing capture element');
}}
await page.setViewportSize({{ width: Math.ceil(box.width), height: Math.ceil(box.height) }});
await element.screenshot({{ path: {str(output_path)!r} }});
await browser.close();
"""


if __name__ == "__main__":
    raise SystemExit(main())
