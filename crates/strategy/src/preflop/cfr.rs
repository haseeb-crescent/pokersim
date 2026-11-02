//! Vectorised Discounted CFR over the abstract preflop game, 169 hand classes per player.
//!
//! Terminal values:
//! - fold: the winner takes the pot (no rake preflop: no flop, no drop)
//! - all-in: exact all-in equity from the class equity table, pot raked 5% capped at 3bb
//! - flop: `R(hand, role) * equity * pot - contribution`, where the realization factor R
//!   absorbs postflop play and postflop rake. R starts from a heuristic and is then calibrated
//!   against postflop-solver solutions (see `calibrate.rs`).
//!
//! Heads-up terminals use card removal between the two live players (class-pair compatibility
//! counts); folded players and three-way pots use independent ranges.

use super::equity_table::EquityTable;
use super::game::*;
use engine::card::{class_combo_count, NUM_CLASSES};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Mutex;

const H: usize = NUM_CLASSES;
type Vec6 = [[f64; H]; NP];

/// Role of a player in a heads-up flop pot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Role {
    pub pot: PotType,
    pub ip: bool,
}

/// Realization factors per role and class.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Realization {
    pub table: BTreeMap<String, Vec<f64>>,
}

impl Realization {
    pub fn key(r: Role) -> String {
        format!("{:?}-{}", r.pot, if r.ip { "IP" } else { "OOP" })
    }

    pub fn get(&self, r: Role) -> &[f64] {
        &self.table[&Self::key(r)]
    }

    /// Heuristic starting point: position and pot type set the base, suitedness and
    /// connectedness add playability.
    pub fn heuristic() -> Realization {
        let mut table = BTreeMap::new();
        for pot in [PotType::Limped, PotType::Single, PotType::ThreeBet, PotType::FourBet] {
            for ip in [true, false] {
                let base = match (pot, ip) {
                    (PotType::Limped, true) => 1.00,
                    (PotType::Limped, false) => 0.86,
                    (PotType::Single, true) => 1.03,
                    (PotType::Single, false) => 0.86,
                    (PotType::ThreeBet, true) => 1.00,
                    (PotType::ThreeBet, false) => 0.91,
                    (PotType::FourBet, true) => 0.98,
                    (PotType::FourBet, false) => 0.95,
                };
                let deep = matches!(pot, PotType::Limped | PotType::Single);
                let v: Vec<f64> = (0..H).map(|c| base * playability(c, deep)).collect();
                table.insert(Self::key(Role { pot, ip }), v);
            }
        }
        Realization { table }
    }
}

fn playability(class: usize, deep: bool) -> f64 {
    let (row, col) = (class / 13, class % 13);
    let scale = if deep { 1.0 } else { 0.5 };
    if row == col {
        return 1.0;
    }
    let suited = row < col;
    let (hi, lo) = if suited { (12 - row, 12 - col) } else { (12 - col, 12 - row) };
    let gap = hi - lo - 1;
    let mut m = 0.0;
    if suited {
        m += 0.05;
    }
    m += match gap {
        0 => 0.03,
        1 => 0.02,
        2 => 0.01,
        _ => -0.02,
    };
    if !suited && gap >= 3 {
        m -= 0.03;
    }
    if lo >= 8 {
        m += 0.01;
    }
    1.0 + scale * m
}

struct NodeData {
    regret: Vec<f64>,
    strat: Vec<f64>,
}

pub struct Solver<'a> {
    pub tree: &'a Tree,
    eq: &'a EquityTable,
    pub real: Realization,
    data: Vec<Mutex<NodeData>>,
    w: [f64; H],
    /// compatibility-weighted equity and compatibility, divided by 1225
    ek: Vec<f64>,
    kk: Vec<f64>,
    rake_pct: f64,
    rake_cap: f64,
    pub iterations: u32,
}

#[derive(Clone, Copy)]
enum Mode {
    Train(u32),
    Average,
    BestResponse(usize),
}

