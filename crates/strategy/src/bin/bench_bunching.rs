//! Measures bunching-effect precomputation and its cost on turn/river re-solves.
use postflop_solver::*;
use std::time::Instant;
use strategy::preflop::charts::Charts;

fn main() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let charts = Charts::load(&root.join("data/preflop/charts.json")).unwrap();
    // BTN open, BB call: UTG, HJ, CO folded first-in, SB folded facing the open
    let key = "F-F-F-R-F-C";
    let to_ps = |grid: &[f32]| {
        let mut r = Range::new();
        for c1 in 0..52u8 {
            for c2 in c1 + 1..52 {
                r.set_weight_by_cards(c1, c2, grid[engine::card::class_of(c1, c2)].clamp(0.0, 1.0));
            }
        }
        r
    };
    // fold ranges: hands that took the fold action at each folder's node
    let fold_range = |prefix: &str| {
        let n = &charts.nodes[prefix];
        let reach = charts.range_along(prefix, n.player);
        let g: Vec<f32> = (0..169).map(|h| reach[h] * n.freq[0][h]).collect();
        to_ps(&g)
    };
    let folds = vec![fold_range(""), fold_range("F"), fold_range("F-F"), fold_range("F-F-F-R")];
    let flop = flop_from_str("Kh7d2c").unwrap();
    for k in [1usize, 2, 3, 4] {
        let t = Instant::now();
        let mut b = BunchingData::new(&folds[..k], flop).unwrap();
        b.process(false);
        println!("bunching data, {k} folded players: {:.1}s, {:.0} MB", t.elapsed().as_secs_f64(), b.memory_usage() as f64 / 1e6);
        if k == 4 {
            for (turn, river) in [("5s", "Jd"), ("5s", "")] {
                for bunch in [false, true] {
                    let oop = to_ps(&charts.range_along(key, 5));
                    let ip = to_ps(&charts.range_along(key, 3));
                    let cc = CardConfig { range: [oop, ip], flop, turn: card_from_str(turn).unwrap(), river: if river.is_empty() { NOT_DEALT } else { card_from_str(river).unwrap() } };
                    let sizes = BetSizeOptions::try_from(("33%,75%,150%", "3x")).unwrap();
                    let tc = TreeConfig {
                        initial_state: if river.is_empty() { BoardState::Turn } else { BoardState::River },
                        starting_pot: 1100, effective_stack: 9475, rake_rate: 0.05, rake_cap: 300.0,
                        flop_bet_sizes: [sizes.clone(), sizes.clone()], turn_bet_sizes: [sizes.clone(), sizes.clone()], river_bet_sizes: [sizes.clone(), sizes],
                        turn_donk_sizes: None, river_donk_sizes: None, add_allin_threshold: 1.5, force_allin_threshold: 0.15, merging_threshold: 0.1,
                        max_bets_per_street: [2, 2, 3],
                    };
                    let mut g = PostFlopGame::with_config(cc, ActionTree::new(tc).unwrap()).unwrap();
                    if bunch {
                        g.set_bunching_effect(&b).unwrap();
                    }
                    g.allocate_memory(true);
                    let t = Instant::now();
                    let e = solve(&mut g, 400, 1100.0 * 0.004, false);
                    println!("  {} re-solve, bunching {bunch}: {:.2}s (expl {:.2}% pot)", if river.is_empty() { "turn" } else { "river" }, t.elapsed().as_secs_f64(), 100.0 * e / 1100.0);
                }
            }
        }
    }
}
