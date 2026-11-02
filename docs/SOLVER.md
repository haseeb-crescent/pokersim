# Postflop solving: measurements and the trees chosen

All numbers measured on this machine (Apple M5 Max, 18 cores, 64 GB) with the vendored
postflop-solver (Discounted CFR, rayon, 16-bit compressed storage), 100bb stacks, 5% rake
capped at 3bb. "Exploitability" is postflop-solver's own measure, in % of the starting pot.
Benchmarks: `crates/strategy/src/bin/bench_solve.rs` and `bench_bunching.rs`.

Ranges used: BTN open ~48% vs BB defend ~55% (single-raised pot, SRP), CO 3-bet vs BTN call
(3-bet pot, 3BP).

## 1. Full trees are too slow live

Unlimited raises (3x) on every street:

| Spot | Tree | Memory | Time to target |
|---|---|---|---|
| SRP flop K72r | 1 size per street | 6.4 GB | 75 s to 0.44% |
| SRP flop T98ss | 1 size per street | 3.7 GB | 31 s to 0.48% |
| 3BP flop Q63r | 1 size per street | 0.4 GB | 3.2 s to 0.45% |
| SRP turn | 1 size | 24 MB | 0.15 s |
| SRP river | 1 size | <1 MB | 0.01 s |
| SRP flop K72r | 2 sizes per street | 11.2 GB | 25.7% after 40 s (unusable) |

The cost is dominated by deep raise wars (3x, 3x, 3x... at 100bb) multiplied across every
turn and river card.

## 2. What made it fast enough

- **Raise caps** (patch to the vendored solver, `max_bets_per_street`): after the cap the only
  raise left is all-in. Bet-raise-jam on the flop, bet-jam on later streets inside the flop
  tree cut the K72r tree from 6.4 GB to 1.2 GB.
- **Rich current street, coarse future streets.** The turn and river are re-solved on the
  actual cards anyway, so inside a flop solve the turn/river subtrees only provide continuation
  values: one size each is enough. The current street gets the full menu.
- **Threads matter** (compute-bound, not bandwidth-bound): K72r to 1% takes 27.8 s on 8
  threads, 23.0 s on 12, 17.0 s on 18. A GPU (Metal) CFR is the obvious next lever.
- Compression (16-bit) halves memory at no speed cost (15.8 s vs 17.0 s).

| Spot | Tree | Memory | 2% | 1% | 0.5% |
|---|---|---|---|---|---|
| SRP flop K72r | flop 33%, turn 66%, river 75% (caps 2/1/1) | 1.2 GB | 13.5 s | 17.0 s | 23.3 s |
| SRP flop K72r | **flop 33%+75%**, turn 66%, river 75% (caps 2/1/1) | 2.0 GB | 25.0 s | 31.2 s | 42.0 s |
| SRP flop T98ss | same (chosen) | 1.1 GB | 14.6 s | 16.6 s | 22.3 s |
| SRP flop K72r | flop 25/50/100%, turn 66%, river 75% | 3.0 GB | 40 s | — | — |
| 3BP flop Q63r | flop 25/50/100% | 0.4 GB | 3.1 s | 7.7 s | 9.4 s |
| SRP turn | turn 33/75/125%, river same, cap 3 | 66 MB | 0.7 s | 1.1 s | 1.8 s |
| SRP river | 33/75/125%, cap 3 | <1 MB | <0.1 s | | |

## 3. The trees in use (`crates/strategy/src/postflop/spot.rs`)

| Preset | Sizes | Caps | Stop at | Max time |
|---|---|---|---|---|
| Flop | flop 33% / 75%; turn 66%; river 75%; raises 3x | bet-raise-jam, then bet-jam | 0.75% pot | 45 s |
| Turn (re-solve) | turn 33% / 75% / 125%; river 50% / 100% | bet-raise-jam | 0.4% pot | 12 s |
| River (re-solve) | 33% / 75% / 150% | up to 3 bets | 0.25% pot | 4 s |

All-in is added whenever the largest bet is at most 1.5x pot, and forced when the stack-to-pot
ratio after a call would be 0.15 or less.

Live behaviour:
- A bot acts once a running solve reaches 3% of pot, or after at most 6 s, using the current
  average strategy; the solve keeps refining and the coach waits for the final result. If a
  solve is not usable by then (under 25 iterations and above 15% of pot), the bot uses the
  equity heuristic for that decision instead of a near-random strategy.
- Solves have two priorities: live (what a bot is waiting for) and background (the coach's
  variants and exploit views, and earlier streets still refining). Background solves pause
  between iterations while any live solve runs, so grading never slows the bots down.
- Single-raised flops not in the cache take roughly 15-35 s to reach the final target; 3-bet
  pots 3-10 s; limped SB-vs-BB pots (the widest ranges) are the slowest.
- Off-tree sizes: on the turn and river the street is re-solved with your exact size added to
  the tree (about 1 s); on the flop your bet is graded as the nearest tree size, and the grade
  says so.

## 4. Cache and overnight pre-solve

