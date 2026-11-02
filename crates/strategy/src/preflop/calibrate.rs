//! Calibrates the preflop model's realization factors against postflop-solver.
//!
//! For representative flop pots (single-raised, 3-bet, 4-bet, limped; both positions), the
//! ranges reaching the flop under the current preflop solution are solved on a fixed,
//! texture-stratified set of flops. For every hand class, realization is measured as
//! `sum(EV) / sum(flop equity * pot)` over the sampled flops and combos, which cancels most of
//! the board luck. Classes that never reach the spot keep their previous factor.

use super::cfr::{Realization, Role, Solver};
use super::game::*;
use crate::postflop::*;
use engine::card::*;
use engine::Range;
use std::collections::BTreeMap;
use std::sync::Arc;

pub struct CalSpot {
    pub name: &'static str,
    /// action sequence that ends in the flop terminal
    pub key: &'static str,
}

pub const CAL_SPOTS: &[CalSpot] = &[
    CalSpot { name: "SRP BTN vs BB", key: "F-F-F-R-F-C" },
    CalSpot { name: "SRP CO vs BB", key: "F-F-R-F-F-C" },
    CalSpot { name: "SRP UTG vs BB", key: "R-F-F-F-F-C" },
    CalSpot { name: "SRP SB vs BB", key: "F-F-F-F-R-C" },
    CalSpot { name: "SRP CO vs BTN", key: "F-F-R-C-F-F" },
    CalSpot { name: "3BP BTN vs CO", key: "F-F-R-R-F-F-C" },
    CalSpot { name: "3BP BB vs BTN", key: "F-F-F-R-F-R-C" },
    CalSpot { name: "3BP SB vs BTN", key: "F-F-F-R-R-F-C" },
    CalSpot { name: "4BP CO vs BTN", key: "F-F-R-R-F-F-R-C" },
    CalSpot { name: "Limped SB vs BB", key: "F-F-F-F-C-K" },
];

pub const CAL_FLOPS: &[&str] = &["As7d2c", "KhTd5s", "Qs9h4h", "Jc8c3d", "Td9s8h", "8s6d2h", "6h5h4c", "KcKd7s", "Ah5h3h", "9c7c2d", "AdKs4h", "QcJh5c", "7d5s3c", "Kh8h8c", "JsTs9s", "4c3d2h"];

#[derive(Default, Clone)]
struct Acc {
    ev: f64,
    base: f64,
}

pub struct CalReport {
    pub lines: Vec<String>,
}

/// Shrinks each class's realization factor toward its grid neighbours of the same type
/// (adjacent pairs; suited/offsuit cells one rank apart). Per-class measurements from a few
/// dozen flops are noisy, and realization varies smoothly between neighbouring hands, so
/// this removes artifacts such as one suited ace folding between two that raise.
pub fn smooth(real: &mut Realization, passes: usize, keep: f64) {
    for v in real.table.values_mut() {
        for _ in 0..passes {
            let old = v.clone();
            for c in 0..NUM_CLASSES {
                let (r, k) = ((c / 13) as i32, (c % 13) as i32);
                let same = |rr: i32, kk: i32| -> bool {
                    (0..13).contains(&rr) && (0..13).contains(&kk) && ((r == k && rr == kk) || (r < k && rr < kk) || (r > k && rr > kk))
                };
                let cand: Vec<(i32, i32)> = if r == k { vec![(r - 1, k - 1), (r + 1, k + 1)] } else { vec![(r - 1, k), (r + 1, k), (r, k - 1), (r, k + 1)] };
                let nb: Vec<f64> = cand.into_iter().filter(|&(a, b)| same(a, b)).map(|(a, b)| old[(a * 13 + b) as usize]).collect();
                if !nb.is_empty() {
                    v[c] = keep * old[c] + (1.0 - keep) * nb.iter().sum::<f64>() / nb.len() as f64;
                }
            }
        }
    }
}

pub fn find_terminal(tree: &Tree, key: &str) -> Option<usize> {
    tree.nodes.iter().position(|n| matches!(n, Node::Terminal { key: k, .. } if k == key))
}

