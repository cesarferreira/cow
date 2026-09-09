.PHONY: all build build-release install install-release clean test check fmt lint run demo release

LEVEL ?= minor

# Default target
all: check build test

# Build debug version
build:
	cargo build

# Build release version
build-release:
	cargo build --release

# Install debug binary to ~/.cargo/bin
install:
	CARGO_INCREMENTAL=0 cargo install --path . --locked --bins --debug --force

# Install release binary to ~/.cargo/bin
install-release:
	CARGO_INCREMENTAL=0 cargo install --path . --locked --bins --force

# Clean build artifacts
clean:
	cargo clean

# Run tests
test:
	cargo test

# Run clippy and check
check:
	cargo check
	cargo clippy -- -D warnings

# Format code
fmt:
	cargo fmt

# Lint (check formatting)
lint:
	cargo fmt -- --check
	cargo clippy -- -D warnings

# Run with arguments (usage: make run ARGS="--hello")
run:
	cargo run -- $(ARGS)

# Quick demo
demo: install
	@echo "=== cow demo ==="
	cow --help

# Bump version, regenerate CHANGELOG.md, tag, publish, and push (requires cargo-release + git-cliff)
release:
	cargo release $(LEVEL) --execute --no-confirm
