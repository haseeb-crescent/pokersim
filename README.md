# Solver Trainer

A local 6-max no-limit hold'em trainer: play unlimited hands against solver-driven bots with
distinct personalities; every decision you make is graded against GTO, explained, saved, and
rolled up into a leak report.

## Run it

```bash
./start.sh
```

Builds what is missing (Rust workspace, React UI) and opens http://127.0.0.1:8787.
For plain-English explanations, see [Explanations](#explanations) below.

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
- **Off-chart hands**: the action that leaves the chart is still graded ("limp: 0% in the chart,
  the chart says raise 16% / fold 84%"). Later streets have no solver ranges, so they show why
  there is no grade plus pot odds and equity against estimated ranges, which still teach the
  price-versus-equity decision. Multiway pots are handled the same way.
- **Logbook**: SQLite, every hand with full action, cards, solver results and grades, notes and
  tags; replayer with solver grids at each node; PokerStars export; stats; leak report with drills.
- **Lab**: equity calculator, range viewer, spot drills, bot-vs-bot simulator (about 10,000
  hands/s), variance simulator, and a Learn section with ten concept pages.

## Explanations

The coach's prose (2-5 sentences per decision, and the "ask why" chat) is written by Claude
from the fact sheet only; every number comes from the solver and the equity engine. The server
picks a backend at startup and prints which one it uses:

1. **Your Claude subscription, through the `claude` CLI** (preferred). If
   [Claude Code](https://code.claude.com) is installed and logged in (`claude auth status`), each
   explanation runs `claude -p --model sonnet --output-format json` and reads the `result`
   field. It does not use `--bare` (bare mode ignores the subscription login and needs an API
   key). Instead the call is isolated so it can only turn the fact sheet into text:
   - it runs from an empty temporary directory, with only `HOME`, `USER`, `PATH`, locale and
     temp-dir variables in its environment (no `ANTHROPIC_*` variables, so it never bills an
     API key),
   - `--safe-mode` turns off CLAUDE.md, hooks, plugins, skills, custom commands and agents and
     MCP servers; `--tools ""`, `--strict-mcp-config` with an empty MCP config,
     `--disable-slash-commands` and `--permission-prompts none` back that up,
   - `--system-prompt` replaces Claude Code's system prompt with the coach's own.

   "Ask why" follow-ups continue the same conversation with `--resume <session_id>` from the
   first reply. Each call takes a few seconds to start, so the coach panel shows "thinking…";
   at most two run at once. Claude Code still attaches your account email as context to every
   conversation; nothing else from your setup is visible to it.
2. **An API key.** If `claude` is not installed or not logged in, put a key in `.env`
   (`cp .env.example .env`, set `ANTHROPIC_API_KEY`); the server then calls the Messages API
   (`claude-sonnet-5-5`) directly. If a CLI call fails at runtime, the server switches to the
   API key when one is set.
3. **Neither.** The coach shows the fact sheets (pot odds, equity, SPR, range and nut advantage,
   blockers, MDF, solver mix) without prose.

Set `POKERSIM_NO_CLI=1` to skip the CLI, or `CLAUDE_BIN=/path/to/claude` to point at a specific
binary.

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
