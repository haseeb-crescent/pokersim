//! The abstract 6-max preflop game the preflop solver works on.
//!
//! Positions in action order: UTG, HJ, CO, BTN, SB, BB. Stacks 100bb (1bb = 100 chips).
//! Abstraction (sizes are "raise to"):
//! - open 2.5bb (SB 3bb); only the SB may limp; the BB may isolate a SB limp to 4bb
//! - 3-bet: 3x in position, 4x from the SB, 4.4x from the BB (3x when the BB 3-bets the SB),
//!   plus one open size per cold caller (squeeze)
//! - 4-bet: 2.25x in position, 2.5x out of position; 5-bet: all-in
//! - cold calls of an open are allowed while at most three players would see the flop;
//!   calling a 3-bet or more is only allowed for players who already put money in voluntarily
//!
//! Every decision node is identified by its action sequence, e.g. `F-F-R-C-F` (UTG folds, HJ
//! folds, CO opens, BTN calls, SB folds; the BB is to act). Codes: F fold, K check, C call,
//! R raise (the size follows from the sequence), A all-in.

use serde::{Deserialize, Serialize};

pub const NP: usize = 6;
pub const STACK: i32 = 10_000;
pub const POS: [&str; NP] = ["UTG", "HJ", "CO", "BTN", "SB", "BB"];
pub const SB: usize = 4;
pub const BB: usize = 5;

