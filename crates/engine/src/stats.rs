//! Per-seat stat flags for one finished hand (the inputs for VPIP, PFR, 3-bet, c-bet, WTSD...)
//! and the all-in adjusted result.

use crate::card::Card;
use crate::equity::{equity, Holding};
use crate::rules::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SeatFlags {
    pub dealt: bool,
    pub vpip: bool,
    pub pfr: bool,
    pub three_bet_opp: bool,
    pub three_bet: bool,
    pub fold_to_3bet_opp: bool,
    pub fold_to_3bet: bool,
    pub saw_flop: bool,
    pub cbet_opp: bool,
    pub cbet: bool,
    pub fold_to_cbet_opp: bool,
    pub fold_to_cbet: bool,
    pub wtsd: bool,
    pub wsd: bool,
    pub post_bets: u32,
    pub post_raises: u32,
    pub post_calls: u32,
    pub net: Chips,
    /// net result replacing all-in runouts by their equity (chips, may be fractional)
    pub net_allin_adj: f64,
}

pub fn seat_flags(h: &Hand) -> Vec<SeatFlags> {
    let n = h.seats.len();
    let r = h.result.as_ref().expect("hand not finished");
    let mut f: Vec<SeatFlags> = (0..n).map(|i| SeatFlags { dealt: h.seats[i].in_hand, net: r.net[i], net_allin_adj: r.net[i] as f64, ..Default::default() }).collect();
    let acts: Vec<(usize, Street, ActKind)> = h
        .events
        .iter()
        .filter_map(|e| match e {
            Event::Act { seat, street, kind, .. } => Some((*seat, *street, *kind)),
            _ => None,
        })
        .collect();

    // preflop
    let mut raises = 0;
    let mut opener: Option<usize> = None;
    let mut last_raiser: Option<usize> = None;
    let mut seen_3bet_opp = vec![false; n];
    let mut seen_fold3_opp = vec![false; n];
    for &(seat, street, kind) in &acts {
        if street != Street::Preflop {
            continue;
        }
        if raises == 1 && Some(seat) != opener && !seen_3bet_opp[seat] {
            seen_3bet_opp[seat] = true;
            f[seat].three_bet_opp = true;
            if kind == ActKind::Raise {
                f[seat].three_bet = true;
            }
        }
        if raises == 2 && Some(seat) == opener && !seen_fold3_opp[seat] {
            seen_fold3_opp[seat] = true;
            f[seat].fold_to_3bet_opp = true;
            if kind == ActKind::Fold {
                f[seat].fold_to_3bet = true;
            }
        }
        match kind {
            ActKind::Call => f[seat].vpip = true,
            ActKind::Raise | ActKind::Bet => {
                f[seat].vpip = true;
                f[seat].pfr = true;
                raises += 1;
                if opener.is_none() {
                    opener = Some(seat);
                }
                last_raiser = Some(seat);
            }
            _ => {}
        }
    }

    // flop participation
    let flop_seen = h.board.len() >= 3;
    if flop_seen {
        let folded_pre: Vec<bool> = (0..n).map(|i| acts.iter().any(|&(s, st, k)| s == i && st == Street::Preflop && k == ActKind::Fold)).collect();
        for i in 0..n {
            if h.seats[i].in_hand && !folded_pre[i] {
                f[i].saw_flop = true;
            }
        }
    }
    let mut flop_bet_by: Option<usize> = None;
    let mut flop_raised = false;
    let mut cbet_seen = vec![false; n];
    for &(seat, street, kind) in &acts {
        if street == Street::Preflop {
            continue;
        }
        match kind {
            ActKind::Bet => f[seat].post_bets += 1,
            ActKind::Raise => f[seat].post_raises += 1,
            ActKind::Call => f[seat].post_calls += 1,
            _ => {}
        }
        if street != Street::Flop {
            continue;
        }
        if Some(seat) == last_raiser && flop_bet_by.is_none() && !cbet_seen[seat] {
            cbet_seen[seat] = true;
            f[seat].cbet_opp = true;
            if kind == ActKind::Bet {
                f[seat].cbet = true;
            }
        }
        if flop_bet_by.is_some() && flop_bet_by == last_raiser && !flop_raised && Some(seat) != last_raiser && !cbet_seen[seat] {
            cbet_seen[seat] = true;
            f[seat].fold_to_cbet_opp = true;
            if kind == ActKind::Fold {
                f[seat].fold_to_cbet = true;
            }
        }
        match kind {
            ActKind::Bet => flop_bet_by = Some(seat),
            ActKind::Raise => flop_raised = true,
            _ => {}
        }
    }
    if r.showdown {
        for i in 0..n {
            if f[i].saw_flop && h.seats[i].live() {
                f[i].wtsd = true;
                f[i].wsd = r.won[i] > 0;
            }
        }
    }
    for (i, adj) in allin_adjusted(h).into_iter().enumerate() {
        if let Some(a) = adj {
            f[i].net_allin_adj = a;
        }
    }
    f
}