impl<'a> Solver<'a> {
    pub fn new(tree: &'a Tree, eq: &'a EquityTable, real: Realization) -> Self {
        let data = tree
            .nodes
            .iter()
            .map(|n| {
                let a = match n {
                    Node::Decision { actions, .. } => actions.len(),
                    _ => 0,
                };
                Mutex::new(NodeData { regret: vec![0.0; a * H], strat: vec![0.0; a * H] })
            })
            .collect();
        let mut w = [0.0; H];
        for c in 0..H {
            w[c] = class_combo_count(c) as f64 / 1326.0;
        }
        let mut ek = vec![0.0; H * H];
        let mut kk = vec![0.0; H * H];
        for a in 0..H {
            for b in 0..H {
                kk[a * H + b] = eq.k(a, b) as f64 / 1225.0;
                ek[a * H + b] = kk[a * H + b] * eq.eq(a, b) as f64;
            }
        }
        Solver { tree, eq, real, data, w, ek, kk, rake_pct: 0.05, rake_cap: 300.0, iterations: 0 }
    }

    pub fn run(&mut self, iterations: u32, mut progress: impl FnMut(u32, &Self)) {
        for _ in 0..iterations {
            self.iterations += 1;
            let t = self.iterations;
            let reach = Box::new([[1.0; H]; NP]);
            let _ = self.walk(0, &reach, Mode::Train(t), 0);
            progress(t, self);
        }
    }

    /// Average strategy at a decision node: actions x classes.
    pub fn average_strategy(&self, id: usize) -> Vec<f64> {
        let Node::Decision { actions, .. } = &self.tree.nodes[id] else { return vec![] };
        let a = actions.len();
        let d = self.data[id].lock().unwrap();
        let mut out = vec![0.0; a * H];
        for h in 0..H {
            let s: f64 = (0..a).map(|i| d.strat[i * H + h]).sum();
            for i in 0..a {
                out[i * H + h] = if s > 0.0 { d.strat[i * H + h] / s } else { 1.0 / a as f64 };
            }
        }
        out
    }

    fn current_strategy(&self, id: usize, a: usize) -> Vec<f64> {
        let d = self.data[id].lock().unwrap();
        let mut out = vec![0.0; a * H];
        for h in 0..H {
            let s: f64 = (0..a).map(|i| d.regret[i * H + h].max(0.0)).sum();
            for i in 0..a {
                out[i * H + h] = if s > 0.0 { d.regret[i * H + h].max(0.0) / s } else { 1.0 / a as f64 };
            }
        }
        out
    }

    fn walk(&self, id: usize, reach: &Vec6, mode: Mode, depth: usize) -> Box<Vec6> {
        match &self.tree.nodes[id] {
            Node::Terminal { kind, contrib, live, level, .. } => self.terminal(*kind, contrib, live, *level, reach),
            Node::Decision { player, actions, children, .. } => {
                let p = *player;
                let a = actions.len();
                let sigma = match mode {
                    Mode::Train(_) => self.current_strategy(id, a),
                    _ => self.average_strategy(id),
                };
                let child_vals: Vec<Box<Vec6>> = if depth < 4 {
                    (0..a)
                        .into_par_iter()
                        .map(|i| {
                            let mut r = Box::new(*reach);
                            for h in 0..H {
                                r[p][h] *= sigma[i * H + h];
                            }
                            self.walk(children[i], &r, mode, depth + 1)
                        })
                        .collect()
                } else {
                    (0..a)
                        .map(|i| {
                            let mut r = Box::new(*reach);
                            for h in 0..H {
                                r[p][h] *= sigma[i * H + h];
                            }
                            self.walk(children[i], &r, mode, depth + 1)
                        })
                        .collect()
                };
                let mut out = Box::new([[0.0; H]; NP]);
                for j in 0..NP {
                    if j == p {
                        continue;
                    }
                    for cv in &child_vals {
                        for h in 0..H {
                            out[j][h] += cv[j][h];
                        }
                    }
                }
                match mode {
                    Mode::BestResponse(br) if br == p => {
                        for h in 0..H {
                            out[p][h] = (0..a).map(|i| child_vals[i][p][h]).fold(f64::MIN, f64::max);
                        }
                    }
                    _ => {
                        for i in 0..a {
                            for h in 0..H {
                                out[p][h] += sigma[i * H + h] * child_vals[i][p][h];
                            }
                        }
                    }
                }
                if let Mode::Train(t) = mode {
                    let t = t as f64;
                    let pos_disc = t.powf(1.5) / (t.powf(1.5) + 1.0);
                    let neg_disc = 0.5;
                    let strat_disc = (t / (t + 1.0)).powi(2);
                    let mut d = self.data[id].lock().unwrap();
                    for i in 0..a {
                        for h in 0..H {
                            let k = i * H + h;
                            let r = d.regret[k];
                            let r = r * if r > 0.0 { pos_disc } else { neg_disc };
                            d.regret[k] = r + child_vals[i][p][h] - out[p][h];
                            d.strat[k] = d.strat[k] * strat_disc + reach[p][h] * sigma[k];
                        }
                    }
                }
                out
            }
        }
    }

