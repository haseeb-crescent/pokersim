//! Grades the hero's decisions.
//!
//! - Preflop: by chart frequency for the hero's hand class (the charts carry no EVs).
//! - Heads-up postflop: by solver EV for the hero's exact hand, from a solve that contains the
//!   hero's hand (floored into the range when needed). Off-tree sizes on the turn and river
//!   are re-solved with the exact size; on the flop they are graded as the nearest size.
//!   Against a non-GTO bot an exploit view is added: the bot's profile is node-locked at its
//!   next decision and the hero's best response is solved.
//! - Multiway: "unsolved, no grade", with pot odds and equity against estimated ranges.

use crate::state::Resources;
use engine::card::*;
use engine::rules::*;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use strategy::bots::decide::{estimated_range, estimated_range_preflop};
use strategy::bots::profile::Profile;
use strategy::bots::view::{PublicState, POS_NAMES};
use strategy::coach::facts::*;
use strategy::coach::grade::*;
use strategy::postflop::grids::grids;
use strategy::postflop::hand::{HandSolve, StreetSolve, Viewer};
use strategy::postflop::locked::lock_profile;
use strategy::postflop::*;
use strategy::preflop::game::postflop_rank;

pub struct HeroDecision {
    pub hand_id: i64,
    pub index: usize,
    pub public: PublicState,
    pub hero_seat: usize,
    pub hole: [Card; 2],
    pub act: Act,
    pub hand_solve: Option<Arc<HandSolve>>,
    pub street_solve: Option<Arc<StreetSolve>>,
    /// villain seat and profile when heads-up
    pub villain: Option<(usize, Profile)>,
}

fn bb(chips: f64) -> f64 {
    (chips / 100.0 * 100.0).round() / 100.0
}

fn pot_type(st: &PublicState) -> &'static str {
    let raises = st.actions.iter().filter(|a| a.street == Street::Preflop && matches!(a.kind, ActKind::Raise | ActKind::Bet)).count();
    match raises {
        0 => "Limped",
        1 => "SRP",
        2 => "3BP",
        _ => "4BP",
    }
}

fn pot_type_long(t: &str) -> &'static str {
    match t {
        "Limped" => "limped pot",
        "SRP" => "single-raised pot",
        "3BP" => "3-bet pot",
        _ => "4-bet pot",
    }
}

fn hand_str(h: [Card; 2]) -> String {
    format!("{}{}", card_to_string(h[0]), card_to_string(h[1]))
}

pub fn grade(res: &Resources, d: &HeroDecision, want_exploit: bool) -> Value {
    if d.public.street == Street::Preflop {
        return grade_preflop(res, d);
    }
    let live = d.public.live_seats().len();
    if let (Some(hs), Some(ss), true) = (&d.hand_solve, &d.street_solve, live == 2) {
        return match grade_postflop(res, d, hs, ss, want_exploit) {
            Ok(v) => v,
            Err(e) => unsolved(res, d, &format!("the solver was unavailable for this spot ({e})")),
        };
    }
    let reason = if live > 2 {
        format!("{live} players are in the pot, and there is no solver solution for multiway pots")
    } else if let Some(r) = off_chart_reason(res, &d.public, d.hero_seat) {
        r
    } else {
        "the pot was multiway on the flop, so there are no solver ranges for the later streets".to_string()
    };
    unsolved(res, d, &reason)
}

/// Why a hand has no solver ranges: the first preflop action the chart tree does not contain.
fn off_chart_reason(res: &Resources, st: &PublicState, hero: usize) -> Option<String> {
    let codes = st.preflop_codes();
    let pre: Vec<_> = st.actions.iter().filter(|a| a.street == Street::Preflop).collect();
    let i = match res.charts.map_key(&codes) {
        Some((_, changed)) => changed.iter().position(|&c| c)?,
        None => return Some("the preflop action went outside the chart tree, so there are no solver ranges for this hand".into()),
    };
    let a = pre[i];
    let who = if a.seat == hero { "you".to_string() } else { format!("the {}", POS_NAMES[st.positions[a.seat]]) };
    let raised_before = pre[..i].iter().any(|x| matches!(x.kind, ActKind::Raise | ActKind::Bet));
    let what = match a.kind {
        ActKind::Call if !raised_before => "open-limped (in the chart tree only the small blind limps)",
        ActKind::Call => "called where the chart tree has no call (a fourth player into the pot, or a cold call of a 3-bet or more)",
        ActKind::Raise | ActKind::Bet => "raised where the chart tree has no raise",
        ActKind::Check => "checked where the chart tree has no check",
        ActKind::Fold => "folded where the chart tree has no fold",
    };
    Some(format!("{who} {what}, so the hand left the chart tree and there are no solver ranges for it"))
}

