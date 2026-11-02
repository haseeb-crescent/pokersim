//! Postflop solving for one heads-up hand, street by street.
//!
//! - Flop: solved on the actual board from both players' preflop chart ranges (cached by
//!   canonical spot).
//! - Turn and river: re-solved on the actual runout from the reach-weighted ranges at the end
//!   of the previous street.
//! - Off-tree sizes: on the turn and river the street is re-solved with the exact size added
//!   to the tree; on the flop the nearest tree size is used and reported.
//! - Gradeability: a player's actual hand must be in that player's range with positive weight
//!   for the solver to have a strategy for it. Variants with the actual hand added at a small
//!   floor weight are solved on demand, separately per viewer: the coach's variant floors only
//!   the hero's hand and a bot's variant floors only that bot's own hand, so no strategy a bot
//!   uses ever depends on cards it cannot see. On the flop, where a variant costs a full flop
//!   solve, bots instead borrow the strategy of the in-range hand with the closest equity.

use super::cache::SolveCache;
use super::spot::*;
use super::worker::*;
use engine::card::*;
use engine::Range;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Weight below which a hand counts as absent from a range.
pub const ABSENT: f32 = 1e-6;
/// Floor weight (relative to the largest weight in the range) given to an actual hand.
pub const FLOOR: f32 = 0.002;

pub struct StreetSolve {
    pub board: Vec<Card>,
    pub pot: i32,
    pub stack: i32,
    pub preset: TreePreset,
    pub ranges: [Range; 2],
    pub public: Arc<Spot>,
    variants: Mutex<HashMap<String, Arc<Spot>>>,
}

pub struct HandSolve {
    /// table seats of [OOP, IP]
    pub seats: [usize; 2],
    streets: Mutex<Vec<Arc<StreetSolve>>>,
    cache: Arc<SolveCache>,
    rake: (f64, f64),
    /// bunching data for river re-solves, computed in the background from the flop on
    bunching: Arc<Mutex<Option<Bunching>>>,
}

/// Most folded players for which bunching data is computed live (4 takes minutes).
pub const MAX_BUNCHING_FOLDERS: usize = 3;

/// Who is asking: determines which actual hand may be floored into the ranges.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Viewer {
    /// a bot asking about its own hand
    Bot,
    /// the coach asking about the hero's hand
    Coach,
}

pub struct SpotChoice {
    pub spot: Arc<Spot>,
    /// the line to query inside `spot` (identical to the request unless re-solved)
    pub line: Vec<LineAction>,
    /// when the hand is not in the solved range: the in-range hand whose strategy is used
    pub proxy: Option<(Card, Card)>,
    pub notes: Vec<String>,
}

impl HandSolve {
    /// `fold_ranges`: chart ranges of the players who folded preflop (for bunching).
    pub fn start_flop(seats: [usize; 2], board: Vec<Card>, ranges: [Range; 2], pot: i32, stack: i32, cache: Arc<SolveCache>, rake: (f64, f64), fold_ranges: Vec<Range>) -> HandSolve {
        let hs = HandSolve { seats, streets: Mutex::new(Vec::new()), cache, rake, bunching: Arc::new(Mutex::new(None)) };
        let folds: Vec<Range> = fold_ranges.into_iter().filter(|r| !r.is_empty()).collect();
        if !folds.is_empty() && folds.len() <= MAX_BUNCHING_FOLDERS {
            let slot = hs.bunching.clone();
            let flop = [board[0], board[1], board[2]];
            std::thread::spawn(move || {
                let to_ps = |r: &Range| {
                    let mut out = postflop_solver::Range::new();
                    for i in 0..NUM_COMBOS {
                        if r.w[i] > 0.0 {
                            let (a, b) = combo_cards(i);
                            out.set_weight_by_cards(a, b, r.w[i].min(1.0));
                        }
                    }
                    out
                };
                let ranges: Vec<postflop_solver::Range> = folds.iter().map(to_ps).collect();
                if let Ok(mut data) = postflop_solver::BunchingData::new(&ranges, flop) {
                    data.process(false);
                    *slot.lock().unwrap() = Some(Bunching(Arc::new(data)));
                }
            });
        }
        hs.push_street(board, ranges, pot, stack);
        hs
    }

