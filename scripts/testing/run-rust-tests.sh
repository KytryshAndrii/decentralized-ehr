#!/usr/bin/env bash
set -euo pipefail

echo "[1/4] Formatting..."
cargo fmt --all -- --check

echo "[2/4] Compilation..."
cargo check --workspace --all-targets

echo "[3/4] Clippy..."
cargo clippy --workspace --all-targets -- -D warnings

echo "[4/4] Tests..."
cargo test --workspace

echo "PASS: Rust checks"