/// "raise 16% / fold 84%" for one hand's chart mix.
fn chart_mix_text(labels: &[String], freqs: &[f64]) -> String {
    let mut parts: Vec<(f64, String)> = labels.iter().zip(freqs).filter(|(_, &f)| f >= 0.005).map(|(l, &f)| (f, format!("{} {:.0}%", l.to_lowercase(), 100.0 * f))).collect();
    parts.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
    parts.into_iter().map(|p| p.1).collect::<Vec<_>>().join(" / ")
}

fn base_facts(d: &HeroDecision) -> FactSheet {
    let st = &d.public;
    let legal = st.legal.as_ref().unwrap();
    let to_call = legal.call_amount as f64;
    let pot = st.pot as f64;
    let opp_stacks: Vec<i64> = st.live_seats().into_iter().filter(|&s| s != d.hero_seat).map(|s| st.stacks[s]).collect();
    let eff = opp_stacks.iter().copied().max().unwrap_or(0).min(st.stacks[d.hero_seat]) as f64;
    FactSheet {
        street: st.street.name().into(),
        position: String::new(),
        pot_bb: bb(pot),
        to_call_bb: bb(to_call),
        spr: if pot > 0.0 { ((eff / pot) * 100.0).round() / 100.0 } else { 0.0 },
        pot_odds_needed: if to_call > 0.0 { Some(((to_call / (pot + to_call)) * 1000.0).round() / 1000.0) } else { None },
        hand_description: hand_description((d.hole[0], d.hole[1]), &st.board),
        board_texture: board_texture(&st.board),
        villain_profile: d.villain.as_ref().map(|(_, p)| p.name.clone()),
        ..Default::default()
    }
}

fn chosen_code(act: Act, legal: &Legal) -> char {
    match act {
        Act::Fold => 'F',
        Act::Check => 'K',
        Act::Call => 'C',
        Act::BetTo(x) => {
            if Some(x) == legal.max_to {
                'A'
            } else {
                'R'
            }
        }
    }
}

