//! PokerStars-format hand history export. Chips are shown as dollars with 1bb = $1
//! (so the default table reads "$0.50/$1 USD"), which tracking tools import as a cash game.

use crate::card::{card_to_string, Card};
use crate::eval::{Category, HandValue};
use crate::rules::*;

pub struct HhInfo<'a> {
    pub hand_id: u64,
    pub table: &'a str,
    pub names: &'a [String],
    /// "2026/11/01 21:04:05"
    pub timestamp: &'a str,
    pub hero_seat: Option<usize>,
}

fn money(chips: Chips, bb: Chips) -> String {
    let cents = chips * 100 / bb;
    if cents % 100 == 0 {
        format!("${}", cents / 100)
    } else {
        format!("${}.{:02}", cents / 100, cents % 100)
    }
}

fn cards(cs: &[Card]) -> String {
    cs.iter().map(|&c| card_to_string(c)).collect::<Vec<_>>().join(" ")
}

fn stars_hand_name(v: HandValue) -> String {
    let rank_plural = ["Deuces", "Threes", "Fours", "Fives", "Sixes", "Sevens", "Eights", "Nines", "Tens", "Jacks", "Queens", "Kings", "Aces"];
    let rank_single = ["Deuce", "Three", "Four", "Five", "Six", "Seven", "Eight", "Nine", "Ten", "Jack", "Queen", "King", "Ace"];
    let r = |i: u32| ((v >> (16 - 4 * i)) & 0xF) as usize;
    match Category::from_value(v) {
        Category::HighCard => format!("high card {}", rank_single[r(0)]),
        Category::Pair => format!("a pair of {}", rank_plural[r(0)]),
        Category::TwoPair => format!("two pair, {} and {}", rank_plural[r(0)], rank_plural[r(1)]),
        Category::Trips => format!("three of a kind, {}", rank_plural[r(0)]),
        Category::Straight => format!("a straight, {} high", rank_single[r(0)]),
        Category::Flush => format!("a flush, {} high", rank_single[r(0)]),
        Category::FullHouse => format!("a full house, {} full of {}", rank_plural[r(0)], rank_plural[r(1)]),
        Category::Quads => format!("four of a kind, {}", rank_plural[r(0)]),
        Category::StraightFlush => {
            if r(0) == 12 {
                "a Royal Flush".into()
            } else {
                format!("a straight flush, {} high", rank_single[r(0)])
            }
        }
    }
}

