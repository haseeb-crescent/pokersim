# Patches to vendored postflop-solver

Source: https://github.com/b-inary/postflop-solver at commit
`9d1509fe5077d019825f833eed04b16d342dfda1` (2023-10-01, development suspended).
License: AGPL-3.0-or-later (see `postflop-solver/LICENSE`); this project uses it for personal,
local use.

1. `Cargo.toml`: `bincode = "=2.0.0-rc.3"`. Cargo otherwise resolves 2.0.1, whose `Decode`
   trait gained a context parameter. `bincode_derive` is pinned to `2.0.0-rc.3` in the
   workspace `Cargo.lock` for the same reason.
2. `src/lib.rs`: `#![allow(dangerous_implicit_autorefs)]`. The lint became deny-by-default in
   newer rustc; the code it flags was fine on the compilers the crate was written for.
3. `src/action_tree.rs`: new `TreeConfig::max_bets_per_street: [i32; 3]` (flop, turn, river).
   Once a street has that many bets/raises, the only raise offered is all-in. `0` keeps the
   original unlimited behaviour. This is what makes 100bb flop trees small enough to solve
   live (see `docs/SOLVER.md`).