- Finished flop solves are stored with flop-only storage: about 0.1 MB each
  (`data/cache/solves`). The key is the suit-canonical spot. It covers the board, the
  effective-stack depth bucket, a hash of the exact range weights of both players (f32 bits, so
  any chart change, even one combo, is a new key), the pot, the stack behind, the tree and the
  rake. Files are named like `2c7dKh-100bb-<hash>.bin`. The 22,100 flops collapse to 1,755, and
  every isomorphic spot is a hit.
- The key format changed when depth buckets were added. `presolve --migrate-v1` re-keys an
  existing cache: it recomputes the old and new key of every library spot and renames the
  matching files. That takes about 30 s, and on the first cache it re-keyed all 102 library
  flops. Turn and river re-solves and one-off live or Lab spots in the old format are no longer
  looked up, and are solved again if they come back.
- `./overnight.sh [hours]` runs `presolve`: every heads-up flop line of the chart tree is
  weighted by how often it happens under the charts, every canonical flop by how often it is
  dealt, and spots are solved in that order with exactly the live specs. It prints the share of
  heads-up flop situations covered so far and resumes where it stopped.
- Spots are ordered by expected cache hits per CPU-second: (line frequency x flop frequency)
  divided by the typical solve time of the pot type. Offline solves get up to 150 s each so they
  reach the 0.75%-of-pot target.
- Measured throughput: 3-bet-pot flops about 10 s each (~340/hour, 0.6-0.75% of pot);
  single-raised flops about 25-45 s (~100/hour); limped SB-vs-BB flops (the widest ranges)
  about 100+ s. That is roughly 1% of all heads-up flop situations covered per hour: a night
  covers the most valuable 8-10%, and full coverage of the 29 common lines x 1,755 flops is
  several days of solving. Every live solve is cached too, so coverage also grows as you play.
- Chart changes change ranges and therefore cache keys: refine charts before pre-solving
  (`./overnight.sh 10 refine` does both in the right order).

## 5. Keeping the hero's hand gradeable

A hand with zero weight in its range has no strategy. Measured options:
- Flooring every combo of both ranges keeps everything cacheable but costs 1.9x on a
  single-raised flop and 10x on a 3-bet flop (narrow ranges become full ranges): rejected.
- Instead the actual hand is floored (weight 0.2% of the range's max) into a separate variant
  solve, per viewer: the coach's variant contains only the hero's hand, a bot's variant only
  its own hand. Bots never use a solve that depends on cards they cannot see.
- Turn/river variants are cheap (about 1 s). On the flop, a variant costs a full flop solve, so
  the coach waits for it in the background; a bot with an off-range flop hand borrows the
  strategy of the in-range hand with the closest equity against the opponent's range.

## 6. Bunching effect

postflop-solver can account for the cards held by players who folded preflop:

| | Time |
|---|---|
| Precompute, 1 / 2 / 3 folded players | 0.1 / 0.3 / 3.2 s (64 MB) |
| Precompute, 4 folded players | 229 s |
| River re-solve without / with bunching | 0.01 / 0.26 s |
| Turn re-solve without / with bunching | 0.95 / 20.9 s |

So bunching is used on river re-solves when at most 3 players folded preflop (computed in the
background from the flop on). With 4 folders (e.g. BTN vs BB after UTG, HJ, CO and SB fold) the
precompute is too slow to do live, and the turn is too slow with bunching at any count.

## 7. Node locking (Hunter, exploit view)

`PostFlopGame::lock_current_strategy` (full or per-hand partial locks) is applied after memory
allocation and before solving. An observed frequency ("folds 68% facing big turn bets") is
turned into a lock by starting from the solver's per-hand strategy at that node and moving
probability mass hand by hand, weakest hands folding first (strongest first when folding
less), until the range-wide frequency matches. The exploit view locks a bot's profile
(its distortion applied to every hand) at its next decision and reads the hero's best
response. Locked solves are never cached.

## 8. Stack depths other than 100bb (Session mode)

The postflop spec is the same tree at a different depth. The depth bucket and the stack behind
both go into the spec, and therefore into the cache key, so a solve is never served for another
depth.

- Within 10% of 100bb, the spot is solved at exactly 100bb, so the overnight library and cache
  apply. The coach notes the real effective stack.
- Otherwise, the depth is rounded to a bucket: 5bb steps below 50bb, 10bb up to 100bb, 20bb
  above. The pot is real and the remaining stack is shifted by the rounding, so the bucket is
  never more than about 12% from the real stack. Spots at a similar depth share cache entries,
  so the library grows per bucket as you play.
- Ranges come from the preflop chart nearest the effective stack (see PREFLOP.md).
- Under 20bb effective, nothing is solved, and the coach shows the unsolved view.
- Measured once: a 160bb single-raised flop took about 3 minutes to reach the final target
  (0.74% of pot), with other solves running. That is several times a 100bb flop, because more
  raises fit before all-in. Bots still act from the running solve after at most 6 s.

## Vendored solver patches

See `vendor/PATCHES.md`.