fn grade_preflop(res: &Resources, d: &HeroDecision) -> Value {
    let st = &d.public;
    let legal = st.legal.as_ref().unwrap();
    let pos = st.positions[d.hero_seat];
    let codes = st.preflop_codes();
    let Some((node, key)) = res.charts.lookup(&codes).filter(|(n, _)| n.player == pos) else {
        let reason = off_chart_reason(res, st, d.hero_seat).unwrap_or_else(|| "this preflop line is outside the chart tree".into());
        return unsolved(res, d, &reason);
    };
    let class = class_of(d.hole[0], d.hole[1]);
    let labels: Vec<String> = node
        .actions
        .iter()
        .zip(&node.sizes)
        .map(|(a, &s)| match a.as_str() {
            "F" => "Fold".to_string(),
            "K" => "Check".to_string(),
            "C" => if node.level == 0 { "Limp".to_string() } else { "Call".to_string() },
            "A" => "All-in".to_string(),
            _ => format!("Raise to {}", fmt_bb(s)),
        })
        .collect();
    let freqs: Vec<f64> = node.freq.iter().map(|f| f[class] as f64).collect();
    let n_classes = NUM_CLASSES;
    let range_freq: Vec<f64> = node
        .freq
        .iter()
        .map(|f| {
            let w: Vec<f64> = (0..n_classes).map(|h| class_combo_count(h) as f64).collect();
            let reach = res.charts.range_along(&key, pos);
            let tot: f64 = (0..n_classes).map(|h| w[h] * reach[h] as f64).sum();
            if tot > 0.0 { (0..n_classes).map(|h| w[h] * reach[h] as f64 * f[h] as f64).sum::<f64>() / tot } else { 0.0 }
        })
        .collect();
    let mut code = chosen_code(d.act, legal);
    if code == 'A' && !node.actions.iter().any(|a| a == "A") {
        code = 'R';
    }
    if code == 'R' && !node.actions.iter().any(|a| a == "R") && node.actions.iter().any(|a| a == "A") {
        code = 'A';
    }
    let chosen_i = node.actions.iter().position(|a| a.starts_with(code));
    let top = (0..freqs.len()).max_by(|&a, &b| freqs[a].partial_cmp(&freqs[b]).unwrap()).unwrap();
    let chosen_freq = chosen_i.map(|i| freqs[i]).unwrap_or(0.0);
    let label = label_from_freq(chosen_freq, chosen_i == Some(top));
    let chosen_label = match d.act {
        Act::Fold => "Fold".to_string(),
        Act::Check => "Check".to_string(),
        Act::Call => if legal.current_bet <= st.big_blind && node.level == 0 { "Limp".into() } else { "Call".into() },
        Act::BetTo(x) => format!("Raise to {}", fmt_bb(x as i32)),
    };
    let (spot, spot_key) = preflop_spot(st, d.hero_seat, node.level);
    let mut facts = base_facts(d);
    facts.position = preflop_position(st, d.hero_seat);
    facts.solver_mix = mix_string(&labels, &freqs.iter().map(|&x| x as f32).collect::<Vec<_>>());
    // equity against the last raiser's chart range (class level, card removal included)
    if let Some(raiser) = st.actions.iter().rev().find(|a| a.street == Street::Preflop && matches!(a.kind, ActKind::Raise | ActKind::Bet)).map(|a| a.seat) {
        if raiser != d.hero_seat {
            let r = res.charts.range_along(&key, st.positions[raiser]);
            let (mut num, mut den) = (0.0f64, 0.0f64);
            for h2 in 0..NUM_CLASSES {
                let k = res.eq.k(class, h2) as f64 * r[h2] as f64;
                num += k * res.eq.eq(class, h2) as f64;
                den += k;
            }
            if den > 0.0 {
                facts.equity_vs_range = Some(((num / den) * 1000.0).round() / 1000.0);
                facts.notes.push(format!("equity_vs_range is all-in equity against the {}'s chart range for this line", POS_NAMES[st.positions[raiser]]));
            }
        }
    }
    let mut note = "Preflop is graded by chart frequency: the charts carry no EVs.".to_string();
    if res.charts.map_key(&codes).map(|(_, ch)| ch.iter().any(|&c| c)).unwrap_or(false) {
        note.push_str(" Earlier action was outside the chart tree (for example an open-limp, treated as an open), so this uses the nearest chart node.");
    }
    let off_chart = if chosen_i.is_none() {
        let mix = chart_mix_text(&labels, &freqs);
        let continues = !matches!(d.act, Act::Fold);
        let mut message = format!("{chosen_label} isn't in the chart here (0%). With {} the chart says {mix}.", class_name(class));
        if continues {
            message.push_str(" This takes the hand off the chart tree: the rest of the hand has no solver ranges, so later decisions show pot odds and equity against estimated ranges instead of a grade.");
        }
        note.push_str(" Your action is not part of the chart strategy here.");
        Some(json!({"action": chosen_label.to_lowercase(), "chart": mix, "message": message, "leaves_chart": continues}))
    } else {
        None
    };
    let actions: Vec<ActionMix> = labels
        .iter()
        .zip(&freqs)
        .zip(&range_freq)
        .map(|((l, &f), &rf)| ActionMix { label: l.clone(), freq: f, ev_bb: None, range_freq: Some(rf) })
        .collect();
    let g = DecisionGrade {
        hand_id: d.hand_id,
        index: d.index,
        street: "preflop".into(),
        spot,
        spot_key,
        kind: "preflop_chart".into(),
        hero_hand: hand_str(d.hole),
        board: vec![],
        actions,
        chosen: chosen_label,
        chosen_freq: Some(chosen_freq),
        best: Some(labels[top].clone()),
        ev_loss_bb: None,
        grade: label.into(),
        precision_pct: None,
        note: Some(note),
        facts: Some(facts),
        explanation: None,
        exploit: None,
    };
    let mut v = serde_json::to_value(&g).unwrap();
    v["preflop_key"] = json!(key);
    v["off_chart"] = off_chart.unwrap_or(Value::Null);
    v
}

