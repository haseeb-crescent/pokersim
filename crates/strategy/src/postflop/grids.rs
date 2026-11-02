//! 13x13 views of a solved node: per-class strategy, ranges and EVs for the replayer and the
//! range viewer.

use super::worker::NodeView;
use engine::card::{class_of, NUM_CLASSES};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeGrids {
    pub player: String,
    pub actions: Vec<String>,
    /// per action, per class: reach-weighted frequency within the class
    pub strategy_grid: Vec<Vec<f32>>,
    /// [OOP, IP] per class: average reach weight of the class's live combos (0..1)
    pub range_grid: [Vec<f32>; 2],
    pub range_freq: Vec<f32>,
    pub equity_grid: Option<Vec<f32>>,
    /// per action, per class, bb
    pub ev_grid: Option<Vec<Vec<f32>>>,
    pub pot_bb: f64,
    pub precision_pct: f32,
    pub finalized: bool,
}

pub fn grids(v: &NodeView) -> NodeGrids {
    let p = v.player;
    let n = v.hands[p].len();
    let a = v.actions.len();
    let mut wsum = vec![0f32; NUM_CLASSES];
    let mut strat = vec![vec![0f32; NUM_CLASSES]; a];
    let mut eqg = vec![0f32; NUM_CLASSES];
    let mut evg = vec![vec![0f32; NUM_CLASSES]; a];
    let mut cnt = vec![0f32; NUM_CLASSES];
    for (i, &(c1, c2)) in v.hands[p].iter().enumerate() {
        let c = class_of(c1, c2);
        let w = v.weights[p][i];
        cnt[c] += 1.0;
        wsum[c] += w;
        for k in 0..a {
            if !v.strategy.is_empty() {
                strat[k][c] += w * v.strategy[k * n + i];
            }
            if let Some(ev) = &v.ev {
                evg[k][c] += w * ev[k * n + i] / 100.0;
            }
        }
        if let Some(eq) = &v.equity {
            eqg[c] += w * eq[i];
        }
    }
    for c in 0..NUM_CLASSES {
        if wsum[c] > 0.0 {
            for k in 0..a {
                strat[k][c] /= wsum[c];
                evg[k][c] /= wsum[c];
            }
            eqg[c] /= wsum[c];
        }
    }
    let range = |pl: usize| -> Vec<f32> {
        let mut s = vec![0f32; NUM_CLASSES];
        let mut k = vec![0f32; NUM_CLASSES];
        for (i, &(c1, c2)) in v.hands[pl].iter().enumerate() {
            let c = class_of(c1, c2);
            s[c] += v.weights[pl][i];
            k[c] += 1.0;
        }
        (0..NUM_CLASSES).map(|c| if k[c] > 0.0 { (s[c] / k[c]).min(1.0) } else { 0.0 }).collect()
    };
    let _ = cnt;
    NodeGrids {
        player: if p == 0 { "OOP".into() } else { "IP".into() },
        actions: v.actions.iter().map(|x| x.label.clone()).collect(),
        strategy_grid: strat,
        range_grid: [range(0), range(1)],
        range_freq: if a > 0 { v.range_frequencies() } else { vec![] },
        equity_grid: v.equity.as_ref().map(|_| eqg),
        ev_grid: v.ev.as_ref().map(|_| evg),
        pot_bb: v.pot as f64 / 100.0,
        precision_pct: v.status.exploitability_pct,
        finalized: v.status.finalized,
    }
}
