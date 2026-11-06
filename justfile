# Dev recipes: https://github.com/casey/just

# On Windows, run recipes with PowerShell instead of sh (no Git Bash needed).
set windows-powershell := true

default: check

# Format-check + clippy + tests
check: lint test

# Formatting and lints
lint:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings

# Apply fmt and safe clippy fixes
fix:
    cargo fmt --all
    cargo clippy --workspace --all-targets --fix --allow-dirty

# Run all tests
test:
    cargo test --workspace

# Run the API server
run:
    cargo run -p arsy-server

# Debug build of the whole workspace
build:
    cargo build --workspace

# Release build (locked to Cargo.lock)
release:
    cargo build --workspace --release --locked

# Dependency audit (requires: cargo install cargo-deny)
deny:
    cargo deny check
