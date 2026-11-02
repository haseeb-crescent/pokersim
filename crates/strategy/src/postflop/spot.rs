//! Description of a heads-up postflop spot, its suit-canonical form and the game tree presets.
//!
//! Trees (chosen from measurements on an M5 Max, see docs/SOLVER.md):
//! - `Flop`: flop 33% / 75% with bet-raise-jam; turn and river one size (66% / 75%) with
//!   bet-jam. The turn and river are re-solved on the real cards, so the flop tree only needs
//!   coarse continuation values for later streets.
//! - `Turn`: turn 33% / 75% / 125%, river 50% / 100%, bet-raise-jam.
//! - `River`: 33% / 75% / 150%, up to three bets.
//! - `Calibration`: one size per street, used offline to calibrate the preflop model.

use engine::card::*;
use engine::Range;
use postflop_solver as ps;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TreePreset {
    Flop,
    Turn,
    River,
    Calibration,
}

impl TreePreset {
    fn sizes(self) -> ([&'static str; 3], [i32; 3]) {
        match self {
            TreePreset::Flop => (["33%,75%", "66%", "75%"], [2, 1, 1]),
            TreePreset::Turn => (["33%", "33%,75%,125%", "50%,100%"], [2, 2, 2]),
            TreePreset::River => (["33%", "66%", "33%,75%,150%"], [2, 2, 3]),
            TreePreset::Calibration => (["33%", "66%", "75%"], [2, 1, 1]),
        }
    }

    pub fn id(self) -> u8 {
        self as u8
    }
}

/// A postflop action with chip amounts as postflop-solver uses them (bet and raise amounts are
/// the player's total for the street).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "amount")]
pub enum LineAction {
    Fold,
    Check,
    Call,
    Bet(i32),
    Raise(i32),
    AllIn(i32),
}

impl LineAction {
    pub fn from_ps(a: ps::Action) -> Option<LineAction> {
        Some(match a {
            ps::Action::Fold => LineAction::Fold,
            ps::Action::Check => LineAction::Check,
            ps::Action::Call => LineAction::Call,
            ps::Action::Bet(x) => LineAction::Bet(x),
            ps::Action::Raise(x) => LineAction::Raise(x),
            ps::Action::AllIn(x) => LineAction::AllIn(x),
            _ => return None,
        })
    }

    pub fn to_ps(self) -> ps::Action {
        match self {
            LineAction::Fold => ps::Action::Fold,
            LineAction::Check => ps::Action::Check,
            LineAction::Call => ps::Action::Call,
            LineAction::Bet(x) => ps::Action::Bet(x),
            LineAction::Raise(x) => ps::Action::Raise(x),
            LineAction::AllIn(x) => ps::Action::AllIn(x),
        }
    }

    pub fn amount(self) -> Option<i32> {
        match self {
            LineAction::Bet(x) | LineAction::Raise(x) | LineAction::AllIn(x) => Some(x),
            _ => None,
        }
    }

    pub fn is_aggressive(self) -> bool {
        self.amount().is_some()
    }

