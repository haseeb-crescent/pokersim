//! No-limit hold'em rules for one hand at a ring-game table.
//!
//! Covered exactly:
//! - Button, blinds and action order, including heads-up (button posts the small blind and acts
//!   first preflop, last postflop) and short-stacked blinds (the full big blind must still be
//!   matched).
//! - Minimum raises: a raise must increase the bet by at least the largest bet/raise increment
//!   of the round (at least one big blind). An all-in for less is allowed, but it does not
//!   reopen the betting for a player who has already acted unless, cumulatively, that player now
//!   faces at least a full raise since their last action (TDA rule 43).
//! - Uncalled bets are returned at the end of each betting round.
//! - Side pots from total contributions (folded players' chips stay in the pots), split pots,
//!   odd chips to the first winner clockwise from the button, rake (percentage with a cap,
//!   no flop no drop) taken proportionally from each pot.
//! - Showdown order: the last aggressor of the final betting round shows first, otherwise the
//!   first live player clockwise from the button; later players show only if they can win or
//!   tie a pot they are in, otherwise they muck. Hands are tabled when someone is all-in and
//!   betting is over.

use crate::card::*;
use crate::eval::{describe, eval, HandValue};
use serde::{Deserialize, Serialize};

pub type Chips = i64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Street {
    Preflop = 0,
    Flop = 1,
    Turn = 2,
    River = 3,
}