/// When betting ended with players all-in before the river, each live player's expected net
/// over the runout (pot equity at the moment of the all-in minus contributions). None when
/// there was no all-in runout.
pub fn allin_adjusted(h: &Hand) -> Vec<Option<f64>> {
    let n = h.seats.len();
    let r = match &h.result {
        Some(r) => r,
        None => return vec![None; n],
    };
    if !r.showdown {
        return vec![None; n];
    }
    // board size when the last action happened
    let mut board_at_last_act = 0;
    let mut cards_seen = 0;
    for e in &h.events {
        match e {
            Event::Board { cards, .. } => cards_seen += cards.len(),
            Event::Act { .. } => board_at_last_act = cards_seen,
            _ => {}
        }
    }
    if board_at_last_act >= 5 || !h.seats.iter().any(|s| s.live() && s.all_in) {
        return vec![None; n];
    }
    let board: Vec<Card> = h.board[..board_at_last_act].to_vec();
    let mut exp_won = vec![0.0f64; n];
    for p in &r.pots {
        let elig: Vec<usize> = p.eligible.clone();
        let amount = (p.amount - p.rake) as f64;
        if elig.len() == 1 {
            exp_won[elig[0]] += amount;
            continue;
        }
        let holdings: Vec<Holding> = elig.iter().map(|&i| Holding::Hand(h.seats[i].hole[0], h.seats[i].hole[1])).collect();
        let dead: Vec<Card> = (0..n).filter(|i| h.seats[*i].in_hand && !elig.contains(i)).flat_map(|i| h.seats[i].hole).collect();
        let _ = dead;
        let e = equity(&holdings, &board, &[], 200_000, 2_000_000);
        for (k, &i) in elig.iter().enumerate() {
            exp_won[i] += e.equity[k] * amount;
        }
    }
    (0..n).map(|i| if h.seats[i].in_hand { Some(exp_won[i] - h.seats[i].total as f64) } else { None }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_3bet_fold_and_cbet() {
        // 6-max, button 0; UTG (3) opens, CO (5) 3-bets, UTG folds
        let mut h = Hand::start(TableConfig::six_max(), &[Some(10_000); 6], 0, (0..52).collect());
        h.apply(3, Act::BetTo(250)).unwrap();
        h.apply(4, Act::Fold).unwrap();
        h.apply(5, Act::BetTo(750)).unwrap();
        h.apply(0, Act::Fold).unwrap();
        h.apply(1, Act::Fold).unwrap();
        h.apply(2, Act::Fold).unwrap();
        h.apply(3, Act::Fold).unwrap();
        let f = seat_flags(&h);
        assert!(f[3].pfr && f[3].vpip && f[3].fold_to_3bet_opp && f[3].fold_to_3bet);
        assert!(f[5].three_bet_opp && f[5].three_bet);
        assert!(f[4].three_bet_opp && !f[4].three_bet);
        assert!(!f[2].vpip);

        // BTN opens, BB calls, BB checks, BTN c-bets, BB folds
        let mut h = Hand::start(TableConfig::six_max(), &[Some(10_000); 6], 0, (0..52).collect());
        for s in [3, 4, 5] {
            h.apply(s, Act::Fold).unwrap();
        }
        h.apply(0, Act::BetTo(250)).unwrap();
        h.apply(1, Act::Fold).unwrap();
        h.apply(2, Act::Call).unwrap();
        h.apply(2, Act::Check).unwrap();
        h.apply(0, Act::BetTo(200)).unwrap();
        h.apply(2, Act::Fold).unwrap();
        let f = seat_flags(&h);
        assert!(f[0].cbet_opp && f[0].cbet);
        assert!(f[2].fold_to_cbet_opp && f[2].fold_to_cbet && f[2].saw_flop && f[2].vpip);
        assert_eq!(f[0].post_bets, 1);
    }
}
