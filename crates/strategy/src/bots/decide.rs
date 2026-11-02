//! Bot decisions.
//!
//! - Preflop: the chart node for the action so far, the bot's hand class, distorted by its
//!   profile; sizes scale with the real bet faced.
//! - Heads-up postflop: the solver strategy for the bot's own hand (see `HandSolve`),
//!   distorted by its profile; the Hunter swaps in a node-locked best response when it has
//!   enough data on the hero.
//! - Multiway postflop: equity against estimated ranges (chart ranges narrowed by the
//!   postflop action), pot odds and the profile. No solver.

use super::distort::*;
use super::hunter::HunterModel;
use super::profile::Profile;
use super::view::*;
use crate::postflop::hand::{HandSolve, Viewer};
use crate::postflop::*;
use crate::preflop::charts::Charts;
use engine::card::*;
use engine::equity::{equity, Holding};
use engine::rules::*;
use engine::Range;
use rand::Rng;
use serde::Serialize;
use std::time::Duration;

pub struct BotEnv<'a> {
    pub charts: &'a Charts,
    pub strength: &'a [f64],
    pub hand_solve: Option<&'a HandSolve>,
    pub hunter: Option<&'a HunterModel>,
    pub hero_seat: Option<usize>,
    /// how long a bot may wait for a solve to reach a usable precision
    pub max_wait: Duration,
    /// exploitability (% pot) at which a running solve is good enough to act on
    pub act_precision: f32,
    /// Monte Carlo samples for heuristic (multiway / unsolved) equity
    pub mc_samples: u64,
    /// simulation mode: skip postflop range narrowing (much faster, cruder ranges)
    pub fast: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct BotDecision {
    pub act: Act,
    pub source: String,
    pub mix: Vec<(String, f64)>,
    pub exploit: Option<String>,
}

pub fn decide<R: Rng>(p: &Profile, st: &PublicState, seat: usize, hole: [Card; 2], env: &BotEnv, rng: &mut R) -> BotDecision {
    let legal = st.legal.clone().expect("bot asked to act with no legal actions");
    if st.street == Street::Preflop {
        if let Some(d) = preflop(p, st, seat, hole, env, &legal, rng) {
            return d;
        }
        return fallback(st, hole, &legal, "preflop fallback");
    }
    let live = st.live_seats();
    if live.len() == 2 {
        if let Some(hs) = env.hand_solve {
            if let Some(player) = hs.player_of(seat) {
                match postflop_hu(p, st, seat, player, hole, hs, env, &legal, rng) {
                    Ok(d) => return d,
                    Err(e) => {
                        let mut d = multiway(p, st, seat, hole, env, &legal, rng);
                        d.source = format!("heuristic (solver unavailable: {e})");
                        return d;
                    }
                }
            }
        }
    }
    multiway(p, st, seat, hole, env, &legal, rng)
}

fn preflop<R: Rng>(p: &Profile, st: &PublicState, seat: usize, hole: [Card; 2], env: &BotEnv, legal: &Legal, rng: &mut R) -> Option<BotDecision> {
    let codes = st.preflop_codes();
    let (node, key) = env.charts.lookup(&codes)?;
    if node.player != st.positions[seat] {
        return None;
    }
    let class = class_of(hole[0], hole[1]);
    let mut choices = distort_preflop(p, node, class, env.strength);
    let mut exploit = None;
    if p.hunter {
        if let (Some(h), Some(hero)) = (env.hunter, env.hero_seat) {
            exploit = h.adjust_preflop(st, hero, node, &key, class, env.strength, &mut choices);
        }
    }
    let probs: Vec<f64> = choices.iter().map(|c| c.1).collect();
    let i = sample(rng, &probs);
    // scale the chart size to the real bet faced
    let abstract_bet = abstract_facing(env.charts, &key);
    let act = match choices[i].0 {
        PfChoice::Fold => if legal.can_check { Act::Check } else { Act::Fold },
        PfChoice::Check => Act::Check,
        PfChoice::Call | PfChoice::Limp => if legal.can_check { Act::Check } else { Act::Call },
        PfChoice::AllIn => raise_to(legal, legal.max_to.unwrap_or(0)),
        PfChoice::Raise(size) => {
            let real = st.current_bet.max(st.big_blind);
            let mut to = (size as f64 * real as f64 / abstract_bet.max(1) as f64).round() as i64;
            if node.level == 0 {
                to += st.big_blind * st.limpers() as i64;
            }
            to = (to / 5) * 5;
            let stack_total = legal.max_to.unwrap_or(0);
            if to as f64 >= 0.42 * stack_total as f64 {
                to = stack_total;
            }
            raise_to(legal, to)
        }
    };
    Some(BotDecision {
        act,
        source: format!("chart {}", if key.is_empty() { "(unopened)".to_string() } else { key }),
        mix: choices.iter().map(|(c, f)| (pf_label(*c), *f)).collect(),
        exploit,
    })
}

