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

# Run benchmarks (requires criterion; generates HTML report in target/criterion/)
bench:
    cargo bench -p revelo

# Run benchmarks and open the HTML report
bench-report: bench
    open target/criterion/report/index.html

# Run cross-tool benchmark comparison from a local manifest.
bench-compare manifest table_config="scripts/perf/table.config.example.json":
    cargo run -p revelo-bench -- compare --manifest "{{manifest}}" --table-config "{{table_config}}" --render-png

# Run cross-tool benchmark comparison and stop after sanitized JSON.
bench-compare-json manifest:
    cargo run -p revelo-bench -- compare --manifest "{{manifest}}" --no-render-table

# Render the standalone comparison table from existing JSON.
bench-compare-table results table_config="scripts/perf/table.config.example.json":
    cargo run -p revelo-bench -- render-table --results "{{results}}" --output "{{replace(results, "results.json", "benchmark-table.html")}}" --config "{{table_config}}"

# Render the standalone comparison table from existing JSON.
bench-evidence-table results table_config="scripts/perf/table.config.example.json":
    cargo run -p revelo-bench -- render-table --results "{{results}}" --output "{{replace(results, "results.json", "benchmark-table.html")}}" --config "{{table_config}}"

# Capture the standalone comparison table as PNG when Chrome/Chromium is available.
bench-compare-png results table_config="scripts/perf/table.config.example.json":
    cargo run -p revelo-bench -- render-table --results "{{results}}" --output "{{replace(results, "results.json", "benchmark-table.html")}}" --config "{{table_config}}" --render-png

# Check that the WASM crate still builds for the browser target.
bench-wasm:
    RUSTC="$(rustup which rustc)" cargo check -p revelo-wasm --target wasm32-unknown-unknown

# Run benchmark compare and table rendering with one shared run id.
bench-evidence manifest table_config="scripts/perf/table.config.example.json":
    cargo run -p revelo-bench -- evidence --manifest "{{manifest}}" --table-config "{{table_config}}"

# Build the WASM package (requires: wasm-pack, rustup target add wasm32-unknown-unknown)
wasm:
    wasm-pack build crates/revelo-wasm --release

# Build WASM for web target (output in pkg/)
wasm-web:
    wasm-pack build crates/revelo-wasm --release --target web

# Publish the WASM package to npm (requires: npm login first)
wasm-publish:
    wasm-pack publish crates/revelo-wasm

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

# Release a version: bump → commit → tag → push → CI builds binaries → cargo publish
# CI (`.github/workflows/release.yml`) handles GitHub Release + binary builds.
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
