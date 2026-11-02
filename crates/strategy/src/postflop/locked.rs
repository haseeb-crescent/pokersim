//! Turning observed tendencies into locked strategies.
//!
//! An observed frequency ("folds to turn bets 68%") is turned into a full per-hand strategy by
//! starting from the solver's strategy at that node and moving probability mass, weakest hands
//! first when adding folds (strongest first when removing them), until the range-wide
//! frequency matches the observation. The result is locked into a fresh solve, and the
//! opponent's best response is read from it.

use super::spot::{LineAction, LockSpec};
use super::worker::NodeView;
use crate::bots::distort::distort_postflop;
use crate::bots::profile::Profile;

/// Per-hand strategy rows (actions x hands) of the acting player at `view`.
fn rows(view: &NodeView) -> (usize, usize, Vec<f32>) {
    let p = view.player;
    (view.actions.len(), view.hands[p].len(), view.strategy.clone())
}

/// Hand order by equity, ascending (weakest first). Hands without equity count as 0.5.
fn by_strength(view: &NodeView) -> Vec<usize> {
    let p = view.player;
    let n = view.hands[p].len();
    let eq = view.equity.clone().unwrap_or_else(|| vec![0.5; n]);
    let mut idx: Vec<usize> = (0..n).collect();
    idx.sort_by(|&a, &b| eq[a].partial_cmp(&eq[b]).unwrap());
    idx
}

fn range_freq(view: &NodeView, s: &[f32], action: usize) -> f32 {
    let p = view.player;
    let n = view.hands[p].len();
    let w = &view.weights[p];
    let tot: f32 = w.iter().sum();
    if tot <= 0.0 {
        return 0.0;
    }
    (0..n).map(|i| w[i] * s[action * n + i]).sum::<f32>() / tot
}

/// Moves mass between `to` and the other actions so that the range frequency of `to` equals
/// `target`. Hands are visited in `order` when increasing `to` and in reverse when decreasing.
fn shift(view: &NodeView, s: &mut [f32], to: usize, target: f32, order: &[usize], prefer_from: Option<usize>) {
    let p = view.player;
    let (a, n, _) = rows(view);
    let w = &view.weights[p];
    let tot: f32 = w.iter().sum();
    if tot <= 0.0 {
        return;
    }
    let cur = range_freq(view, s, to);
    let mut need = (target - cur) * tot;
    if need > 0.0 {
        for &i in order {
            if need <= 0.0 {
                break;
            }
            if w[i] <= 0.0 {
                continue;
            }
            let room = (1.0 - s[to * n + i]) * w[i];
            let take = room.min(need);
            let frac = take / w[i];
            let others: f32 = (0..a).filter(|&k| k != to).map(|k| s[k * n + i]).sum();
            if others > 0.0 {
                for k in 0..a {
                    if k != to {
                        s[k * n + i] -= s[k * n + i] / others * frac;
                    }
                }
            }
            s[to * n + i] += frac;
            need -= take;
        }
    } else if need < 0.0 {
        let mut need = -need;
        for &i in order.iter().rev() {
            if need <= 0.0 {
                break;
            }
            if w[i] <= 0.0 {
                continue;
            }
            let have = s[to * n + i] * w[i];
            let take = have.min(need);
            let frac = take / w[i];
            s[to * n + i] -= frac;
            match prefer_from {
                Some(k) => s[k * n + i] += frac,
                None => {
                    let others: Vec<usize> = (0..a).filter(|&k| k != to).collect();
                    for &k in &others {
                        s[k * n + i] += frac / others.len() as f32;
                    }
                }
            }
            need -= take;
        }
    }
    for x in s.iter_mut() {
        *x = x.clamp(0.0, 1.0);
    }
}

/// Lock for a player facing a bet: range fold frequency set to `fold`, weakest hands fold
/// first; when folding less, the strongest folding hands call instead.
pub fn lock_fold_frequency(view: &NodeView, line: &[LineAction], fold: f32) -> Option<LockSpec> {
    let fi = view.actions.iter().position(|a| a.action == LineAction::Fold)?;
    let ci = view.actions.iter().position(|a| a.action == LineAction::Call);
    let order = by_strength(view);
    let (_, _, mut s) = rows(view);
    shift(view, &mut s, fi, fold.clamp(0.0, 1.0), &order, ci);
    Some(LockSpec { line: line.to_vec(), hands: view.hands[view.player].clone(), strategy: s })
}

/// Lock for a player with the option to bet: range betting frequency (all sizes together)
/// set to `bet`. More betting adds the strongest checking hands first; less betting turns the
/// weakest bets into checks first.
pub fn lock_bet_frequency(view: &NodeView, line: &[LineAction], bet: f32) -> Option<LockSpec> {
    let ki = view.actions.iter().position(|a| a.action == LineAction::Check)?;
    let order = by_strength(view);
    let (_, _, mut s) = rows(view);
    // treat "check" as the moved action: checking frequency = 1 - bet; weakest check first
    shift(view, &mut s, ki, (1.0 - bet).clamp(0.0, 1.0), &order, None);
    Some(LockSpec { line: line.to_vec(), hands: view.hands[view.player].clone(), strategy: s })
}

/// Lock reproducing a bot profile: every hand's solver mix distorted by the profile, exactly
/// as the bot plays it at the table.
pub fn lock_profile(view: &NodeView, line: &[LineAction], profile: &Profile) -> LockSpec {
    let p = view.player;
    let (a, n, s) = rows(view);
    let actions: Vec<LineAction> = view.actions.iter().map(|x| x.action).collect();
    let eq = view.equity.clone().unwrap_or_else(|| vec![0.5; n]);
    let mut out = vec![0.0f32; a * n];
    for i in 0..n {
        let mix: Vec<f32> = (0..a).map(|k| s[k * n + i]).collect();
        let d = distort_postflop(profile, &actions, &mix, eq[i]);
        for k in 0..a {
            out[k * n + i] = d[k] as f32;
        }
    }
    LockSpec { line: line.to_vec(), hands: view.hands[p].clone(), strategy: out }
}

/// Range frequency of each action under a lock (for reporting).
pub fn lock_frequencies(view: &NodeView, lock: &LockSpec) -> Vec<f32> {
    (0..view.actions.len()).map(|k| range_freq(view, &lock.strategy, k)).collect()
}
