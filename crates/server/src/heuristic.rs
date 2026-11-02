//! Labelled fallback grades for preflop decisions after the hand has left the chart tree.
//!
//! When an earlier action is not in the chart tree (a limp outside the small blind, a call the
//! tree does not allow) or a raise is far from the chart's size, the nearest chart node
//! describes a different situation: an isolation raise over a limp would be scored as a 3-bet
//! of an open. Instead, every option the hero has gets a one-step EV:
//! - opponents' ranges are their actual ranges: for each bot, the probability its personality
//!   gave the action it took, class by class (the same code that made it act);
//! - a raise is answered by each bot's modelled response (its personality at the chart node it
//!   would use), folding to a re-raise is assumed for the hero;
//! - hands that reach the flop are valued with the preflop model's realization factors.
//!
//! The result is labelled "off-chart: heuristic grade" and is lenient: only a large EV loss
//! (3bb or more) is called a blunder.

use crate::coach::{off_chart_reason, HeroDecision};
use crate::state::Resources;
use engine::card::*;
use engine::equity::{equity, Holding};
use engine::rules::*;
use engine::Range;
use serde_json::{json, Value};
use strategy::bots::distort::{distort_preflop, PfChoice};
use strategy::bots::profile::Profile;
use strategy::bots::view::{PublicState, POS_NAMES};
use strategy::coach::facts::*;
use strategy::coach::grade::*;
use strategy::postflop::fmt_bb;
use strategy::preflop::cfr::Role;
use strategy::preflop::charts::{ChartNode, Charts};
use strategy::preflop::game::{postflop_rank, PotType};

/// Raise-size ratio (real / chart) beyond which the chart node no longer describes the spot.
const SIZE_RATIO_LIMIT: f64 = 1.5;

/// Why the chart tree no longer describes the hand at the hero's current preflop decision.
pub fn line_off_chart(charts: &Charts, st: &PublicState, hero: usize) -> Option<String> {
    if let Some(r) = off_chart_reason(charts, st, hero) {
        return Some(r);
    }
    let codes = st.preflop_codes();
    let pre: Vec<_> = st.actions.iter().filter(|a| a.street == Street::Preflop).collect();
    for i in 0..pre.len() {
        let a = pre[i];
        if !matches!(a.kind, ActKind::Raise | ActKind::Bet) || a.all_in {
            continue;
        }
        let Some((key, _)) = charts.map_key(&codes[..i]) else { continue };
        let Some(node) = charts.nodes.get(&key) else { continue };
        let Some(ai) = node.actions.iter().position(|x| x == "R") else { continue };
        let chart_to = node.sizes[ai] as f64;
        let ratio = a.to as f64 / chart_to.max(1.0);
        if !(1.0 / SIZE_RATIO_LIMIT..=SIZE_RATIO_LIMIT).contains(&ratio) {
            let who = if a.seat == hero { "you".to_string() } else { format!("the {}", POS_NAMES[st.positions[a.seat]]) };
            return Some(format!(
                "{who} raised to {} where the chart raises to {}, so the chart node no longer matches the hand",
                fmt_bb(a.to as i32),
                fmt_bb(chart_to as i32)
            ));
        }
    }
    None
}

/// Probability that a player took `kind` at `node`, per class: the bot's personality-distorted
/// mix when the profile is known, the chart otherwise.
fn action_prob(res: &Resources, node: &ChartNode, profile: Option<&Profile>, kind: ActKind, class: usize) -> f64 {
    let choices: Vec<(PfChoice, f64)> = match profile {
        Some(p) => distort_preflop(p, node, class, &res.strength),
        None => node
            .actions
            .iter()
            .zip(&node.sizes)
            .enumerate()
            .map(|(i, (a, &s))| {
                let c = match a.as_str() {
                    "F" => PfChoice::Fold,
                    "K" => PfChoice::Check,
                    "C" => PfChoice::Call,
                    "A" => PfChoice::AllIn,
                    _ => PfChoice::Raise(s),
                };
                (c, node.freq[i][class] as f64)
            })
            .collect(),
    };
    choices
        .iter()
        .filter(|(c, _)| match kind {
            ActKind::Fold => *c == PfChoice::Fold,
            ActKind::Check => *c == PfChoice::Check,
            ActKind::Call => matches!(c, PfChoice::Call | PfChoice::Limp),
            ActKind::Bet | ActKind::Raise => matches!(c, PfChoice::Raise(_) | PfChoice::AllIn),
        })
        .map(|(_, p)| *p)
        .sum()
}