pub fn calibrate(solver: &Solver, tree: &Tree, prev: &Realization, flops_per_spot: usize, log: &mut dyn FnMut(String)) -> (Realization, CalReport) {
    let cache = Arc::new(SolveCache::disabled());
    let mut acc: BTreeMap<String, Vec<Acc>> = BTreeMap::new();
    let mut report = Vec::new();
    for spot in CAL_SPOTS {
        let Some(tid) = find_terminal(tree, spot.key) else {
            log(format!("skip {}: terminal {} not in tree", spot.name, spot.key));
            continue;
        };
        let Node::Terminal { contrib, live, level, .. } = &tree.nodes[tid] else { continue };
        if live.len() != 2 {
            continue;
        }
        let mut players = live.clone();
        players.sort_by_key(|&p| postflop_rank(p));
        let (oop, ip) = (players[0], players[1]);
        let pot: i32 = contrib.iter().sum();
        let stack = STACK - contrib[oop].max(contrib[ip]);
        let grid = |p: usize| -> Range {
            let g: Vec<f32> = solver.range_at(tid, p).iter().map(|&x| x as f32).collect();
            let mx = g.iter().cloned().fold(0.0f32, f32::max).max(1e-9);
            // floor hands that the solution plays at least occasionally so the postflop solve
            // values them too
            let g: Vec<f32> = g.iter().map(|&x| { let x = x / mx; if x >= 1e-3 { x.max(0.003) } else { 0.0 } }).collect();
            Range::from_grid(&g)
        };
        let ranges = [grid(oop), grid(ip)];
        for flop in CAL_FLOPS.iter().take(flops_per_spot) {
            let board = cards_from_str(flop).unwrap();
            let mut rs = ranges.clone();
            rs[0].remove_dead(mask_of(&board));
            rs[1].remove_dead(mask_of(&board));
            let spec = SpotSpec { board: board.clone(), ranges: rs, pot, stack, preset: TreePreset::Calibration, rake_pct: 0.05, rake_cap: 300.0, extra_lines: vec![], locks: vec![], bunching: None };
            let t0 = std::time::Instant::now();
            let s = Spot::start(spec, cache.clone(), SolveTargets { target_pct: 2.5, max_secs: 25.0, background: false });
            let st = s.wait_final(std::time::Duration::from_secs(120));
            let vals = match s.values(&[]) {
                Ok(v) => v,
                Err(e) => {
                    log(format!("  {} {flop}: error {e}", spot.name));
                    continue;
                }
            };
            for (pi, &p) in [oop, ip].iter().enumerate() {
                let role = Role { pot: pot_type(*level), ip: pi == 1 };
                let a = acc.entry(Realization::key(role)).or_insert_with(|| vec![Acc::default(); NUM_CLASSES]);
                let v = &vals[pi];
                for (i, &(c1, c2)) in v.hands.iter().enumerate() {
                    let cls = class_of(c1, c2);
                    a[cls].ev += v.ev[i] as f64;
                    a[cls].base += v.equity[i] as f64 * pot as f64;
                }
                let _ = p;
            }
            log(format!("  {:<16} {flop}  {:>5.1}s  expl {:.2}%  combos {:.0}/{:.0}", spot.name, t0.elapsed().as_secs_f64(), st.exploitability_pct, ranges[0].combos(0), ranges[1].combos(0)));
        }
    }
    let mut out = prev.clone();
    for (key, a) in &acc {
        let old = prev.table.get(key).cloned().unwrap_or_else(|| vec![1.0; NUM_CLASSES]);
        let mut new = old.clone();
        let mut sum = 0.0;
        let mut n = 0.0;
        for c in 0..NUM_CLASSES {
            if a[c].base > 1.0 {
                let r = (a[c].ev / a[c].base).clamp(0.3, 1.6);
                new[c] = r;
                sum += r;
                n += 1.0;
            }
        }
        report.push(format!("{key:<14} measured {n:>3.0} classes, mean R {:.3}", if n > 0.0 { sum / n } else { 0.0 }));
        out.table.insert(key.clone(), new);
    }
    smooth(&mut out, 2, 0.4);
    (out, CalReport { lines: report })
}
