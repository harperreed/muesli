# ABOUTME: Build automation for muesli CLI
# ABOUTME: Provides targets for building, testing, and installing with various feature combinations

.PHONY: all build build-all test test-all install clean help

# Default target
all: build

# Build with default features only (core sync functionality)
build:
	cargo build --release

# Build with all features enabled (index, summaries, embeddings)
build-all:
	cargo build --release --all-features

# Build with specific feature combinations
build-index:
	cargo build --release --features index

build-summaries:
	cargo build --release --features summaries

build-embeddings:
	cargo build --release --features embeddings

# Run tests with default features
test:
	cargo test --lib

# Run all tests with all features
test-all:
	cargo test --lib --all-features --no-fail-fast

# Install to cargo bin directory
install:
	cargo install --path . --all-features

# Install with only specific features
install-core:
	cargo install --path .

install-index:
	cargo install --path . --features index

# Clean build artifacts
clean:
	cargo clean

# Development targets
check:
	cargo check --all-features

fmt:
	cargo fmt

lint:
	cargo clippy --all-features -- -D warnings

# Help target
help:
	@echo "Muesli Build Targets:"
	@echo "  make build          - Build release binary (core features only)"
	@echo "  make build-all      - Build release binary with all features"
	@echo "  make build-index    - Build with text search (index feature)"
	@echo "  make build-summaries - Build with AI summaries (summaries feature)"
	@echo "  make build-embeddings - Build with embeddings (embeddings feature)"
	@echo ""
	@echo "  make test           - Run tests (default features)"
	@echo "  make test-all       - Run all tests (all features)"
	@echo ""
	@echo "  make install        - Install to ~/.cargo/bin (all features)"
	@echo "  make install-core   - Install core version only"
	@echo "  make install-index  - Install with search feature"
	@echo ""
	@echo "  make check          - Check compilation"
	@echo "  make fmt            - Format code"
	@echo "  make lint           - Run clippy linter"
	@echo "  make clean          - Remove build artifacts"
	@echo ""
	@echo "Features:"
	@echo "  index       - Full-text search with Tantivy"
	@echo "  summaries   - AI summaries via OpenAI (requires API key)"
	@echo "  embeddings  - Local embeddings with ONNX Runtime"
