.PHONY: install dev build test fmt fmt-check lint check clean

install: ## Install root (Tauri CLI) and frontend npm dependencies
	npm install
	npm --prefix frontend install

dev: ## Run the desktop app in development mode
	npm run tauri -- dev

build: ## Build the desktop app for release
	npm run tauri -- build

test: ## Run the Rust workspace test suite
	cargo test --workspace

fmt: ## Format Rust code
	cargo fmt --all

fmt-check: ## Check Rust formatting without modifying files
	cargo fmt --all -- --check

lint: ## Run clippy with warnings denied
	cargo clippy --workspace --all-targets -- -D warnings

check: fmt-check lint test ## Run the same checks as CI

clean: ## Remove Rust and frontend build artifacts
	cargo clean
	rm -rf frontend/build frontend/.svelte-kit
