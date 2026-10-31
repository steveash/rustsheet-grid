.DEFAULT_GOAL := help
SHELL := bash

help: ## List targets
	@grep -E '^[a-z-]+:.*## ' $(MAKEFILE_LIST) | awk -F':.*## ' '{printf "  %-14s %s\n", $$1, $$2}'

# ---------------------------------------------------------------- Rust (server, display)
check-server: ## Rust: fmt, clippy -D warnings, tests
	cargo fmt --all --check
	cargo clippy --workspace --all-targets -- -D warnings
	cargo test --workspace

check-wasm: ## Build the display crate for the browser (wasm32)
	cargo build -p grid-display-wasm --target wasm32-unknown-unknown --release

# ---------------------------------------------------------------- TypeScript (web, gym)
node_modules: package.json pnpm-lock.yaml
	pnpm install --frozen-lockfile
	@touch node_modules

check-web: node_modules ## Grid packages: typecheck + tests
	pnpm run check:web

check-gym: node_modules ## Gym server and bridge add-in: typecheck + tests
	pnpm run check:gym

# ---------------------------------------------------------------- All
check: check-server check-wasm check-web check-gym ## Every area

.PHONY: help check check-server check-wasm check-web check-gym