    /// Whether river re-solves of this hand include the bunching effect.
    pub fn has_bunching(&self) -> bool {
        self.bunching.lock().unwrap().is_some()
    }

    fn preset_for(board_len: usize) -> TreePreset {
        match board_len {
            3 => TreePreset::Flop,
            4 => TreePreset::Turn,
            _ => TreePreset::River,
        }
    }

    fn spec(&self, board: &[Card], ranges: [Range; 2], pot: i32, stack: i32, extra: Vec<Vec<LineAction>>) -> SpotSpec {
        let mut ranges = ranges;
        let dead = mask_of(board);
        ranges[0].remove_dead(dead);
        ranges[1].remove_dead(dead);
        let bunching = if board.len() == 5 { self.bunching.lock().unwrap().clone() } else { None };
        SpotSpec { board: board.to_vec(), ranges, pot, stack, preset: Self::preset_for(board.len()), rake_pct: self.rake.0, rake_cap: self.rake.1, extra_lines: extra, locks: vec![], bunching }
    }

    fn push_street(&self, board: Vec<Card>, ranges: [Range; 2], pot: i32, stack: i32) {
        let spec = self.spec(&board, ranges, pot, stack, vec![]);
        let preset = spec.preset;
        let ranges = spec.ranges.clone();
        let public = Spot::start(spec, self.cache.clone(), SolveTargets::for_preset(preset));
        self.streets.lock().unwrap().push(Arc::new(StreetSolve { board, pot, stack, preset, ranges, public, variants: Mutex::new(HashMap::new()) }));
    }

    pub fn current(&self) -> Arc<StreetSolve> {
        self.streets.lock().unwrap().last().unwrap().clone()
    }

    pub fn street(&self, idx: usize) -> Option<Arc<StreetSolve>> {
        self.streets.lock().unwrap().get(idx).cloned()
    }

    /// Index of the current street solve (0 = flop).
    pub fn current_index(&self) -> usize {
        self.streets.lock().unwrap().len() - 1
    }

    /// Moves to the next street. `line` is the completed action line of the current street.
    /// Solves of the finished street keep refining for the coach, in the background.
    pub fn advance(&self, board: Vec<Card>, line: &[LineAction], pot: i32, stack: i32) -> Result<(), String> {
        let cur = self.current();
        cur.public.set_background(true);
        for v in cur.variants.lock().unwrap().values() {
            v.set_background(true);
        }
        // the public ranges follow the public solve (re-solved if the line was off-tree)
        let (spot, l) = self.public_spot_for_line(&cur, line);
        let ranges = spot.ranges_after(&l)?;
        self.push_street(board, ranges, pot, stack);
        Ok(())
    }

    /// The public solve to use for a line: the street's public solve, or on the turn/river a
    /// re-solve with the line's off-tree sizes added to the tree.
    fn public_spot_for_line(&self, st: &StreetSolve, line: &[LineAction]) -> (Arc<Spot>, Vec<LineAction>) {
        let extra = self.off_tree_prefixes(st, &st.public, line);
        if extra.is_empty() || st.preset == TreePreset::Flop {
            return (st.public.clone(), line.to_vec());
        }
        let spec = self.spec(&st.board, st.ranges.clone(), st.pot, st.stack, extra);
        (self.variant(st, spec, false), line.to_vec())
    }

    /// Prefixes of `line` ending at each action the tree does not contain exactly.
    fn off_tree_prefixes(&self, _st: &StreetSolve, spot: &Spot, line: &[LineAction]) -> Vec<Vec<LineAction>> {
        let mut out = Vec::new();
        for k in 0..line.len() {
            if !line[k].is_aggressive() {
                continue;
            }
            let prefix: Vec<LineAction> = line[..k].to_vec();
            if let Ok(v) = spot.node(&prefix, false) {
                if !v.actions.iter().any(|a| a.action == line[k]) {
                    // amounts outside the tree; an all-in always exists as AllIn
                    out.push(line[..=k].to_vec());
                }
            }
        }
        // successive prefixes must also apply on the re-solved tree: keep them all, in order
        out
    }

