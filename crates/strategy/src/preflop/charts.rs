//! Preflop chart data: the solved strategy at every node of the abstract preflop game,
//! stored as JSON in `data/preflop/charts.json`, plus mapping from a real action sequence to
//! the nearest chart node.

use super::game::{Node, PfAction, Tree, NP, POS};
use engine::card::NUM_CLASSES;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChartMeta {
    pub source: String,
    pub method: String,
    pub iterations: u32,
    /// best-response gain per position (bb/hand), a convergence gauge
    pub br_gain_bb: Vec<f64>,
    /// EV per position (bb/hand)
    pub ev_bb: Vec<f64>,
    pub calibration_rounds: u32,
    pub created: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChartNode {
    /// position index 0..6 (UTG, HJ, CO, BTN, SB, BB)
    pub player: usize,
    /// action codes: F, K, C, R, A
    pub actions: Vec<String>,
    /// raise-to amounts in chips (1bb = 100) for R/A, 0 otherwise
    pub sizes: Vec<i32>,
    /// frequency of each action for each of the 169 classes
    pub freq: Vec<Vec<f32>>,
    /// model EV (bb, relative to the start of the hand) of each action for each class
    pub ev: Vec<Vec<f32>>,
    /// probability that play reaches this node
    pub reach: f64,
    /// raise level when the decision is made (0 unopened, 1 facing an open, ...)
    pub level: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Charts {
    pub meta: ChartMeta,
    pub nodes: BTreeMap<String, ChartNode>,
}

impl Charts {
    pub fn load(path: &std::path::Path) -> anyhow::Result<Charts> {
        let text = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&text)?)
    }

    pub fn save(&self, path: &std::path::Path) -> anyhow::Result<()> {
        std::fs::write(path, serde_json::to_string(self)?)?;
        Ok(())
    }

    pub fn from_solver(solver: &super::cfr::Solver, tree: &Tree, meta: ChartMeta, with_ev: bool) -> Charts {
        let reach = solver.node_reach();
        let mut nodes = BTreeMap::new();
        for (id, n) in tree.nodes.iter().enumerate() {
            if let Node::Decision { player, actions, key, level, .. } = n {
                let s = solver.average_strategy(id);
                let a = actions.len();
                let freq = (0..a).map(|i| (0..NUM_CLASSES).map(|h| round3(s[i * NUM_CLASSES + h])).collect()).collect();
                let ev = if with_ev && reach[id] > 1e-6 {
                    let e = solver.node_action_evs(id);
                    (0..a).map(|i| (0..NUM_CLASSES).map(|h| round3(e[i * NUM_CLASSES + h])).collect()).collect()
                } else {
                    vec![]
                };
                nodes.insert(
                    key.clone(),
                    ChartNode {
                        player: *player,
                        actions: actions.iter().map(|x| x.code().to_string()).collect(),
                        sizes: actions
                            .iter()
                            .map(|x| match x {
                                PfAction::Raise(v) => *v,
                                PfAction::AllIn => super::game::STACK,
                                _ => 0,
                            })
                            .collect(),
                        freq,
                        ev,
                        reach: reach[id],
                        level: *level,
                    },
                );
            }
        }
        Charts { meta, nodes }
    }

    /// Finds the chart node for a real preflop action sequence (one code per action, in
    /// order). Off-tree actions are mapped to the nearest abstract action: a limp outside the
    /// small blind counts as an open, and a call that the abstraction forbids (fourth player
    /// into the pot, cold-calling a 3-bet) is treated as a fold for the purposes of everyone
    /// else's strategy. Returns the node and the mapped sequence.
    pub fn lookup(&self, real: &[char]) -> Option<(&ChartNode, String)> {
        let mut mapped: Vec<char> = Vec::new();
        for &c in real {
            let key = mapped.iter().map(|c| c.to_string()).collect::<Vec<_>>().join("-");
            let node = self.nodes.get(&key)?;
            let has = |x: &str| node.actions.iter().any(|a| a == x);
            let m = match c {
                'R' | 'A' if has("R") => 'R',
                'R' | 'A' if has("A") => 'A',
                'R' | 'A' => if has("C") { 'C' } else if has("K") { 'K' } else { 'F' },
                'C' if has("C") => 'C',
                'C' if has("R") && node.level == 0 => 'R',
                'C' => 'F',
                'K' if has("K") => 'K',
                'K' => 'F',
                _ => 'F',
            };
            mapped.push(m);
        }
        let key = mapped.iter().map(|c| c.to_string()).collect::<Vec<_>>().join("-");
        self.nodes.get(&key).map(|n| (n, key))
    }

    /// Maps a real action sequence (possibly complete, ending at a terminal) to the abstract
    /// key using the same rules as [`lookup`]. Returns the mapped key and, for each real
    /// action, whether it had to be changed.
    pub fn map_key(&self, real: &[char]) -> Option<(String, Vec<bool>)> {
        let mut mapped: Vec<char> = Vec::new();
        let mut changed = Vec::new();
        for &c in real {
            let key = mapped.iter().map(|c| c.to_string()).collect::<Vec<_>>().join("-");
            let node = self.nodes.get(&key)?;
            let has = |x: &str| node.actions.iter().any(|a| a == x);
            let m = match c {
                'R' | 'A' if has("R") => 'R',
                'R' | 'A' if has("A") => 'A',
                'R' | 'A' => if has("C") { 'C' } else if has("K") { 'K' } else { 'F' },
                'C' if has("C") => 'C',
                'C' if has("R") && node.level == 0 => 'R',
                'C' => 'F',
                'K' if has("K") => 'K',
                _ => 'F',
            };
            changed.push(m != c && !(c == 'A' && m == 'R'));
            mapped.push(m);
        }
        Some((mapped.iter().map(|c| c.to_string()).collect::<Vec<_>>().join("-"), changed))
    }

    /// Range (per class weight) that `player` has when reaching the node `key`, following the
    /// chart frequencies along the path.
    pub fn range_along(&self, key: &str, player: usize) -> Vec<f32> {
        let mut r = vec![1.0f32; NUM_CLASSES];
        let codes: Vec<&str> = if key.is_empty() { vec![] } else { key.split('-').collect() };
        let mut prefix: Vec<&str> = Vec::new();
        for code in codes {
            let k = prefix.join("-");
            if let Some(n) = self.nodes.get(&k) {
                if n.player == player {
                    if let Some(ai) = n.actions.iter().position(|a| a == code) {
                        for h in 0..NUM_CLASSES {
                            r[h] *= n.freq[ai][h];
                        }
                    }
                }
            }
            prefix.push(code);
        }
        r
    }

    /// Human description of a node key, e.g. "UTG folds, HJ folds, CO raises, BTN to act".
    pub fn describe(&self, key: &str) -> String {
        let mut parts = Vec::new();
        let codes: Vec<&str> = if key.is_empty() { vec![] } else { key.split('-').collect() };
        let mut prefix: Vec<&str> = Vec::new();
        for code in &codes {
            if let Some(n) = self.nodes.get(&prefix.join("-")) {
                let verb = match *code {
                    "F" => "folds",
                    "K" => "checks",
                    "C" => "calls",
                    "R" => "raises",
                    _ => "goes all-in",
                };
                if *code != "F" {
                    parts.push(format!("{} {}", POS[n.player], verb));
                }
            }
            prefix.push(code);
        }
        if let Some(n) = self.nodes.get(key) {
            parts.push(format!("{} to act", POS[n.player]));
        }
        if parts.len() == 1 && codes.iter().all(|c| *c == "F") {
            return format!("folded to {}", POS[self.nodes[key].player]);
        }
        parts.join(", ")
    }
}

fn round3(x: f64) -> f32 {
    ((x * 1000.0).round() / 1000.0) as f32
}

pub const POSITIONS: [&str; NP] = POS;