    pub fn label(self) -> String {
        match self {
            LineAction::Fold => "Fold".into(),
            LineAction::Check => "Check".into(),
            LineAction::Call => "Call".into(),
            LineAction::Bet(x) => format!("Bet {}", fmt_bb(x)),
            LineAction::Raise(x) => format!("Raise to {}", fmt_bb(x)),
            LineAction::AllIn(x) => format!("All-in {}", fmt_bb(x)),
        }
    }
}

pub fn fmt_bb(chips: i32) -> String {
    let bb = chips as f64 / 100.0;
    if (bb - bb.round()).abs() < 1e-9 {
        format!("{bb:.0}bb")
    } else {
        format!("{bb:.1}bb")
    }
}

/// A suit permutation: `map[real suit] = canonical suit`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SuitPerm(pub [u8; 4]);

impl SuitPerm {
    pub fn identity() -> Self {
        SuitPerm([0, 1, 2, 3])
    }
    #[inline]
    pub fn card(&self, c: Card) -> Card {
        make_card(rank(c), self.0[suit(c) as usize])
    }
    pub fn inverse(&self) -> SuitPerm {
        let mut inv = [0u8; 4];
        for s in 0..4 {
            inv[self.0[s] as usize] = s as u8;
        }
        SuitPerm(inv)
    }
    pub fn range(&self, r: &Range) -> Range {
        let mut out = Range::empty();
        for i in 0..NUM_COMBOS {
            if r.w[i] != 0.0 {
                let (a, b) = combo_cards(i);
                out.set(self.card(a), self.card(b), r.w[i]);
            }
        }
        out
    }
    pub fn all() -> Vec<SuitPerm> {
        let mut out = Vec::new();
        for a in 0..4u8 {
            for b in 0..4u8 {
                for c in 0..4u8 {
                    for d in 0..4u8 {
                        let p = [a, b, c, d];
                        let mut s = p;
                        s.sort();
                        if s == [0, 1, 2, 3] {
                            out.push(SuitPerm(p));
                        }
                    }
                }
            }
        }
        out
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SpotSpec {
    /// 3 to 5 board cards (flop first)
    pub board: Vec<Card>,
    /// ranges of [OOP, IP]
    pub ranges: [Range; 2],
    /// pot at the start of the street, chips
    pub pot: i32,
    /// effective stack behind, chips
    pub stack: i32,
    pub preset: TreePreset,
    pub rake_pct: f64,
    pub rake_cap: f64,
    /// extra lines from the root of this street (real chip amounts)
    pub extra_lines: Vec<Vec<LineAction>>,
    /// node locks applied before solving (Hunter and exploit view)
    #[serde(default)]
    pub locks: Vec<LockSpec>,
    /// card removal from players who folded preflop (river re-solves only; see hand.rs)
    #[serde(skip)]
    pub bunching: Option<Bunching>,
}

/// Shared, precomputed bunching-effect data for one flop and set of folded ranges.
#[derive(Clone)]
pub struct Bunching(pub std::sync::Arc<ps::BunchingData>);

impl std::fmt::Debug for Bunching {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Bunching")
    }
}

/// A locked strategy at one node: the acting player's mix for each listed hand (real cards).
/// Hands not listed stay free for the solver to optimise.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LockSpec {
    pub line: Vec<LineAction>,
    pub hands: Vec<(Card, Card)>,
    /// actions x hands, in the node's action order
    pub strategy: Vec<f32>,
}

impl SpotSpec {
    pub fn street(&self) -> ps::BoardState {
        match self.board.len() {
            3 => ps::BoardState::Flop,
            4 => ps::BoardState::Turn,
            _ => ps::BoardState::River,
        }
    }

    /// The canonical suit permutation: the one that yields the smallest (sorted flop, turn,
    /// river) card sequence, then the smallest ranges. Isomorphic spots share a cache entry.
    pub fn canonical_perm(&self) -> SuitPerm {
        // bunching data is computed for the real flop, so keep real suits
        if self.bunching.is_some() {
            return SuitPerm::identity();
        }
        let mut best: Option<(Vec<Card>, SuitPerm)> = None;
        for p in SuitPerm::all() {
            let mut flop: Vec<Card> = self.board[..3].iter().map(|&c| p.card(c)).collect();
            flop.sort();
            let mut key = flop;
            key.extend(self.board[3..].iter().map(|&c| p.card(c)));
            match &best {
                Some((k, _)) if *k <= key => {}
                _ => best = Some((key, p)),
            }
        }
        // among permutations giving the same board, prefer the one with the smallest ranges
        let (bkey, mut bp) = best.unwrap();
        let mut brange: Option<Vec<u16>> = None;
        for p in SuitPerm::all() {
            let mut flop: Vec<Card> = self.board[..3].iter().map(|&c| p.card(c)).collect();
            flop.sort();
            let mut key = flop;
            key.extend(self.board[3..].iter().map(|&c| p.card(c)));
            if key != bkey {
                continue;
            }
            let q: Vec<u16> = self.ranges.iter().flat_map(|r| quantize(&p.range(r))).collect();
            if brange.as_ref().map_or(true, |b| q < *b) {
                brange = Some(q);
                bp = p;
            }
        }
        bp
    }

