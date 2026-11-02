//! The fact sheet for one decision. Every number the coach (and Claude) talks about is
//! computed here; the language model only puts these facts into words.

use crate::postflop::NodeView;
use engine::card::*;
use engine::eval::{eval, Category};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Pair2 {
    pub hero: f64,
    pub villain: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FactSheet {
    pub street: String,
    pub position: String,
    pub pot_bb: f64,
    pub to_call_bb: f64,
    pub spr: f64,
    pub pot_odds_needed: Option<f64>,
    pub equity_vs_range: Option<f64>,
    pub hand_description: String,
    pub board_texture: String,
    pub range_advantage: Option<Pair2>,
    pub nut_advantage: Option<Pair2>,
    pub blockers: Vec<String>,
    pub mdf: Option<f64>,
    pub bluff_ratio: Option<f64>,
    pub solver_mix: String,
    pub villain_profile: Option<String>,
    pub notes: Vec<String>,
}

const RANK_NAMES: [&str; 13] = ["deuce", "three", "four", "five", "six", "seven", "eight", "nine", "ten", "jack", "queen", "king", "ace"];

/// Plain description of a holding on a board, e.g. "top pair, good kicker, with a flush draw".
pub fn hand_description(hole: (Card, Card), board: &[Card]) -> String {
    if board.is_empty() {
        let c = class_name(class_of(hole.0, hole.1));
        return c;
    }
    let mut cards = board.to_vec();
    cards.push(hole.0);
    cards.push(hole.1);
    let v = eval(&cards);
    let cat = Category::from_value(v);
    let board_v = if board.len() >= 5 { Some(eval(board)) } else { None };
    let mut board_ranks: Vec<u8> = board.iter().map(|&c| rank(c)).collect();
    board_ranks.sort_unstable_by(|a, b| b.cmp(a));
    board_ranks.dedup();
    let (h1, h2) = (rank(hole.0), rank(hole.1));
    let top = board_ranks[0];
    let pocket = h1 == h2;
    let mut made = match cat {
        Category::HighCard => {
            let hi = h1.max(h2);
            if hi > top {
                format!("{} high (overcards)", RANK_NAMES[hi as usize])
            } else {
                format!("{} high", RANK_NAMES[hi as usize])
            }
        }
        Category::Pair => {
            let pr = ((v >> 16) & 0xF) as u8;
            if pocket && pr == h1 {
                if h1 > top {
                    "overpair".to_string()
                } else if board_ranks.len() > 1 && h1 > board_ranks[1] {
                    "pocket pair below the top card".to_string()
                } else {
                    "underpair".to_string()
                }
            } else if pr != h1 && pr != h2 {
                "no pair (the pair is on the board)".to_string()
            } else {
                let kicker = if pr == h1 { h2 } else { h1 };
                let idx = board_ranks.iter().position(|&r| r == pr).unwrap_or(0);
                let which = match idx {
                    0 => "top pair",
                    1 => "second pair",
                    _ if idx == board_ranks.len() - 1 => "bottom pair",
                    _ => "middle pair",
                };
                if idx == 0 {
                    let k = if kicker >= 11 || (kicker == 10 && top == 12) { "good kicker" } else if kicker >= 8 { "medium kicker" } else { "weak kicker" };
                    format!("{which}, {k}")
                } else {
                    which.to_string()
                }
            }
        }
        Category::TwoPair => {
            let p1 = ((v >> 16) & 0xF) as u8;
            let p2 = ((v >> 12) & 0xF) as u8;
            let uses_both = [p1, p2].iter().all(|&p| p == h1 || p == h2) && !pocket;
            if uses_both {
                "two pair".to_string()
            } else if pocket {
                "a pocket pair plus the board pair".to_string()
            } else {
                "one pair plus the board pair".to_string()
            }
        }
        Category::Trips => {
            if pocket {
                "a set".to_string()
            } else {
                "trips".to_string()
            }
        }
        Category::Flush => {
            let fs = flush_suit(&cards).unwrap();
            let mine: Vec<u8> = [hole.0, hole.1].iter().filter(|&&c| suit(c) == fs).map(|&c| rank(c)).collect();
            if mine.is_empty() {
                "a flush on the board".to_string()
            } else if mine.contains(&12) || is_nut_flush(&mine, board, fs) {
                "the nut flush".to_string()
            } else {
                format!("a {}-high flush", RANK_NAMES[*mine.iter().max().unwrap() as usize])
            }
        }
        c => c.name().to_string(),
    };
    if let Some(bv) = board_v {
        if bv == v {
            made = format!("{made} (playing the board)");
        }
    }
    if board.len() < 5 {
        let draws = draws(hole, board);
        if !draws.is_empty() && cat <= Category::Trips {
            made = format!("{made}, with {}", draws.join(" and "));
        }
    }
    made
}

fn flush_suit(cards: &[Card]) -> Option<u8> {
    (0..4u8).find(|&s| cards.iter().filter(|&&c| suit(c) == s).count() >= 5)
}

fn is_nut_flush(mine: &[u8], board: &[Card], s: u8) -> bool {
    let on_board: Vec<u8> = board.iter().filter(|&&c| suit(c) == s).map(|&c| rank(c)).collect();
    let best_missing = (0..13u8).rev().find(|r| !on_board.contains(r));
    best_missing.map(|r| mine.contains(&r)).unwrap_or(false)
}

fn draws(hole: (Card, Card), board: &[Card]) -> Vec<String> {
    let mut out = Vec::new();
    let mut cards = board.to_vec();
    cards.push(hole.0);
    cards.push(hole.1);
    for s in 0..4u8 {
        let n = cards.iter().filter(|&&c| suit(c) == s).count();
        let mine = [hole.0, hole.1].iter().filter(|&&c| suit(c) == s).count();
        if n == 4 && mine >= 1 {
            let my_ranks: Vec<u8> = [hole.0, hole.1].iter().filter(|&&c| suit(c) == s).map(|&c| rank(c)).collect();
            out.push(if is_nut_flush(&my_ranks, board, s) { "the nut flush draw".to_string() } else { "a flush draw".to_string() });
        } else if n == 3 && mine >= 1 && board.len() == 3 {
            out.push("a backdoor flush draw".to_string());
        }
    }
    let mut ranks: u32 = 0;
    for &c in &cards {
        ranks |= 1 << rank(c);
    }
    let mut board_ranks: u32 = 0;
    for &c in board {
        board_ranks |= 1 << rank(c);
    }
    let m = (ranks << 1) | ((ranks >> 12) & 1);
    let mb = (board_ranks << 1) | ((board_ranks >> 12) & 1);
    // made straights are not draws
    let made = (0..10).any(|lo| (m >> lo) & 0b11111 == 0b11111);
    if !made {
        let mut oesd = false;
        let mut gut = false;
        for lo in 0..=10i32 {
            let win = (m >> lo) & 0b1111;
            let bwin = (mb >> lo) & 0b1111;
            if win == 0b1111 && bwin != 0b1111 {
                // open-ended when both the card below and above are possible ranks
                let below_ok = lo >= 1;
                let above_ok = lo + 4 <= 13;
                if below_ok && above_ok {
                    oesd = true;
                } else {
                    gut = true;
                }
            }
        }
        if !oesd {
            for lo in 0..10i32 {
                let win = (m >> lo) & 0b11111;
                let bwin = (mb >> lo) & 0b11111;
                if win.count_ones() == 4 && bwin.count_ones() < 4 {
                    gut = true;
                }
            }
        }
        if oesd {
            out.push("an open-ended straight draw".to_string());
        } else if gut {
            out.push("a gutshot".to_string());
        }
    }
    out
}

/// Plain board texture, e.g. "K-high, two-tone, disconnected".
pub fn board_texture(board: &[Card]) -> String {
    if board.is_empty() {
        return "preflop".into();
    }
    let flop = &board[..board.len().min(5)];
    let mut ranks: Vec<u8> = flop.iter().map(|&c| rank(c)).collect();
    ranks.sort_unstable_by(|a, b| b.cmp(a));
    let hi = RANK_CHARS[ranks[0] as usize];
    let mut suits = [0; 4];
    for &c in flop {
        suits[suit(c) as usize] += 1;
    }
    let max_suit = *suits.iter().max().unwrap();
    let suitedness = match (board.len(), max_suit) {
        (_, n) if n >= 5 => "five to a flush",
        (_, 4) => "four to a flush",
        (3, 3) | (_, 3) => if board.len() == 3 { "monotone" } else { "three to a flush" },
        (3, 2) => "two-tone",
        (_, 2) => "with flush draws possible",
        _ => "rainbow",
    };
    let mut distinct = ranks.clone();
    distinct.dedup();
    let paired = match ranks.len() - distinct.len() {
        0 => "",
        1 => ", paired",
        _ => ", double-paired or trips",
    };
    // connectedness: count distinct ranks within any 5-rank window
    let mut mask: u32 = 0;
    for &r in &distinct {
        mask |= 1 << r;
    }
    let m = (mask << 1) | ((mask >> 12) & 1);
    let best = (0..10).map(|lo| ((m >> lo) & 0b11111).count_ones()).max().unwrap_or(0);
    let conn = match best {
        0..=1 => "disconnected",
        2 => "somewhat connected",
        _ => "connected",
    };
    let wet = (max_suit >= 2 && best >= 2) || max_suit >= 3 || best >= 3;
    format!("{hi}-high, {suitedness}{paired}, {conn}{}", if wet { " (wet)" } else if best <= 1 && max_suit <= 1 && paired.is_empty() { " (dry)" } else { "" })
}

/// Range-vs-range facts from a solved node where the hero is the acting player.
pub fn range_facts(view: &NodeView, hero: usize, hero_hand: (Card, Card), board: &[Card]) -> (Option<Pair2>, Option<Pair2>, Vec<String>) {
    let (Some(eq_h), Some(eq_v)) = (view.equity.as_ref(), view.opp_equity.as_ref()) else {
        return (None, None, vec![]);
    };
    if view.player != hero {
        return (None, None, vec![]);
    }
    let vil = hero ^ 1;
    let avg = |eq: &[f32], w: &[f32]| {
        let t: f64 = w.iter().map(|&x| x as f64).sum();
        if t <= 0.0 { 0.0 } else { eq.iter().zip(w).map(|(&e, &x)| e as f64 * x as f64).sum::<f64>() / t }
    };
    let strong = |eq: &[f32], w: &[f32]| {
        let t: f64 = w.iter().map(|&x| x as f64).sum();
        if t <= 0.0 { 0.0 } else { eq.iter().zip(w).filter(|(&e, _)| e >= 0.8).map(|(_, &x)| x as f64).sum::<f64>() / t }
    };
    let ra = Pair2 { hero: avg(eq_h, &view.weights[hero]), villain: avg(eq_v, &view.weights[vil]) };
    let na = Pair2 { hero: strong(eq_h, &view.weights[hero]), villain: strong(eq_v, &view.weights[vil]) };
    // blockers: share of the villain's strong and weak combos that contain one of our cards
    let my = (1u64 << hero_hand.0) | (1u64 << hero_hand.1);
    let (mut s_tot, mut s_blk, mut w_tot, mut w_blk) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    for (i, &(a, b)) in view.hands[vil].iter().enumerate() {
        let w = view.weights[vil][i] as f64;
        if w <= 0.0 {
            continue;
        }
        let blocked = ((1u64 << a) | (1u64 << b)) & my != 0;
        if eq_v[i] >= 0.75 {
            s_tot += w;
            if blocked {
                s_blk += w;
            }
        } else if eq_v[i] < 0.3 {
            w_tot += w;
            if blocked {
                w_blk += w;
            }
        }
    }
    let mut blockers = Vec::new();
    if s_tot > 0.0 {
        blockers.push(format!(
            "your hand removes {:.0}% of the villain's strong combos and {:.0}% of their weak combos",
            100.0 * s_blk / s_tot,
            if w_tot > 0.0 { 100.0 * w_blk / w_tot } else { 0.0 }
        ));
    }
    // flush blockers on three-flush boards
    for s in 0..4u8 {
        if board.iter().filter(|&&c| suit(c) == s).count() >= 3 {
            for &c in &[hero_hand.0, hero_hand.1] {
                if suit(c) == s && !is_nut_flush(&[rank(c)], board, s) && rank(c) >= 10 {
                    blockers.push(format!("your {} blocks some of the best flushes", card_to_string(c)));
                } else if suit(c) == s && is_nut_flush(&[rank(c)], board, s) {
                    blockers.push(format!("your {} blocks the nut flush", card_to_string(c)));
                }
            }
        }
    }
    (Some(ra), Some(na), blockers)
}

/// Minimum defence frequency against a bet `bet` into a pot of `pot_before` (pot before the
/// bet): pot / (pot + bet).
pub fn mdf(pot_before: f64, bet: f64) -> f64 {
    pot_before / (pot_before + bet)
}

/// Bluff share of a polarized betting range that makes the caller indifferent:
/// bet / (pot + 2 * bet).
pub fn bluff_share(pot_before: f64, bet: f64) -> f64 {
    bet / (pot_before + 2.0 * bet)
}

pub fn mix_string(labels: &[String], freqs: &[f32]) -> String {
    labels
        .iter()
        .zip(freqs)
        .filter(|(_, &f)| f >= 0.005)
        .map(|(l, f)| format!("{l} {:.0}%", 100.0 * f))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    fn h(s: &str) -> (Card, Card) {
        let c = cards_from_str(s).unwrap();
        (c[0], c[1])
    }
    #[test]
    fn descriptions() {
        let b = cards_from_str("Kh7d2c").unwrap();
        assert_eq!(hand_description(h("AsKs"), &b), "top pair, good kicker");
        assert_eq!(hand_description(h("QsQd"), &b), "pocket pair below the top card");
        assert_eq!(hand_description(h("AsAd"), &b), "overpair");
        assert_eq!(hand_description(h("7s7c"), &b), "a set");
        let b2 = cards_from_str("Th9h2c").unwrap();
        assert_eq!(hand_description(h("AhKh"), &b2), "ace high (overcards), with the nut flush draw");
        assert_eq!(hand_description(h("QsJd"), &b2), "queen high (overcards), with an open-ended straight draw");
        assert_eq!(hand_description(h("Qs8d"), &b2), "queen high (overcards), with a gutshot");
        assert_eq!(hand_description(h("KsJd"), &b2), "king high (overcards), with a gutshot");
        assert_eq!(board_texture(&cards_from_str("Kh7d2c").unwrap()), "K-high, rainbow, disconnected (dry)");
        assert_eq!(board_texture(&b2), "T-high, two-tone, somewhat connected (wet)");
    }
    #[test]
    fn ratios() {
        assert!((mdf(100.0, 50.0) - 2.0 / 3.0).abs() < 1e-9);
        assert!((bluff_share(100.0, 100.0) - 1.0 / 3.0).abs() < 1e-9);
    }
}