fn preflop_position(st: &PublicState, hero: usize) -> String {
    let opener = st.actions.iter().find(|a| a.street == Street::Preflop && matches!(a.kind, ActKind::Raise | ActKind::Bet)).map(|a| a.seat);
    match opener {
        Some(o) if o != hero => {
            if postflop_rank(st.positions[hero]) > postflop_rank(st.positions[o]) { "IP".into() } else { "OOP".into() }
        }
        _ => POS_NAMES[st.positions[hero]].into(),
    }
}

fn preflop_spot(st: &PublicState, hero: usize, level: u8) -> (String, String) {
    let pos = POS_NAMES[st.positions[hero]];
    let raisers: Vec<usize> = st.actions.iter().filter(|a| a.street == Street::Preflop && matches!(a.kind, ActKind::Raise | ActKind::Bet)).map(|a| a.seat).collect();
    match level {
        0 => {
            if pos == "BB" {
                (format!("{pos} · vs SB limp"), format!("PF|vs_limp|{pos}"))
            } else {
                (format!("{pos} · unopened (first in)"), format!("PF|RFI|{pos}"))
            }
        }
        1 => match raisers.first() {
            Some(&r0) => {
                let o = POS_NAMES[st.positions[r0]];
                let callers = st.actions.iter().filter(|a| a.street == Street::Preflop && a.kind == ActKind::Call).count();
                if callers > 0 {
                    (format!("{pos} vs {o} open with callers (squeeze spot)"), format!("PF|squeeze|{pos}"))
                } else {
                    (format!("{pos} vs {o} open"), format!("PF|vs_open|{pos}"))
                }
            }
            None => (format!("{pos} vs a limper"), format!("PF|vs_limp|{pos}")),
        },
        2 => match (raisers.first(), raisers.last()) {
            (Some(&first), Some(&last)) => {
                let t = POS_NAMES[st.positions[last]];
                if first == hero {
                    let ip = postflop_rank(st.positions[hero]) > postflop_rank(st.positions[last]);
                    (format!("{pos} vs {t} 3-bet"), format!("PF|vs_3bet|{}", if ip { "IP" } else { "OOP" }))
                } else {
                    (format!("{pos} facing a 3-bet (cold)"), "PF|vs_3bet_cold".into())
                }
            }
            _ => (format!("{pos} facing a raise over a limp"), "PF|vs_isolate".into()),
        },
        3 => (format!("{pos} facing a 4-bet"), "PF|vs_4bet".into()),
        _ => (format!("{pos} facing a 5-bet all-in"), "PF|vs_5bet".into()),
    }
}

/// Turns the hero's engine action into a solver line action.
fn to_line_action(act: Act, legal: &Legal) -> LineAction {
    match act {
        Act::Fold => LineAction::Fold,
        Act::Check => LineAction::Check,
        Act::Call => LineAction::Call,
        Act::BetTo(x) => {
            if Some(x) == legal.max_to {
                LineAction::AllIn(x as i32)
            } else if legal.current_bet == 0 {
                LineAction::Bet(x as i32)
            } else {
                LineAction::Raise(x as i32)
            }
        }
    }
}

fn postflop_spot(st: &PublicState, hero: usize, villain: usize, legal: &Legal) -> (String, String) {
    let pt = pot_type(st);
    let ip = postflop_rank(st.positions[hero]) > postflop_rank(st.positions[villain]);
    let street = st.street.name();
    let line: Vec<_> = st.actions.iter().filter(|a| a.street == st.street).collect();
    let (desc, key) = if legal.can_check {
        if line.is_empty() {
            ("first to act".to_string(), "first")
        } else {
            ("checked to".to_string(), "vs_check")
        }
    } else {
        let raises = line.iter().filter(|a| matches!(a.kind, ActKind::Raise)).count();
        if raises > 0 {
            ("facing a raise".to_string(), "vs_raise")
        } else {
            let bet = (legal.current_bet - legal.committed) as f64;
            let pot_before = (st.pot as f64 - bet).max(1.0);
            let frac = bet / pot_before;
            (format!("facing a {:.0}% pot bet", 100.0 * frac), if frac <= 0.55 { "vs_bet_small" } else { "vs_bet_big" })
        }
    };
    let hp = POS_NAMES[st.positions[hero]];
    let vp = POS_NAMES[st.positions[villain]];
    (
        format!("{hp} vs {vp} · {} · {street} · {}, {desc}", pot_type_long(pt), if ip { "in position" } else { "out of position" }),
        format!("{pt}|{}|{street}|{key}", if ip { "IP" } else { "OOP" }),
    )
}