    fn mass(&self, r: &[f64; H]) -> f64 {
        (0..H).map(|h| self.w[h] * r[h]).sum()
    }

    fn matvec(&self, m: &[f64], v: &[f64; H]) -> [f64; H] {
        let mut out = [0.0; H];
        for a in 0..H {
            let row = &m[a * H..(a + 1) * H];
            let mut s = 0.0;
            for b in 0..H {
                s += row[b] * v[b];
            }
            out[a] = s;
        }
        out
    }

    fn terminal(&self, kind: TermKind, contrib: &[i32; NP], live: &[usize], level: u8, reach: &Vec6) -> Box<Vec6> {
        let pot: f64 = contrib.iter().map(|&c| c as f64).sum();
        let masses: Vec<f64> = (0..NP).map(|j| self.mass(&reach[j])).collect();
        let mut out = Box::new([[0.0; H]; NP]);
        let others = |i: usize, set: &dyn Fn(usize) -> bool| -> f64 {
            (0..NP).filter(|&j| j != i && set(j)).map(|j| masses[j]).product()
        };
        // folded players lose their contribution regardless of cards
        for i in 0..NP {
            if !live.contains(&i) {
                let v = -(contrib[i] as f64) * others(i, &|_| true);
                out[i] = [v; H];
            }
        }
        match kind {
            TermKind::Fold => {
                let w = live[0];
                let v = (pot - contrib[w] as f64) * others(w, &|_| true);
                out[w] = [v; H];
            }
            _ if live.len() == 2 => {
                let rake = (self.rake_pct * pot).min(self.rake_cap);
                for (x, y) in [(live[0], live[1]), (live[1], live[0])] {
                    let f = others(x, &|j| !live.contains(&j));
                    let s1 = self.matvec(&self.ek, &reach[y]);
                    let s0 = self.matvec(&self.kk, &reach[y]);
                    let c = contrib[x] as f64;
                    match kind {
                        TermKind::AllIn => {
                            for h in 0..H {
                                out[x][h] = f * (s1[h] * (pot - rake) - c * s0[h]);
                            }
                        }
                        _ => {
                            let role = Role { pot: pot_type(level), ip: postflop_rank(x) > postflop_rank(y) };
                            let r = self.real.get(role);
                            for h in 0..H {
                                out[x][h] = f * (r[h] * s1[h] * pot - c * s0[h]);
                            }
                        }
                    }
                }
            }
            _ => {
                // three-way approximation: equity ~ product of heads-up equities, normalised
                let n = live.len();
                let mut q: Vec<[f64; H]> = Vec::new();
                let mut m: Vec<[f64; H]> = Vec::new();
                for &x in live {
                    let mut qx = [1.0; H];
                    let mut mx = [1.0; H];
                    for &y in live {
                        if y == x {
                            continue;
                        }
                        let s1 = self.matvec(&self.ek, &reach[y]);
                        let s0 = self.matvec(&self.kk, &reach[y]);
                        for h in 0..H {
                            qx[h] *= if s0[h] > 0.0 { s1[h] / s0[h] } else { 0.5 };
                            mx[h] *= s0[h];
                        }
                    }
                    q.push(qx);
                    m.push(mx);
                }
                let z: f64 = (0..n)
                    .map(|k| {
                        let x = live[k];
                        let mass = self.mass(&reach[x]).max(1e-12);
                        (0..H).map(|h| self.w[h] * reach[x][h] * q[k][h]).sum::<f64>() / mass
                    })
                    .sum::<f64>()
                    .max(1e-9);
                let rake = (self.rake_pct * pot).min(self.rake_cap);
                let mut order: Vec<usize> = live.to_vec();
                order.sort_by_key(|&x| postflop_rank(x));
                for (k, &x) in live.iter().enumerate() {
                    let f = others(x, &|j| !live.contains(&j));
                    let c = contrib[x] as f64;
                    let seat_factor = match order.iter().position(|&y| y == x).unwrap() {
                        0 => 0.90,
                        1 => 0.97,
                        _ => 1.05,
                    };
                    let role = Role { pot: pot_type(level), ip: true };
                    let r = self.real.get(role);
                    for h in 0..H {
                        let share = q[k][h] / z;
                        let v = match kind {
                            TermKind::AllIn => share * (pot - rake),
                            _ => seat_factor * (r[h] / r_mean(r)) * 0.95 * share * pot,
                        };
                        out[x][h] = f * m[k][h] * (v - c);
                    }
                }
            }
        }
        out
    }

