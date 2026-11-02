//! Cards, combos and the 13x13 hand-class grid.
//!
//! Card ids use the same encoding as postflop-solver so no translation is needed:
//! `card = 4 * rank + suit`, rank 0 = deuce .. 12 = ace, suit 0 = clubs, 1 = diamonds,
//! 2 = hearts, 3 = spades.

pub type Card = u8;

pub const RANK_CHARS: [char; 13] = ['2', '3', '4', '5', '6', '7', '8', '9', 'T', 'J', 'Q', 'K', 'A'];
pub const SUIT_CHARS: [char; 4] = ['c', 'd', 'h', 's'];
pub const NUM_COMBOS: usize = 1326;
pub const NUM_CLASSES: usize = 169;

#[inline]
pub const fn rank(c: Card) -> u8 {
    c >> 2
}

#[inline]
pub const fn suit(c: Card) -> u8 {
    c & 3
}

#[inline]
pub const fn make_card(rank: u8, suit: u8) -> Card {
    4 * rank + suit
}

pub fn card_to_string(c: Card) -> String {
    format!("{}{}", RANK_CHARS[rank(c) as usize], SUIT_CHARS[suit(c) as usize])
}

pub fn rank_from_char(ch: char) -> Option<u8> {
    let ch = ch.to_ascii_uppercase();
    RANK_CHARS.iter().position(|&r| r == ch).map(|p| p as u8)
}

pub fn suit_from_char(ch: char) -> Option<u8> {
    let ch = ch.to_ascii_lowercase();
    SUIT_CHARS.iter().position(|&s| s == ch).map(|p| p as u8)
}

pub fn card_from_str(s: &str) -> Option<Card> {
    let mut it = s.chars();
    let r = rank_from_char(it.next()?)?;
    let su = suit_from_char(it.next()?)?;
    if it.next().is_some() {
        return None;
    }
    Some(make_card(r, su))
}

/// Parses a run of cards like "AsKd7h" (whitespace and commas allowed).
pub fn cards_from_str(s: &str) -> Option<Vec<Card>> {
    let chars: Vec<char> = s.chars().filter(|c| !c.is_whitespace() && *c != ',').collect();
    if chars.len() % 2 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(chars.len() / 2);
    for pair in chars.chunks(2) {
        let r = rank_from_char(pair[0])?;
        let su = suit_from_char(pair[1])?;
        let c = make_card(r, su);
        if out.contains(&c) {
            return None;
        }
        out.push(c);
    }
    Some(out)
}

pub fn cards_to_string(cards: &[Card]) -> String {
    cards.iter().map(|&c| card_to_string(c)).collect::<Vec<_>>().join("")
}

#[inline]
pub fn mask_of(cards: &[Card]) -> u64 {
    cards.iter().fold(0u64, |m, &c| m | (1u64 << c))
}

/// Index of an unordered pair of distinct cards in 0..1326. Matches postflop-solver's
/// `Range` layout (higher card's triangular index plus the lower card).
#[inline]
pub fn combo_index(a: Card, b: Card) -> usize {
    let (lo, hi) = if a < b { (a, b) } else { (b, a) };
    hi as usize * (hi as usize - 1) / 2 + lo as usize
}

/// Inverse of [`combo_index`]; returns (low card, high card).
pub fn combo_cards(idx: usize) -> (Card, Card) {
    COMBO_TABLE.with(|t| t[idx])
}

thread_local! {
    static COMBO_TABLE: [(Card, Card); NUM_COMBOS] = {
        let mut t = [(0u8, 0u8); NUM_COMBOS];
        for hi in 1..52u8 {
            for lo in 0..hi {
                t[combo_index(lo, hi)] = (lo, hi);
            }
        }
        t
    };
}

/// All 1326 combos as (low, high) in index order.
pub fn all_combos() -> Vec<(Card, Card)> {
    (0..NUM_COMBOS).map(combo_cards).collect()
}

/// Hand class 0..169 laid out as the usual 13x13 grid with aces in the top-left:
/// row = 12 - high rank, col = 12 - low rank; pairs on the diagonal, suited above it
/// (row < col), offsuit below it. `class = row * 13 + col`.
#[inline]
pub fn class_of(a: Card, b: Card) -> usize {
    let (ra, rb) = (rank(a), rank(b));
    let (hi, lo) = if ra >= rb { (ra, rb) } else { (rb, ra) };
    let (rh, rl) = (12 - hi as usize, 12 - lo as usize);
    if hi == lo {
        rh * 13 + rh
    } else if suit(a) == suit(b) {
        rh * 13 + rl
    } else {
        rl * 13 + rh
    }
}

