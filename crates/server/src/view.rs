//! JSON views of the live table for the UI (see ui/src/api/types.ts: TableState).

use engine::card::*;
use engine::rules::*;
use serde_json::{json, Value};
use strategy::bots::view::{positions, POS_NAMES};
use strategy::postflop::{fmt_bb, LineAction, NodeView};

pub struct SeatInfo {
    pub name: String,
    pub profile: String,
}

pub fn log_line(h: &Hand, e: &Event, seats: &[SeatInfo]) -> Option<String> {
    let pos = positions(h);
    let who = |s: usize| format!("{} ({})", seats[s].name, POS_NAMES.get(pos[s]).copied().unwrap_or("?"));
    let b = |x: Chips| fmt_bb(x as i32);
    Some(match e {
        Event::Blind { seat, big, amount, .. } => format!("{} posts {} {}", who(*seat), if *big { "BB" } else { "SB" }, b(*amount)),
        Event::Act { seat, kind, to, added, all_in, .. } => {
            let ai = if *all_in { " (all-in)" } else { "" };
            match kind {
                ActKind::Fold => format!("{} folds", who(*seat)),
                ActKind::Check => format!("{} checks", who(*seat)),
                ActKind::Call => format!("{} calls {}{ai}", who(*seat), b(*added)),
                ActKind::Bet => format!("{} bets {}{ai}", who(*seat), b(*to)),
                ActKind::Raise => format!("{} raises to {}{ai}", who(*seat), b(*to)),
            }
        }
        Event::Board { street, cards } => format!("— {} {} —", street.name(), cards_to_string(cards)),
        Event::Returned { seat, amount } => format!("{} returned to {}", b(*amount), who(*seat)),
        Event::Show { seat, cards, description, .. } => format!("{} shows {} ({description})", who(*seat), cards_to_string(cards)),
        Event::Muck { seat } => format!("{} mucks", who(*seat)),
        Event::Win { seat, amount, pot } => format!("{} wins {}{}", who(*seat), b(*amount), if *pot > 0 { format!(" (side pot {pot})") } else { String::new() }),
        Event::Rake { amount } => format!("rake {}", b(*amount)),
        Event::Ante { .. } => return None,
    })
}

/// Bet-size buttons for the hero: the solver tree's sizes when a solve is available,
/// otherwise standard sizes. Keys 1..5.
pub fn presets(h: &Hand, legal: &Legal, node: Option<&NodeView>) -> Vec<Value> {
    let (Some(min), Some(max)) = (legal.min_to, legal.max_to) else { return vec![] };
    let mut out: Vec<(String, Chips)> = Vec::new();
    if let Some(v) = node {
        for a in &v.actions {
            if let Some(x) = a.action.amount() {
                let label = match a.action {
                    LineAction::AllIn(_) => "All-in".to_string(),
                    _ => {
                        let pot_before = v.pot as f64 - (v.committed[0] + v.committed[1]) as f64 + v.committed[0].max(v.committed[1]) as f64;
                        let size = x as f64 - v.committed[v.player] as f64;
                        if matches!(a.action, LineAction::Bet(_)) {
                            format!("{:.0}%", 100.0 * size / v.pot.max(1) as f64)
                        } else {
                            let _ = pot_before;
                            format!("{:.1}x", x as f64 / v.committed[v.player ^ 1].max(1) as f64)
                        }
                    }
                };
                out.push((label, (x as Chips).clamp(min, max)));
            }
        }
    }
    if out.is_empty() {
        let pot = h.pot_total() as f64;
        if h.street == Street::Preflop {
            let cur = legal.current_bet as f64;
            if cur <= h.config.big_blind as f64 {
                for m in [2.5, 3.0, 4.0] {
                    out.push((format!("{m}bb"), (m * h.config.big_blind as f64) as Chips));
                }
            } else {
                for m in [2.5, 3.0, 4.0] {
                    out.push((format!("{m}x"), (m * cur) as Chips));
                }
            }
        } else if legal.current_bet == 0 {
            for f in [0.33, 0.5, 0.75, 1.0] {
                out.push((format!("{:.0}%", f * 100.0), (f * pot) as Chips));
            }
        } else {
            for m in [2.5, 3.0, 4.0] {
                out.push((format!("{m}x"), (m * legal.current_bet as f64) as Chips));
            }
        }
        out.push(("All-in".into(), max));
    }
    let mut seen = std::collections::HashSet::new();
    out.into_iter()
        .map(|(l, x)| (l, x.clamp(min, max)))
        .filter(|(_, x)| seen.insert(*x))
        .take(5)
        .enumerate()
        .map(|(i, (label, to))| json!({"label": label, "to": to, "key": format!("{}", i + 1)}))
        .collect()
}