    /// Expected value per player (bb per hand) under the average strategy, and the gain each
    /// player could get by deviating to a best response (a convergence gauge).
    pub fn evaluate(&self) -> (Vec<f64>, Vec<f64>) {
        let reach = Box::new([[1.0; H]; NP]);
        let avg = self.walk(0, &reach, Mode::Average, 0);
        let ev: Vec<f64> = (0..NP).map(|i| (0..H).map(|h| self.w[h] * avg[i][h]).sum::<f64>() / 100.0).collect();
        let gains: Vec<f64> = (0..NP)
            .map(|i| {
                let br = self.walk(0, &reach, Mode::BestResponse(i), 0);
                (0..H).map(|h| self.w[h] * br[i][h]).sum::<f64>() / 100.0 - ev[i]
            })
            .collect();
        (ev, gains)
    }

    /// Per-action EV (bb) for each class at a decision node for the acting player, under the
    /// average strategy, normalised by the opponents' probability of reaching the node.
    pub fn node_action_evs(&self, id: usize) -> Vec<f64> {
        // computed by walking from the root with average strategies and recording at `id`
        let reach = Box::new([[1.0; H]; NP]);
        let mut path = Vec::new();
        self.find_path(0, id, &mut path);
        let mut r = *reach;
        for &(node, action) in &path {
            if let Node::Decision { player, .. } = &self.tree.nodes[node] {
                let s = self.average_strategy(node);
                for h in 0..H {
                    r[*player][h] *= s[action * H + h];
                }
            }
        }
        let Node::Decision { player, children, .. } = &self.tree.nodes[id] else { return vec![] };
        let p = *player;
        let opp_mass: f64 = (0..NP).filter(|&j| j != p).map(|j| self.mass(&r[j])).product::<f64>().max(1e-12);
        let mut out = Vec::new();
        for &c in children {
            let v = self.walk(c, &Box::new(r), Mode::Average, 10);
            for h in 0..H {
                out.push(v[p][h] / opp_mass / 100.0);
            }
        }
        out
    }

    fn find_path(&self, from: usize, target: usize, path: &mut Vec<(usize, usize)>) -> bool {
        if from == target {
            return true;
        }
        if let Node::Decision { children, .. } = &self.tree.nodes[from] {
            for (i, &c) in children.iter().enumerate() {
                path.push((from, i));
                if self.find_path(c, target, path) {
                    return true;
                }
                path.pop();
            }
        }
        false
    }

    /// Probability (from the root, all hands) that play reaches each node under the average
    /// strategy.
    pub fn node_reach(&self) -> Vec<f64> {
        let mut out = vec![0.0; self.tree.nodes.len()];
        let reach = [[1.0; H]; NP];
        self.reach_rec(0, &reach, &mut out);
        out
    }

    fn reach_rec(&self, id: usize, r: &Vec6, out: &mut Vec<f64>) {
        out[id] = (0..NP).map(|j| self.mass(&r[j])).product();
        if let Node::Decision { player, children, .. } = &self.tree.nodes[id] {
            let s = self.average_strategy(id);
            for (i, &c) in children.iter().enumerate() {
                let mut r2 = *r;
                for h in 0..H {
                    r2[*player][h] *= s[i * H + h];
                }
                self.reach_rec(c, &r2, out);
            }
        }
    }

    /// The reach-weighted range (per class probability) of `player` at node `id`.
    pub fn range_at(&self, id: usize, player: usize) -> Vec<f64> {
        let mut path = Vec::new();
        self.find_path(0, id, &mut path);
        let mut r = vec![1.0; H];
        for &(node, action) in &path {
            if let Node::Decision { player: p, .. } = &self.tree.nodes[node] {
                if *p == player {
                    let s = self.average_strategy(node);
                    for h in 0..H {
                        r[h] *= s[action * H + h];
                    }
                }
            }
        }
        r
    }

    pub fn equity_table(&self) -> &EquityTable {
        self.eq
    }
}

fn r_mean(r: &[f64]) -> f64 {
    r.iter().sum::<f64>() / r.len() as f64
}
