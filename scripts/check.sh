#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
cargo fmt --all --check
cargo clippy -p meshclip-core --all-targets --locked -- -D warnings
cargo test -p meshclip-core --locked
cd apps/desktop
npm ci
npm run build
