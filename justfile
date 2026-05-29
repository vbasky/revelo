# revelo developer tasks — `just` command runner

_default:
    @just --list

# Build the workspace
build:
    cargo build

# Build release with LTO
build-release:
    cargo build --release

# Run all tests
test:
    cargo test

# Run tests with release optimizations (faster)
test-release:
    cargo test --release

# Run clippy with workspace lints
lint:
    cargo clippy --workspace --all-targets

# Format all code
fmt:
    cargo fmt

# Check formatting without changing files
fmt-check:
    cargo fmt --check

# Build documentation
docs:
    cargo doc --no-deps --document-private-items

# Open docs in browser
docs-open: docs
    open target/doc/revelo_core/index.html

# Run the diff harness against a file
diff file:
    cargo run --release -p revelo-diff -- {{file}}

# Run diff with strict (order-sensitive) comparison
diff-strict file:
    cargo run --release -p revelo-diff -- --strict {{file}}

# Inspect a file with the CLI (text output)
inspect file:
    cargo run -p revelo-cli -- {{file}}

# Inspect with XML output
inspect-xml file:
    cargo run -p revelo-cli -- --xml {{file}}

# Inspect with JSON output
inspect-json file:
    cargo run -p revelo-cli -- --json {{file}}

# Run all checks: fmt, clippy, test, doc
check-all: fmt-check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test
    cargo doc --no-deps --document-private-items

# Release a version end-to-end: bump → commit → tag → push → GitHub release → publish
# Usage: just release 0.2.2   (tree must be clean, on master, gh authenticated)
release version:
    ./scripts/release.sh {{version}}

# Clean build artifacts
clean:
    cargo clean

# Update dependencies
update:
    cargo update

# Run cargo audit (requires: cargo install cargo-audit)
audit:
    cargo audit

# Run cargo deny (requires: cargo install cargo-deny)
deny-check:
    cargo deny check
