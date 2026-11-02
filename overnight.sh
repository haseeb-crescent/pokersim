#!/usr/bin/env bash
# Unattended overnight run: optionally refines the preflop charts, then fills the solve
# cache with the most common heads-up flop spots until the time runs out.
#   ./overnight.sh            pre-solve for 8 hours
#   ./overnight.sh 10 refine  one more chart calibration round (16 flops/spot) first, then pre-solve
# Refining changes the chart ranges, which changes cache keys, so refine before pre-solving.
set -euo pipefail
cd "$(dirname "$0")"
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
HOURS="${1:-8}"
cargo build --release -p strategy --bin presolve --bin build_charts
if [ "${2:-}" = "refine" ]; then
  cp data/preflop/charts.json "data/preflop/charts.$(date +%Y%m%d-%H%M).json.bak"
  ./target/release/build_charts 1 16 3000 --resume
  HOURS=$(python3 -c "print(max(1, $HOURS - 1))")
fi
./target/release/presolve --hours "$HOURS" | tee -a data/cache/presolve.log