/// A player's actual preflop range (per class), from every action it took so far.
pub fn actual_range(res: &Resources, charts: &Charts, st: &PublicState, profiles: &[Option<Profile>], seat: usize) -> Vec<f64> {
    let codes = st.preflop_codes();
    let pre: Vec<_> = st.actions.iter().filter(|a| a.street == Street::Preflop).collect();
    let mut r = vec![1.0f64; NUM_CLASSES];
    for (i, a) in pre.iter().enumerate() {
        if a.seat != seat {
            continue;
        }
        let Some((key, _)) = charts.map_key(&codes[..i]) else { continue };
        let Some(node) = charts.nodes.get(&key) else { continue };
        let profile = profiles.get(seat).and_then(|p| p.as_ref());
        for (h, w) in r.iter_mut().enumerate() {
            *w *= action_prob(res, node, profile, a.kind, h);
        }
    }
    r
}

fn combos_percent(r: &[f64]) -> (f64, f64) {
    let c: f64 = (0..NUM_CLASSES).map(|h| r[h] * class_combo_count(h) as f64).sum();
    (c, 100.0 * c / 1326.0)
}

/// Equity of the hero's hand against independent opponent ranges (preflop, Monte Carlo).
fn equity_vs(hole: [Card; 2], ranges: &[Vec<f64>]) -> f64 {
    let mut players = vec![Holding::Hand(hole[0], hole[1])];
    for r in ranges {
        let g: Vec<f32> = r.iter().map(|&x| x as f32).collect();
        let mut range = Range::from_grid(&g);
        range.remove_dead((1u64 << hole[0]) | (1u64 << hole[1]));
        if range.is_empty() {
            range = Range::full();
        }
        players.push(Holding::Range(range));
    }
    equity(&players, &[], &[], 40_000, 0).equity[0]
}

struct Response {
    fold: f64,
    call: f64,
    raise: f64,
    calling_range: Vec<f64>,
}

/// How a live opponent answers a raise: its personality at the chart node it would use.
fn response(res: &Resources, charts: &Charts, codes: &[char], seat: usize, st: &PublicState, profile: Option<&Profile>, range: &[f64]) -> Response {
    let node = charts.map_key(codes).and_then(|(k, _)| charts.nodes.get(&k)).filter(|n| n.player == st.positions[seat]);
    let Some(node) = node else {
        return Response { fold: 0.55, call: 0.4, raise: 0.05, calling_range: range.to_vec() };
    };
    let (mut f, mut c, mut r, mut tot) = (0.0, 0.0, 0.0, 0.0);
    let mut calling = vec![0.0; NUM_CLASSES];
    for h in 0..NUM_CLASSES {
        let w = range[h] * class_combo_count(h) as f64;
        if w <= 0.0 {
            continue;
        }
        let pf = action_prob(res, node, profile, ActKind::Fold, h);
        let pc = action_prob(res, node, profile, ActKind::Call, h);
        let pr = action_prob(res, node, profile, ActKind::Raise, h);
        let s = (pf + pc + pr).max(1e-9);
        f += w * pf / s;
        c += w * pc / s;
        r += w * pr / s;
        tot += w;
        calling[h] = range[h] * pc / s;
    }
    if tot <= 0.0 {
        return Response { fold: 1.0, call: 0.0, raise: 0.0, calling_range: calling };
    }
    Response { fold: f / tot, call: c / tot, raise: r / tot, calling_range: calling }
}