    fn variant(&self, st: &StreetSolve, spec: SpotSpec, background: bool) -> Arc<Spot> {
        let perm = spec.canonical_perm();
        let key = spec.permuted(perm).key();
        let mut v = st.variants.lock().unwrap();
        if let Some(s) = v.get(&key) {
            if !background {
                s.set_background(false);
            }
            return s.clone();
        }
        let mut t = SolveTargets::for_preset(spec.preset);
        t.background = background;
        let s = Spot::start(spec.clone(), self.cache.clone(), t);
        v.insert(key, s.clone());
        s
    }

    /// Picks the solve that has a strategy for `hand` (player 0 = OOP, 1 = IP) at `line`.
    pub fn spot_for(&self, viewer: Viewer, player: usize, hand: (Card, Card), line: &[LineAction]) -> SpotChoice {
        let st = self.current();
        self.spot_for_street(&st, viewer, player, hand, line)
    }

    /// Same as `spot_for`, on a given street solve (graders keep the street of the decision).
    pub fn spot_for_street(&self, st: &StreetSolve, viewer: Viewer, player: usize, hand: (Card, Card), line: &[LineAction]) -> SpotChoice {
        let mut notes = Vec::new();
        let w = st.ranges[player].get(hand.0, hand.1);
        let max_w = st.ranges[player].w.iter().cloned().fold(0.0f32, f32::max).max(1e-9);
        let absent = w < ABSENT * max_w.max(1.0);
        let extra = if st.preset == TreePreset::Flop { vec![] } else { self.off_tree_prefixes(st, &st.public, line) };
        if !extra.is_empty() {
            notes.push("re-solved with the exact bet size added to the tree".into());
        }
        if !absent && extra.is_empty() {
            return SpotChoice { spot: st.public.clone(), line: line.to_vec(), proxy: None, notes };
        }
        if absent && st.preset == TreePreset::Flop && viewer == Viewer::Bot {
            // too slow to re-solve the flop before a bot acts: borrow the closest hand
            let proxy = self.proxy_hand(st, player, hand, line);
            if let Some(p) = proxy {
                notes.push(format!("hand not in its solver range; playing it like {}{}", card_to_string(p.0), card_to_string(p.1)));
            }
            return SpotChoice { spot: st.public.clone(), line: line.to_vec(), proxy, notes };
        }
        let mut ranges = st.ranges.clone();
        if absent {
            ranges[player].set(hand.0, hand.1, FLOOR * max_w);
            notes.push("hand added to its range at a small floor weight so the solver has a strategy for it".into());
        }
        let spec = self.spec(&st.board, ranges, st.pot, st.stack, extra);
        // coach-only solves run in the background so they never slow the bots down
        let spot = self.variant(st, spec, viewer == Viewer::Coach);
        SpotChoice { spot, line: line.to_vec(), proxy: None, notes }
    }

    /// The in-range hand with the closest equity against the opponent's current range.
    fn proxy_hand(&self, st: &StreetSolve, player: usize, hand: (Card, Card), line: &[LineAction]) -> Option<(Card, Card)> {
        let v = st.public.node(line, true).ok()?;
        let opp = player ^ 1;
        let opp_list: Vec<(Card, Card, f32)> =
            v.hands[opp].iter().zip(&v.weights[opp]).filter(|(_, &w)| w > 0.0).map(|(&(a, b), &w)| (a, b, w)).collect();
        let my_eq = engine::equity::hand_vs_combos_exact(hand, &opp_list, &st.board).0 as f32;
        let eqs = if v.player == player { v.equity.clone()? } else { v.opp_equity.clone()? };
        let blocked = (1u64 << hand.0) | (1u64 << hand.1);
        v.hands[player]
            .iter()
            .zip(eqs.iter())
            .zip(v.weights[player].iter())
            .filter(|((h, _), &w)| w > 0.0 && ((1u64 << h.0) | (1u64 << h.1)) & blocked == 0)
            .min_by(|a, b| (a.0 .1 - my_eq).abs().partial_cmp(&(b.0 .1 - my_eq).abs()).unwrap())
            .map(|((h, _), _)| *h)
    }

    /// Player index (0 OOP / 1 IP) of a table seat, if in this pot.
    pub fn player_of(&self, seat: usize) -> Option<usize> {
        self.seats.iter().position(|&s| s == seat)
    }

    pub fn street_count(&self) -> usize {
        self.streets.lock().unwrap().len()
    }
}
