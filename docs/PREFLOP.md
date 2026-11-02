# Where the preflop charts come from

**Short version:** the charts in `data/preflop/charts.json` are computed by this repo. They are
the output of a 6-max preflop CFR solver (`crates/strategy/src/preflop`) whose postflop values
are calibrated against postflop-solver. They are *not* copied from GTO Wizard, PioSolver,
MonkerSolver or any published chart pack (those are proprietary), and they are an
approximation of a full-tree preflop solution, not an equivalent of one.

## Pipeline

1. **Exact class equities** (`gen_equity_table`, 76 s): all-in equity between the 169 hand
   classes, enumerating all 1,712,304 boards for each of the 50,258 suit-distinct combo
   matchups, plus card-removal compatibility counts. Spot checks: AA vs KK 81.95%, AKo vs QQ
   43.2%, 72o vs AA 11.8%. Stored in `data/preflop/equity169.bin`.
2. **Abstract game** (`game.rs`, 1,937 decision nodes): 6-max, 100bb, blinds 0.5/1, rake 5%
   capped at 3bb, no flop no drop. Opens 2.5bb (SB 3bb), SB limp allowed (BB may iso to 4bb),
   3-bets 3x in position / 4x from the SB / 4.4x from the BB (3x when the BB 3-bets the SB) plus
   one open size per cold caller, 4-bets 2.25x in position / 2.5x out of position, 5-bet all-in.
   Cold calls of an open are allowed while at most three players see the flop; calling a 3-bet
   or more requires money already in the pot.
3. **Solver** (`cfr.rs`): vectorised Discounted CFR (alpha 1.5, beta 0, gamma 2) over 169
   classes per player, 3,000 iterations (~35 s). Convergence gauge: the best-response gain for
   every position is about 0.0001 bb/hand.
   - Folds: the winner takes the pot (no rake).
   - All-ins: exact all-in equity, pot raked.
   - Flops: `R(hand, role) x equity x pot - contribution`. `R`, the realization factor, absorbs
     postflop play and postflop rake. Heads-up terminals use exact class-pair card removal;
     three-way terminals use a product-of-heads-up-equities approximation.
4. **Calibration** (`calibrate.rs`, `build_charts`): for ten representative flop pots
   (single-raised BTN/CO/UTG vs BB, SB vs BB, CO vs BTN; 3-bet pots BTN vs CO, BB vs BTN, SB vs
   BTN; a 4-bet pot; SB limp vs BB check), the ranges reaching the flop under the current
   solution are solved with postflop-solver on a texture-stratified set of flops (10 flops in
   rounds 1-2, 16 in round 3). For each class, `R = sum(EV) / sum(flop equity x pot)` across
   flops and combos (a ratio estimator that cancels most of the board luck). The preflop game is
   re-solved with the new factors (damped 70/30, then neighbour-smoothed), three rounds in total
   (about 1.5 h).
   `data/preflop/realization.json` holds the factors, `calibration.log` the log.

## Result (current charts)

After round 3 the per-class factors were noisy (16 flops per spot: ATo realized 0.78 next to
QJo at 1.09), which made near-indifferent hands flip between pure fold and pure raise (A9s
folding between A8s and ATs raising). Since realization varies smoothly between neighbouring
hands, each factor is shrunk toward its grid neighbours of the same type (two passes, 40% own
value) before the final solve; `calibrate.rs` now does this after every round.

| Spot | Strategy |
|---|---|
| UTG first in | raise 16.2% |
| HJ first in | raise 19.8% |
| CO first in | raise 26.9% |
| BTN first in | raise 38.3% |
| SB first in | limp 48.8%, raise 6.5% |
| BB vs SB limp | check 68.3%, raise 31.7% |
| BB vs SB open | call 50.9%, 3-bet 11.1% |
| BB vs BTN open | call 33.3%, 3-bet 12.7% |
| BB vs CO open | call 28.0%, 3-bet 8.3% |
| SB vs BTN open | 3-bet 13.9%, call 1.0% |
| BTN vs CO open | 3-bet 10.4%, call 5.2% |
| BTN vs BB 3-bet | call 48.1%, 4-bet 6.2% |
| CO vs BTN 3-bet | call 53.4%, 4-bet 9.2% |
| BB vs BTN 4-bet | fold 32.4%, call 53.1%, all-in 14.5% |

UTG opens 33+, A4s+, suited broadways, K9s, 87s+ and AJo+. Expected value per position
(bb/hand): UTG +0.10, HJ +0.11, CO +0.14, BTN +0.18, SB -0.31, BB -0.48. In all-GTO self-play the
chart bot's signature is about VPIP 22 / PFR 16 / 3-bet 9.

## Known deviations from full-tree solutions

Compared with published solver outputs for similar rake, the opening ranges are close, but:
- **BB defence vs BTN is tight** (46% vs about 55-60%). The calibration tree is coarse (one
  bet size per street), which lowers the out-of-position player's measured realization.
- **Defence vs 4-bets is sticky** (BB folds 32% to a BTN 4-bet; full solutions fold more). The
  4-bet-pot factors are measured on few classes and SPR-1 pots are close to all-in.
- **SB limps more than most solutions** (49% limp / 6.5% raise), and UTG folds KQo.
- Realization depends only on hand class and role (pot type x position), not on the exact
  matchup; three-way pots use a heuristic.

These matter less than they look for grading: preflop decisions are graded by chart frequency
and labelled as such, and every postflop grade comes from postflop-solver on the actual board.

## Improving or replacing the charts

- More calibration rounds with more flops (fully unattended): `./overnight.sh 10 refine`, or
  `cargo run --release -p strategy --bin build_charts -- 1 16 3000 --resume`.
- The chart file format is plain JSON: one node per action sequence (`F-F-R` = UTG folds, HJ
  folds, CO raises; next to act), actions `F K C R A`, 169 frequencies per action. If you have
  licensed solver exports, converting them into this format replaces the charts without code
  changes (the postflop cache must then be rebuilt, since it is keyed by ranges).
