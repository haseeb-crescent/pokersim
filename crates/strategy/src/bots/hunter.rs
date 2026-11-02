//! The Hunter: tracks the hero's frequencies by spot type and, once a spot has enough samples,
//! node-locks the hero's observed tendency into the solver and plays the best response.
//! Where it lacks data it plays GTO.
//!
//! Postflop spot types (heads-up, from the hero's point of view):
//! - `fb:<street>:<ip|oop>:<small|big>` facing a bet (fold / call / raise), small <= 55% pot
//! - `opt:<street>:<ip|oop>` first to act or checked to (bet / check)
//! Preflop: `pf:vs3bet` (hero opened and faced a 3-bet), `pf:vsopen` (hero faced an open).

use super::distort::PfChoice;
use super::view::*;
use crate::postflop::hand::{HandSolve, SpotChoice};
use crate::postflop::locked::*;
use crate::postflop::*;
use crate::preflop::charts::ChartNode;
use engine::rules::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const MIN_SAMPLES_POSTFLOP: u32 = 12;
pub const MIN_SAMPLES_PREFLOP: u32 = 15;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SpotCounts {
    pub n: u32,
    pub fold: u32,
    pub call: u32,
    pub raise: u32,
    pub check: u32,
    pub bet: u32,
}

impl SpotCounts {
    pub fn fold_freq(&self) -> f32 {
        self.fold as f32 / self.n.max(1) as f32
    }
    pub fn bet_freq(&self) -> f32 {
        self.bet as f32 / self.n.max(1) as f32
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExploitRecord {
    pub hand_no: u64,
    pub spot: String,
    pub observed: f32,
    pub solver: f32,
    pub samples: u32,
    pub note: String,
}

#[derive(Default)]
pub struct HunterModel {
    pub counts: Mutex<BTreeMap<String, SpotCounts>>,
    pub exploits: Mutex<Vec<ExploitRecord>>,
    pub hand_no: Mutex<u64>,
}

/// Spot type of the decision the player at `seat` is facing, if it is tracked.
pub fn spot_key(st: &PublicState, seat: usize) -> Option<String> {
    let legal = st.legal.as_ref()?;
    if st.street == Street::Preflop {
        let my_raises = st.actions.iter().filter(|a| a.street == Street::Preflop && a.seat == seat && matches!(a.kind, ActKind::Raise | ActKind::Bet)).count();
        let raises = st.actions.iter().filter(|a| a.street == Street::Preflop && matches!(a.kind, ActKind::Raise | ActKind::Bet)).count();
        return match (my_raises, raises) {
            (1, 2) => Some("pf:vs3bet".into()),
            (0, 1) => Some("pf:vsopen".into()),
            _ => None,
        };
    }
    let live = st.live_seats();
    if live.len() != 2 {
        return None;
    }
    let opp = *live.iter().find(|&&s| s != seat)?;
    let ip = crate::preflop::game::postflop_rank(st.positions[seat]) > crate::preflop::game::postflop_rank(st.positions[opp]);
    let pos = if ip { "ip" } else { "oop" };
    let street = st.street.name();
    if legal.can_check {
        Some(format!("opt:{street}:{pos}"))
    } else {
        let bet = (legal.current_bet - legal.committed) as f64;
        let pot_before = (st.pot as f64 - bet).max(1.0);
        let size = if bet / pot_before <= 0.55 { "small" } else { "big" };
        Some(format!("fb:{street}:{pos}:{size}"))
    }
}

impl HunterModel {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records what the hero did in a tracked spot.
    pub fn observe(&self, key: &str, act: Act, can_check: bool) {
        let mut c = self.counts.lock().unwrap();
        let e = c.entry(key.to_string()).or_default();
        e.n += 1;
        match act {
            Act::Fold => e.fold += 1,
            Act::Check => e.check += 1,
            Act::Call => e.call += 1,
            Act::BetTo(_) => {
                if can_check {
                    e.bet += 1
                } else {
                    e.raise += 1
                }
            }
        }
    }

    pub fn counts(&self, key: &str) -> Option<SpotCounts> {
        self.counts.lock().unwrap().get(key).cloned()
    }

    fn record(&self, spot: &str, observed: f32, solver: f32, samples: u32, note: String) {
        let hand_no = *self.hand_no.lock().unwrap();
        let mut ex = self.exploits.lock().unwrap();
        if ex.last().map(|e| e.hand_no == hand_no && e.spot == spot).unwrap_or(false) {
            return;
        }
        ex.push(ExploitRecord { hand_no, spot: spot.to_string(), observed, solver, samples, note });
    }

    /// Preflop: if the hero folds to 3-bets much more than a balanced player (about 55% at
    /// these sizes), widen 3-bets with weaker hands, scaled by the excess. Returns a note.
    pub fn adjust_preflop(&self, st: &PublicState, hero: usize, node: &ChartNode, _key: &str, class: usize, strength: &[f64], choices: &mut [(PfChoice, f64)]) -> Option<String> {
        if node.level != 1 {
            return None;
        }
        // only when the hero is the opener
        let opener = st.actions.iter().find(|a| a.street == Street::Preflop && matches!(a.kind, ActKind::Raise | ActKind::Bet))?.seat;
        if opener != hero {
            return None;
        }
        let c = self.counts("pf:vs3bet")?;
        if c.n < MIN_SAMPLES_PREFLOP {
            return None;
        }
        let f = c.fold_freq();
        let baseline = 0.55;
        if f <= baseline + 0.05 {
            return None;
        }
        let mut sorted: Vec<f64> = strength.to_vec();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let weakish = strength[class] < sorted[sorted.len() * 3 / 4];
        if !weakish {
            return None;
        }
        let boost = 1.0 + 4.0 * (f - baseline) as f64;
        let fold_i = choices.iter().position(|c| c.0 == PfChoice::Fold)?;
        let raise_i = choices.iter().position(|c| matches!(c.0, PfChoice::Raise(_)))?;
        let extra = (choices[raise_i].1 * (boost - 1.0) + 0.05 * (boost - 1.0)).min(choices[fold_i].1);
        choices[fold_i].1 -= extra;
        choices[raise_i].1 += extra;
        let note = format!("you fold to 3-bets {:.0}% (n={}), so it 3-bets wider vs your opens", 100.0 * f, c.n);
        self.record("pf:vs3bet", f, baseline, c.n, note.clone());
        Some(note)
    }

    /// Postflop: builds a solve with the hero's observed tendencies locked at the hero's next
    /// decision on this street (after each Hunter action that has enough data), and returns it
    /// with a note. None when there is not enough data, in which case the Hunter plays GTO.
    pub fn locked_spot(&self, _hs: &HandSolve, st: &PublicState, seat: usize, hero: usize, choice: &SpotChoice, max_wait: Duration) -> Option<(Arc<Spot>, String)> {
        let live = st.live_seats();
        if live.len() != 2 || !live.contains(&hero) {
            return None;
        }
        let base = &choice.spot;
        base.wait_quality(3.0, max_wait);
        let here = base.node(&choice.line, false).ok()?;
        let ip = crate::preflop::game::postflop_rank(st.positions[hero]) > crate::preflop::game::postflop_rank(st.positions[seat]);
        let pos = if ip { "ip" } else { "oop" };
        let street = st.street.name();
        let mut locks = Vec::new();
        let mut notes = Vec::new();
        for a in &here.actions {
            let mut line = choice.line.clone();
            line.push(a.action);
            let Ok(next) = base.node(&line, true) else { continue };
            if next.terminal || next.chance {
                continue;
            }
            if a.action.is_aggressive() {
                let bet = a.action.amount().unwrap_or(0) - here.committed[here.player];
                let pot_before = (here.pot as f64).max(1.0);
                let size = if bet as f64 / pot_before <= 0.55 { "small" } else { "big" };
                let key = format!("fb:{street}:{pos}:{size}");
                if let Some(c) = self.counts(&key) {
                    if c.n >= MIN_SAMPLES_POSTFLOP {
                        let fi = next.actions.iter().position(|x| x.action == LineAction::Fold);
                        let solver_fold = fi.map(|i| next.range_frequencies()[i]).unwrap_or(0.0);
                        if (c.fold_freq() - solver_fold).abs() >= 0.06 {
                            if let Some(l) = lock_fold_frequency(&next, &line, c.fold_freq()) {
                                locks.push(l);
                                let n = format!("you fold {:.0}% facing {size} {street} bets (n={}) vs solver {:.0}%", 100.0 * c.fold_freq(), c.n, 100.0 * solver_fold);
                                self.record(&key, c.fold_freq(), solver_fold, c.n, n.clone());
                                notes.push(n);
                            }
                        }
                    }
                }
            } else if a.action == LineAction::Check {
                let key = format!("opt:{street}:{pos}");
                if let Some(c) = self.counts(&key) {
                    if c.n >= MIN_SAMPLES_POSTFLOP && next.actions.iter().any(|x| x.action == LineAction::Check) {
                        let ki = next.actions.iter().position(|x| x.action == LineAction::Check).unwrap();
                        let solver_bet = 1.0 - next.range_frequencies()[ki];
                        if (c.bet_freq() - solver_bet).abs() >= 0.08 {
                            if let Some(l) = lock_bet_frequency(&next, &line, c.bet_freq()) {
                                locks.push(l);
                                let n = format!("when checked to on the {street} you bet {:.0}% (n={}) vs solver {:.0}%", 100.0 * c.bet_freq(), c.n, 100.0 * solver_bet);
                                self.record(&key, c.bet_freq(), solver_bet, c.n, n.clone());
                                notes.push(n);
                            }
                        }
                    }
                }
            }
        }
        if locks.is_empty() {
            return None;
        }
        let mut spec = base.spec.clone();
        spec.locks = locks;
        let targets = match spec.preset {
            TreePreset::Flop => SolveTargets { target_pct: 2.5, max_secs: 12.0, background: false },
            TreePreset::Turn => SolveTargets { target_pct: 0.6, max_secs: 6.0, background: false },
            _ => SolveTargets { target_pct: 0.3, max_secs: 3.0, background: false },
        };
        let spot = Spot::start(spec, Arc::new(SolveCache::disabled()), targets);
        spot.wait_quality(targets.target_pct, Duration::from_secs_f64(targets.max_secs + 2.0));
        Some((spot, format!("node-locked: {}", notes.join("; "))))
    }
}
