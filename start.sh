#!/usr/bin/env bash
# One command: builds what is missing and opens the trainer at http://127.0.0.1:8787
set -euo pipefail
cd "$(dirname "$0")"
if ! command -v cargo >/dev/null 2>&1; then
  if [ -f "$HOME/.cargo/env" ]; then . "$HOME/.cargo/env"; else
    echo "Rust is not installed: https://rustup.rs" >&2; exit 1
  fi
fi
if [ ! -f data/preflop/charts.json ] || [ ! -f data/preflop/equity169.bin ]; then
  echo "Preflop data missing; regenerating (about an hour, see docs/PREFLOP.md)…"
  cargo run --release -p strategy --bin gen_equity_table
  cargo run --release -p strategy --bin build_charts -- 2 10 3000
fi
if [ ! -d ui/node_modules ]; then (cd ui && npm install --no-audit --no-fund); fi
if [ ! -f ui/dist/index.html ] || [ -n "$(find ui/src ui/index.html -newer ui/dist/index.html 2>/dev/null | head -1)" ]; then
  (cd ui && npm run build)
fi
cargo build --release -p server
exec ./target/release/server "$@"
