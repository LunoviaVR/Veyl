#!/usr/bin/env bash
# The gate every unit must pass: format, lint, Rust tests, JS tests.
set -euo pipefail
cd "$(dirname "$0")/src-tauri"
cargo fmt --check
cargo clippy --all-targets --quiet -- -D warnings
cargo test --quiet
cd ..
if compgen -G "ui/*.test.js" > /dev/null; then node --test ui/*.test.js; fi
