#!/usr/bin/env bash
# Build revelo .deb and .rpm with nFPM. Run from anywhere; uses the git root.
set -euo pipefail

NFPM_VERSION="${NFPM_VERSION:-2.48.0}"
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

if [[ -x "$HOME/.cargo/bin/cargo" ]]; then
  export PATH="$HOME/.cargo/bin:$PATH"
fi

export REVELO_VERSION
REVELO_VERSION="$(python3 -c "import tomllib; print(tomllib.load(open('crates/revelo-cli/Cargo.toml','rb'))['package']['version'])")"

machine="$(uname -m)"
case "$machine" in
  x86_64) export NFPM_ARCH=amd64; rpm_arch=x86_64; nfpm_asset=Linux_x86_64 ;;
  aarch64|arm64) export NFPM_ARCH=arm64; rpm_arch=aarch64; nfpm_asset=Linux_arm64 ;;
  *) echo "unsupported architecture: ${machine}" >&2; exit 1 ;;
esac

if ! command -v nfpm >/dev/null 2>&1; then
  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' EXIT
  tarball="nfpm_${NFPM_VERSION}_${nfpm_asset}.tar.gz"
  curl -fsSL -o "${tmp}/${tarball}" \
    "https://github.com/goreleaser/nfpm/releases/download/v${NFPM_VERSION}/${tarball}"
  tar -xzf "${tmp}/${tarball}" -C "$tmp" nfpm
  export PATH="${tmp}:${PATH}"
fi

cargo build --release -p revelo-cli

mkdir -p dist
deb="dist/revelo_${REVELO_VERSION}-1_${NFPM_ARCH}.deb"
rpm="dist/revelo-${REVELO_VERSION}-1.${rpm_arch}.rpm"
nfpm pkg --config packaging/nfpm.yaml --packager deb --target "$deb"
nfpm pkg --config packaging/nfpm.yaml --packager rpm --target "$rpm"
ls -lh "$deb" "$rpm"
