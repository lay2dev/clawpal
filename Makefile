.PHONY: help doctor install dev dev-frontend \
        test-frontend test-rust test-unit test-coverage \
        typecheck lint-frontend lint-rust-fmt lint-rust-clippy lint-rust lint fmt \
        build-frontend build build-release \
        artifacts ci clean

help: ## Show available commands
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-18s\033[0m %s\n", $$1, $$2}'

doctor: ## Check development environment prerequisites
	@echo "🔍 Checking prerequisites..."
	@command -v rustc >/dev/null 2>&1 && echo "✅ Rust $$(rustc --version | cut -d' ' -f2)" || echo "❌ Rust not found"
	@command -v bun >/dev/null 2>&1 && echo "✅ Bun $$(bun --version)" || echo "❌ Bun not found"
	@command -v cargo >/dev/null 2>&1 && echo "✅ Cargo $$(cargo --version | cut -d' ' -f2)" || echo "❌ Cargo not found"
	@echo "🔍 Checking Tauri system dependencies..."
	@pkg-config --exists webkit2gtk-4.1 2>/dev/null && echo "✅ webkit2gtk-4.1" || echo "⚠️  webkit2gtk-4.1 not found (Linux only)"
	@echo "---"
	@echo "If prerequisites are missing, see: https://v2.tauri.app/start/prerequisites/"

install: ## Install all dependencies
	bun install
	@echo "✅ Frontend dependencies installed"

dev: ## Start development mode (frontend + Tauri)
	bun run dev:tauri

dev-frontend: ## Start frontend only (no Tauri)
	bun run dev

test-frontend: ## Run frontend unit tests
	bun test

test-rust: ## Run Rust unit tests
	cargo test --workspace

test-unit: test-frontend test-rust ## Run all unit tests (frontend + Rust)

test-coverage: ## Run Rust tests with coverage
	cargo llvm-cov --workspace --lcov --output-path lcov.info
	@echo "✅ Coverage report: lcov.info"

typecheck: ## TypeScript type check
	bun run typecheck

lint-frontend: typecheck ## Frontend lint (type check)

lint-rust-fmt: ## Rust format check
	cargo fmt --check

lint-rust-clippy: ## Rust clippy
	cargo clippy --workspace --all-targets -- -D warnings

lint-rust: lint-rust-fmt lint-rust-clippy ## Rust lint (fmt + clippy)

lint: lint-frontend lint-rust ## Run all lints (frontend + Rust)

fmt: ## Auto-fix Rust formatting
	cargo fmt --all
	@echo "✅ Rust formatted"

build-frontend: ## Build frontend
	bun run build

build: ## Build Tauri application (debug)
	bun run build:tauri

build-release: ## Build Tauri application (release)
	bun run build:tauri -- --release

artifacts: ## Collect artifacts into harness/artifacts/
	@mkdir -p harness/artifacts
	@echo "📦 Collecting artifacts..."
	@cp -r lcov.info harness/artifacts/ 2>/dev/null || true
	@echo "✅ Artifacts collected in harness/artifacts/"

ci: lint test-unit build-frontend ## Run full CI check locally
	@echo "✅ All CI checks passed locally"

clean: ## Clean build artifacts
	cargo clean
	rm -rf node_modules dist
	@echo "✅ Cleaned"