fn grade_postflop(res: &Resources, d: &HeroDecision, hs: &Arc<HandSolve>, ss: &Arc<StreetSolve>, want_exploit: bool) -> Result<Value, String> {
    let st = &d.public;
    let legal = st.legal.as_ref().unwrap();
    let hero_player = hs.player_of(d.hero_seat).ok_or("hero not in solve")?;
    let villain_seat = hs.seats[hero_player ^ 1];
    let hand = (d.hole[0], d.hole[1]);
    let line = st.street_line(st.street);
    let mut choice = hs.spot_for_street(ss, Viewer::Coach, hero_player, hand, &line);
    let status = choice.spot.wait_final(Duration::from_secs(900));
    let mut view = choice.spot.node(&choice.line, true)?;
    if view.player != hero_player || view.terminal || view.chance {
        return Err("solver node does not match the decision".into());
    }
    let chosen = to_line_action(d.act, legal);
    let mut notes: Vec<String> = choice.notes.clone();
    if choice.spot.spec.bunching.is_some() {
        notes.push("river solved with the bunching effect (card removal from the players who folded preflop)".into());
    }
    // exact size on later streets: re-solve with the hero's action added
    if chosen.is_aggressive() && !view.actions.iter().any(|a| a.action == chosen) && ss.preset != TreePreset::Flop {
        let mut spec = choice.spot.spec.clone();
        let mut l = line.clone();
        l.push(chosen);
        spec.extra_lines.push(l);
        let s = Spot::start(spec, res.cache.clone(), SolveTargets::for_preset(ss.preset).background());
        s.wait_final(Duration::from_secs(60));
        if let Ok(v) = s.node(&choice.line, true) {
            if v.actions.iter().any(|a| a.action == chosen) {
                view = v;
                choice.spot = s;
                notes.push("re-solved with your exact bet size added to the tree".into());
            }
        }
    }
    let mix = view.hand_strategy(hand).ok_or("hand not in the solve")?;
    let evs = view.hand_ev(hand);
    let labels: Vec<String> = view.actions.iter().map(|a| a.label.clone()).collect();
    let actions_ps: Vec<postflop_solver::Action> = view.actions.iter().map(|a| a.action.to_ps()).collect();
    let (ci, used) = match_action(&actions_ps, chosen, view.pot).ok_or("could not map your action")?;
    if used != chosen {
        notes.push(format!("{} is not in the flop tree; graded as the nearest size, {}", chosen.label(), used.label()));
    }
    let range_freq = view.range_frequencies();
    let top_freq = (0..mix.len()).max_by(|&a, &b| mix[a].partial_cmp(&mix[b]).unwrap()).unwrap();
    let (best, ev_loss) = match &evs {
        Some(e) => {
            let bi = (0..e.len()).max_by(|&a, &b| e[a].partial_cmp(&e[b]).unwrap()).unwrap();
            (bi, ((e[bi] - e[ci]) as f64 / 100.0).max(0.0))
        }
        None => (top_freq, 0.0),
    };
    let pot_bb = view.pot as f64 / 100.0;
    let label = if evs.is_some() { label_from_ev(mix[ci] as f64, ci == top_freq, ev_loss, pot_bb) } else { label_from_freq(mix[ci] as f64, ci == top_freq) };
    let (spot, spot_key) = postflop_spot(st, d.hero_seat, villain_seat, legal);

    // facts
    let mut facts = base_facts(d);
    facts.position = if hero_player == 1 { "IP".into() } else { "OOP".into() };
    if let (Some(eq), Some(i)) = (&view.equity, view.hand_index(hero_player, hand)) {
        facts.equity_vs_range = Some((eq[i] as f64 * 1000.0).round() / 1000.0);
    }
    let (ra, na, blockers) = range_facts(&view, hero_player, hand, &st.board);
    facts.range_advantage = ra.map(|p| Pair2 { hero: (p.hero * 1000.0).round() / 1000.0, villain: (p.villain * 1000.0).round() / 1000.0 });
    facts.nut_advantage = na.map(|p| Pair2 { hero: (p.hero * 1000.0).round() / 1000.0, villain: (p.villain * 1000.0).round() / 1000.0 });
    facts.blockers = blockers;
    if !legal.can_check {
        let bet = (legal.current_bet - legal.committed) as f64;
        let pot_before = st.pot as f64 - bet;
        facts.mdf = Some((mdf(pot_before, bet) * 1000.0).round() / 1000.0);
        facts.bluff_ratio = Some((bluff_share(pot_before, bet) * 1000.0).round() / 1000.0);
        facts.notes.push("mdf and bluff_ratio refer to the bet you are facing".into());
    } else if let Some((bi, amt)) = view.actions.iter().enumerate().filter_map(|(i, a)| a.action.amount().map(|x| (i, x))).max_by(|a, b| range_freq[a.0].partial_cmp(&range_freq[b.0]).unwrap()) {
        let bet = (amt - view.committed[hero_player]) as f64;
        let pot_before = view.pot as f64;
        facts.bluff_ratio = Some((bluff_share(pot_before, bet) * 1000.0).round() / 1000.0);
        facts.mdf = Some((mdf(pot_before, bet) * 1000.0).round() / 1000.0);
        facts.notes.push(format!("mdf and bluff_ratio refer to the solver's most used bet here ({})", labels[bi]));
    }
    facts.solver_mix = mix_string(&labels, &mix);
    facts.notes.push(format!("solver range frequencies: {}", mix_string(&labels, &range_freq)));
    facts.notes.extend(notes.iter().cloned());

    let actions: Vec<ActionMix> = (0..labels.len())
        .map(|i| ActionMix {
            label: labels[i].clone(),
            freq: (mix[i] as f64 * 1000.0).round() / 1000.0,
            ev_bb: evs.as_ref().map(|e| (e[i] as f64 / 100.0 * 100.0).round() / 100.0),
            range_freq: Some((range_freq[i] as f64 * 1000.0).round() / 1000.0),
        })
        .collect();

    let exploit = if want_exploit {
        d.villain.as_ref().filter(|(_, p)| !p.is_gto() && !p.hunter).and_then(|(_, p)| exploit_view(&choice.spot, &choice.line, &view, hero_player, hand, p))
    } else {
        None
    };

    let g = DecisionGrade {
        hand_id: d.hand_id,
        index: d.index,
        street: st.street.name().into(),
        spot,
        spot_key,
        kind: "solver".into(),
        hero_hand: hand_str(d.hole),
        board: st.board.iter().map(|&c| card_to_string(c)).collect(),
        actions,
        chosen: chosen.label(),
        chosen_freq: Some((mix[ci] as f64 * 1000.0).round() / 1000.0),
        best: Some(labels[best].clone()),
        ev_loss_bb: Some((ev_loss * 100.0).round() / 100.0),
        grade: label.into(),
        precision_pct: Some((status.exploitability_pct as f64 * 100.0).round() / 100.0),
        note: if notes.is_empty() { None } else { Some(notes.join("; ")) },
        facts: Some(facts),
        explanation: None,
        exploit,
    };
    let mut v = serde_json::to_value(&g).unwrap();
    v["grids"] = serde_json::to_value(grids(&view)).unwrap();
    v["solver_line"] = json!(line.iter().map(|a| a.label()).collect::<Vec<_>>());
    Ok(v)
}