impl Street {
    pub fn next(self) -> Option<Street> {
        match self {
            Street::Preflop => Some(Street::Flop),
            Street::Flop => Some(Street::Turn),
            Street::Turn => Some(Street::River),
            Street::River => None,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Street::Preflop => "preflop",
            Street::Flop => "flop",
            Street::Turn => "turn",
            Street::River => "river",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RakeConfig {
    /// e.g. 0.05 for 5%
    pub percent: f64,
    /// cap in chips
    pub cap: Chips,
    pub no_flop_no_drop: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TableConfig {
    pub seats: usize,
    pub small_blind: Chips,
    pub big_blind: Chips,
    pub ante: Chips,
    pub rake: RakeConfig,
}

impl TableConfig {
    /// 6-max with 1bb = 100 chips, 5% rake capped at 3bb.
    pub fn six_max() -> Self {
        TableConfig {
            seats: 6,
            small_blind: 50,
            big_blind: 100,
            ante: 0,
            rake: RakeConfig { percent: 0.05, cap: 300, no_flop_no_drop: true },
        }
    }
}

/// A player's decision.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "to")]
pub enum Act {
    Fold,
    Check,
    Call,
    /// Bet or raise to this total amount committed on the current street.
    /// Committing the whole stack is always allowed (all-in).
    BetTo(Chips),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActKind {
    Fold,
    Check,
    Call,
    Bet,
    Raise,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "e")]
pub enum Event {
    Blind { seat: usize, big: bool, amount: Chips, all_in: bool },
    Ante { seat: usize, amount: Chips },
    Act { seat: usize, street: Street, kind: ActKind, added: Chips, to: Chips, all_in: bool },
    Board { street: Street, cards: Vec<Card> },
    Returned { seat: usize, amount: Chips },
    Show { seat: usize, cards: [Card; 2], value: HandValue, description: String },
    Muck { seat: usize },
    Win { seat: usize, pot: usize, amount: Chips },
    Rake { amount: Chips },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SeatState {
    /// whether this seat is dealt into the hand
    pub in_hand: bool,
    pub stack: Chips,
    pub starting_stack: Chips,
    pub committed: Chips,
    pub total: Chips,
    pub folded: bool,
    pub all_in: bool,
    pub hole: [Card; 2],
    pub has_acted: bool,
    /// the bet level this player last acted at (used for the reopening rule)
    pub faced: Chips,
}

impl SeatState {
    pub fn live(&self) -> bool {
        self.in_hand && !self.folded
    }
    pub fn can_act(&self) -> bool {
        self.in_hand && !self.folded && !self.all_in
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Legal {
    pub seat: usize,
    pub can_fold: bool,
    pub can_check: bool,
    /// chips needed to call (already capped at the stack); 0 when checking is possible
    pub call_amount: Chips,
    pub call_is_all_in: bool,
    /// smallest legal bet/raise-to, if raising is possible
    pub min_to: Option<Chips>,
    /// largest bet/raise-to (all-in), if raising is possible
    pub max_to: Option<Chips>,
    pub current_bet: Chips,
    pub committed: Chips,
    pub pot: Chips,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Pot {
    pub amount: Chips,
    pub rake: Chips,
    pub eligible: Vec<usize>,
    pub winners: Vec<usize>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HandResult {
    pub pots: Vec<Pot>,
    pub rake: Chips,
    /// chips received from pots (after rake) per seat
    pub won: Vec<Chips>,
    /// won - contributed, per seat
    pub net: Vec<Chips>,
    pub showdown: bool,
    /// seats whose cards were shown at showdown
    pub shown: Vec<usize>,
    pub values: Vec<Option<HandValue>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleError {
    NotYourTurn,
    HandOver,
    Illegal(String),
}

impl std::fmt::Display for RuleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RuleError::NotYourTurn => write!(f, "not this seat's turn"),
            RuleError::HandOver => write!(f, "hand is over"),
            RuleError::Illegal(s) => write!(f, "illegal action: {s}"),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Hand {
    pub config: TableConfig,
    pub seats: Vec<SeatState>,
    pub button: usize,
    pub sb_seat: Option<usize>,
    pub bb_seat: usize,
    pub board: Vec<Card>,
    pub deck: Vec<Card>,
    pub deck_pos: usize,
    pub street: Street,
    pub to_act: Option<usize>,
    pub current_bet: Chips,
    pub min_raise: Chips,
    /// bets and raises on the current street (preflop the big blind counts as the first bet)
    pub bets_this_street: u32,
    pub last_aggressor: Option<usize>,
    /// last aggressor per street
    pub aggressors: [Option<usize>; 4],
    pub events: Vec<Event>,
    pub result: Option<HandResult>,
}

impl Hand {
    /// Starts a hand: posts antes and blinds and deals hole cards.
    ///
    /// `stacks[i]` is `None` for an empty or sitting-out seat. The button must be on an
    /// occupied seat. `deck` is the full shuffled deck order.
    pub fn start(config: TableConfig, stacks: &[Option<Chips>], button: usize, deck: Vec<Card>) -> Hand {
        let n = config.seats;
        assert_eq!(stacks.len(), n);
        assert_eq!(deck.len(), 52);
        let seats: Vec<SeatState> = stacks
            .iter()
            .map(|s| SeatState {
                in_hand: matches!(s, Some(x) if *x > 0),
                stack: s.unwrap_or(0),
                starting_stack: s.unwrap_or(0),
                committed: 0,
                total: 0,
                folded: false,
                all_in: false,
                hole: [0, 0],
                has_acted: false,
                faced: 0,
            })
            .collect();
        let active: Vec<usize> = (0..n).filter(|&i| seats[i].in_hand).collect();
        assert!(active.len() >= 2, "need at least two players");
        assert!(seats[button].in_hand, "button must be on an occupied seat");

        let mut h = Hand {
            config,
            seats,
            button,
            sb_seat: None,
            bb_seat: 0,
            board: Vec::new(),
            deck,
            deck_pos: 0,
            street: Street::Preflop,
            to_act: None,
            current_bet: 0,
            min_raise: 0,
            bets_this_street: 1,
            last_aggressor: None,
            aggressors: [None; 4],
            events: Vec::new(),
            result: None,
        };

        let (sb, bb) = if active.len() == 2 {
            (button, h.next_in_hand(button))
        } else {
            let sb = h.next_in_hand(button);
            (sb, h.next_in_hand(sb))
        };
        h.sb_seat = Some(sb);
        h.bb_seat = bb;

        if h.config.ante > 0 {
            for &i in &active {
                let a = h.config.ante.min(h.seats[i].stack);
                h.seats[i].stack -= a;
                h.seats[i].total += a;
                if h.seats[i].stack == 0 {
                    h.seats[i].all_in = true;
                }
                h.events.push(Event::Ante { seat: i, amount: a });
            }
        }
        let sbv = h.config.small_blind;
        let bbv = h.config.big_blind;
        h.post_blind(sb, sbv, false);
        h.post_blind(bb, bbv, true);
        h.current_bet = bbv;
        h.min_raise = bbv;

        // deal two rounds starting left of the button
        let mut order = Vec::new();
        let mut s = h.next_in_hand(button);
        for _ in 0..active.len() {
            order.push(s);
            s = h.next_in_hand(s);
        }
        for round in 0..2 {
            for &seat in &order {
                let c = h.draw();
                h.seats[seat].hole[round] = c;
            }
        }

        let first = if active.len() == 2 { button } else { h.next_in_hand(bb) };
        h.to_act = h.find_actor_from(first);
        if h.to_act.is_none() {
            h.end_round();
        }
        h
    }

    fn post_blind(&mut self, seat: usize, amount: Chips, big: bool) {
        let s = &mut self.seats[seat];
        let a = amount.min(s.stack);
        s.stack -= a;
        s.committed += a;
        s.total += a;
        if s.stack == 0 {
            s.all_in = true;
        }
        self.events.push(Event::Blind { seat, big, amount: a, all_in: s.all_in });
    }

    fn draw(&mut self) -> Card {
        let c = self.deck[self.deck_pos];
        self.deck_pos += 1;
        c
    }

    pub fn num_seats(&self) -> usize {
        self.seats.len()
    }

    pub fn next_in_hand(&self, from: usize) -> usize {
        let n = self.seats.len();
        let mut i = (from + 1) % n;
        while !self.seats[i].in_hand {
            i = (i + 1) % n;
        }
        i
    }

    fn needs_action(&self, i: usize) -> bool {
        let s = &self.seats[i];
        s.can_act() && (!s.has_acted || s.committed < self.current_bet)
    }

    /// First seat, starting at `start` and going clockwise, that still has to act.
    fn find_actor_from(&self, start: usize) -> Option<usize> {
        let n = self.seats.len();
        // betting is over if at most one player can act and nobody owes chips
        let can_act = (0..n).filter(|&i| self.seats[i].can_act()).count();
        if self.live_count() <= 1 {
            return None;
        }
        if can_act == 0 {
            return None;
        }
        if can_act == 1 {
            let i = (0..n).find(|&i| self.seats[i].can_act()).unwrap();
            // a lone player with chips only acts if facing a bet
            return if self.seats[i].committed < self.current_bet { Some(i) } else { None };
        }
        for k in 0..n {
            let i = (start + k) % n;
            if self.needs_action(i) {
                return Some(i);
            }
        }
        None
    }

    pub fn live_count(&self) -> usize {
        self.seats.iter().filter(|s| s.live()).count()
    }

    pub fn pot_total(&self) -> Chips {
        self.seats.iter().map(|s| s.total).sum()
    }

    pub fn is_over(&self) -> bool {
        self.result.is_some()
    }

    /// Whether `seat` may raise right now (ignoring stack size).
    fn raise_reopened(&self, seat: usize) -> bool {
        let s = &self.seats[seat];
        !s.has_acted || self.current_bet - s.faced >= self.min_raise
    }

    pub fn legal(&self) -> Option<Legal> {
        let seat = self.to_act?;
        let s = &self.seats[seat];
        let to_call = (self.current_bet - s.committed).max(0);
        let call_amount = to_call.min(s.stack);
        let can_raise = s.stack > to_call && self.raise_reopened(seat) && self.others_can_respond(seat);
        let max_to = s.committed + s.stack;
        let min_to = if self.current_bet == 0 {
            self.config.big_blind.min(max_to)
        } else {
            (self.current_bet + self.min_raise).min(max_to)
        };
        Some(Legal {
            seat,
            can_fold: true,
            can_check: to_call == 0,
            call_amount,
            call_is_all_in: to_call > 0 && call_amount == s.stack,
            min_to: can_raise.then_some(min_to),
            max_to: can_raise.then_some(max_to),
            current_bet: self.current_bet,
            committed: s.committed,
            pot: self.pot_total(),
        })
    }

    /// Raising is pointless (and not offered) when every other live player is all-in.
    fn others_can_respond(&self, seat: usize) -> bool {
        self.seats.iter().enumerate().any(|(i, s)| i != seat && s.can_act())
    }

    pub fn apply(&mut self, seat: usize, act: Act) -> Result<(), RuleError> {
        if self.result.is_some() {
            return Err(RuleError::HandOver);
        }
        if self.to_act != Some(seat) {
            return Err(RuleError::NotYourTurn);
        }
        let legal = self.legal().unwrap();
        let street = self.street;
        match act {
            Act::Fold => {
                self.seats[seat].folded = true;
                self.events.push(Event::Act { seat, street, kind: ActKind::Fold, added: 0, to: self.seats[seat].committed, all_in: false });
            }
            Act::Check => {
                if !legal.can_check {
                    return Err(RuleError::Illegal("cannot check facing a bet".into()));
                }
                self.events.push(Event::Act { seat, street, kind: ActKind::Check, added: 0, to: self.seats[seat].committed, all_in: false });
            }
            Act::Call => {
                if legal.can_check {
                    return Err(RuleError::Illegal("nothing to call".into()));
                }
                let pay = legal.call_amount;
                let s = &mut self.seats[seat];
                s.stack -= pay;
                s.committed += pay;
                s.total += pay;
                if s.stack == 0 {
                    s.all_in = true;
                }
                let (to, ai) = (s.committed, s.all_in);
                self.events.push(Event::Act { seat, street, kind: ActKind::Call, added: pay, to, all_in: ai });
            }
            Act::BetTo(to) => {
                let (Some(min_to), Some(max_to)) = (legal.min_to, legal.max_to) else {
                    return Err(RuleError::Illegal("raising is not allowed here".into()));
                };
                if to > max_to {
                    return Err(RuleError::Illegal(format!("{to} exceeds the stack (max {max_to})")));
                }
                if to < min_to && to != max_to {
                    return Err(RuleError::Illegal(format!("minimum is {min_to}")));
                }
                if to <= self.current_bet {
                    return Err(RuleError::Illegal("a raise must exceed the current bet".into()));
                }
                let kind = if self.current_bet == 0 { ActKind::Bet } else { ActKind::Raise };
                let increment = to - self.current_bet;
                let full = if self.current_bet == 0 { to >= self.config.big_blind.min(max_to) && to >= min_to } else { increment >= self.min_raise };
                let s = &mut self.seats[seat];
                let pay = to - s.committed;
                s.stack -= pay;
                s.committed = to;
                s.total += pay;
                if s.stack == 0 {
                    s.all_in = true;
                }
                let ai = s.all_in;
                if full {
                    self.min_raise = if self.current_bet == 0 { to.max(self.config.big_blind) } else { increment };
                }
                self.current_bet = to;
                self.bets_this_street += 1;
                self.last_aggressor = Some(seat);
                self.aggressors[street as usize] = Some(seat);
                self.events.push(Event::Act { seat, street, kind, added: pay, to, all_in: ai });
            }
        }
        let s = &mut self.seats[seat];
        s.has_acted = true;
        s.faced = self.current_bet;

        if self.live_count() == 1 {
            self.finish_by_fold();
            return Ok(());
        }
        let next = self.next_in_hand(seat);
        self.to_act = self.find_actor_from(next);
        if self.to_act.is_none() {
            self.end_round();
        }
        Ok(())
    }

    /// Returns the part of the largest bet nobody matched this round.
    fn return_uncalled(&mut self) {
        let n = self.seats.len();
        let mut order: Vec<usize> = (0..n).filter(|&i| self.seats[i].in_hand).collect();
        order.sort_by_key(|&i| std::cmp::Reverse(self.seats[i].committed));
        if order.len() < 2 {
            return;
        }
        let top = order[0];
        let second = self.seats[order[1]].committed;
        let excess = self.seats[top].committed - second;
        if excess > 0 {
            let s = &mut self.seats[top];
            s.committed -= excess;
            s.total -= excess;
            s.stack += excess;
            if s.stack > 0 {
                s.all_in = false;
            }
            self.events.push(Event::Returned { seat: top, amount: excess });
        }
    }

    fn end_round(&mut self) {
        self.return_uncalled();
        for s in &mut self.seats {
            s.committed = 0;
            s.has_acted = false;
            s.faced = 0;
        }
        self.current_bet = 0;
        self.min_raise = self.config.big_blind;
        self.bets_this_street = 0;
        loop {
            let Some(next) = self.street.next() else {
                self.showdown();
                return;
            };
            self.street = next;
            let cards: Vec<Card> = match next {
                Street::Flop => {
                    let _burn = self.draw();
                    vec![self.draw(), self.draw(), self.draw()]
                }
                _ => {
                    let _burn = self.draw();
                    vec![self.draw()]
                }
            };
            self.board.extend_from_slice(&cards);
            self.events.push(Event::Board { street: next, cards });
            let first = self.next_in_hand(self.button);
            self.to_act = self.find_actor_from(first);
            if self.to_act.is_some() {
                return;
            }
        }
    }

    fn finish_by_fold(&mut self) {
        self.return_uncalled();
        let n = self.seats.len();
        let winner = (0..n).find(|&i| self.seats[i].live()).unwrap();
        let total = self.pot_total();
        let rake = self.compute_rake(total);
        let mut won = vec![0; n];
        won[winner] = total - rake;
        if rake > 0 {
            self.events.push(Event::Rake { amount: rake });
        }
        self.events.push(Event::Win { seat: winner, pot: 0, amount: total - rake });
        let eligible = vec![winner];
        self.finish(vec![Pot { amount: total, rake, eligible, winners: vec![winner] }], rake, won, false, vec![], vec![None; n]);
    }

    fn compute_rake(&self, total: Chips) -> Chips {
        let r = &self.config.rake;
        if r.no_flop_no_drop && self.board.is_empty() {
            return 0;
        }
        (((total as f64) * r.percent).floor() as Chips).min(r.cap).max(0)
    }

    /// Main pot and side pots from total contributions. Each pot lists the live players eligible
    /// to win it.
    pub fn build_pots(&self) -> Vec<Pot> {
        let n = self.seats.len();
        let mut levels: Vec<Chips> =
            (0..n).filter(|&i| self.seats[i].live()).map(|i| self.seats[i].total).filter(|&t| t > 0).collect();
        levels.sort_unstable();
        levels.dedup();
        let mut pots = Vec::new();
        let mut prev = 0;
        for &lvl in &levels {
            let amount: Chips = self.seats.iter().map(|s| s.total.min(lvl) - s.total.min(prev)).sum();
            let eligible: Vec<usize> = (0..n).filter(|&i| self.seats[i].live() && self.seats[i].total >= lvl).collect();
            if amount > 0 {
                pots.push(Pot { amount, rake: 0, eligible, winners: vec![] });
            }
            prev = lvl;
        }
        // chips folded players put in above the top live level (cannot normally happen after
        // uncalled bets are returned) go to the last pot
        let extra: Chips = self.seats.iter().map(|s| (s.total - prev).max(0)).sum();
        if extra > 0 {
            if let Some(p) = pots.last_mut() {
                p.amount += extra;
            }
        }
        pots
    }

    /// Seat order starting with the first seat left of the button.
    fn order_from_button(&self) -> Vec<usize> {
        let n = self.seats.len();
        (1..=n).map(|k| (self.button + k) % n).collect()
    }

    fn showdown(&mut self) {
        self.to_act = None;
        let n = self.seats.len();
        let mut pots = self.build_pots();
        let total: Chips = pots.iter().map(|p| p.amount).sum();
        let rake = self.compute_rake(total);
        // proportional rake, remainder from the main pot
        let mut taken = 0;
        for p in pots.iter_mut() {
            p.rake = ((rake as i128 * p.amount as i128) / total.max(1) as i128) as Chips;
            taken += p.rake;
        }
        if let Some(p) = pots.first_mut() {
            p.rake += rake - taken;
        }
        if rake > 0 {
            self.events.push(Event::Rake { amount: rake });
        }

        let mut values: Vec<Option<HandValue>> = vec![None; n];
        for i in 0..n {
            if self.seats[i].live() {
                let mut cards = self.board.clone();
                cards.extend_from_slice(&self.seats[i].hole);
                values[i] = Some(eval(&cards));
            }
        }

        // showdown order and mucking
        let any_all_in = self.seats.iter().any(|s| s.live() && s.all_in);
        let river_aggr = self.aggressors[Street::River as usize].filter(|&a| self.seats[a].live());
        let mut order: Vec<usize> = Vec::new();
        let start = match river_aggr {
            Some(a) => a,
            None => self.next_in_hand(self.button),
        };
        for k in 0..n {
            let i = (start + k) % n;
            if self.seats[i].live() {
                order.push(i);
            }
        }
        let mut shown = Vec::new();
        let mut best_shown: Vec<Option<HandValue>> = vec![None; pots.len()];
        for &i in &order {
            let v = values[i].unwrap();
            let can_win = pots.iter().enumerate().any(|(k, p)| p.eligible.contains(&i) && best_shown[k].map_or(true, |b| v >= b));
            if any_all_in || shown.is_empty() || can_win {
                shown.push(i);
                for (k, p) in pots.iter().enumerate() {
                    if p.eligible.contains(&i) {
                        best_shown[k] = Some(best_shown[k].map_or(v, |b| b.max(v)));
                    }
                }
                let cards = self.seats[i].hole;
                self.events.push(Event::Show { seat: i, cards, value: v, description: describe(v) });
            } else {
                self.events.push(Event::Muck { seat: i });
            }
        }

        let mut won = vec![0; n];
        let clockwise = self.order_from_button();
        for (k, p) in pots.iter_mut().enumerate() {
            let best = p.eligible.iter().map(|&i| values[i].unwrap()).max().unwrap();
            let mut winners: Vec<usize> = clockwise.iter().copied().filter(|i| p.eligible.contains(i) && values[*i] == Some(best)).collect();
            if winners.is_empty() {
                winners = p.eligible.clone();
            }
            let amount = p.amount - p.rake;
            let share = amount / winners.len() as Chips;
            let mut odd = amount - share * winners.len() as Chips;
            for &w in &winners {
                let mut x = share;
                if odd > 0 {
                    x += 1;
                    odd -= 1;
                }
                won[w] += x;
                self.events.push(Event::Win { seat: w, pot: k, amount: x });
            }
            p.winners = winners;
        }
        self.finish(pots, rake, won, true, shown, values);
    }

    fn finish(&mut self, pots: Vec<Pot>, rake: Chips, won: Vec<Chips>, showdown: bool, shown: Vec<usize>, values: Vec<Option<HandValue>>) {
        self.to_act = None;
        let n = self.seats.len();
        let net: Vec<Chips> = (0..n).map(|i| won[i] - self.seats[i].total).collect();
        for i in 0..n {
            self.seats[i].stack += won[i];
        }
        self.result = Some(HandResult { pots, rake, won, net, showdown, shown, values });
    }

    /// Final stacks after the hand.
    pub fn stacks(&self) -> Vec<Chips> {
        self.seats.iter().map(|s| s.stack).collect()
    }

    /// Actions so far on the given street, in order.
    pub fn street_actions(&self, street: Street) -> Vec<(usize, ActKind, Chips, bool)> {
        self.events
            .iter()
            .filter_map(|e| match e {
                Event::Act { seat, street: s, kind, to, all_in, .. } if *s == street => Some((*seat, *kind, *to, *all_in)),
                _ => None,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(seats: usize) -> TableConfig {
        let mut c = TableConfig::six_max();
        c.seats = seats;
        c
    }

    /// A deck that deals the given hole cards (in dealing order starting left of the button,
    /// first card to everyone then second card) and board, with burns.
    fn deck_for(holes: &[(&str, &str)], board: &str) -> Vec<Card> {
        let mut d = Vec::new();
        for &(a, _) in holes {
            d.push(card_from_str(a).unwrap());
        }
        for &(_, b) in holes {
            d.push(card_from_str(b).unwrap());
        }
        let b = cards_from_str(board).unwrap();
        let mut rest: Vec<Card> = (0..52).filter(|c| !d.contains(c) && !b.contains(c)).collect();
        let mut burn = || rest.remove(0);
        let mut out = d.clone();
        out.push(burn());
        out.extend_from_slice(&b[0..3]);
        out.push(burn());
        out.push(b[3]);
        out.push(burn());
        out.push(b[4]);
        out.extend(rest);
        assert_eq!(out.len(), 52);
        out
    }

    #[test]
    fn blinds_and_order_six_max() {
        let h = Hand::start(cfg(6), &[Some(10_000); 6], 0, (0..52).collect());
        assert_eq!(h.sb_seat, Some(1));
        assert_eq!(h.bb_seat, 2);
        assert_eq!(h.to_act, Some(3)); // UTG
        assert_eq!(h.seats[1].committed, 50);
        assert_eq!(h.seats[2].committed, 100);
        let l = h.legal().unwrap();
        assert_eq!(l.call_amount, 100);
        assert_eq!(l.min_to, Some(200));
    }

    #[test]
    fn heads_up_button_is_small_blind() {
        let mut h = Hand::start(cfg(6), &[None, Some(10_000), None, Some(10_000), None, None], 1, (0..52).collect());
        assert_eq!(h.sb_seat, Some(1));
        assert_eq!(h.bb_seat, 3);
        assert_eq!(h.to_act, Some(1)); // button acts first preflop
        h.apply(1, Act::Call).unwrap();
        assert_eq!(h.to_act, Some(3)); // BB option
        h.apply(3, Act::Check).unwrap();
        assert_eq!(h.street, Street::Flop);
        assert_eq!(h.to_act, Some(3)); // BB first postflop
    }

    #[test]
    fn big_blind_option_and_min_raise() {
        let mut h = Hand::start(cfg(3), &[Some(10_000); 3], 0, (0..52).collect());
        // seats: 0 btn, 1 sb, 2 bb
        assert_eq!(h.to_act, Some(0));
        h.apply(0, Act::Call).unwrap();
        h.apply(1, Act::Call).unwrap();
        let l = h.legal().unwrap();
        assert_eq!(l.seat, 2);
        assert!(l.can_check);
        assert_eq!(l.min_to, Some(200));
        // raise to 350 (increment 250) then the min re-raise is 600
        h.apply(2, Act::BetTo(350)).unwrap();
        let l = h.legal().unwrap();
        assert_eq!(l.min_to, Some(600));
        assert!(h.apply(0, Act::BetTo(599)).is_err());
        h.apply(0, Act::BetTo(600)).unwrap();
        assert_eq!(h.legal().unwrap().min_to, Some(850));
    }

    #[test]
    fn incomplete_all_in_does_not_reopen() {
        // seat 0 btn, 1 sb, 2 bb; flop: sb bets 1000, bb all-in for 1400 (incomplete),
        // button calls; sb may only call or fold.
        let mut h = Hand::start(cfg(3), &[Some(10_000), Some(10_000), Some(1_500)], 0, (0..52).collect());
        h.apply(0, Act::Call).unwrap();
        h.apply(1, Act::Call).unwrap();
        h.apply(2, Act::Check).unwrap();
        assert_eq!(h.street, Street::Flop);
        assert_eq!(h.to_act, Some(1));
        h.apply(1, Act::BetTo(1000)).unwrap();
        assert_eq!(h.legal().unwrap().max_to, Some(1400));
        h.apply(2, Act::BetTo(1400)).unwrap(); // all-in, +400 < 1000
        assert!(h.seats[2].all_in);
        let l = h.legal().unwrap();
        assert_eq!(l.seat, 0);
        assert_eq!(l.min_to, Some(2400)); // button has not acted: may raise, min = 1400 + 1000
        h.apply(0, Act::Call).unwrap();
        let l = h.legal().unwrap();
        assert_eq!(l.seat, 1);
        assert_eq!(l.call_amount, 400);
        assert!(l.min_to.is_none(), "incomplete raise must not reopen the betting");
        assert!(h.apply(1, Act::BetTo(5000)).is_err());
        h.apply(1, Act::Call).unwrap();
        assert_eq!(h.street, Street::Turn);
    }

    #[test]
    fn cumulative_incomplete_raises_reopen() {
        // 4 players: 0 btn, 1 sb, 2 bb, 3 utg. Flop: sb bets 1000, bb all-in 1500 (+500),
        // utg all-in 2100 (+600), button calls; sb now faces 1100 >= 1000 more: reopened.
        let mut h = Hand::start(cfg(4), &[Some(20_000), Some(20_000), Some(1_600), Some(2_200)], 0, (0..52).collect());
        h.apply(3, Act::Call).unwrap();
        h.apply(0, Act::Call).unwrap();
        h.apply(1, Act::Call).unwrap();
        h.apply(2, Act::Check).unwrap();
        assert_eq!(h.to_act, Some(1));
        h.apply(1, Act::BetTo(1000)).unwrap();
        h.apply(2, Act::BetTo(1500)).unwrap();
        h.apply(3, Act::BetTo(2100)).unwrap();
        h.apply(0, Act::Call).unwrap();
        let l = h.legal().unwrap();
        assert_eq!(l.seat, 1);
        assert_eq!(l.min_to, Some(3100));
    }

    #[test]
    fn short_all_in_after_full_raise() {
        // a full raise reopens; a following incomplete all-in does not reopen for the raiser
        let mut h = Hand::start(cfg(3), &[Some(10_000), Some(10_000), Some(800)], 0, (0..52).collect());
        h.apply(0, Act::BetTo(300)).unwrap(); // btn opens 3bb
        h.apply(1, Act::BetTo(700)).unwrap(); // sb 3-bets to 7bb (inc 400)
        h.apply(2, Act::BetTo(800)).unwrap(); // bb all-in 8bb (+100 incomplete)
        let l = h.legal().unwrap();
        assert_eq!(l.seat, 0);
        assert_eq!(l.min_to, Some(1200)); // btn faces 500 more >= 400: reopened, min = 800 + 400
        h.apply(0, Act::Call).unwrap();
        let l = h.legal().unwrap();
        assert_eq!(l.seat, 1);
        assert!(l.min_to.is_none()); // sb only faces +100 since acting
    }

    #[test]
    fn side_pots_three_way_all_in() {
        // button 0 (10000) jams, sb 1 (3000) calls all-in, bb 2 (6000) calls all-in.
        // button: AA, sb: KK, bb: QQ; board blank: AA wins everything except nothing returned
        let holes = [("Kc", "Kd"), ("Qc", "Qd"), ("Ac", "Ad")]; // dealing order: sb, bb, btn
        let deck = deck_for(&holes, "2s7h9cJs3d");
        let mut h = Hand::start(cfg(3), &[Some(10_000), Some(3_000), Some(6_000)], 0, deck);
        h.apply(0, Act::BetTo(10_000)).unwrap();
        h.apply(1, Act::Call).unwrap();
        h.apply(2, Act::Call).unwrap();
        let r = h.result.as_ref().unwrap();
        // 4000 returned to the button (uncalled above 6000)
        assert!(h.events.contains(&Event::Returned { seat: 0, amount: 4000 }));
        assert_eq!(r.pots.len(), 2);
        assert_eq!(r.pots[0].amount, 9000);
        assert_eq!(r.pots[0].eligible, vec![0, 1, 2]);
        assert_eq!(r.pots[1].amount, 6000);
        assert_eq!(r.pots[1].eligible, vec![0, 2]);
        assert_eq!(r.rake, 300); // 5% of 15000 = 750 capped at 300
        assert_eq!(r.won[0], 15_000 - 300);
        assert_eq!(h.stacks(), vec![4_000 + 14_700, 0, 0]);
        assert_eq!(r.net.iter().sum::<Chips>(), -300);
    }

    #[test]
    fn side_pot_won_by_short_stack() {
        // sb (short) has the best hand: wins main pot, side pot goes to the better of the rest
        let holes = [("Ac", "Ad"), ("Kc", "Kd"), ("Qc", "Qd")]; // sb AA, bb KK, btn QQ
        let deck = deck_for(&holes, "2s7h9cJs3d");
        let mut h = Hand::start(cfg(3), &[Some(10_000), Some(3_000), Some(6_000)], 0, deck);
        h.apply(0, Act::BetTo(10_000)).unwrap();
        h.apply(1, Act::Call).unwrap();
        h.apply(2, Act::Call).unwrap();
        let r = h.result.as_ref().unwrap();
        assert_eq!(r.pots[0].winners, vec![1]);
        assert_eq!(r.pots[1].winners, vec![2]);
        // rake 300 split proportionally: 9000/15000*300 = 180 from main, 120 from side
        assert_eq!(r.pots[0].rake, 180);
        assert_eq!(r.pots[1].rake, 120);
        assert_eq!(r.won[1], 9000 - 180);
        assert_eq!(r.won[2], 6000 - 120);
        assert_eq!(r.won[0], 0);
        assert_eq!(h.stacks()[0], 4_000);
    }

    #[test]
    fn three_way_split_with_incomplete_all_in() {
        // board plays (royal flush) so all three tie; the short big blind's all-in for 1 more
        // chip is an incomplete raise that the others may only call
        let mut c = cfg(3);
        c.rake = RakeConfig { percent: 0.0, cap: 0, no_flop_no_drop: true };
        c.small_blind = 1;
        c.big_blind = 2;
        let holes = [("2c", "3c"), ("2d", "3d"), ("4h", "5h")]; // sb, bb, btn
        let deck = deck_for(&holes, "AsKsQsJsTs");
        let mut h = Hand::start(c, &[Some(100), Some(100), Some(3)], 0, deck);
        h.apply(0, Act::Call).unwrap();
        h.apply(1, Act::Call).unwrap();
        h.apply(2, Act::BetTo(3)).unwrap();
        assert!(h.legal().unwrap().min_to.is_none());
        h.apply(0, Act::Call).unwrap();
        h.apply(1, Act::Call).unwrap();
        while let Some(s) = h.to_act {
            h.apply(s, Act::Check).unwrap();
        }
        let r = h.result.as_ref().unwrap();
        assert_eq!(r.pots[0].amount, 9);
        assert_eq!(r.pots[0].winners.len(), 3);
        assert_eq!(r.won, vec![3, 3, 3]);
    }

    #[test]
    fn odd_chip_goes_left_of_button() {
        // pot of 7 with an ante: three players, antes 1, blinds 1/2, btn folds preflop
        let mut c = cfg(3);
        c.rake = RakeConfig { percent: 0.0, cap: 0, no_flop_no_drop: true };
        c.small_blind = 1;
        c.big_blind = 2;
        c.ante = 1;
        let holes = [("2c", "3c"), ("2d", "3d"), ("4h", "5h")]; // sb, bb, btn
        let deck = deck_for(&holes, "AsKsQsJsTs");
        let mut h = Hand::start(c, &[Some(100), Some(100), Some(100)], 0, deck);
        h.apply(0, Act::Fold).unwrap();
        h.apply(1, Act::Call).unwrap();
        h.apply(2, Act::Check).unwrap();
        for _ in 0..3 {
            h.apply(1, Act::Check).unwrap();
            h.apply(2, Act::Check).unwrap();
        }
        let r = h.result.as_ref().unwrap();
        // pot = 3 antes + 2 + 2 = 7, split between sb (seat 1) and bb (seat 2): sb is first
        // clockwise from the button and gets the odd chip
        assert_eq!(r.pots.iter().map(|p| p.amount).sum::<Chips>(), 7);
        assert_eq!(r.won, vec![0, 4, 3]);
    }

    #[test]
    fn showdown_order_and_muck() {
        // river: bb bets, sb calls; bb (aggressor) shows first; sb loses and mucks
        let holes = [("2c", "3d"), ("Ac", "Ad"), ("9h", "8h")]; // sb, bb, btn
        let deck = deck_for(&holes, "Ks7h4cJs5d");
        let mut h = Hand::start(cfg(3), &[Some(10_000); 3], 0, deck);
        h.apply(0, Act::Fold).unwrap();
        h.apply(1, Act::Call).unwrap();
        h.apply(2, Act::Check).unwrap();
        for _ in 0..2 {
            h.apply(1, Act::Check).unwrap();
            h.apply(2, Act::Check).unwrap();
        }
        h.apply(1, Act::Check).unwrap();
        h.apply(2, Act::BetTo(150)).unwrap();
        h.apply(1, Act::Call).unwrap();
        let shows: Vec<&Event> = h.events.iter().filter(|e| matches!(e, Event::Show { .. } | Event::Muck { .. })).collect();
        assert!(matches!(shows[0], Event::Show { seat: 2, .. }));
        assert!(matches!(shows[1], Event::Muck { seat: 1 }));
        let r = h.result.as_ref().unwrap();
        assert_eq!(r.pots[0].winners, vec![2]);
    }

    #[test]
    fn showdown_without_river_bet_starts_left_of_button() {
        let holes = [("Ac", "Ad"), ("2c", "3d"), ("9h", "8h")]; // sb AA, bb junk
        let deck = deck_for(&holes, "Ks7h4cJs5d");
        let mut h = Hand::start(cfg(3), &[Some(10_000); 3], 0, deck);
        h.apply(0, Act::Fold).unwrap();
        h.apply(1, Act::Call).unwrap();
        h.apply(2, Act::Check).unwrap();
        for _ in 0..3 {
            h.apply(1, Act::Check).unwrap();
            h.apply(2, Act::Check).unwrap();
        }
        let shows: Vec<&Event> = h.events.iter().filter(|e| matches!(e, Event::Show { .. } | Event::Muck { .. })).collect();
        assert!(matches!(shows[0], Event::Show { seat: 1, .. }));
        assert!(matches!(shows[1], Event::Muck { seat: 2 }));
    }

    #[test]
    fn no_flop_no_drop_and_uncalled_return() {
        let mut h = Hand::start(cfg(6), &[Some(10_000); 6], 0, (0..52).collect());
        h.apply(3, Act::BetTo(250)).unwrap();
        for s in [4, 5, 0, 1, 2] {
            h.apply(s, Act::Fold).unwrap();
        }
        let r = h.result.as_ref().unwrap();
        assert_eq!(r.rake, 0);
        assert!(h.events.contains(&Event::Returned { seat: 3, amount: 150 }));
        assert_eq!(r.net[3], 150);
        assert_eq!(r.net[1], -50);
        assert_eq!(r.net[2], -100);
    }

    #[test]
    fn short_big_blind_still_owes_full_blind() {
        let mut h = Hand::start(cfg(3), &[Some(10_000), Some(10_000), Some(40)], 0, (0..52).collect());
        assert!(h.seats[2].all_in);
        let l = h.legal().unwrap();
        assert_eq!(l.seat, 0);
        assert_eq!(l.call_amount, 100);
        h.apply(0, Act::Call).unwrap();
        h.apply(1, Act::Call).unwrap();
        // bb is all-in and both others matched: betting continues between btn and sb
        assert_eq!(h.street, Street::Flop);
        let pots = h.build_pots();
        assert_eq!(pots[0].amount, 120);
        assert_eq!(pots[1].amount, 120);
    }

    #[test]
    fn all_in_runout_tables_hands() {
        let holes = [("Ac", "Ad"), ("Kc", "Kd")]; // heads-up: dealing starts left of button = bb
        let deck = deck_for(&holes, "2s7h9cJs3d");
        let mut h = Hand::start(cfg(2), &[Some(10_000), Some(10_000)], 0, deck);
        h.apply(0, Act::BetTo(10_000)).unwrap();
        h.apply(1, Act::Call).unwrap();
        assert_eq!(h.board.len(), 5);
        let r = h.result.as_ref().unwrap();
        assert_eq!(r.shown.len(), 2);
        assert_eq!(r.pots[0].winners, vec![1]);
    }

    #[test]
    fn chips_are_conserved_random_play() {
        use rand::{Rng, SeedableRng};
        let mut rng = rand::rngs::SmallRng::seed_from_u64(7);
        for hand_no in 0..20_000 {
            let n = rng.gen_range(2..=6);
            let mut c = cfg(n);
            if hand_no % 3 == 0 {
                c.ante = 10;
            }
            let stacks: Vec<Option<Chips>> = (0..n).map(|_| Some(rng.gen_range(1..20_000))).collect();
            let mut deck: Vec<Card> = (0..52).collect();
            use rand::seq::SliceRandom;
            deck.shuffle(&mut rng);
            let start_total: Chips = stacks.iter().map(|s| s.unwrap()).sum();
            let mut h = Hand::start(c, &stacks, rng.gen_range(0..n), deck);
            let mut steps = 0;
            while let Some(l) = h.legal() {
                steps += 1;
                assert!(steps < 500);
                let act = match rng.gen_range(0..10) {
                    0 => Act::Fold,
                    1..=4 => if l.can_check { Act::Check } else { Act::Call },
                    _ => match (l.min_to, l.max_to) {
                        (Some(lo), Some(hi)) => Act::BetTo(if rng.gen_bool(0.3) { hi } else { rng.gen_range(lo..=hi) }),
                        _ => if l.can_check { Act::Check } else { Act::Call },
                    },
                };
                h.apply(l.seat, act).unwrap();
            }
            let r = h.result.as_ref().unwrap();
            let end_total: Chips = h.stacks().iter().sum();
            assert_eq!(start_total, end_total + r.rake, "hand {hand_no}");
            assert!(h.stacks().iter().all(|&s| s >= 0));
            assert_eq!(r.net.iter().sum::<Chips>(), -r.rake);
        }
    }
}
