# Run `just` with no arguments to list every recipe below.
default:
    @just --list

# --- Setup -------------------------------------------------------------

# Install frontend dependencies (Rust deps are fetched on first cargo/tauri run)
install:
    bun install

# Run the app in development mode (Rust + Vite dev server, hot-reloading)
dev:
    bun run tauri dev

# --- Frontend checks -----------------------------------------------------

# Type-check the frontend (tsc -b --noEmit)
typecheck:
    bun run typecheck

# Lint the frontend (eslint, type-aware)
lint:
    bun run lint

# Auto-fix whatever eslint can fix on its own
lint-fix:
    bun run lint:fix

# Check frontend formatting without changing anything
format-check:
    bun run format:check

# Reformat the frontend in place (prettier --write)
format:
    bun run format

# Run frontend unit tests (vitest)
test:
    bun run test

# --- Rust checks ---------------------------------------------------------

# Check Rust formatting without changing anything
fmt-check:
    cd src-tauri && cargo fmt --check

# Reformat Rust in place
fmt:
    cd src-tauri && cargo fmt

# Lint Rust (clippy, pedantic, warnings as errors)
clippy:
    cd src-tauri && cargo clippy --all-targets -- -D warnings

# Run Rust tests that don't need real ffmpeg/ffprobe on PATH (the default set)
test-rust:
    cd src-tauri && cargo test

# Run the real-ffmpeg-gated Rust tests too (needs ffmpeg/ffprobe installed)
test-rust-real:
    cd src-tauri && cargo test -- --ignored

# --- Aggregate ------------------------------------------------------------

# Everything CI runs, in one shot — the fast local gate before pushing
check: typecheck lint format-check test fmt-check clippy test-rust

# --- Build -----------------------------------------------------------------

# Production frontend build only (no Tauri bundling)
build-frontend:
    bun run build

# Full release build: Linux installers (.deb/.rpm/.AppImage) — see notes/plan.md M14 for why Windows/macOS aren't targeted here
build:
    bun run tauri build

# Release binary only, no installer packaging (faster iteration)
build-bin:
    bun run tauri build --no-bundle

# --- Cleanup -----------------------------------------------------------------

# Remove all build output (frontend dist + Rust target)
clean:
    rm -rf dist src-tauri/target
