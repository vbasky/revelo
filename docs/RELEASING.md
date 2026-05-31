# Releasing

Releases are **pipeline-driven**: pushing a `vX.Y.Z` tag triggers
[`.github/workflows/release.yml`](../.github/workflows/release.yml), which builds
binaries, creates the GitHub Release, **publishes every crate to crates.io**, and
updates the Homebrew tap. Nothing is published by hand.

## Versioning model

- **Lockstep.** All workspace crates share one version; a release bumps them
  together.
- **Exact inter-crate pins.** Path dependencies carry an exact `version = "X.Y.Z"`
  (e.g. `revelo-core = { path = "...", version = "0.4.2" }`), so every crate must
  be republished each release and they must be published in dependency order.
- `revelo-diff` and `revelo-exif-diff` are `publish = false` (dev-only harnesses)
  and are excluded from crates.io.

## Cutting a release

1. Land the changes on `main` (themed commits are encouraged).
2. **Bump versions** — every crate's package version *and* its inter-crate
   dependency requirements:
   ```sh
   find crates -name Cargo.toml -exec sed -i '' 's/"0\.4\.1"/"0.4.2"/g' {} +
   # macOS sed shown; on Linux use: sed -i 's/.../.../g'
   ```
3. **Update [`CHANGELOG.md`](../CHANGELOG.md)** — add a `## [X.Y.Z] - YYYY-MM-DD`
   section. The pipeline extracts this block verbatim as the GitHub Release notes;
   no section means empty notes.
4. Commit: `release: bump workspace to X.Y.Z`.
5. Annotated tag at that commit: `git tag -a vX.Y.Z -m "revelo X.Y.Z"`.
6. Push, then push the tag (the tag is the trigger):
   ```sh
   git push origin main
   git push origin vX.Y.Z
   ```

## What the pipeline does (on `tags: [v*]`)

1. **Build** release binaries for 4 targets (macOS arm64/x64, Linux x64, Windows x64).
2. **Create GitHub Release** with the CHANGELOG notes + SHA-256 checksums.
3. **Publish to crates.io** — in dependency order, **skipping any version already
   on crates.io** (so partial-failure reruns are safe). It also **bails if a prior
   release run started < 10 minutes ago**, to respect the crates.io rate limit.
4. **Homebrew tap** — regenerate and push the formula to `vbasky/homebrew-revelo`.

### The crates.io publish list

The publish job hard-codes the crate list **in dependency order** — a crate must
appear *after* everything it depends on. It must include **every** publishable
crate. Two that are easy to forget (and were historically omitted, leaving the
`revelo` badge "not found" and breaking the `revelo-parsers-tag` publish):

- **`revelo-exiftool-tables`** — an *optional* dependency of `revelo-parsers-tag`.
  crates.io requires even optional deps to exist, so it must be published (before
  `revelo-parsers-tag`) at every version.
- **`revelo`** — the facade/umbrella crate (depends on `revelo-core`,
  `revelo-dispatcher`, `revelo-parsers-tag`); publish it after those.

Current order: `util → core → exiftool-tables → parsers-{video,archive,audio,image}
→ parsers-tag → parsers-text → parsers-container → reader → export → dispatcher →
revelo → cli → cdylib`.

## crates.io rate limits

- A **brand-new crate** (e.g. the first `revelo` publish) hits the strictest limit
  (small burst, then ~1 per 10 min). New *versions* of existing crates are more
  lenient.
- The pipeline's skip-existing + 10-minute bail guard mean you can safely re-push a
  tag to complete a partially-published release.

## Recovering a partial publish

If the publish job fails partway (e.g. a missing crate in the list), some versions
are already live on crates.io and **cannot be unpublished** — only completed:

1. Fix the publish list (or whatever failed) on `main` and commit.
2. Move the tag to the fixed commit and force-push it:
   ```sh
   git tag -d vX.Y.Z && git tag -a vX.Y.Z -m "..." && git push -f origin vX.Y.Z
   ```
3. The re-run skips already-published versions and publishes the rest. (Wait out
   the 10-minute bail window if needed, or re-run just the publish job.)

> This is exactly how 0.4.2 was completed: the first run published 6 crates then
> failed on `revelo-parsers-tag` (missing `revelo-exiftool-tables@0.4.2`); adding
> the two missing crates to the list and re-pushing the tag finished the set.

## Backport / maintenance releases (`release/v0.3.x`)

1. Branch off the maintenance branch; cherry-pick the fixes (security/panic fixes
   are the usual candidates).
2. Bump versions + CHANGELOG, tag `vX.Y.Z`, push the tag.
3. Publish in a **separate rate-limit window** from any `main`-line release.
4. The crate set differs per branch — e.g. `0.3.x` predates the `revelo` facade and
   `revelo-exiftool-tables`, so they aren't in its publish list.
