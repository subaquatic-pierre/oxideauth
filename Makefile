.PHONY: help run build dev test test-all clean deploy

.DEFAULT_GOAL := help

help: ## Show this help message
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) \
		| sort \
		| awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-12s\033[0m %s\n", $$1, $$2}'

run: ## Run the API server
	cargo run

build: ## Build release binary
	cargo build --release
	cp ./target/release/oxideauth .
	chmod +x ./oxideauth

dev: ## Run with auto-reload via cargo watch
	cargo watch -x "run --bin oxideauth"

test: ## Run library tests
	cargo test --lib

# All tests include integration tests under test/ directory
test-all: ## Run all tests (single-threaded)
	cargo test -- --test-threads=1

clean: ## Remove build artifacts
	cargo clean

deploy: ## Deploy API via version bump (major|minor|patch) and tag push (build-only, no deploy target)
	bash scripts/deploy.sh --yes $(filter-out $@,$(MAKECMDGOALS))

# ── passthrough for deploy args ────────────────────────────────────────
%:
	@:
