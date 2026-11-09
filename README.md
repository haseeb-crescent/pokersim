# pokersim

**A poker trainer that grades your decisions against a game-theory solver.**

You play unlimited hands of 6-max no-limit hold'em against bots with distinct playing styles.
After each decision, the coach shows what the solver's strategy does in that spot, what your
choice cost in big blinds, and why, in plain English. Every hand is saved and rolled up into
stats and a leak report. Everything runs on your own machine.

- **Its own preflop solver.** A 6-max CFR solver (counterfactual regret minimization, the
  algorithm behind modern poker AI) over all 169 starting-hand classes, calibrated against a
  postflop solver. A best response gains about 0.0001 big blinds per hand against it in its
  abstract game. The charts are computed here, not copied from a chart pack.
- **Live postflop solving.** Each flop is solved on the actual board from both players' ranges;
  the turn and river are re-solved on the real cards in about 1 s and 0.1 s.
- **A bot that hunts your leaks.** The Hunter starts as a solver bot and tracks how often you
  fold, call and bluff in each spot. Once it has enough samples, it locks your tendency into the
  solver and plays the exact counter-strategy. Every exploit it finds goes into your leak report.
- **Verified foundations.** The hand evaluator is checked against brute force on all
  133,784,560 seven-card hands. The table handles side pots, incomplete all-ins, odd chips and
  rake.
- **A lab.** Equity calculator, range viewer, spot drills, and a bot-vs-bot simulator that plays
  about 10,000 hands a second.

Rust for the engine, solvers and server; React for the UI.

![A hand being played and graded: the preflop call is graded against the chart and explained,
then the flop bet is graded against a postflop-solver solution with the EV of each action and
an exploit view against the opponent](docs/demo.gif)

*Above: a Training hand.*
1. Hero defends the big blind with T7o against the fish's open. The chart folds T7o, so the
   coach grades the call a blunder by chart frequency, and Claude explains why from the fact
   sheet.
2. Hero bets two pair on the flop. postflop-solver checks 100% here (check 11.32bb vs bet
   11.00bb), so the bet is a 0.32bb mistake.
3. The exploit view node-locks the fish's tendencies. Against this opponent, the bigger bet is
   the best response.

## Run it

```bash
./start.sh
```

It builds whatever is missing (the Rust workspace and the React UI), then serves
http://127.0.0.1:8787. It needs Rust (via [rustup](https://rustup.rs)) and Node.js; it has been
tested on macOS on an Apple M5 Max. Server flags:
- `--port N`
- `--no-open`
- `--db PATH`, a separate logbook for demos and experiments

Two optional long jobs fill in data. Run them separately and unattended:

```bash
./overnight.sh 8
```

```bash
cargo run --release -p strategy --bin build_depth_charts
```

- `overnight.sh` pre-solves the most common heads-up flops into the cache so they are instant
  at the table. It resumes where it stopped.
- `build_depth_charts` builds the preflop charts for 40, 60, 150 and 200bb stacks, which Session
  mode uses. It takes about 3 minutes and is not part of the overnight run.

## How it works

### The table
- Blinds and button movement, heads-up included.
- Minimum raises, with incomplete all-ins that don't reopen the betting.
- Uncalled bets, side pots, split pots, odd chips clockwise from the button.
- Rake: 5% capped at 3bb by default, no flop no drop.
- Showdown order with mucking.
- The hand evaluator is checked against a brute-force reference on all 133,784,560 seven-card
  hands.

### The solver
- **Postflop, heads-up:** [postflop-solver](https://github.com/b-inary/postflop-solver)
  (vendored and patched, Discounted CFR) solves the flop on the actual board from both players'
  chart ranges.
  - The turn and river are re-solved on the real cards from the reach-weighted ranges.
  - River re-solves include the bunching effect (card removal from preflop folders) when at
    most 3 players folded.
  - Trees, sizes and the measurements behind them: [docs/SOLVER.md](docs/SOLVER.md).
- **Cache:** finished flop solves are cached by suit-canonical spot. The key covers the board,
  the stack-depth bucket, a hash of both exact ranges, the pot, the stack behind, the tree and
  the rake. A solve is therefore never served for other charts or another depth.
- **Preflop:** the charts come from this repo's own 6-max preflop CFR solver over 169 hand
  classes. Its postflop values are calibrated against postflop-solver. They are not copied from
  any commercial chart pack. Details and known deviations: [docs/PREFLOP.md](docs/PREFLOP.md).

### The bots
Each bot samples from the strategy for its own hand, then bends it with explicit parameters:
range width, limping, aggression, fold to pressure, bluffing, bet sizing and postflop
stickiness. Their measured signatures (VPIP / PFR / 3-bet) are:

| Profile | Plays like | VPIP / PFR / 3-bet |
|---|---|---|
| GTO reg | the solver strategy, undistorted | 22 / 16 / 9 |
| Nit | narrow, folds to pressure, small value bets | 14 / 11 / 4 |
| Calling station | too many hands, almost never folds | 45 / 9 / 2.5 |
| Maniac | wide and very aggressive, big sizes, many bluffs | 55 / 42 / 18 |
| Loose-passive fish | limps and calls, folds to big bets late | 40 / 10 / 3 |
| LAG reg | wider than GTO, applies pressure | 30 / 25 / 11 |
| The Hunter | starts as a GTO reg, then exploits you | adapts |

**The Hunter** tracks your frequencies spot by spot: fold to c-bets, fold to big turn bets,
river bluffs, and so on.
- Once it has enough samples (15 preflop, 12 postflop), it node-locks your observed tendency
  into the solver and plays the best response.
- Every exploit it finds is logged, so the leak report can show you what it took advantage of.

### The coach
Every hero decision is graded, from Best through Correct, Inaccuracy, Mistake and Blunder.
- **Postflop (solved):** graded by the solver's EV loss relative to the pot.
  - The card shows your hand's solver mix, the EV of each action, and the range frequencies.
  - Against a non-GTO bot, an **exploit view** node-locks that bot's profile and shows the best
    response.
- **Preflop:** graded by chart frequency. The charts carry no EVs, and the grade says so.
- **Off-chart lines:** a bot can take the hand off the chart (a limp, an odd size). Your
  decision is then never matched to a chart node it doesn't belong to. It gets a grade labelled
  "off-chart: heuristic grade" instead.
  - That grade compares raise, call or check, and fold with a one-step EV model. The model uses
    equity against the bots' actual ranges and each bot's modelled answer to a raise.
  - It includes sizing guidance, and is called a blunder only at an EV loss of 3bb or more.
- **Multiway pots and very short stacks:** these have no solver solution. The coach says why,
  and shows pot odds and equity against estimated ranges.
- **Fact sheet:** equity, pot odds, SPR, range and nut advantage, blockers, MDF and bluff share.
- **Explanation:** 2-5 sentences from Claude, written only from the fact sheet, plus an "ask
  why" chat.

### Everything else
- **Logbook:** SQLite. It stores every hand with full action, cards, solver results, grades,
  notes and tags, plus a replayer with solver grids at each node, and PokerStars hand-history
  export.
- **Stats and leaks:** VPIP, PFR, 3-bet, c-bet, WTSD and more, by position, with bb/100 and an
  all-in-adjusted line. The leak report links to drills.
- **Lab:**
  - equity calculator, range viewer, preflop chart browser
  - spot drills
  - bot-vs-bot simulator and variance simulator
  - Learn section with ten concept pages, and a glossary

## Training and Session modes

Pick the mode on the setup screen.

- **Training:** every stack resets to 100bb each hand, so every spot matches the 100bb charts
  and the solver library exactly.
- **Session:** a cash game.
  - **Stacks:** they carry over from hand to hand and across app restarts. They are saved in
    SQLite after every hand. Leaving mid-hand drops that hand and keeps the stacks from before
    it.
  - **Buying in:** you buy in for 100bb, rebuy for 100bb when you bust, and can top up to 100bb
    between hands. A top-up clicked during a hand is applied before the next one.
  - **Results:** the table shows your buy-ins, session and lifetime profit in bb and dollars,
    and a results graph with the all-in-adjusted line. You set the big blind's dollar value at
    setup. Stats has the lifetime view across sessions.
  - **Bots:** they buy in for 100bb and rebuy when busted. The GTO reg, the LAG reg and the
    Hunter top up to 100bb before a hand when below it. The fish, station, maniac and nit let
    their stacks ride.
  - **Dealing:** hands deal automatically after a pause you set. `P` pauses, and `N` deals now.
  - **Leaving:** **Leave** keeps the session open, and the setup screen offers to resume it.
    **Cash out** closes it. Starting a new Session cashes out the open one.

Grading is the same in both modes. In Session mode, stack depth is handled like this:

- **Postflop:** within 10% of 100bb effective, the 100bb library and cache are used. Otherwise
  the flop is solved live at a depth bucket and cached under it. Buckets are 5bb apart below
  50bb, 10bb up to 100bb and 20bb above, so a bucket is never more than about 12% from the real
  stack.
- **Preflop:** each player uses the chart nearest its effective stack (40, 60, 100, 150 or
  200bb). The coach says when a grade used a non-100bb chart.
- **Under 20bb effective:** preflop decisions get the heuristic grade, and postflop decisions
  are unsolved.

## Explanations

The coach's prose (the 2-5 sentences per decision, and the "ask why" chat) is written by Claude
from the fact sheet only. Every number comes from the solver and the equity engine. At startup,
the server picks one of three backends and prints which:

1. **Your Claude Code login (preferred).** This applies when
   [Claude Code](https://code.claude.com) is installed and logged in (`claude auth status`).
   - Each explanation runs `claude -p --model sonnet --output-format json` and reads the
     `result` field.
   - It deliberately does not use `--bare`: bare mode ignores the subscription login and needs
     an API key.
   - Instead, the call is isolated so it can only turn the fact sheet into text:
     - It runs from an empty temporary directory. Its environment holds only `HOME`, `USER`,
       `PATH`, locale and temp-dir variables, with no `ANTHROPIC_*` variables, so it never
       bills an API key.
     - `--safe-mode` turns off CLAUDE.md, hooks, plugins, skills, custom commands, agents and
       MCP servers.
     - `--tools ""`, `--strict-mcp-config` with an empty MCP config, `--disable-slash-commands`
       and `--permission-prompts none` back that up.
     - `--system-prompt` replaces Claude Code's system prompt with the coach's own.
   - "Ask why" follow-ups continue the same conversation with `--resume <session_id>`.
   - Each call takes a few seconds to start, so the coach panel shows "thinking…". At most two
     calls run at once.
   - Claude Code still attaches your account email as context to every conversation. Nothing
     else from your setup is visible to it.
2. **`ANTHROPIC_API_KEY` fallback.** This applies when `claude` is missing or not logged in.
   - Put a key in `.env` (`cp .env.example .env`, then set `ANTHROPIC_API_KEY`). The server then
     calls the Messages API (`claude-sonnet-5-5`) directly.
   - If a CLI call fails at runtime, the server switches to the key when one is set.
3. **Neither.** The coach shows the fact sheets without prose.

Set `POKERSIM_NO_CLI=1` to skip the CLI, or `CLAUDE_BIN=/path/to/claude` to use a specific
binary.

## Benchmarks (Apple M5 Max, 18 cores, 64 GB)

| What | Measured |
|---|---|
| Seven-card evaluator, exhaustive check against brute force | all 133,784,560 hands in about 2 min |
| Exact preflop class equities (169 x 169, every board) | 76 s |
| Preflop CFR, 100bb abstract game (1,937 decisions, 3,000 iterations) | about 35 s; best-response gain about 0.0001bb/hand |
| Preflop calibration against postflop-solver (3 rounds) | about 1.5 h |
| Preflop charts at 40 / 60 / 150 / 200bb | 12 / 21 / 56 / 80 s |
| Single-raised flop solve, tree in use | 1.1-2.0 GB; 17-31 s to 1% of pot |
| Flop solve, thread scaling (K72r to 1%) | 27.8 s on 8 threads, 23.0 s on 12, 17.0 s on 18 |
| Uncached flop at the table, final target | 15-35 s for single-raised pots, 3-10 s for 3-bet pots |
| Uncached flop at a 160bb depth bucket | about 3 min (measured once) |
| Turn and river re-solves | about 1 s and under 0.1 s; a river with bunching 0.26 s |
| Bunching precompute (1 / 2 / 3 folders) | 0.1 / 0.3 / 3.2 s |
| Overnight pre-solve | about 100 single-raised or 340 3-bet flops per hour; roughly 1% of heads-up flop situations per hour |
| Bot-vs-bot simulator | about 10,000 hands/s |

The full tables are in [docs/SOLVER.md](docs/SOLVER.md) and [docs/PREFLOP.md](docs/PREFLOP.md).

## Known gaps

- **The preflop charts approximate a full-tree solution.** They come from an abstract tree with
  calibrated realization factors.
  - BB defence vs BTN is tight (about 46%), defence vs 4-bets is sticky, and the SB limps more
    than most published solutions.
  - Preflop grades are by chart frequency, with no EVs.
  - They are not exploitative: a call that the chart folds is graded against the chart, even
    against a fish.
- **The 150 and 200bb charts barely differ from 100bb in opening ranges.** They reuse the 100bb
  realization factors. `build_depth_charts --rounds 1` recalibrates them, at roughly 15-20
  minutes per depth.
- **Multiway pots are not solved.** They get pot odds and equity, but no grade.
- **Flop sizing:** your flop bets are graded as the nearest tree size (33% or 75%). Turn and
  river sizes are re-solved exactly.
- **Off-chart grades come from a one-step heuristic model,** not a solver. They are labelled as
  such.
- **Cache coverage:** uncached flops make the coach wait (15-35 s, or minutes at deep buckets).
  A night of pre-solving covers the most common 8-10% of heads-up flop play.
- **Bunching is only used on river re-solves with at most 3 preflop folders.** A turn with
  bunching is too slow to run live.
- **No GPU solver yet.** Flop solves are CPU-bound; a Metal CFR is the obvious next lever.
- **Phone layout:** at phone width the table hides the bots' face-down cards to make room. Folded
  seats are dimmed, and shown cards still appear.

## What is where

| Area | Path |
|---|---|
| Rules, hand evaluator, equity, ranges, stats, hand histories | `crates/engine` |
| Preflop solver and charts | `crates/strategy/src/preflop`, `data/preflop` |
| postflop-solver integration (specs, cache, per-hand solves, node locking) | `crates/strategy/src/postflop` |
| Bots, personalities, the Hunter | `crates/strategy/src/bots` |
| Coach fact sheets and grades, off-chart heuristic | `crates/strategy/src/coach`, `crates/server/src/coach.rs`, `crates/server/src/heuristic.rs` |
| Server (axum, WebSocket table, SQLite logbook, Claude, simulator) | `crates/server` |
| UI (React + Vite) | `ui/src` |
| Vendored solver and its patches | `vendor/postflop-solver`, `vendor/PATCHES.md` |

## Keyboard (table)

| Key | Action |
|---|---|
| `F` | fold |
| `C` / `Space` | check or call |
| `1`-`5` | bet sizes |
| `R`, then `Enter` | custom size |
| `A` | all-in |
| `N` | next hand |
| `P` | pause |

## Tests

```bash
cargo test --release --workspace --exclude postflop-solver
```

```bash
cargo test --release -p engine -- --ignored exhaustive
```

- The first runs the unit tests: rules edge cases, evaluator, ranges, equity, hand histories,
  stats, cache keys, depth buckets and Session-mode persistence. It also includes regression
  tests, such as hand #17 (an isolation raise over a fish limp that was once graded as a 3-bet).
- The second checks all 133,784,560 seven-card hands, in about 2 minutes.

## License

Copyright (C) 2026 the pokersim contributors.

This program is free software: you can redistribute it and/or modify it under the terms of the
GNU Affero General Public License as published by the Free Software Foundation, either version 3
of the License, or (at your option) any later version. See [LICENSE](LICENSE).

The vendored postflop-solver is also AGPL-3.0-or-later. The server listens on 127.0.0.1 only.
If you expose it to other people over a network, the AGPL requires you to offer them the source.
