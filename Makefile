.PHONY: install dev build test fmt fmt-check lint check clean kill reset-db

HELP_TARGET_COLUMN_WIDTH = 40

# Tauri's app_data_dir for this app's identifier (src-tauri/tauri.conf.json).
APP_IDENTIFIER = com.scrat.app

# Vite dev server port (src-tauri/tauri.conf.json "devUrl").
DEV_PORT = 1420

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

# The `[x]yz` bracket trick keeps each pattern from matching the shell that runs
# this recipe — its own argv contains the pattern text verbatim, so a plain
# pattern would make the recipe kill itself.
kill: ## Stop running Scrat processes (app, tauri dev CLI, Vite dev server)
	@pids=$$( { \
		pgrep -f '$(CURDIR)/[t]arget/(debug|release)/scrat'; \
		pgrep -f '[s]crat\.app/Contents/MacOS/scrat'; \
		pgrep -f '$(CURDIR)/[n]ode_modules/(\.bin/tauri|@tauri-apps)'; \
		pgrep -f '$(CURDIR)/[f]rontend/node_modules/.*(vite|esbuild)'; \
		lsof -ti tcp:$(DEV_PORT) -sTCP:LISTEN; \
	} 2>/dev/null | sort -u); \
	if [ -z "$$pids" ]; then \
		echo "No Scrat processes running."; \
	else \
		echo "Stopping PIDs: $$(echo $$pids | tr '\n' ' ')"; \
		kill $$pids 2>/dev/null || true; \
		sleep 1; \
		stubborn=$$(for pid in $$pids; do kill -0 $$pid 2>/dev/null && echo $$pid; done); \
		if [ -n "$$stubborn" ]; then \
			echo "Force-killing: $$(echo $$stubborn | tr '\n' ' ')"; \
			kill -9 $$stubborn 2>/dev/null || true; \
		fi; \
		echo "Stopped."; \
	fi

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
