#!/usr/bin/env bash
#
# Bootstrap the Homebrew tap repository (vbasky/homebrew-revelo).
#
# Run this once after the first release to set up the tap repo.
# Subsequent releases are published automatically by CI.
#
# Prerequisites:
#   - gh CLI authenticated
#   - A GitHub release with binaries already exists (run release.sh first)

set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

TAP_REPO="vbasky/homebrew-revelo"
TAG="$(git describe --tags --abbrev=0 2>/dev/null || true)"

if [ -z "$TAG" ]; then
  echo "✗ no git tag found — create a release first"
  exit 1
fi

echo "==> Bootstrapping tap: ${TAP_REPO}"
echo "==> Using tag: ${TAG}"

# Create the tap repo
gh repo create "${TAP_REPO}" --public --description "Homebrew tap for revelo — read technical metadata from any media file" || {
  echo "→ tap repo may already exist, continuing..."
}

# Clone it
TMPDIR="$(mktemp -d)"
git clone "https://github.com/${TAP_REPO}.git" "${TMPDIR}"

mkdir -p "${TMPDIR}/Formula"

# Generate formula by downloading the artifact from the latest release run,
# or from the release assets directly.
#
# We download the tarballs and compute SHAs inline — this works even
# without a completed CI run.
echo "==> Downloading release assets..."
mkdir -p /tmp/revelo-assets

for arch in aarch64-apple-darwin x86_64-apple-darwin x86_64-unknown-linux-gnu; do
  asset="revelo-${TAG}-${arch}.tar.gz"
  echo "  downloading ${asset}..."
  curl -sL "https://github.com/vbasky/revelo/releases/download/${TAG}/${asset}" \
    -o "/tmp/revelo-assets/${asset}"
done

arm64_sha=$(shasum -a 256 /tmp/revelo-assets/revelo-${TAG}-aarch64-apple-darwin.tar.gz | cut -d' ' -f1)
x64_sha=$(shasum -a 256 /tmp/revelo-assets/revelo-${TAG}-x86_64-apple-darwin.tar.gz | cut -d' ' -f1)
linux_sha=$(shasum -a 256 /tmp/revelo-assets/revelo-${TAG}-x86_64-unknown-linux-gnu.tar.gz | cut -d' ' -f1)

cat > "${TMPDIR}/Formula/revelo.rb" <<RUBY
class Revelo < Formula
  desc "Read technical metadata from any media file, in pure Rust"
  homepage "https://github.com/vbasky/revelo"
  license "BSD-2-Clause"

  on_macos do
    on_arm do
      url "https://github.com/vbasky/revelo/releases/download/${TAG}/revelo-${TAG}-aarch64-apple-darwin.tar.gz"
      sha256 "${arm64_sha}"
    end
    on_intel do
      url "https://github.com/vbasky/revelo/releases/download/${TAG}/revelo-${TAG}-x86_64-apple-darwin.tar.gz"
      sha256 "${x64_sha}"
    end
  end

  on_linux do
    url "https://github.com/vbasky/revelo/releases/download/${TAG}/revelo-${TAG}-x86_64-unknown-linux-gnu.tar.gz"
    sha256 "${linux_sha}"
  end

  def install
    bin.install "revelo"
  end

  test do
    assert_match version.to_s, shell_output("\#{bin}/revelo --version")
  end
end
RUBY

echo "==> Committing and pushing formula..."
(cd "${TMPDIR}"
  git add Formula/revelo.rb
  git commit -m "revelo ${TAG}"
  git push
)

rm -rf "${TMPDIR}" /tmp/revelo-assets
echo "✓ Tap repo ready: https://github.com/${TAP_REPO}"
echo "  Users can now run: brew tap vbasky/revelo && brew install revelo"
