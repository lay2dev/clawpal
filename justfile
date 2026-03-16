# ClawPal Development Commands
# Usage: just <command>

# Default: show available commands
default:
    @just --list

# Check development environment prerequisites
doctor:
    @echo "🔍 Checking prerequisites..."
    @command -v rustc >/dev/null 2>&1 && echo "✅ Rust $(rustc --version | cut -d' ' -f2)" || echo "❌ Rust not found"
    @command -v bun >/dev/null 2>&1 && echo "✅ Bun $(bun --version)" || echo "❌ Bun not found"
    @command -v cargo >/dev/null 2>&1 && echo "✅ Cargo $(cargo --version | cut -d' ' -f2)" || echo "❌ Cargo not found"
    @echo "🔍 Checking Tauri system dependencies..."
    @pkg-config --exists webkit2gtk-4.1 2>/dev/null && echo "✅ webkit2gtk-4.1" || echo "⚠️  webkit2gtk-4.1 not found (Linux only)"
    @echo "---"
    @echo "If prerequisites are missing, see: https://v2.tauri.app/start/prerequisites/"

# Install all dependencies
install:
    bun install
    @echo "✅ Frontend dependencies installed"

# Start development mode (frontend + Tauri)
dev:
    bun run dev:tauri

# Start frontend only (no Tauri)
dev-frontend:
    bun run dev

# Run frontend unit tests
test-frontend:
    bun test

# Run Rust unit tests
test-rust:
    cargo test --workspace

# Run all unit tests (frontend + Rust)
test-unit: test-frontend test-rust

# Run Rust tests with coverage
test-coverage:
    cargo llvm-cov --workspace --lcov --output-path lcov.info
    @echo "✅ Coverage report: lcov.info"

# TypeScript type check
typecheck:
    bun run typecheck

# Frontend lint (type check)
lint-frontend: typecheck

# Rust format check
lint-rust-fmt:
    cargo fmt --check

# Rust clippy
lint-rust-clippy:
    cargo clippy --workspace --all-targets -- -D warnings

# Rust lint (fmt + clippy)
lint-rust: lint-rust-fmt lint-rust-clippy

# Run all lints (frontend + Rust)
lint: lint-frontend lint-rust

# Auto-fix Rust formatting
fmt:
    cargo fmt --all
    @echo "✅ Rust formatted"

# Build frontend
build-frontend:
    bun run build

# Build Tauri application (debug)
build:
    bun run build:tauri

# Build Tauri application (release)
build-release:
    bun run build:tauri -- --release

# Collect artifacts (logs, screenshots, traces) into harness/artifacts/
artifacts:
    @mkdir -p harness/artifacts
    @echo "📦 Collecting artifacts..."
    @cp -r lcov.info harness/artifacts/ 2>/dev/null || true
    @echo "✅ Artifacts collected in harness/artifacts/"

# Run full CI check locally (what CI runs)
ci: lint test-unit build-frontend
    @echo "✅ All CI checks passed locally"

# Clean build artifacts
clean:
    cargo clean
    rm -rf node_modules dist
    @echo "✅ Cleaned"