fn pf_label(c: PfChoice) -> String {
    match c {
        PfChoice::Fold => "Fold".into(),
        PfChoice::Check => "Check".into(),
        PfChoice::Call => "Call".into(),
        PfChoice::Limp => "Limp".into(),
        PfChoice::Raise(x) => format!("Raise {}", fmt_bb(x)),
        PfChoice::AllIn => "All-in".into(),
    }
}

/// The bet faced at the chart node `key` in the abstract game.
pub fn abstract_facing(charts: &Charts, key: &str) -> i32 {
    let mut bet = 100;
    let codes: Vec<&str> = if key.is_empty() { vec![] } else { key.split('-').collect() };
    let mut prefix: Vec<&str> = Vec::new();
    for c in codes {
        if let Some(n) = charts.nodes.get(&prefix.join("-")) {
            if let Some(ai) = n.actions.iter().position(|a| a == c) {
                if n.sizes[ai] > 0 {
                    bet = n.sizes[ai];
                }
            }
        }
        prefix.push(c);
    }
    bet
}

fn raise_to(legal: &Legal, to: i64) -> Act {
    match (legal.min_to, legal.max_to) {
        (Some(lo), Some(hi)) => Act::BetTo(to.clamp(lo, hi)),
        _ => if legal.can_check { Act::Check } else { Act::Call },
    }
}

/// Converts a solver action to a legal engine action.
pub fn line_to_act(a: LineAction, legal: &Legal) -> Act {
    match a {
        LineAction::Fold => if legal.can_check { Act::Check } else { Act::Fold },
        LineAction::Check => if legal.can_check { Act::Check } else { Act::Call },
        LineAction::Call => if legal.can_check { Act::Check } else { Act::Call },
        LineAction::Bet(x) | LineAction::Raise(x) => raise_to(legal, x as i64),
        LineAction::AllIn(_) => raise_to(legal, legal.max_to.unwrap_or(0)),
    }
}

#[allow(clippy::too_many_arguments)]
fn postflop_hu<R: Rng>(p: &Profile, st: &PublicState, seat: usize, player: usize, hole: [Card; 2], hs: &HandSolve, env: &BotEnv, legal: &Legal, rng: &mut R) -> Result<BotDecision, String> {
    let line = st.street_line(st.street);
    let hand = (hole[0], hole[1]);
    let mut exploit = None;
    let mut choice = hs.spot_for(Viewer::Bot, player, hand, &line);
    if p.hunter {
        if let (Some(h), Some(hero)) = (env.hunter, env.hero_seat) {
            if let Some((spot, note)) = h.locked_spot(hs, st, seat, hero, &choice, env.max_wait) {
                choice.spot = spot;
                exploit = Some(note);
            }
        }
    }
    let status = choice.spot.wait_quality(env.act_precision, env.max_wait);
    if !status.finalized && status.iterations < 25 && status.exploitability_pct > 15.0 {
        return Err(format!("solve not ready after {:.0}s ({} iterations)", env.max_wait.as_secs_f64(), status.iterations));
    }
    let node = choice.spot.node(&choice.line, true)?;
    if node.terminal || node.chance || node.player != player {
        return Err("solver node does not match the table".into());
    }
    let use_hand = choice.proxy.unwrap_or(hand);
    let mix = node.hand_strategy(use_hand).ok_or("hand not in solve")?;
    let idx = node.hand_index(player, use_hand).unwrap();
    let eq = node.equity.as_ref().map(|e| e[idx]).unwrap_or(0.5);
    let actions: Vec<LineAction> = node.actions.iter().map(|a| a.action).collect();
    let probs = distort_postflop(p, &actions, &mix, eq);
    let i = sample(rng, &probs);
    let act = line_to_act(actions[i], legal);
    let mut source = format!("solver {} ({:.1}% pot, {} it{})", st.street.name(), status.exploitability_pct.min(99.0), status.iterations, if status.from_cache { ", cached" } else { "" });
    if !choice.notes.is_empty() {
        source.push_str(&format!("; {}", choice.notes.join("; ")));
    }
    let _ = seat;
    Ok(BotDecision { act, source, mix: node.actions.iter().zip(&probs).map(|(a, &f)| (a.label.clone(), f)).collect(), exploit })
}