pub fn grade_heuristic(res: &Resources, d: &HeroDecision, reason: &str) -> Value {
    let st = &d.public;
    let charts = &*d.charts;
    let legal = st.legal.as_ref().unwrap();
    let hero = d.hero_seat;
    let class = class_of(d.hole[0], d.hole[1]);
    let codes = st.preflop_codes();
    let pre: Vec<_> = st.actions.iter().filter(|a| a.street == Street::Preflop).collect();
    let opps: Vec<usize> = st.live_seats().into_iter().filter(|&s| s != hero).collect();
    let ranges: Vec<Vec<f64>> = opps.iter().map(|&o| actual_range(res, charts, st, &d.profiles, o)).collect();
    let eq = equity_vs(d.hole, &ranges);
    let ip = opps.iter().all(|&o| postflop_rank(st.positions[hero]) > postflop_rank(st.positions[o]));
    let raises = pre.iter().filter(|a| matches!(a.kind, ActKind::Raise | ActKind::Bet)).count();
    let limpers = if raises == 0 { pre.iter().filter(|a| a.kind == ActKind::Call).count() } else { 0 };
    let pot_type = |n: usize| match n {
        0 => PotType::Limped,
        1 => PotType::Single,
        2 => PotType::ThreeBet,
        _ => PotType::FourBet,
    };
    let multi = if opps.len() > 1 { 0.9 } else { 1.0 };
    let realization = |n: usize| res.realization.get(Role { pot: pot_type(n), ip })[class] * multi;
    let pot = st.pot as f64;
    let hero_in = st.committed[hero] as f64;

    // EV of each option, chips, relative to folding now
    let mut opts: Vec<(String, f64)> = Vec::new();
    if legal.can_check {
        opts.push(("Check".into(), realization(raises) * eq * pot));
    } else {
        opts.push(("Fold".into(), 0.0));
        let call = legal.call_amount as f64;
        let label = if raises == 0 { "Limp behind" } else { "Call" };
        opts.push((label.into(), realization(raises) * eq * (pot + call) - call));
    }
    let raise_ev = |to: f64| -> (f64, Response) {
        // each live opponent, in acting order after the hero, answers the raise
        let mut sim: Vec<char> = codes.clone();
        sim.push('R');
        let n = st.n;
        let mut order = Vec::new();
        for k in 1..n {
            let s = (hero + k) % n;
            if opps.contains(&s) {
                order.push(s);
            }
        }
        let mut all_fold = 1.0;
        let mut any_raise = 0.0;
        let mut best: Option<(usize, Response)> = None;
        for &s in &order {
            let i = opps.iter().position(|&o| o == s).unwrap();
            let prof = d.profiles.get(s).and_then(|p| p.as_ref());
            let r = response(res, charts, &sim, s, st, prof, &ranges[i]);
            all_fold *= r.fold;
            any_raise += (1.0 - any_raise) * r.raise;
            sim.push('C');
            if best.as_ref().map_or(true, |(_, b)| r.call > b.call) {
                best = Some((s, r));
            }
        }
        let Some((caller, resp)) = best else {
            return (pot, Response { fold: 1.0, call: 0.0, raise: 0.0, calling_range: vec![0.0; NUM_CLASSES] });
        };
        let called = (1.0 - all_fold - any_raise).max(0.0);
        let added = to - hero_in;
        let pot_after = pot + added + (to - st.committed[caller] as f64);
        let eq_c = equity_vs(d.hole, std::slice::from_ref(&resp.calling_range));
        let ev = all_fold * pot + called * (realization(raises + 1) * eq_c * pot_after - added) - any_raise * added;
        (ev, Response { fold: all_fold, call: called, raise: any_raise, calling_range: resp.calling_range })
    };
    let mut std_size = None;
    let mut fold_to_raise = None;
    if let (Some(lo), Some(hi)) = (legal.min_to, legal.max_to) {
        let b = st.big_blind as f64;
        let target = if raises == 0 {
            b * ((if ip { 3.5 } else { 4.0 }) + limpers as f64)
        } else {
            let callers = pre.iter().skip_while(|a| !matches!(a.kind, ActKind::Raise | ActKind::Bet)).filter(|a| a.kind == ActKind::Call).count();
            legal.current_bet as f64 * ((if ip { 3.0 } else { 3.75 }) + callers as f64)
        };
        let s = ((target / 50.0).round() * 50.0).clamp(lo as f64, hi as f64);
        let (ev, r) = raise_ev(s);
        std_size = Some(s);
        fold_to_raise = Some(r.fold);
        opts.push((format!("Raise to {}", fmt_bb(s as i32)), ev));
        if let Act::BetTo(x) = d.act {
            if (x as f64 - s).abs() >= 50.0 {
                let (ev2, _) = raise_ev(x as f64);
                opts.push((format!("Raise to {}", fmt_bb(x as i32)), ev2));
            }
        }
    }
    let chosen_label = match d.act {
        Act::Fold => "Fold".to_string(),
        Act::Check => "Check".to_string(),
        Act::Call => if raises == 0 { "Limp behind".into() } else { "Call".into() },
        Act::BetTo(x) => {
            let s = std_size.unwrap_or(0.0);
            if (x as f64 - s).abs() < 50.0 { format!("Raise to {}", fmt_bb(s as i32)) } else { format!("Raise to {}", fmt_bb(x as i32)) }
        }
    };
    let best_i = (0..opts.len()).max_by(|&a, &b| opts[a].1.partial_cmp(&opts[b].1).unwrap()).unwrap();
    let chosen_ev = opts.iter().find(|o| o.0 == chosen_label).map(|o| o.1).unwrap_or(0.0);
    let loss = ((opts[best_i].1 - chosen_ev) / 100.0).max(0.0);
    let is_best = opts[best_i].0 == chosen_label;
    let label = if is_best || loss < 0.15 {
        if is_best { "Best" } else { "Correct" }
    } else if loss < 0.5 {
        "Correct"
    } else if loss < 1.5 {
        "Inaccuracy"
    } else if loss < 3.0 {
        "Mistake"
    } else {
        "Blunder"
    };

    let pos = POS_NAMES[st.positions[hero]];
    let range_info: Vec<Value> = opps
        .iter()
        .zip(&ranges)
        .map(|(&o, r)| {
            let (combos, percent) = combos_percent(r);
            let prof = d.profiles.get(o).and_then(|p| p.as_ref()).map(|p| p.name.clone()).unwrap_or_else(|| "unknown".into());
            json!({"position": POS_NAMES[st.positions[o]], "profile": prof, "combos": (combos * 10.0).round() / 10.0, "percent": (percent * 10.0).round() / 10.0})
        })
        .collect();
    let range_text = range_info
        .iter()
        .map(|r| format!("{} {} ({}% of hands)", r["profile"].as_str().unwrap_or(""), r["position"].as_str().unwrap_or(""), r["percent"]))
        .collect::<Vec<_>>()
        .join(", ");
    let sizing = std_size.map(|s| {
        let rule = if raises == 0 {
            format!("standard isolation over {} limper{} {}: {} ({} + 1bb per limper)", limpers, if limpers == 1 { "" } else { "s" }, if ip { "in position" } else { "out of position" }, fmt_bb(s as i32), if ip { "3.5bb" } else { "4bb" })
        } else {
            format!("standard re-raise {}: {}", if ip { "in position (3x)" } else { "out of position (3.75x)" }, fmt_bb(s as i32))
        };
        match d.act {
            Act::BetTo(x) => format!("{rule}; you raised to {}", fmt_bb(x as i32)),
            _ => rule,
        }
    });

    let mut facts = FactSheet {
        street: "preflop".into(),
        position: if ip { "IP".into() } else { "OOP".into() },
        pot_bb: (pot / 100.0 * 100.0).round() / 100.0,
        to_call_bb: legal.call_amount as f64 / 100.0,
        spr: 0.0,
        pot_odds_needed: if legal.call_amount > 0 { Some(((legal.call_amount as f64 / (pot + legal.call_amount as f64)) * 1000.0).round() / 1000.0) } else { None },
        equity_vs_range: Some((eq * 1000.0).round() / 1000.0),
        hand_description: class_name(class),
        board_texture: "preflop".into(),
        villain_profile: d.profiles.get(*opps.first().unwrap_or(&hero)).and_then(|p| p.as_ref()).map(|p| p.name.clone()),
        ..Default::default()
    };
    let eff = opps.iter().map(|&o| st.stacks[o]).max().unwrap_or(0).min(st.stacks[hero]) as f64;
    facts.spr = if pot > 0.0 { (eff / pot * 100.0).round() / 100.0 } else { 0.0 };
    facts.solver_mix = String::new();
    facts.notes.push(format!("equity_vs_range is all-in equity against the actual ranges: {range_text}"));
    if let Some(f) = fold_to_raise {
        facts.notes.push(format!("modelled chance everyone folds to the raise: {:.0}%", 100.0 * f));
    }
    if let Some(s) = &sizing {
        facts.notes.push(format!("sizing: {s}"));
    }
    if d.chart_depth != 100 {
        facts.notes.push(format!("opponents' ranges come from the {}bb preflop chart (effective stack {:.0}bb)", d.chart_depth, d.eff_bb));
    }

    let actions: Vec<ActionMix> = opts
        .iter()
        .enumerate()
        .map(|(i, (l, ev))| ActionMix { label: l.clone(), freq: if i == best_i { 1.0 } else { 0.0 }, ev_bb: Some((ev / 100.0 * 100.0).round() / 100.0), range_freq: None })
        .collect();
    let mut reason_text = reason.to_string();
    if let Some(c) = reason_text.get(0..1) {
        reason_text = c.to_uppercase() + &reason_text[1..];
    }
    let g = DecisionGrade {
        hand_id: d.hand_id,
        index: d.index,
        street: "preflop".into(),
        spot: format!("{pos} · off-chart line (heuristic)"),
        spot_key: format!("PF|offchart|{pos}"),
        kind: "heuristic".into(),
        hero_hand: format!("{}{}", card_to_string(d.hole[0]), card_to_string(d.hole[1])),
        board: vec![],
        actions,
        chosen: chosen_label,
        chosen_freq: None,
        best: Some(opts[best_i].0.clone()),
        ev_loss_bb: Some((loss * 100.0).round() / 100.0),
        grade: label.into(),
        precision_pct: None,
        note: Some(format!(
            "Off-chart: heuristic grade. {reason_text}. EVs come from a one-step model (equity against the actual ranges, the bots' modelled answer to a raise, preflop realization factors), not from the solver or the charts.{}",
            if d.chart_depth != 100 { format!(" The ranges come from the {}bb chart (effective stack {:.0}bb), not the 100bb one.", d.chart_depth, d.eff_bb) } else { String::new() }
        )),
        facts: Some(facts),
        explanation: None,
        exploit: None,
    };
    let mut v = serde_json::to_value(&g).unwrap();
    v["heuristic"] = json!({
        "label": "off-chart: heuristic grade",
        "reason": reason_text,
        "ranges": range_info,
        "sizing": sizing,
        "fold_to_raise": fold_to_raise,
    });
    v
}