pub fn pokerstars(h: &Hand, info: &HhInfo) -> String {
    let bb = h.config.big_blind;
    let n = h.seats.len();
    let name = |s: usize| info.names.get(s).cloned().unwrap_or_else(|| format!("Seat{}", s + 1));
    let mut out = String::new();
    out.push_str(&format!(
        "PokerStars Hand #{}:  Hold'em No Limit ({}/{} USD) - {} ET\n",
        info.hand_id,
        money(h.config.small_blind, bb),
        money(h.config.big_blind, bb),
        info.timestamp
    ));
    out.push_str(&format!("Table '{}' {}-max Seat #{} is the button\n", info.table, n, h.button + 1));
    for (i, s) in h.seats.iter().enumerate() {
        if s.in_hand {
            out.push_str(&format!("Seat {}: {} ({} in chips)\n", i + 1, name(i), money(s.starting_stack, bb)));
        }
    }
    let mut street_open = Street::Preflop;
    let mut hole_header = false;
    let mut current_bet: Chips = 0;
    let mut committed: Vec<Chips> = vec![0; n];
    let mut board: Vec<Card> = Vec::new();
    let mut shown_any = false;
    for e in &h.events {
        match e {
            Event::Ante { seat, amount } => out.push_str(&format!("{}: posts the ante {}\n", name(*seat), money(*amount, bb))),
            Event::Blind { seat, big, amount, all_in } => {
                committed[*seat] += amount;
                current_bet = current_bet.max(*amount);
                out.push_str(&format!(
                    "{}: posts {} blind {}{}\n",
                    name(*seat),
                    if *big { "big" } else { "small" },
                    money(*amount, bb),
                    if *all_in { " and is all-in" } else { "" }
                ));
            }
            Event::Act { seat, street, kind, added, to, all_in } => {
                if !hole_header {
                    out.push_str("*** HOLE CARDS ***\n");
                    if let Some(hs) = info.hero_seat {
                        out.push_str(&format!("Dealt to {} [{}]\n", name(hs), cards(&h.seats[hs].hole)));
                    }
                    hole_header = true;
                }
                let _ = street;
                let ai = if *all_in { " and is all-in" } else { "" };
                let line = match kind {
                    ActKind::Fold => format!("{}: folds", name(*seat)),
                    ActKind::Check => format!("{}: checks", name(*seat)),
                    ActKind::Call => format!("{}: calls {}{}", name(*seat), money(*added, bb), ai),
                    ActKind::Bet => format!("{}: bets {}{}", name(*seat), money(*to, bb), ai),
                    ActKind::Raise => format!("{}: raises {} to {}{}", name(*seat), money(*to - current_bet, bb), money(*to, bb), ai),
                };
                committed[*seat] = *to;
                if matches!(kind, ActKind::Bet | ActKind::Raise) {
                    current_bet = *to;
                }
                out.push_str(&line);
                out.push('\n');
            }
            Event::Board { street, cards: c } => {
                if !hole_header {
                    out.push_str("*** HOLE CARDS ***\n");
                    if let Some(hs) = info.hero_seat {
                        out.push_str(&format!("Dealt to {} [{}]\n", name(hs), cards(&h.seats[hs].hole)));
                    }
                    hole_header = true;
                }
                let prev = cards(&board);
                board.extend_from_slice(c);
                street_open = *street;
                current_bet = 0;
                committed.iter_mut().for_each(|x| *x = 0);
                match street {
                    Street::Flop => out.push_str(&format!("*** FLOP *** [{}]\n", cards(&board))),
                    Street::Turn => out.push_str(&format!("*** TURN *** [{}] [{}]\n", prev, cards(c))),
                    Street::River => out.push_str(&format!("*** RIVER *** [{}] [{}]\n", prev, cards(c))),
                    _ => {}
                }
            }
            Event::Returned { seat, amount } => out.push_str(&format!("Uncalled bet ({}) returned to {}\n", money(*amount, bb), name(*seat))),
            Event::Show { seat, cards: c, value, .. } => {
                if !shown_any {
                    out.push_str("*** SHOW DOWN ***\n");
                    shown_any = true;
                }
                out.push_str(&format!("{}: shows [{}] ({})\n", name(*seat), cards(c), stars_hand_name(*value)));
            }
            Event::Muck { seat } => {
                if !shown_any {
                    out.push_str("*** SHOW DOWN ***\n");
                    shown_any = true;
                }
                out.push_str(&format!("{}: mucks hand\n", name(*seat)));
            }
            Event::Win { seat, pot, amount } => {
                let r = h.result.as_ref();
                let multi = r.map(|r| r.pots.len() > 1).unwrap_or(false);
                let which = if !multi {
                    "pot".to_string()
                } else if *pot == 0 {
                    "main pot".to_string()
                } else {
                    format!("side pot-{}", pot)
                };
                out.push_str(&format!("{} collected {} from {}\n", name(*seat), money(*amount, bb), which));
            }
            Event::Rake { .. } => {}
        }
    }
    let _ = street_open;
    out.push_str("*** SUMMARY ***\n");
    if let Some(r) = &h.result {
        let total: Chips = r.pots.iter().map(|p| p.amount).sum();
        if r.pots.len() > 1 {
            let parts: Vec<String> = r
                .pots
                .iter()
                .enumerate()
                .map(|(i, p)| if i == 0 { format!("Main pot {}.", money(p.amount, bb)) } else { format!("Side pot-{} {}.", i, money(p.amount, bb)) })
                .collect();
            out.push_str(&format!("Total pot {} {} | Rake {}\n", money(total, bb), parts.join(" "), money(r.rake, bb)));
        } else {
            out.push_str(&format!("Total pot {} | Rake {}\n", money(total, bb), money(r.rake, bb)));
        }
        if !h.board.is_empty() {
            out.push_str(&format!("Board [{}]\n", cards(&h.board)));
        }
        for (i, s) in h.seats.iter().enumerate() {
            if !s.in_hand {
                continue;
            }
            let role = if i == h.button {
                " (button)"
            } else if Some(i) == h.sb_seat {
                " (small blind)"
            } else if i == h.bb_seat {
                " (big blind)"
            } else {
                ""
            };
            let won = r.won[i];
            let line = if r.shown.contains(&i) {
                let v = r.values[i].unwrap();
                if won > 0 {
                    format!("showed [{}] and won ({}) with {}", cards(&s.hole), money(won, bb), stars_hand_name(v))
                } else {
                    format!("showed [{}] and lost with {}", cards(&s.hole), stars_hand_name(v))
                }
            } else if s.folded {
                let when = folded_when(h, i);
                format!("folded {when}")
            } else if won > 0 {
                format!("collected ({})", money(won, bb))
            } else {
                "mucked".to_string()
            };
            out.push_str(&format!("Seat {}: {}{} {}\n", i + 1, name(i), role, line));
        }
    }
    out.push('\n');
    out
}

fn folded_when(h: &Hand, seat: usize) -> &'static str {
    for e in &h.events {
        if let Event::Act { seat: s, street, kind: ActKind::Fold, .. } = e {
            if *s == seat {
                let voluntary = h.events.iter().any(|e| matches!(e, Event::Act { seat: s2, street: Street::Preflop, kind: ActKind::Call | ActKind::Raise | ActKind::Bet, .. } if *s2 == seat));
                return match street {
                    Street::Preflop => if voluntary { "before Flop" } else { "before Flop (didn't bet)" },
                    Street::Flop => "on the Flop",
                    Street::Turn => "on the Turn",
                    Street::River => "on the River",
                };
            }
        }
    }
    "before Flop"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exports_a_hand() {
        let mut h = Hand::start(TableConfig::six_max(), &[Some(10_000); 6], 0, (0..52).collect());
        h.apply(3, Act::BetTo(250)).unwrap();
        for s in [4, 5, 0, 1] {
            h.apply(s, Act::Fold).unwrap();
        }
        h.apply(2, Act::Call).unwrap();
        while let Some(s) = h.to_act {
            h.apply(s, Act::Check).unwrap();
        }
        let names: Vec<String> = (0..6).map(|i| format!("P{i}")).collect();
        let text = pokerstars(&h, &HhInfo { hand_id: 42, table: "Test", names: &names, timestamp: "2026/11/01 12:00:00", hero_seat: Some(3) });
        assert!(text.starts_with("PokerStars Hand #42:  Hold'em No Limit ($0.50/$1 USD)"));
        assert!(text.contains("P3: raises $1.50 to $2.50"));
        assert!(text.contains("P2: calls $1.50"));
        assert!(text.contains("*** FLOP *** ["));
        assert!(text.contains("*** SUMMARY ***"));
        assert!(text.contains("Rake $0.27"), "{text}");
    }
}