    pub fn permuted(&self, p: SuitPerm) -> SpotSpec {
        SpotSpec {
            board: self.board.iter().map(|&c| p.card(c)).collect(),
            ranges: [p.range(&self.ranges[0]), p.range(&self.ranges[1])],
            locks: self
                .locks
                .iter()
                .map(|l| LockSpec { line: l.line.clone(), hands: l.hands.iter().map(|&(a, b)| (p.card(a), p.card(b))).collect(), strategy: l.strategy.clone() })
                .collect(),
            ..self.clone()
        }
    }

    /// Stable cache key of the spec (already canonical).
    pub fn key(&self) -> String {
        let mut h = Fnv::new();
        h.write(b"v1");
        if self.bunching.is_some() {
            h.write(b"bunching");
        }
        h.write(&[self.preset.id()]);
        h.write(&self.pot.to_le_bytes());
        h.write(&self.stack.to_le_bytes());
        h.write(&self.rake_pct.to_le_bytes());
        h.write(&self.rake_cap.to_le_bytes());
        let mut flop = self.board[..3].to_vec();
        flop.sort();
        h.write(&flop);
        h.write(&self.board[3..]);
        for r in &self.ranges {
            for q in quantize(r) {
                h.write(&q.to_le_bytes());
            }
        }
        for line in &self.extra_lines {
            h.write(serde_json::to_string(line).unwrap().as_bytes());
        }
        for lock in &self.locks {
            h.write(b"lock");
            h.write(serde_json::to_string(&lock.line).unwrap().as_bytes());
            for (i, &(a, b)) in lock.hands.iter().enumerate() {
                h.write(&[a, b]);
                let n = lock.hands.len();
                for k in 0..lock.strategy.len() / n.max(1) {
                    h.write(&((lock.strategy[k * n + i] * 1000.0) as i32).to_le_bytes());
                }
            }
        }
        let mut shown = self.board[..3].to_vec();
        shown.sort();
        shown.extend_from_slice(&self.board[3..]);
        format!("{}-{:016x}", cards_to_string(&shown), h.finish())
    }

