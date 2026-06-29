#!/usr/bin/env bash
#
# Release revelo to crates.io.
#
# Binary builds + Homebrew formula generation happen automatically in CI
# (`.github/workflows/release.yml`) when the tag is pushed.
#
# Usage:
#   scripts/release.sh <version>      e.g.  scripts/release.sh 0.2.2
#
# Steps:
#   1. Bump versions in all crate Cargo.toml
#   2. Sync README/rustdoc dependency examples (e.g. 0.5.1 -> revelo = "0.5")
#   3. Commit, tag, push (triggers CI binary build)
#   4. Publish to crates.io (dependency order)
#
# Notes:
#   * Update CHANGELOG.md and commit it before running (the tree must be clean).
#   * CI handles the GitHub Release + binary artifacts automatically.

set -euo pipefail

VERSION="${1:?usage: scripts/release.sh <version>   e.g. 0.2.2}"
TAG="v${VERSION}"

cd "$(git rev-parse --show-toplevel)"

# Workspace crates in dependency order. revelo-diff is publish = false (dev-only).
CRATES=(
  revelo
  revelo-util
  revelo-core
  revelo-parsers-video
  revelo-parsers-archive
  revelo-parsers-audio
  revelo-parsers-image
  revelo-exiftool-tables
  revelo-parsers-tag
  revelo-parsers-text
  revelo-parsers-container
  revelo-reader
  revelo-export
  revelo-dispatcher
  revelo-cli
  revelo-cdylib
)

# ── pre-flight ─────────────────────────────────────────────────────────────
[ "$(git rev-parse --abbrev-ref HEAD)" = "main" ] || { echo "✗ not on main"; exit 1; }
[ -z "$(git status --porcelain)" ]                  || { echo "✗ working tree not clean — commit or stash first"; exit 1; }
git rev-parse "$TAG" >/dev/null 2>&1                 && { echo "✗ tag $TAG already exists"; exit 1; }
command -v gh >/dev/null                             || { echo "✗ gh CLI not found"; exit 1; }
gh auth status >/dev/null 2>&1                       || { echo "✗ gh not authenticated — run: gh auth login"; exit 1; }
cargo publish --help >/dev/null 2>&1                 || { echo "✗ cargo not found"; exit 1; }

CUR=$(grep -m1 '^version = ' crates/revelo-core/Cargo.toml | sed -E 's/version = "([^"]+)"/\1/')
echo "==> releasing revelo  ${CUR}  ->  ${VERSION}"

cargo test --workspace

# ── auto-update docs with live numbers ──────────────────────────────────────
PARSER_COUNT=$(awk '/^pub fn table/,/^\]/' crates/revelo-dispatcher/src/lib.rs | grep -c 'parse_')
CRATE_COUNT=$(find crates -maxdepth 1 -name 'revelo-*' -type d | wc -l | tr -d ' ')

perl -i -pe "s/parsers-\K\d+/${PARSER_COUNT}/" README.md || true
perl -i -pe "s/Crates \| \K\d+/${CRATE_COUNT}/" README.md || true
echo "==> docs: ${PARSER_COUNT} parsers, ${CRATE_COUNT} crates"

# ── bump package versions + internal dep pins ──────────────────────────────
# Both the package `version` and `{ path = "...", version = "X" }` dep pins use
# the same `version = "X"` token, so one substitution covers both.
for f in crates/*/Cargo.toml; do
  perl -i -pe "s/version = \"\Q${CUR}\E\"/version = \"${VERSION}\"/g" "$f"
done

# ── sync README / rustdoc dependency examples ───────────────────────────────
COMPAT="${VERSION%.*}"
export COMPAT
while IFS= read -r -d '' f; do
  perl -i -pe '
    s/(revelo(?:-[a-z-]+)?)\s*=\s*"\K0\.\d+(?:\.\d+)?(?=")/$ENV{COMPAT}/g;
    s/(revelo(?:-[a-z-]+)?)\s*=\s*\{\s*version\s*=\s*"\K0\.\d+(?:\.\d+)?(?=")/$ENV{COMPAT}/g;
  ' "$f"
done < <(find crates -type f \( -name README.md -o -path '*/src/lib.rs' \) -print0)
echo "==> docs: dependency examples -> \"${COMPAT}\""

cargo build --workspace   # validate manifests + compile before tagging

# ── commit, tag, push ──────────────────────────────────────────────────────
git add crates/*/Cargo.toml crates/*/README.md crates/*/src/lib.rs CHANGELOG.md
git commit -m "release: ${TAG}"
git tag -a "${TAG}" -m "revelo ${VERSION}"
git push origin main
git push origin "${TAG}"
echo "==> tag pushed — CI is building binaries and creating the GitHub Release"

# ── wait for CI to finish, then publish to crates.io ────────────────────────
# cargo waits for each crate to index before the next can resolve it.
for c in "${CRATES[@]}"; do
  echo "==> cargo publish ${c}@${VERSION}"
  cargo publish -p "${c}"
done

echo "✓ released revelo ${VERSION} — crates.io, tag ${TAG}, and GitHub release all in sync"
