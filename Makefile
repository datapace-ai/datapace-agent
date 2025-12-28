.PHONY: build run test lint fmt clean docker docker-run help

# Variables
BINARY_NAME := datapace-agent
VERSION := $(shell git describe --tags --always --dirty 2>/dev/null || echo "dev")
DOCKER_IMAGE := ghcr.io/datapace-ai/datapace-agent

# Default target
.DEFAULT_GOAL := help

## Build Commands

build: ## Build release binary
	cargo build --release

build-debug: ## Build debug binary
	cargo build

run: ## Run the agent (requires DATABASE_URL and DATAPACE_API_KEY)
	cargo run --release

run-debug: ## Run with debug logging
	RUST_LOG=debug cargo run

watch: ## Run in watch mode (requires cargo-watch)
	cargo watch -x run

## Test Commands

test: ## Run all tests
	cargo test

test-verbose: ## Run tests with verbose output
	cargo test -- --nocapture

test-integration: ## Run integration tests (requires test database)
	cargo test --features integration -- --ignored

coverage: ## Generate test coverage report (requires cargo-tarpaulin)
	cargo tarpaulin --out Html

## Code Quality

lint: ## Run clippy linter
	cargo clippy -- -D warnings

lint-fix: ## Run clippy and apply suggestions
	cargo clippy --fix --allow-dirty

fmt: ## Format code
	cargo fmt

fmt-check: ## Check code formatting
	cargo fmt -- --check

audit: ## Audit dependencies for security vulnerabilities
	cargo audit

check: fmt-check lint test ## Run all checks (format, lint, test)

## Docker Commands

docker: ## Build Docker image
	docker build -t $(DOCKER_IMAGE):$(VERSION) -t $(DOCKER_IMAGE):latest .

docker-run: ## Run Docker container (requires env vars)
	docker run --rm \
		-e DATAPACE_API_KEY \
		-e DATABASE_URL \
		$(DOCKER_IMAGE):latest

docker-push: ## Push Docker image to registry
	docker push $(DOCKER_IMAGE):$(VERSION)
	docker push $(DOCKER_IMAGE):latest

## Release Commands

release-linux: ## Build for Linux (x86_64)
	cargo build --release --target x86_64-unknown-linux-musl

release-macos: ## Build for macOS (x86_64)
	cargo build --release --target x86_64-apple-darwin

release-macos-arm: ## Build for macOS (ARM64)
	cargo build --release --target aarch64-apple-darwin

release-windows: ## Build for Windows
	cargo build --release --target x86_64-pc-windows-msvc

## Utility Commands

clean: ## Clean build artifacts
	cargo clean
	rm -rf dist/

deps: ## Install development dependencies
	cargo install cargo-watch cargo-audit cargo-tarpaulin

update: ## Update dependencies
	cargo update

docs: ## Generate and open documentation
	cargo doc --open

## Help

help: ## Show this help message
	@echo "Datapace Agent - Development Commands"
	@echo ""
	@echo "Usage: make [target]"
	@echo ""
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-20s\033[0m %s\n", $$1, $$2}'