/// Name of a class such as "AKs", "T9o", "77".
pub fn class_name(class: usize) -> String {
    let (row, col) = (class / 13, class % 13);
    if row == col {
        let r = RANK_CHARS[12 - row];
        format!("{r}{r}")
    } else if row < col {
        format!("{}{}s", RANK_CHARS[12 - row], RANK_CHARS[12 - col])
    } else {
        format!("{}{}o", RANK_CHARS[12 - col], RANK_CHARS[12 - row])
    }
}

pub fn class_from_name(name: &str) -> Option<usize> {
    let ch: Vec<char> = name.chars().collect();
    if ch.len() < 2 {
        return None;
    }
    let r1 = rank_from_char(ch[0])? as usize;
    let r2 = rank_from_char(ch[1])? as usize;
    let (hi, lo) = if r1 >= r2 { (r1, r2) } else { (r2, r1) };
    if hi == lo {
        return if ch.len() == 2 { Some((12 - hi) * 14) } else { None };
    }
    match ch.get(2) {
        Some('s') => Some((12 - hi) * 13 + (12 - lo)),
        Some('o') => Some((12 - lo) * 13 + (12 - hi)),
        _ => None,
    }
}

/// Number of combos in each class with no dead cards: 6 / 4 / 12.
pub fn class_combo_count(class: usize) -> usize {
    let (row, col) = (class / 13, class % 13);
    if row == col {
        6
    } else if row < col {
        4
    } else {
        12
    }
}

/// Combo indices belonging to each class.
pub fn class_combos() -> Vec<Vec<usize>> {
    let mut out = vec![Vec::new(); NUM_CLASSES];
    for i in 0..NUM_COMBOS {
        let (a, b) = combo_cards(i);
        out[class_of(a, b)].push(i);
    }
    out
}

/// A simple deck that deals from a shuffled vector.
#[derive(Clone, Debug)]
pub struct Deck {
    pub cards: Vec<Card>,
    pub pos: usize,
}

impl Deck {
    pub fn shuffled<R: rand::Rng>(rng: &mut R) -> Self {
        use rand::seq::SliceRandom;
        let mut cards: Vec<Card> = (0..52).collect();
        cards.shuffle(rng);
        Deck { cards, pos: 0 }
    }

    pub fn from_order(cards: Vec<Card>) -> Self {
        Deck { cards, pos: 0 }
    }

    pub fn deal(&mut self) -> Card {
        let c = self.cards[self.pos];
        self.pos += 1;
        c
    }

    /// Removes specific cards from the undealt part (used to set up drills).
    pub fn remove(&mut self, cards: &[Card]) {
        let rest: Vec<Card> = self.cards[self.pos..].iter().copied().filter(|c| !cards.contains(c)).collect();
        self.cards.truncate(self.pos);
        self.cards.extend(rest);
    }

    /// Puts specific cards at the front of the undealt part, in order.
    pub fn stack_front(&mut self, cards: &[Card]) {
        self.remove(cards);
        let rest = self.cards.split_off(self.pos);
        self.cards.extend_from_slice(cards);
        self.cards.extend(rest);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combo_index_roundtrip() {
        let mut seen = vec![false; NUM_COMBOS];
        for hi in 1..52u8 {
            for lo in 0..hi {
                let i = combo_index(lo, hi);
                assert!(!seen[i]);
                seen[i] = true;
                assert_eq!(combo_cards(i), (lo, hi));
                assert_eq!(combo_index(hi, lo), i);
            }
        }
    }

    #[test]
    fn classes() {
        let cc = class_combos();
        assert_eq!(cc.iter().map(|v| v.len()).sum::<usize>(), 1326);
        for c in 0..169 {
            assert_eq!(cc[c].len(), class_combo_count(c));
            assert_eq!(class_from_name(&class_name(c)), Some(c));
        }
        let ak = class_of(card_from_str("As").unwrap(), card_from_str("Ks").unwrap());
        assert_eq!(class_name(ak), "AKs");
        let t9 = class_of(card_from_str("9d").unwrap(), card_from_str("Th").unwrap());
        assert_eq!(class_name(t9), "T9o");
        assert_eq!(class_name(0), "AA");
        assert_eq!(class_name(168), "22");
    }

    #[test]
    fn parse_cards() {
        assert_eq!(cards_from_str("AsKd").unwrap(), vec![51, 45]);
        assert!(cards_from_str("AsAs").is_none());
        assert_eq!(card_to_string(0), "2c");
    }
}
