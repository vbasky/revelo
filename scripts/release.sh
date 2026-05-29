#!/usr/bin/env bash
#
# Release revelo end-to-end, in the one order that keeps crates.io, the git tag,
# and the GitHub release from ever drifting:
#
#   pre-flight  ->  bump versions  ->  commit  ->  tag  ->  push
#               ->  GitHub release  ->  cargo publish (dependency order)
#
# Usage:
#   scripts/release.sh <version>      e.g.  scripts/release.sh 0.2.2
#
# Notes:
#   * cargo publish is driven by the Cargo.toml `version`, not by git tags — but
#     we still tag/release FIRST so the published code always has a matching tag.
#   * Update CHANGELOG.md and commit it before running (the tree must be clean);
#     GitHub release notes are auto-generated from commits via --generate-notes.
#   * Publishing new *versions* of existing crates is not rate-limited. Adding a
#     brand-new crate name for the first time IS (1/10 min) and isn't handled here.

set -euo pipefail

VERSION="${1:?usage: scripts/release.sh <version>   e.g. 0.2.2}"
TAG="v${VERSION}"

cd "$(git rev-parse --show-toplevel)"

# Workspace crates in dependency order. revelo-diff is publish = false (dev-only).
CRATES=(
  revelo-util
  revelo-core
  revelo-parsers-video
  revelo-parsers-archive
  revelo-parsers-audio
  revelo-parsers-image
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
[ "$(git rev-parse --abbrev-ref HEAD)" = "master" ] || { echo "✗ not on master"; exit 1; }
[ -z "$(git status --porcelain)" ]                  || { echo "✗ working tree not clean — commit or stash first"; exit 1; }
git rev-parse "$TAG" >/dev/null 2>&1                 && { echo "✗ tag $TAG already exists"; exit 1; }
command -v gh >/dev/null                             || { echo "✗ gh CLI not found"; exit 1; }
gh auth status >/dev/null 2>&1                       || { echo "✗ gh not authenticated — run: gh auth login"; exit 1; }
cargo publish --help >/dev/null 2>&1                 || { echo "✗ cargo not found"; exit 1; }

CUR=$(grep -m1 '^version = ' crates/revelo-core/Cargo.toml | sed -E 's/version = "([^"]+)"/\1/')
echo "==> releasing revelo  ${CUR}  ->  ${VERSION}"

cargo test --workspace

# ── bump package versions + internal dep pins ──────────────────────────────
# Both the package `version` and `{ path = "...", version = "X" }` dep pins use
# the same `version = "X"` token, so one substitution covers both.
for f in crates/*/Cargo.toml; do
  perl -i -pe "s/version = \"\Q${CUR}\E\"/version = \"${VERSION}\"/g" "$f"
done
cargo build --workspace   # validate manifests + compile before tagging

# ── commit, tag, push ──────────────────────────────────────────────────────
git add crates/*/Cargo.toml CHANGELOG.md
git commit -m "release: ${TAG}"
git tag -a "${TAG}" -m "revelo ${VERSION}"
git push origin master
git push origin "${TAG}"

# ── GitHub release ─────────────────────────────────────────────────────────
gh release create "${TAG}" --title "revelo ${VERSION}" --generate-notes --latest

# ── publish to crates.io in dependency order ───────────────────────────────
# cargo waits for each crate to index before the next can resolve it.
for c in "${CRATES[@]}"; do
  echo "==> cargo publish ${c}@${VERSION}"
  cargo publish -p "${c}"
done

echo "✓ released revelo ${VERSION} — crates.io, tag ${TAG}, and GitHub release all in sync"