/// Best response for the hero when the villain's profile is locked at its next decision.
fn exploit_view(spot: &Arc<Spot>, line: &[LineAction], view: &NodeView, hero_player: usize, hand: (Card, Card), p: &Profile) -> Option<ExploitView> {
    let mut locks = Vec::new();
    for a in &view.actions {
        let mut l = line.to_vec();
        l.push(a.action);
        if let Ok(child) = spot.node(&l, true) {
            if !child.terminal && !child.chance && child.player != hero_player {
                locks.push(lock_profile(&child, &l, p));
            }
        }
    }
    if locks.is_empty() {
        return None;
    }
    let mut spec = spot.spec.clone();
    spec.locks = locks;
    let targets = match spec.preset {
        TreePreset::Flop => SolveTargets { target_pct: 1.5, max_secs: 30.0, background: true },
        TreePreset::Turn => SolveTargets { target_pct: 0.5, max_secs: 10.0, background: true },
        _ => SolveTargets { target_pct: 0.3, max_secs: 4.0, background: true },
    };
    let s = Spot::start(spec, Arc::new(SolveCache::disabled()), targets);
    s.wait_final(Duration::from_secs(600));
    let v = s.node(line, false).ok()?;
    let mix = v.hand_strategy(hand)?;
    let evs = v.hand_ev(hand)?;
    let labels: Vec<String> = v.actions.iter().map(|a| a.label.clone()).collect();
    let bi = (0..evs.len()).max_by(|&a, &b| evs[a].partial_cmp(&evs[b]).unwrap())?;
    let rf = v.range_frequencies();
    Some(ExploitView {
        profile: p.name.clone(),
        actions: (0..labels.len())
            .map(|i| ActionMix { label: labels[i].clone(), freq: (mix[i] as f64 * 1000.0).round() / 1000.0, ev_bb: Some((evs[i] as f64).round() / 100.0), range_freq: Some(rf[i] as f64) })
            .collect(),
        best: labels[bi].clone(),
        note: format!("Best response with the {}'s tendencies node-locked at its next decision (solver precision {:.2}% pot).", p.name, v.status.exploitability_pct),
    })
}

