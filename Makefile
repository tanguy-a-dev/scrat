.PHONY: install dev build test fmt fmt-check lint check clean reset-db

HELP_TARGET_COLUMN_WIDTH = 40

# Tauri's app_data_dir for this app's identifier (src-tauri/tauri.conf.json).
APP_IDENTIFIER = com.scrat.app

help:
	@grep -E '^[a-zA-Z_/-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-$(HELP_TARGET_COLUMN_WIDTH)s\033[0m %s\n", $$1, $$2}'

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

reset-db: ## Permanently delete the local encrypted database (all data is lost)
	@case "$$(uname)" in \
		Darwin) db_path="$$HOME/Library/Application Support/$(APP_IDENTIFIER)/scrat.db" ;; \
		Linux) db_path="$${XDG_DATA_HOME:-$$HOME/.local/share}/$(APP_IDENTIFIER)/scrat.db" ;; \
		*) echo "reset-db doesn't know the app data path for $$(uname)"; exit 1 ;; \
	esac; \
	if [ ! -f "$$db_path" ]; then \
		echo "No database found at $$db_path"; \
	else \
		printf "This will permanently delete %s and all its data. Continue? [y/N] " "$$db_path"; \
		read confirm; \
		case "$$confirm" in \
			y|Y) rm -f "$$db_path"; echo "Deleted $$db_path" ;; \
			*) echo "Aborted." ;; \
		esac; \
	fi