    pub fn build_game(&self) -> Result<ps::PostFlopGame, String> {
        let to_ps = |r: &Range| {
            let mut out = ps::Range::new();
            for i in 0..NUM_COMBOS {
                let w = r.w[i];
                if w > 0.0 {
                    let (a, b) = combo_cards(i);
                    out.set_weight_by_cards(a, b, w.min(1.0));
                }
            }
            out
        };
        let mut flop = [self.board[0], self.board[1], self.board[2]];
        flop.sort();
        let card_config = ps::CardConfig {
            range: [to_ps(&self.ranges[0]), to_ps(&self.ranges[1])],
            flop,
            turn: self.board.get(3).copied().unwrap_or(ps::NOT_DEALT),
            river: self.board.get(4).copied().unwrap_or(ps::NOT_DEALT),
        };
        let (sizes, caps) = self.preset.sizes();
        let bs = |s: &str| ps::BetSizeOptions::try_from((s, "3x")).unwrap();
        let tree_config = ps::TreeConfig {
            initial_state: self.street(),
            starting_pot: self.pot,
            effective_stack: self.stack,
            rake_rate: self.rake_pct,
            rake_cap: self.rake_cap,
            flop_bet_sizes: [bs(sizes[0]), bs(sizes[0])],
            turn_bet_sizes: [bs(sizes[1]), bs(sizes[1])],
            river_bet_sizes: [bs(sizes[2]), bs(sizes[2])],
            turn_donk_sizes: None,
            river_donk_sizes: None,
            add_allin_threshold: 1.5,
            force_allin_threshold: 0.15,
            merging_threshold: 0.1,
            max_bets_per_street: caps,
        };
        let mut tree = ps::ActionTree::new(tree_config)?;
        for line in &self.extra_lines {
            let line: Vec<ps::Action> = line.iter().map(|a| a.to_ps()).collect();
            // ignore lines that already exist or are invalid
            let _ = tree.add_line(&line);
        }
        ps::PostFlopGame::with_config(card_config, tree)
    }
}

pub fn quantize(r: &Range) -> Vec<u16> {
    r.w.iter().map(|&w| (w.clamp(0.0, 1.0) * 10_000.0).round() as u16).collect()
}

struct Fnv(u64);
impl Fnv {
    fn new() -> Self {
        Fnv(0xcbf2_9ce4_8422_2325)
    }
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 ^= b as u64;
            self.0 = self.0.wrapping_mul(0x0100_0000_01b3);
        }
    }
    fn finish(&self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(board: &str, oop: &str, ip: &str) -> SpotSpec {
        SpotSpec {
            board: cards_from_str(board).unwrap(),
            ranges: [Range::parse(oop).unwrap(), Range::parse(ip).unwrap()],
            pot: 550,
            stack: 9750,
            preset: TreePreset::Flop,
            rake_pct: 0.05,
            rake_cap: 300.0,
            extra_lines: vec![],
            locks: vec![],
            bunching: None,
        }
    }

    fn key(s: &SpotSpec) -> String {
        s.permuted(s.canonical_perm()).key()
    }

    #[test]
    fn isomorphic_flops_share_a_key() {
        // suit-symmetric ranges: any suit relabelling of the board is the same spot
        let a = spec("Kh7d2c", "22+,A2s+,KTo+", "22+,ATs+,KQo");
        let b = spec("Ks7c2h", "22+,A2s+,KTo+", "22+,ATs+,KQo");
        let c = spec("7d2cKh", "22+,A2s+,KTo+", "22+,ATs+,KQo");
        assert_eq!(key(&a), key(&b));
        assert_eq!(key(&a), key(&c));
        // a different texture (two-tone) is a different spot
        let d = spec("Kh7h2c", "22+,A2s+,KTo+", "22+,ATs+,KQo");
        assert_ne!(key(&a), key(&d));
        // different ranges, different key
        let e = spec("Kh7d2c", "22+,A2s+", "22+,ATs+,KQo");
        assert_ne!(key(&a), key(&e));
    }

    #[test]
    fn asymmetric_ranges_follow_the_permutation() {
        // a range holding one specific combo: isomorphic boards with the combo relabelled
        // the same way must match, but not with the combo in another suit
        let a = spec("Kh7d2c", "AhAd", "QQ");
        let b = spec("Ks7c2h", "AsAc", "QQ");
        let c = spec("Ks7c2h", "AhAd", "QQ");
        assert_eq!(key(&a), key(&b));
        assert_ne!(key(&a), key(&c));
    }

    #[test]
    fn perm_inverse_roundtrip() {
        for p in SuitPerm::all() {
            let inv = p.inverse();
            for c in 0..52u8 {
                assert_eq!(inv.card(p.card(c)), c);
            }
        }
    }

    #[test]
    fn nearest_action_mapping() {
        use super::super::worker::match_action;
        let acts = vec![ps::Action::Check, ps::Action::Bet(180), ps::Action::Bet(410), ps::Action::AllIn(9750)];
        // 200 into 550 is closest to the 33% bet
        assert_eq!(match_action(&acts, LineAction::Bet(200), 550).unwrap().1, LineAction::Bet(180));
        // 500 is closest to the 75% bet
        assert_eq!(match_action(&acts, LineAction::Bet(500), 550).unwrap().1, LineAction::Bet(410));
        // any all-in maps to the tree's all-in
        assert_eq!(match_action(&acts, LineAction::AllIn(12000), 550).unwrap().1, LineAction::AllIn(9750));
        assert_eq!(match_action(&acts, LineAction::Check, 550).unwrap().0, 0);
        assert!(match_action(&acts, LineAction::Call, 550).is_none());
    }
}
