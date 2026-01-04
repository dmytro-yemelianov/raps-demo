# Makefile for RAPS Demo Workflows

.PHONY: build test check fmt clippy clean run help

# Default target
help:
	@echo "Available targets:"
	@echo "  build    - Build the project"
	@echo "  test     - Run all tests"
	@echo "  check    - Check code without building"
	@echo "  fmt      - Format code"
	@echo "  clippy   - Run clippy linter"
	@echo "  clean    - Clean build artifacts"
	@echo "  run      - Run the application"
	@echo "  help     - Show this help message"

# Build the project
build:
	cargo build

# Build for release
release:
	cargo build --release

# Run tests
test:
	cargo test

# Check code without building
check:
	cargo check

# Format code
fmt:
	cargo fmt

# Run clippy linter
clippy:
	cargo clippy -- -D warnings

# Clean build artifacts
clean:
	cargo clean

# Run the application
run:
	cargo run

# Run with verbose logging
run-verbose:
	cargo run -- --verbose

# Run in non-interactive mode
run-no-tui:
	cargo run -- --no-tui

# Development workflow: format, check, test
dev: fmt check test

# CI workflow: format check, clippy, test
ci: fmt clippy test