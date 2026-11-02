# Solver Trainer

A local 6-max no-limit hold'em trainer: play unlimited hands against solver-driven bots with
distinct personalities; every decision you make is graded against GTO, explained, saved, and
rolled up into a leak report.

## Run it

```bash
./start.sh
```

Builds what is missing (Rust workspace, React UI) and opens http://127.0.0.1:8787.
Explanations need a Claude API key: `cp .env.example .env` and set `ANTHROPIC_API_KEY`.

Overnight, fill the solve cache so common flops are instant:

```bash
./overnight.sh 8
```

## What is where

| Area | Path |
|---|---|
| Rules, hand evaluator, equity, ranges, stats, hand histories | `crates/engine` |
| Preflop solver and charts | `crates/strategy/src/preflop`, `data/preflop` |
| postflop-solver integration (specs, cache, per-hand solves, node locking) | `crates/strategy/src/postflop` |
| Bots, personalities, the Hunter | `crates/strategy/src/bots` |
| Coach fact sheets and grades | `crates/strategy/src/coach`, `crates/server/src/coach.rs` |
| Server (axum, WebSocket table, SQLite logbook, Claude, simulator) | `crates/server` |
| UI (React + Vite) | `ui/src` |
| Vendored solver and its patches | `vendor/postflop-solver`, `vendor/PATCHES.md` |

Docs: [solver measurements and trees](docs/SOLVER.md) · [where the preflop charts come
from](docs/PREFLOP.md).

## How it works

- **Table**: exact NLHE rules: blinds and button movement (heads-up included), minimum raises with
  incomplete all-ins that don't reopen the betting (cumulative rule), uncalled bets, side pots,
  split pots, odd chips clockwise from the button, rake (default 5% capped at 3bb, no flop no
  drop), and showdown order with mucking.
- **Evaluator**: bitmask evaluator, verified against a brute-force reference on all
  133,784,560 seven-card hands, plus the published category counts.
- **Preflop**: charts from the repo's own preflop CFR solver, calibrated against postflop-solver.
- **Postflop, heads-up**: the flop is solved on the actual board from both chart ranges; turn and
  river are re-solved on the actual runout from the reach-weighted ranges. Results are cached by
  suit-canonical spot. River re-solves include the bunching effect when at most 3 players folded.
- **Postflop, multiway**: bots play from equity against estimated ranges, pot odds and their
  profile; the coach labels these "unsolved, no grade".
- **Bots** sample from the strategy for their own hand, then bend it with explicit parameters
  (range width, limping, aggression, fold to pressure, bluffing, sizing, stickiness). Profiles:
  GTO reg, nit, calling station, maniac, loose-passive fish, LAG, and the Hunter, which
  node-locks your observed tendencies and plays the best response.
- **Coach**: grades by solver EV loss (postflop) or chart frequency (preflop). It shows your
  hand's solver mix and the EV of each action, an exploit view against non-GTO bots, a fact sheet
  (equity, pot odds, SPR, range and nut advantage, blockers, MDF, bluff share), a 2-5 sentence
  explanation written by Claude from those facts only, and an "ask why" chat.
- **Logbook**: SQLite, every hand with full action, cards, solver results and grades, notes and
  tags; replayer with solver grids at each node; PokerStars export; stats; leak report with drills.
- **Lab**: equity calculator, range viewer, spot drills, bot-vs-bot simulator (about 10,000
  hands/s), variance simulator, and a Learn section with ten concept pages.

## Keyboard (table)

`F` fold · `C`/`Space` check/call · `1`-`5` bet sizes · `R` custom size, `Enter` to confirm ·
`A` all-in · `N` next hand · `P` pause.

## Tests

```bash
cargo test --release                                     # unit tests (rules edge cases, eval, ranges, equity, HH, stats)
cargo test --release -p engine -- --ignored exhaustive   # all 133,784,560 seven-card hands (~2 min)
```

## License

The vendored postflop-solver is AGPL-3.0-or-later; this project is for personal local use.