#[cfg(test)]
mod tests {
    use crate::coach::{grade, HeroDecision};
    use crate::state::Resources;
    use engine::card::*;
    use engine::rules::*;
    use strategy::bots::profile::{by_id, Profile};
    use strategy::bots::view::PublicState;

    fn resources() -> Resources {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap();
        Resources::load(root).unwrap()
    }

    /// Six-max hand with the hero (seat 0) in the big blind (button on seat 4) holding `hole`.
    fn hero_bb_hand(hole: [Card; 2]) -> Hand {
        let button = 4;
        // dealing starts left of the button: SB (5), BB (0), UTG (1), HJ (2), CO (3), BTN (4)
        let rest: Vec<Card> = (0..52).filter(|c| !hole.contains(c)).collect();
        let mut it = rest.into_iter();
        let mut deck = vec![it.next().unwrap(), hole[0]];
        deck.extend(it.by_ref().take(4));
        deck.push(it.next().unwrap());
        deck.push(hole[1]);
        deck.extend(it);
        Hand::start(TableConfig::six_max(), &[Some(10_000); 6], button, deck)
    }

    fn lineup(hj: &str) -> Vec<Option<Profile>> {
        let mut p: Vec<Option<Profile>> = (0..6).map(|_| by_id("gto")).collect();
        p[0] = None;
        p[2] = by_id(hj);
        p
    }