/// A hand's rough strength on a board for range narrowing: made-hand value, with draws
/// bumped up to the middle of the range.
fn strength_on_board(h: (Card, Card), board: &[Card]) -> f64 {
    let mut cards = board.to_vec();
    cards.push(h.0);
    cards.push(h.1);
    let v = engine::eval::eval(&cards) as f64;
    let mut s = v / (9u32 << 20) as f64;
    if board.len() < 5 {
        let suits = |c: &[Card]| {
            let mut n = [0; 4];
            for &x in c {
                n[suit(x) as usize] += 1;
            }
            n
        };
        if suits(&cards).iter().any(|&n| n == 4) {
            s = s.max(0.45);
        }
        let mut ranks: u32 = 0;
        for &x in &cards {
            ranks |= 1 << rank(x);
        }
        let m = (ranks << 1) | ((ranks >> 12) & 1);
        for lo in 0..10 {
            if (m >> lo) & 0b1111 == 0b1111 {
                s = s.max(0.42);
            }
        }
    }
    s
}

/// The chart range of `seat` for the preflop line, without postflop narrowing.
pub fn estimated_range_preflop(st: &PublicState, seat: usize, charts: &Charts) -> Range {
    let codes = st.preflop_codes();
    let grid: Vec<f32> = match charts.lookup(&codes) {
        Some((_, key)) => charts.range_along(&key, st.positions[seat]),
        None => match charts.map_key(&codes) {
            Some((key, _)) => charts.range_along(&key, st.positions[seat]),
            None => vec![0.5; NUM_CLASSES],
        },
    };
    let mut r = Range::from_grid(&grid);
    r.remove_dead(mask_of(&st.board));
    r
}

thread_local! {
    /// combo order by strength (strongest first) for recently seen boards
    static ORDER_CACHE: std::cell::RefCell<Vec<(u64, std::rc::Rc<Vec<usize>>)>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Combo indices sorted by strength on `board`, strongest first (cached per thread).
fn strength_order(board: &[Card]) -> std::rc::Rc<Vec<usize>> {
    let key = mask_of(board);
    if let Some(o) = ORDER_CACHE.with(|c| c.borrow().iter().find(|(k, _)| *k == key).map(|(_, o)| o.clone())) {
        return o;
    }
    let mut scored: Vec<(f64, usize)> = (0..NUM_COMBOS)
        .filter(|&i| {
            let (x, y) = combo_cards(i);
            key & ((1u64 << x) | (1u64 << y)) == 0
        })
        .map(|i| (strength_on_board(combo_cards(i), board), i))
        .collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
    let order = std::rc::Rc::new(scored.into_iter().map(|x| x.1).collect::<Vec<_>>());
    ORDER_CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if c.len() >= 8 {
            c.remove(0);
        }
        c.push((key, order.clone()));
    });
    order
}

/// Estimated range of `seat`: its chart range for the preflop line, narrowed by postflop
/// aggression (bets/raises keep the top ~45% at full weight) and calls (top ~70%).
pub fn estimated_range(st: &PublicState, seat: usize, charts: &Charts) -> Range {
    let mut r = estimated_range_preflop(st, seat, charts);
    for street in [Street::Flop, Street::Turn, Street::River] {
        let n = match street {
            Street::Flop => 3,
            Street::Turn => 4,
            _ => 5,
        };
        if st.board.len() < n {
            break;
        }
        let order = strength_order(&st.board[..n]);
        for a in st.actions.iter().filter(|a| a.street == street && a.seat == seat) {
            // flop bets are wide (c-bets), later bets and any raise are stronger
            let keep = match (a.kind, street) {
                (ActKind::Raise, _) => 0.35,
                (ActKind::Bet, Street::Flop) => 0.62,
                (ActKind::Bet, Street::Turn) => 0.5,
                (ActKind::Bet, _) => 0.42,
                (ActKind::Call, _) => 0.75,
                _ => continue,
            };
            let total: f32 = order.iter().map(|&i| r.w[i]).sum();
            let mut acc = 0.0;
            for &i in order.iter() {
                acc += r.w[i];
                if acc > keep * total {
                    r.w[i] *= 0.25;
                }
            }
        }
    }
    r.remove_dead(mask_of(&st.board));
    r
}

