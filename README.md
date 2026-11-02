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

Preflop charts for stacks other than 100bb (Session mode), a separate command that takes about
three minutes and is not part of the overnight run:

```bash
cargo run --release -p strategy --bin build_depth_charts
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
- **Off-chart hands**: when your own action leaves the chart, it is still graded against the
  chart ("limp: 0% in the chart, the chart says raise 16% / fold 84%").
  - When a bot takes the hand off the chart (a limp, an odd raise size), your next decision is
    never matched to a chart node it doesn't belong to. It gets a grade labelled "off-chart:
    heuristic grade" instead.
  - That grade compares raise, call or check, and fold with a one-step EV model: equity against
    the bots' actual ranges (their profile's version of the chart), each bot's modelled answer to
    a raise, and sizing guidance (for example 4bb plus 1bb per limper out of position).
  - It is called a blunder only when the model finds an EV loss of 3bb or more.
  - Later streets have no solver ranges, so they show why there is no grade, plus pot odds and
    equity against estimated ranges. Multiway pots are handled the same way.
- **Logbook**: SQLite, every hand with full action, cards, solver results and grades, notes and
  tags; replayer with solver grids at each node; PokerStars export; stats; leak report with drills.
- **Lab**: equity calculator, range viewer, spot drills, bot-vs-bot simulator (about 10,000
  hands/s), variance simulator, and a Learn section with ten concept pages.

## Training and Session modes

Pick the mode on the setup screen.

- **Training**: every stack resets to 100bb each hand, so every spot matches the 100bb charts and
  the solver library exactly.
- **Session**: a cash game.
  - Stacks carry over from hand to hand and across app restarts. They are saved in SQLite after
    every hand; leaving mid-hand drops that hand and keeps the stacks from before it.
  - You buy in for 100bb, rebuy for 100bb when you bust, and can top up to 100bb between hands.
    A top-up clicked during a hand is applied before the next one.
  - The table shows your buy-ins, session and lifetime profit in bb and dollars (set the big
    blind's dollar value at setup), and a results graph with the all-in-adjusted (EV) line.
    Stats has the lifetime view across sessions.
  - Bots buy in for 100bb and rebuy when busted. The GTO reg, the LAG reg and the Hunter (which
    starts as a GTO reg) top up to 100bb before a hand when below it. The fish, station, maniac
    and nit let their stacks ride.
  - Hands deal automatically after a pause you set at setup or in the toolbar. `P` pauses and `N`
    deals now.
  - **Leave** keeps the session open: the setup screen offers to resume it with its lineup,
    stacks and stakes. **Cash out** closes it. Starting a new Session cashes out the open one.

Grading is the same in both modes. In Session mode, stack depth is handled like this:

- **Postflop**: within 10% of 100bb effective, the 100bb solver library and cache are used.
  Otherwise the flop is solved live at a depth bucket and cached under that bucket, so later hands
  at a similar depth are instant. Buckets are 5bb apart below 50bb, 10bb apart up to 100bb and 20bb
  apart above, so a bucket is never more than about 12% from the real stack. Deep buckets solve
  more slowly than 100bb (more raises fit before all-in).
- **Preflop**: charts at 40, 60, 150 and 200bb sit next to the 100bb ones. Each player uses the
  chart nearest its effective stack, and the coach says when a grade used a non-100bb chart.
- **Very short stacks** (under 20bb effective) have no chart or solver spot: preflop decisions get
  the labelled heuristic grade, and postflop decisions get the unsolved view. Multiway pots stay
  unsolved, as in Training.

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