    fn decide(hand: &Hand, hole: [Card; 2], act: Act, profiles: Vec<Option<Profile>>) -> HeroDecision {
        HeroDecision {
            hand_id: 17,
            index: 0,
            public: PublicState::from_hand(hand),
            hero_seat: 0,
            hole,
            act,
            hand_solve: None,
            street_solve: None,
            villain: None,
            profiles,
            charts: std::sync::Arc::new(resources().charts.as_ref().clone()),
            chart_depth: 100,
            eff_bb: 100.0,
            depth_note: None,
        }
    }

    /// Regression, hand #17: a Loose-Passive Fish limps from the HJ, everyone folds to the hero
    /// in the BB with AQo, the hero raises. The old grader mapped the limp to an open and scored
    /// the isolation raise as a 3-bet of a HJ open (AQo: call 100%, 3-bet 0%) -> "Blunder".
    #[test]
    fn hand_17_iso_raise_over_fish_limp_is_not_graded_as_a_3bet() {
        let res = resources();
        let hole = [card_from_str("Ad").unwrap(), card_from_str("Qc").unwrap()];
        let mut hand = hero_bb_hand(hole);
        hand.apply(1, Act::Fold).unwrap(); // UTG
        hand.apply(2, Act::Call).unwrap(); // HJ fish limps
        for s in [3, 4, 5] {
            hand.apply(s, Act::Fold).unwrap(); // CO, BTN, SB
        }
        assert_eq!(hand.to_act, Some(0));
        let legal = hand.legal().unwrap();
        assert!(legal.can_check, "the BB can check behind a limp");

        let g = grade(&res, &decide(&hand, hole, Act::BetTo(500), lineup("fish")), false);
        println!("hand 17 grade: {} | best {} | loss {} | options {} | {}", g["grade"], g["best"], g["ev_loss_bb"], g["actions"], g["heuristic"]);
        assert_eq!(g["kind"], "heuristic", "{g:#}");
        assert_ne!(g["grade"], "Blunder");
        assert_ne!(g["spot_key"], "PF|vs_open|BB");
        assert!(g["heuristic"]["reason"].as_str().unwrap().contains("HJ open-limped"));
        assert_eq!(g["heuristic"]["label"], "off-chart: heuristic grade");
        // the options are the real ones: check (free) or isolate, not fold / call / 3-bet
        let labels: Vec<&str> = g["actions"].as_array().unwrap().iter().map(|a| a["label"].as_str().unwrap()).collect();
        assert!(labels.contains(&"Check"), "{labels:?}");
        assert!(!labels.iter().any(|l| *l == "Fold" || *l == "Call"), "{labels:?}");
        // AQo isolates a fish limp
        assert!(g["best"].as_str().unwrap().starts_with("Raise"), "{}", g["best"]);
        // the range used is the fish's limping range, much wider than a HJ open
        let pct = g["heuristic"]["ranges"][0]["percent"].as_f64().unwrap();
        assert!(pct > 15.0, "fish HJ limping range {pct}%");
        assert_eq!(g["heuristic"]["ranges"][0]["profile"], "Loose-Passive Fish");
    }