#[allow(clippy::too_many_arguments)]
pub fn table_state(
    h: Option<&Hand>,
    seats: &[SeatInfo],
    hero_seat: usize,
    spectate: bool,
    thinking: Option<(usize, String)>,
    legal_presets: Option<(Legal, Vec<Value>)>,
    ctl: &Value,
    extra: Value,
) -> Value {
    let Some(h) = h else {
        return json!({"running": true, "street": "idle", "seats": [], "board": [], "log": [], "presets": [], "legal": null, "to_act": null, "hero_seat": hero_seat, "button": 0, "pot": 0, "hand_no": 0, "hand_id": null, "session_id": null, "paused": false, "coach_mode": ctl["coach_mode"], "bot_delay_ms": ctl["bot_delay_ms"], "hero_net": 0, "hands_played": 0, "last_result": null});
    };
    let pos = positions(h);
    let shown: Vec<usize> = h.events.iter().filter_map(|e| if let Event::Show { seat, .. } = e { Some(*seat) } else { None }).collect();
    let last_act: Vec<Option<String>> = (0..h.seats.len())
        .map(|s| {
            h.events.iter().rev().find_map(|e| match e {
                Event::Act { seat, kind, to, all_in, street, .. } if *seat == s && *street == h.street => Some(match kind {
                    ActKind::Fold => "Fold".to_string(),
                    ActKind::Check => "Check".to_string(),
                    ActKind::Call => format!("Call {}", fmt_bb(*to as i32)),
                    ActKind::Bet => format!("Bet {}{}", fmt_bb(*to as i32), if *all_in { " all-in" } else { "" }),
                    ActKind::Raise => format!("Raise {}{}", fmt_bb(*to as i32), if *all_in { " all-in" } else { "" }),
                }),
                Event::Act { seat, kind: ActKind::Fold, .. } if *seat == s => Some("Fold".to_string()),
                _ => None,
            })
        })
        .collect();
    let seat_views: Vec<Value> = h
        .seats
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let show_cards = (i == hero_seat && !spectate) || shown.contains(&i) || (spectate && h.is_over());
            json!({
                "seat": i,
                "name": seats[i].name,
                "profile": seats[i].profile,
                "position": POS_NAMES.get(pos[i]).copied().unwrap_or(""),
                "stack": s.stack,
                "committed": s.committed,
                "folded": s.folded,
                "all_in": s.all_in,
                "in_hand": s.in_hand,
                "cards": if show_cards { json!([card_to_string(s.hole[0]), card_to_string(s.hole[1])]) } else { Value::Null },
                "last_action": last_act[i],
                "thinking": thinking.as_ref().filter(|t| t.0 == i).map(|t| t.1.clone()),
            })
        })
        .collect();
    let (legal, presets) = match legal_presets {
        Some((l, p)) => (serde_json::to_value(l).unwrap(), p),
        None => (Value::Null, vec![]),
    };
    let log: Vec<String> = h.events.iter().filter_map(|e| log_line(h, e, seats)).collect();
    let mut v = json!({
        "running": true,
        "button": h.button,
        "hero_seat": hero_seat,
        "seats": seat_views,
        "board": h.board.iter().map(|&c| card_to_string(c)).collect::<Vec<_>>(),
        "street": if h.is_over() { "showdown".to_string() } else { h.street.name().to_string() },
        "pot": h.pot_total(),
        "to_act": h.to_act,
        "legal": legal,
        "presets": presets,
        "log": log,
        "paused": ctl["paused"],
        "coach_mode": ctl["coach_mode"],
        "bot_delay_ms": ctl["bot_delay_ms"],
    });
    if let (Some(obj), Value::Object(ext)) = (v.as_object_mut(), extra) {
        for (k, x) in ext {
            obj.insert(k, x);
        }
    }
    v
}