/// Postflop acting order rank (lower acts first): SB, BB, UTG, HJ, CO, BTN.
pub fn postflop_rank(p: usize) -> usize {
    match p {
        SB => 0,
        BB => 1,
        x => x + 2,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PfAction {
    Fold,
    Check,
    Call,
    Raise(i32),
    AllIn,
}

impl PfAction {
    pub fn code(self) -> char {
        match self {
            PfAction::Fold => 'F',
            PfAction::Check => 'K',
            PfAction::Call => 'C',
            PfAction::Raise(_) => 'R',
            PfAction::AllIn => 'A',
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TermKind {
    Fold,
    AllIn,
    Flop,
}

#[derive(Clone, Debug)]
pub enum Node {
    Decision { player: usize, actions: Vec<PfAction>, children: Vec<usize>, key: String, level: u8 },
    Terminal { kind: TermKind, contrib: [i32; NP], live: Vec<usize>, level: u8, key: String },
}

#[derive(Clone, Debug)]
struct St {
    contrib: [i32; NP],
    folded: [bool; NP],
    voluntary: [bool; NP],
    /// raise level at which the player last acted, -1 if not yet
    acted: [i8; NP],
    level: u8,
    bet: i32,
    /// size of the last raise "to" amounts, used to size the next raise
    open_size: i32,
    three_size: i32,
    callers_of_open: u8,
    raisers: Vec<usize>,
    seq: Vec<char>,
}

impl St {
    fn root() -> St {
        let mut contrib = [0; NP];
        contrib[SB] = 50;
        contrib[BB] = 100;
        St {
            contrib,
            folded: [false; NP],
            voluntary: [false; NP],
            acted: [-1; NP],
            level: 0,
            bet: 100,
            open_size: 0,
            three_size: 0,
            callers_of_open: 0,
            raisers: vec![],
            seq: vec![],
        }
    }

    fn all_in(&self, p: usize) -> bool {
        self.contrib[p] >= STACK
    }

    fn needs_action(&self, p: usize) -> bool {
        !self.folded[p] && !self.all_in(p) && (self.acted[p] < self.level as i8 || self.contrib[p] < self.bet)
    }

    fn live(&self) -> Vec<usize> {
        (0..NP).filter(|&p| !self.folded[p]).collect()
    }

    fn key(&self) -> String {
        self.seq.iter().map(|c| c.to_string()).collect::<Vec<_>>().join("-")
    }
}

/// Is `p` in position (acts later postflop) relative to `q`?
fn ip(p: usize, q: usize) -> bool {
    postflop_rank(p) > postflop_rank(q)
}

fn raise_to(st: &St, p: usize) -> i32 {
    let r = match st.level {
        0 => {
            if p == SB {
                300
            } else if p == BB {
                400
            } else {
                250
            }
        }
        1 => {
            let opener = st.raisers[0];
            let base = if p == BB && opener == SB {
                3.0
            } else if ip(p, opener) {
                3.0
            } else if p == SB {
                4.0
            } else {
                4.4
            };
            (st.open_size as f64 * (base + st.callers_of_open as f64)).round() as i32
        }
        2 => {
            let three_bettor = *st.raisers.last().unwrap();
            let m = if ip(p, three_bettor) { 2.25 } else { 2.5 };
            (st.three_size as f64 * m).round() as i32
        }
        _ => STACK,
    };
    let r = (r / 5) * 5;
    if r as f64 >= 0.42 * STACK as f64 {
        STACK
    } else {
        r
    }
}

pub struct Tree {
    pub nodes: Vec<Node>,
}

impl Tree {
    pub fn build() -> Tree {
        let mut t = Tree { nodes: Vec::new() };
        t.build_rec(St::root(), 0);
        t
    }

    fn next_actor(st: &St, from: usize) -> Option<usize> {
        let live = st.live();
        if live.len() <= 1 {
            return None;
        }
        let can_act: Vec<usize> = live.iter().copied().filter(|&p| !st.all_in(p)).collect();
        if can_act.is_empty() {
            return None;
        }
        if can_act.len() == 1 && st.contrib[can_act[0]] >= st.bet {
            return None;
        }
        (0..NP).map(|k| (from + k) % NP).find(|&p| st.needs_action(p))
    }

    fn build_rec(&mut self, st: St, from: usize) -> usize {
        let id = self.nodes.len();
        let Some(p) = Self::next_actor(&st, from) else {
            let live = st.live();
            let kind = if live.len() == 1 {
                TermKind::Fold
            } else if live.iter().any(|&q| st.all_in(q)) {
                TermKind::AllIn
            } else {
                TermKind::Flop
            };
            self.nodes.push(Node::Terminal { kind, contrib: st.contrib, live, level: st.level, key: st.key() });
            return id;
        };
        // placeholder, filled after children are built
        self.nodes.push(Node::Terminal { kind: TermKind::Fold, contrib: [0; NP], live: vec![], level: 0, key: String::new() });

        let mut actions = Vec::new();
        let facing = st.contrib[p] < st.bet;
        let in_pot_voluntary = (0..NP).filter(|&q| !st.folded[q] && st.voluntary[q]).count();
        if facing {
            actions.push(PfAction::Fold);
            let may_call = match st.level {
                0 => p == SB,
                1 => in_pot_voluntary + 1 <= 3 || st.voluntary[p],
                _ => st.voluntary[p] && (0..NP).filter(|&q| !st.folded[q] && (st.voluntary[q] || q == p)).count() <= 3,
            };
            if may_call {
                actions.push(PfAction::Call);
            }
        } else {
            actions.push(PfAction::Check);
        }
        let others_can_respond = (0..NP).any(|q| q != p && !st.folded[q] && !st.all_in(q));
        if st.level < 4 && others_can_respond {
            let to = raise_to(&st, p);
            if to >= STACK {
                actions.push(PfAction::AllIn);
            } else {
                actions.push(PfAction::Raise(to));
            }
        }

        let mut children = Vec::new();
        for &a in &actions {
            let mut s = st.clone();
            s.seq.push(a.code());
            s.acted[p] = s.level as i8;
            match a {
                PfAction::Fold => s.folded[p] = true,
                PfAction::Check => {}
                PfAction::Call => {
                    s.contrib[p] = s.bet.min(STACK);
                    s.voluntary[p] = true;
                    if s.level == 1 {
                        s.callers_of_open += 1;
                    }
                }
                PfAction::Raise(_) | PfAction::AllIn => {
                    let to = if let PfAction::Raise(x) = a { x } else { STACK };
                    s.contrib[p] = to;
                    s.bet = to;
                    s.voluntary[p] = true;
                    s.level += 1;
                    s.acted[p] = s.level as i8;
                    s.raisers.push(p);
                    match s.level {
                        1 => s.open_size = to,
                        2 => s.three_size = to,
                        _ => {}
                    }
                }
            }
            let child = self.build_rec(s, (p + 1) % NP);
            children.push(child);
        }
        self.nodes[id] = Node::Decision { player: p, actions, children, key: st.key(), level: st.level };
        id
    }

    pub fn decision_count(&self) -> usize {
        self.nodes.iter().filter(|n| matches!(n, Node::Decision { .. })).count()
    }
}

/// Pot category of a flop terminal, used to pick realization factors.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub enum PotType {
    Limped,
    Single,
    ThreeBet,
    FourBet,
}

pub fn pot_type(level: u8) -> PotType {
    match level {
        0 => PotType::Limped,
        1 => PotType::Single,
        2 => PotType::ThreeBet,
        _ => PotType::FourBet,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tree_shape() {
        let t = Tree::build();
        let n = t.decision_count();
        assert!(n > 100, "{n}");
        // root is UTG with fold / open
        match &t.nodes[0] {
            Node::Decision { player, actions, .. } => {
                assert_eq!(*player, 0);
                assert_eq!(actions, &vec![PfAction::Fold, PfAction::Raise(250)]);
            }
            _ => panic!(),
        }
        // folded to the SB: fold / limp / raise 3bb
        let sb = t.nodes.iter().find(|n| matches!(n, Node::Decision { key, .. } if key == "F-F-F-F")).unwrap();
        match sb {
            Node::Decision { player, actions, .. } => {
                assert_eq!(*player, SB);
                assert_eq!(actions, &vec![PfAction::Fold, PfAction::Call, PfAction::Raise(300)]);
            }
            _ => panic!(),
        }
        // BB vs BTN open: 3-bet to 11bb
        let bb = t.nodes.iter().find(|n| matches!(n, Node::Decision { key, .. } if key == "F-F-F-R-F")).unwrap();
        match bb {
            Node::Decision { player, actions, .. } => {
                assert_eq!(*player, BB);
                assert_eq!(actions, &vec![PfAction::Fold, PfAction::Call, PfAction::Raise(1100)]);
            }
            _ => panic!(),
        }
    }
}
