//! What a bot is allowed to see: public table state plus its own hole cards. Built from the
//! engine's hand state by dropping every other player's cards and the deck.

use engine::card::Card;
use engine::rules::*;
use crate::postflop::LineAction;

#[derive(Clone, Debug)]
pub struct PublicAction {
    pub street: Street,
    pub seat: usize,
    pub kind: ActKind,
    pub to: i64,
    pub all_in: bool,
    /// current bet facing the player before this action
    pub facing: i64,
}

#[derive(Clone, Debug)]
pub struct PublicState {
    pub n: usize,
    pub button: usize,
    pub big_blind: i64,
    /// preflop position index per seat (0 UTG .. 5 BB), `usize::MAX` for empty seats
    pub positions: Vec<usize>,
    pub in_hand: Vec<bool>,
    pub folded: Vec<bool>,
    pub all_in: Vec<bool>,
    pub stacks: Vec<i64>,
    pub committed: Vec<i64>,
    pub totals: Vec<i64>,
    pub board: Vec<Card>,
    pub street: Street,
    pub actions: Vec<PublicAction>,
    pub to_act: Option<usize>,
    pub legal: Option<Legal>,
    pub current_bet: i64,
    pub pot: i64,
}

pub const POS_NAMES: [&str; 6] = ["UTG", "HJ", "CO", "BTN", "SB", "BB"];

/// Positions by preflop action order; with fewer than six players the early positions drop.
pub fn positions(h: &Hand) -> Vec<usize> {
    let n = h.seats.len();
    let mut order = Vec::new();
    let mut s = h.next_in_hand(h.bb_seat);
    let active = h.seats.iter().filter(|x| x.in_hand).count();
    for _ in 0..active {
        order.push(s);
        s = h.next_in_hand(s);
    }
    let mut out = vec![usize::MAX; n];
    if active == 2 {
        out[h.button] = 4;
        out[h.bb_seat] = 5;
        return out;
    }
    for (i, &seat) in order.iter().enumerate() {
        out[seat] = 6 - active + i;
    }
    out
}

impl PublicState {
    pub fn from_hand(h: &Hand) -> PublicState {
        let mut actions = Vec::new();
        let mut bet = h.config.big_blind;
        let mut street = Street::Preflop;
        for e in &h.events {
            match e {
                Event::Board { street: s, .. } => {
                    street = *s;
                    bet = 0;
                }
                Event::Act { seat, street: s, kind, to, all_in, .. } => {
                    if *s != street {
                        street = *s;
                        bet = 0;
                    }
                    actions.push(PublicAction { street: *s, seat: *seat, kind: *kind, to: *to, all_in: *all_in, facing: bet });
                    if matches!(kind, ActKind::Bet | ActKind::Raise) {
                        bet = *to;
                    }
                }
                _ => {}
            }
        }
        PublicState {
            n: h.seats.len(),
            button: h.button,
            big_blind: h.config.big_blind,
            positions: positions(h),
            in_hand: h.seats.iter().map(|s| s.in_hand).collect(),
            folded: h.seats.iter().map(|s| s.folded).collect(),
            all_in: h.seats.iter().map(|s| s.all_in).collect(),
            stacks: h.seats.iter().map(|s| s.stack).collect(),
            committed: h.seats.iter().map(|s| s.committed).collect(),
            totals: h.seats.iter().map(|s| s.total).collect(),
            board: h.board.clone(),
            street: h.street,
            actions,
            to_act: h.to_act,
            legal: h.legal(),
            current_bet: h.current_bet,
            pot: h.pot_total(),
        }
    }

    pub fn live_seats(&self) -> Vec<usize> {
        (0..self.n).filter(|&i| self.in_hand[i] && !self.folded[i]).collect()
    }

    /// Preflop action codes in order (F, K, C, R, A) for chart lookup.
    pub fn preflop_codes(&self) -> Vec<char> {
        self.actions
            .iter()
            .filter(|a| a.street == Street::Preflop)
            .map(|a| match a.kind {
                ActKind::Fold => 'F',
                ActKind::Check => 'K',
                ActKind::Call => 'C',
                ActKind::Bet | ActKind::Raise => if a.all_in { 'A' } else { 'R' },
            })
            .collect()
    }

    /// Actions on the current street as solver lines.
    pub fn street_line(&self, street: Street) -> Vec<LineAction> {
        to_line(self.actions.iter().filter(|a| a.street == street))
    }

    /// Stack behind at the start of the current street, per seat.
    pub fn stack_at_street_start(&self, seat: usize) -> i64 {
        self.stacks[seat] + self.committed[seat]
    }

    /// Number of limpers (voluntary preflop calls of the big blind before any raise), not
    /// counting the small blind completing.
    pub fn limpers(&self) -> usize {
        let mut n = 0;
        for a in self.actions.iter().filter(|a| a.street == Street::Preflop) {
            match a.kind {
                ActKind::Bet | ActKind::Raise => break,
                ActKind::Call => n += 1,
                _ => {}
            }
        }
        n
    }
}

pub fn to_line<'a>(acts: impl Iterator<Item = &'a PublicAction>) -> Vec<LineAction> {
    acts.map(|a| match a.kind {
        ActKind::Fold => LineAction::Fold,
        ActKind::Check => LineAction::Check,
        ActKind::Call => LineAction::Call,
        ActKind::Bet | ActKind::Raise if a.all_in => LineAction::AllIn(a.to as i32),
        ActKind::Bet => LineAction::Bet(a.to as i32),
        ActKind::Raise => LineAction::Raise(a.to as i32),
    })
    .collect()
}
