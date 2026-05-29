# Homebrew tap

Revelo publishes pre-built binaries for macOS (ARM + Intel) and Linux to every
GitHub Release. The CI workflow (`.github/workflows/release.yml`) builds them and
generates a Homebrew formula with the correct SHA256 checksums.

## Setup

```sh
brew tap vbasky/revelo
brew install revelo
```

## How it works

Separate repos:

| Repo | Purpose |
| --- | --- |
| `vbasky/revelo` | Source code, CI, release builds |
| `vbasky/homebrew-revelo` | Homebrew tap — contains `Formula/revelo.rb` |

On every `v*` tag push the release workflow:

1. Builds release binaries for `aarch64-apple-darwin`, `x86_64-apple-darwin`,
   and `x86_64-unknown-linux-gnu`.
2. Creates a GitHub Release and uploads the tarballs + SHA256 files.
3. Generates a Homebrew formula (`revelo.rb`) with the checksums baked in.
4. Pushes the formula to `vbasky/homebrew-revelo` (if `TAP_REPO` secret is set).

## Manual formula update

If you don't want auto-push, download the `homebrew-formula` artifact from the
release run and copy it into the tap repo manually:

```sh
# In the homebrew-tap repo
cp ~/Downloads/revelo.rb Formula/
git add Formula/revelo.rb
git commit -m "revelo v<VERSION>"
git push
```

## Bootstrap the tap repo (one-time)

```sh
scripts/bootstrap-homebrew-tap.sh
```

This creates `vbasky/homebrew-revelo` on GitHub with the initial formula.
