#!/usr/bin/env python3
"""Capture the standalone benchmark table HTML as a PNG when Playwright exists."""

from __future__ import annotations

import argparse
import shutil
import subprocess
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
    script_path = args.output.with_suffix(".capture.mjs")
    try:
        script_path.write_text(script, encoding="utf-8")
        completed = subprocess.run(["node", str(script_path)], text=True, capture_output=True)
        if completed.returncode != 0:
            print("Playwright capture unavailable; HTML table is still available")
            if completed.stderr:
                print(completed.stderr.strip())
            return 0
    finally:
        script_path.unlink(missing_ok=True)
    print(args.output)
    return 0


def build_script(html_path: Path, output_path: Path, selector: str) -> str:
    return f"""
import {{ chromium }} from 'playwright';

const browser = await chromium.launch();
const page = await browser.newPage({{ viewport: {{ width: 1600, height: 1200 }}, deviceScaleFactor: 2 }});
await page.goto({html_path.as_uri()!r});
const locator = page.locator({selector!r});
await locator.waitFor({{ state: 'visible' }});
const measure = async () => locator.evaluate((element) => {{
  const rects = [element, ...element.querySelectorAll('*')].map((node) => node.getBoundingClientRect());
  const left = Math.min(...rects.map((rect) => rect.left));
  const top = Math.min(...rects.map((rect) => rect.top));
  const right = Math.max(...rects.map((rect) => rect.right));
  const bottom = Math.max(...rects.map((rect) => rect.bottom));
  return {{
    x: Math.max(0, Math.floor(left)),
    y: Math.max(0, Math.floor(top)),
    width: Math.ceil(Math.max(element.scrollWidth, right - left)),
    height: Math.ceil(Math.max(element.scrollHeight, bottom - top)),
  }};
}});
const initial = await measure();
await page.setViewportSize({{ width: initial.width, height: initial.height }});
const box = await measure();
if (!box || box.width <= 0 || box.height <= 0) {{
  throw new Error('missing capture element');
}}
await page.screenshot({{
  path: {str(output_path)!r},
  clip: {{
    x: box.x,
    y: box.y,
    width: box.width,
    height: box.height,
  }},
}});
await browser.close();
"""


if __name__ == "__main__":
    raise SystemExit(main())