fn unsolved(res: &Resources, d: &HeroDecision, reason: &str) -> Value {
    let st = &d.public;
    let legal = st.legal.as_ref().unwrap();
    let mut facts = base_facts(d);
    let opps: Vec<usize> = st.live_seats().into_iter().filter(|&s| s != d.hero_seat).collect();
    facts.position = if opps.len() > 1 { "multiway".into() } else { "heads-up".into() };
    // equity against estimated ranges: chart ranges for the preflop line, narrowed by the
    // postflop action
    let mut players = vec![engine::equity::Holding::Hand(d.hole[0], d.hole[1])];
    for &o in &opps {
        let mut r = if st.street == Street::Preflop { estimated_range_preflop(st, o, &res.charts) } else { estimated_range(st, o, &res.charts) };
        r.remove_dead((1u64 << d.hole[0]) | (1u64 << d.hole[1]));
        if r.is_empty() {
            r = engine::Range::full();
        }
        players.push(engine::equity::Holding::Range(r));
    }
    if !opps.is_empty() {
        let e = engine::equity::equity(&players, &st.board, &[], 20_000, 0);
        facts.equity_vs_range = Some((e.equity[0] * 1000.0).round() / 1000.0);
        facts.notes.push(if st.street == Street::Preflop {
            "equity_vs_range is all-in equity against the opponents' estimated chart ranges, not solver ranges".into()
        } else {
            "equity_vs_range is against estimated ranges (chart ranges narrowed by the postflop action), not solver ranges".into()
        });
    }
    let hp = POS_NAMES[st.positions[d.hero_seat]];
    let (spot, spot_key) = if st.street == Street::Preflop {
        (format!("{hp} · off-chart line"), "PF|offchart".to_string())
    } else if opps.len() > 1 {
        (format!("{hp} · {}-way pot", opps.len() + 1), format!("MW|{}", st.street.name()))
    } else {
        let vp = POS_NAMES[st.positions[opps[0]]];
        (format!("{hp} vs {vp} · off-chart line"), format!("OFF|{}", st.street.name()))
    };
    let chosen = match d.act {
        Act::Fold => "Fold".into(),
        Act::Check => "Check".into(),
        Act::Call => "Call".into(),
        Act::BetTo(x) => if legal.current_bet == 0 { format!("Bet {}", fmt_bb(x as i32)) } else { format!("Raise to {}", fmt_bb(x as i32)) },
    };
    let mut reason_text = reason.to_string();
    if let Some(c) = reason_text.get(0..1) {
        reason_text = c.to_uppercase() + &reason_text[1..];
    }
    let g = DecisionGrade {
        hand_id: d.hand_id,
        index: d.index,
        street: st.street.name().into(),
        spot,
        spot_key,
        kind: "unsolved".into(),
        hero_hand: hand_str(d.hole),
        board: st.board.iter().map(|&c| card_to_string(c)).collect(),
        actions: vec![],
        chosen,
        chosen_freq: None,
        best: None,
        ev_loss_bb: None,
        grade: "Unsolved".into(),
        precision_pct: None,
        note: Some(format!("{reason_text}. Not graded; the pot odds and equity below still apply.")),
        facts: Some(facts),
        explanation: None,
        exploit: None,
    };
    let mut v = serde_json::to_value(&g).unwrap();
    v["unsolved_reason"] = json!(reason_text);
    v
}
