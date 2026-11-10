# Canonical verb set (engineering-standards: scripts canonical, CI mirrors).
.PHONY: build test lint fmt fmt-check doc coverage contract ci clean

build:
	cargo build --workspace

test:
	cargo nextest run --workspace 2>/dev/null || cargo test --workspace

lint:
	cargo clippy --workspace --all-targets -- -D warnings

fmt:
	cargo +nightly fmt --all

fmt-check:
	cargo +nightly fmt --all --check

doc:
	cargo doc --workspace --no-deps

coverage:
	cargo llvm-cov --workspace --summary-only 2>/dev/null || echo "llvm-cov not installed (tier A gate lands with phase 1 code)"

contract:
	cargo deny check 2>/dev/null || echo "cargo-deny not installed"

ci: fmt-check lint test doc

clean:
	cargo clean