    /// Checking behind with a strong hand loses EV but is not a blunder unless the loss is large.
    #[test]
    fn hand_17_check_is_graded_leniently() {
        let res = resources();
        let hole = [card_from_str("Ad").unwrap(), card_from_str("Qc").unwrap()];
        let mut hand = hero_bb_hand(hole);
        hand.apply(1, Act::Fold).unwrap();
        hand.apply(2, Act::Call).unwrap();
        for s in [3, 4, 5] {
            hand.apply(s, Act::Fold).unwrap();
        }
        let g = grade(&res, &decide(&hand, hole, Act::Check, lineup("fish")), false);
        assert_eq!(g["kind"], "heuristic");
        let loss = g["ev_loss_bb"].as_f64().unwrap();
        assert!(loss > 0.0, "checking AQo should lose some EV vs isolating");
        assert!(g["grade"] != "Blunder" || loss >= 3.0);
    }

    /// An on-chart line (the HJ opens 2.5bb) is still graded against the chart.
    #[test]
    fn on_chart_open_still_uses_the_chart() {
        let res = resources();
        let hole = [card_from_str("Ad").unwrap(), card_from_str("Qc").unwrap()];
        let mut hand = hero_bb_hand(hole);
        hand.apply(1, Act::Fold).unwrap();
        hand.apply(2, Act::BetTo(250)).unwrap();
        for s in [3, 4, 5] {
            hand.apply(s, Act::Fold).unwrap();
        }
        let g = grade(&res, &decide(&hand, hole, Act::Call, lineup("gto")), false);
        assert_eq!(g["kind"], "preflop_chart");
        assert_eq!(g["spot_key"], "PF|vs_open|BB");
    }
}
