# Publishing to Homebrew

Revelo aims to be installable via `brew install revelo` directly from
[Homebrew/homebrew-core](https://github.com/Homebrew/homebrew-core).

## Current status

Not yet submitted. The formula is created from source, so it must build
on Homebrew's CI (which it does — `cargo build` with no system deps).

## Submit homebrew-core PR

After each release, open a PR against `Homebrew/homebrew-core`:

```sh
# Clone the homebrew-core tap
git clone https://github.com/Homebrew/homebrew-core.git
cd homebrew-core

# Create a branch
git checkout -b revelo-<version>

# Use brew to generate the formula stub
brew extract --version <version> revelo homebrew/core

# Or write manually:
cat > Formula/r/revelo.rb <<EOF
class Revelo < Formula
  desc "Read technical metadata from any media file, in pure Rust"
  homepage "https://github.com/vbasky/revelo"
  url "https://github.com/vbasky/revelo/archive/refs/tags/v<VERSION>.tar.gz"
  sha256 "<SHA256_OF_SOURCE_TARBALL>"
  license "BSD-2-Clause"
  depends_on "rust" => :build
  def install
    system "cargo", "install", *std_cargo_args(path: "crates/revelo-cli")
  end
  test do
    assert_match version.to_s, shell_output("\#{bin}/revelo --version")
  end
end
EOF

# Test locally
brew install --build-from-source Formula/r/revelo.rb

# Commit and push
git add Formula/r/revelo.rb
git commit -m "revelo <VERSION> (new formula)"
gh pr create --repo Homebrew/homebrew-core --fill
```

## CI formula artifact

The release workflow (`.github/workflows/release.yml`) prints the formula
with the correct SHA to its build log in the `formula` job. Find it under
the workflow run for the release tag.

## Requirements for homebrew-core acceptance

- Formula builds on macOS (ARM + Intel) and Linux — CI verifies this
- `brew audit --strict revelo` passes
- `brew test revelo` passes
- No vendored dependencies (cargo handles this)
- No system dependencies (pure Rust)