fn multiway<R: Rng>(p: &Profile, st: &PublicState, seat: usize, hole: [Card; 2], env: &BotEnv, legal: &Legal, rng: &mut R) -> BotDecision {
    let opps: Vec<usize> = st.live_seats().into_iter().filter(|&s| s != seat).collect();
    let mut players = vec![Holding::Hand(hole[0], hole[1])];
    for &o in &opps {
        let mut r = estimated_range(st, o, env.charts);
        r.remove_dead((1u64 << hole[0]) | (1u64 << hole[1]));
        if r.is_empty() {
            r = Range::full();
        }
        players.push(Holding::Range(r));
    }
    let eq = equity(&players, &st.board, &[], env.mc_samples.max(100), 0).equity[0];
    let n = opps.len() as f64;
    let fair = 1.0 / (n + 1.0);
    let pot = st.pot as f64;
    let to_call = legal.call_amount as f64;
    let mut probs: Vec<(String, f64, Act)> = Vec::new();
    let size_frac = (0.55 + 0.3 * p.size_bias).clamp(0.25, 1.25);
    let bet_to = |frac: f64| -> Act {
        let base = legal.current_bet as f64;
        let pot_after_call = pot + (legal.current_bet - legal.committed) as f64;
        let to = if legal.can_check { pot * frac } else { base + pot_after_call * frac };
        raise_to(legal, to.round() as i64)
    };
    let can_raise = legal.min_to.is_some();
    // the last preflop raiser has the range advantage on the flop and c-bets often
    let pfr = st.actions.iter().rev().find(|a| a.street == Street::Preflop && matches!(a.kind, ActKind::Raise | ActKind::Bet)).map(|a| a.seat) == Some(seat);
    let ip = opps.iter().all(|&o| crate::preflop::game::postflop_rank(st.positions[seat]) > crate::preflop::game::postflop_rank(st.positions[o]));
    let rel = eq / fair; // 1.0 = fair share of the pot
    if legal.can_check {
        let first_bet_of_street = !st.actions.iter().any(|a| a.street == st.street && matches!(a.kind, ActKind::Bet | ActKind::Raise));
        let (value, bluff) = if rel > 1.7 {
            (0.8, 0.0)
        } else if rel > 1.25 {
            (0.45, 0.0)
        } else if rel > 0.85 {
            (0.15, 0.0)
        } else {
            (0.0, 0.22 / n)
        };
        let mut bet = value + bluff * p.bluff;
        if pfr && first_bet_of_street && st.street == Street::Flop {
            // range c-bet: heads-up most of the range, multiway mostly value
            let cbet = if n <= 1.0 { if ip { 0.62 } else { 0.48 } } else { 0.3 };
            bet = bet.max(cbet * if rel < 0.85 { p.bluff.min(1.5) } else { 1.0 });
        } else if !pfr && st.street == Street::Flop && n <= 1.0 && !ip {
            bet *= 0.5; // donk bets are rare
        }
        bet = (bet * p.aggression).clamp(0.0, 0.95);
        if can_raise {
            probs.push(("Bet".into(), bet, bet_to(size_frac)));
        }
        probs.push(("Check".into(), 1.0 - if can_raise { bet } else { 0.0 }, Act::Check));
    } else {
        // continue when realized equity beats the price; the profile scales the threshold
        let price = to_call / (pot + to_call);
        let realize = if ip { 0.95 } else { 0.85 } * if st.street == Street::River { 1.0 / 0.92 } else { 1.0 };
        let need = price * p.fold_to_pressure.max(0.3);
        let realized = eq * realize;
        let strong = eq > (fair * 2.0).max(0.62);
        if strong && can_raise {
            let r = (0.4 * p.aggression).min(0.9);
            probs.push(("Raise".into(), r, bet_to(0.8 + 0.2 * p.size_bias)));
            probs.push(("Call".into(), 1.0 - r, Act::Call));
        } else if realized >= need || (p.postflop_call > 0.0 && realized >= need * (1.0 - 0.45 * p.postflop_call)) {
            let r = if can_raise && n <= 1.0 && eq < fair * 0.7 { 0.05 * p.bluff * p.aggression } else { 0.0 };
            probs.push(("Raise".into(), r, bet_to(0.8)));
            probs.push(("Call".into(), 1.0 - r, Act::Call));
        } else {
            let float = (0.04 * p.postflop_call + if can_raise && n <= 1.0 { 0.03 * (p.bluff - 1.0).max(0.0) } else { 0.0 }).min(0.3);
            probs.push(("Call".into(), float, Act::Call));
            probs.push(("Fold".into(), 1.0 - float, Act::Fold));
        }
    }
    let ws: Vec<f64> = probs.iter().map(|x| x.1).collect();
    let i = sample(rng, &ws);
    BotDecision {
        act: probs[i].2,
        source: format!("multiway heuristic (equity {:.0}% vs {} estimated ranges)", 100.0 * eq, opps.len()),
        mix: probs.iter().map(|x| (x.0.clone(), x.1)).collect(),
        exploit: None,
    }
}

/// When nothing else applies: check if possible, otherwise call with a decent hand.
fn fallback(st: &PublicState, hole: [Card; 2], legal: &Legal, why: &str) -> BotDecision {
    let class = class_of(hole[0], hole[1]);
    let strong = class / 13 <= 3 && class % 13 <= 3;
    let act = if legal.can_check {
        Act::Check
    } else if strong || legal.call_amount <= st.big_blind {
        Act::Call
    } else {
        Act::Fold
    };
    BotDecision { act, source: why.into(), mix: vec![], exploit: None }
}
