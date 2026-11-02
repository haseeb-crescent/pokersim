//! Hand evaluation.
//!
//! [`eval`] is the fast evaluator used everywhere: it works on per-suit 13-bit rank masks and
//! needs no lookup tables. [`reference`] is a deliberately naive, independent implementation
//! (brute force over every 5-card subset) used to verify the fast one exhaustively over all
//! 133,784,560 seven-card hands (`cargo test -p engine --release -- --ignored`).
//!
//! A hand value is a `u32` that orders hands correctly: `category << 20` followed by up to five
//! 4-bit ranks (the significant ranks first, then kickers).

use crate::card::{rank, suit, Card};

pub type HandValue = u32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub enum Category {
    HighCard = 0,
    Pair = 1,
    TwoPair = 2,
    Trips = 3,
    Straight = 4,
    Flush = 5,
    FullHouse = 6,
    Quads = 7,
    StraightFlush = 8,
}

impl Category {
    pub fn from_value(v: HandValue) -> Category {
        match v >> 20 {
            0 => Category::HighCard,
            1 => Category::Pair,
            2 => Category::TwoPair,
            3 => Category::Trips,
            4 => Category::Straight,
            5 => Category::Flush,
            6 => Category::FullHouse,
            7 => Category::Quads,
            _ => Category::StraightFlush,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Category::HighCard => "high card",
            Category::Pair => "pair",
            Category::TwoPair => "two pair",
            Category::Trips => "three of a kind",
            Category::Straight => "straight",
            Category::Flush => "flush",
            Category::FullHouse => "full house",
            Category::Quads => "four of a kind",
            Category::StraightFlush => "straight flush",
        }
    }
}

#[inline]
fn encode(category: u32, ranks: &[u32]) -> HandValue {
    let mut v = category << 20;
    for (i, &r) in ranks.iter().enumerate() {
        v |= r << (16 - 4 * i);
    }
    v
}

#[inline]
fn hi_bit(x: u32) -> u32 {
    31 - x.leading_zeros()
}

/// Top `n` set ranks of a 13-bit mask, highest first, encoded into consecutive nibbles
/// starting at `shift`.
#[inline]
fn top_ranks(mut mask: u32, n: usize, mut shift: i32, mut acc: u32) -> u32 {
    for _ in 0..n {
        if mask == 0 {
            break;
        }
        let r = hi_bit(mask);
        acc |= r << shift;
        mask &= !(1 << r);
        shift -= 4;
    }
    acc
}

/// Highest straight in a 13-bit rank mask (ace may play low). Returns the top rank.
#[inline]
fn straight_top(mask: u32) -> Option<u32> {
    let m = (mask << 1) | ((mask >> 12) & 1);
    let s = m & (m >> 1) & (m >> 2) & (m >> 3) & (m >> 4);
    if s == 0 {
        None
    } else {
        Some(hi_bit(s) + 3)
    }
}

/// Per-suit rank masks of a set of cards.
#[derive(Clone, Copy, Default, Debug)]
pub struct SuitMasks(pub [u32; 4]);

impl SuitMasks {
    #[inline]
    pub fn from_cards(cards: &[Card]) -> Self {
        let mut m = [0u32; 4];
        for &c in cards {
            m[suit(c) as usize] |= 1 << rank(c);
        }
        SuitMasks(m)
    }

    #[inline]
    pub fn from_mask(mask: u64) -> Self {
        let mut m = [0u32; 4];
        let mut x = mask;
        while x != 0 {
            let c = x.trailing_zeros() as u8;
            m[suit(c) as usize] |= 1 << rank(c);
            x &= x - 1;
        }
        SuitMasks(m)
    }

    #[inline]
    pub fn add(mut self, c: Card) -> Self {
        self.0[suit(c) as usize] |= 1 << rank(c);
        self
    }
}

/// Evaluates the best 5-card hand among 5 to 7 cards given as suit masks.
#[inline]
pub fn eval_masks(sm: SuitMasks) -> HandValue {
    let [m0, m1, m2, m3] = sm.0;
    let all = m0 | m1 | m2 | m3;

    let mut flush_mask = 0u32;
    for m in sm.0 {
        if m.count_ones() >= 5 {
            flush_mask = m;
        }
    }
    if flush_mask != 0 {
        if let Some(top) = straight_top(flush_mask) {
            return (8 << 20) | (top << 16);
        }
    }

    let c4 = m0 & m1 & m2 & m3;
    if c4 != 0 {
        let q = hi_bit(c4);
        let kick = all & !(1 << q);
        return top_ranks(kick, 1, 12, (7 << 20) | (q << 16));
    }

    let c3 = (m0 & m1 & m2) | (m0 & m1 & m3) | (m0 & m2 & m3) | (m1 & m2 & m3);
    let c2 = (m0 & m1) | (m0 & m2) | (m0 & m3) | (m1 & m2) | (m1 & m3) | (m2 & m3);

    if c3 != 0 {
        let t = hi_bit(c3);
        let pairs = c2 & !(1 << t);
        if pairs != 0 {
            return (6 << 20) | (t << 16) | (hi_bit(pairs) << 12);
        }
    }

    if flush_mask != 0 {
        return top_ranks(flush_mask, 5, 16, 5 << 20);
    }

    if let Some(top) = straight_top(all) {
        return (4 << 20) | (top << 16);
    }

    if c3 != 0 {
        let t = hi_bit(c3);
        return top_ranks(all & !(1 << t), 2, 12, (3 << 20) | (t << 16));
    }

    if c2 != 0 {
        let p1 = hi_bit(c2);
        let rest = c2 & !(1 << p1);
        if rest != 0 {
            let p2 = hi_bit(rest);
            let kick = all & !(1 << p1) & !(1 << p2);
            return top_ranks(kick, 1, 8, (2 << 20) | (p1 << 16) | (p2 << 12));
        }
        return top_ranks(all & !(1 << p1), 3, 12, (1 << 20) | (p1 << 16));
    }

    top_ranks(all, 5, 16, 0)
}

#[inline]
pub fn eval(cards: &[Card]) -> HandValue {
    eval_masks(SuitMasks::from_cards(cards))
}

#[inline]
pub fn eval_mask(mask: u64) -> HandValue {
    eval_masks(SuitMasks::from_mask(mask))
}

/// Human-readable description, e.g. "two pair, kings and sevens".
pub fn describe(v: HandValue) -> String {
    let names = [
        "deuces", "threes", "fours", "fives", "sixes", "sevens", "eights", "nines", "tens", "jacks", "queens",
        "kings", "aces",
    ];
    let single = ["deuce", "three", "four", "five", "six", "seven", "eight", "nine", "ten", "jack", "queen", "king", "ace"];
    let r = |i: u32| ((v >> (16 - 4 * i)) & 0xF) as usize;
    match Category::from_value(v) {
        Category::HighCard => format!("{} high", single[r(0)]),
        Category::Pair => format!("pair of {}", names[r(0)]),
        Category::TwoPair => format!("two pair, {} and {}", names[r(0)], names[r(1)]),
        Category::Trips => format!("three {}", names[r(0)]),
        Category::Straight => format!("{}-high straight", single[r(0)]),
        Category::Flush => format!("{}-high flush", single[r(0)]),
        Category::FullHouse => format!("{} full of {}", names[r(0)], names[r(1)]),
        Category::Quads => format!("four {}", names[r(0)]),
        Category::StraightFlush => {
            if r(0) == 12 {
                "royal flush".to_string()
            } else {
                format!("{}-high straight flush", single[r(0)])
            }
        }
    }
}

/// Naive reference evaluator, written independently of the fast one.
pub mod reference {
    use super::{encode, HandValue};
    use crate::card::{rank, suit, Card};

    /// Classifies exactly five cards by sorting and counting.
    pub fn eval5(cards: [Card; 5]) -> HandValue {
        let mut ranks: Vec<u32> = cards.iter().map(|&c| rank(c) as u32).collect();
        ranks.sort_unstable_by(|a, b| b.cmp(a));
        let is_flush = cards.iter().all(|&c| suit(c) == suit(cards[0]));
        let mut distinct = ranks.clone();
        distinct.dedup();
        let straight_top = if distinct.len() == 5 {
            if ranks[0] - ranks[4] == 4 {
                Some(ranks[0])
            } else if ranks == vec![12, 3, 2, 1, 0] {
                Some(3)
            } else {
                None
            }
        } else {
            None
        };
        // groups: (count, rank) sorted by count desc then rank desc
        let mut groups: Vec<(u32, u32)> = Vec::new();
        for &r in &distinct {
            let n = ranks.iter().filter(|&&x| x == r).count() as u32;
            groups.push((n, r));
        }
        groups.sort_unstable_by(|a, b| b.cmp(a));
        let pattern: Vec<u32> = groups.iter().map(|g| g.0).collect();
        let grouped: Vec<u32> = groups.iter().map(|g| g.1).collect();

        if let (Some(top), true) = (straight_top, is_flush) {
            return encode(8, &[top]);
        }
        if pattern == [4, 1] {
            return encode(7, &grouped);
        }
        if pattern == [3, 2] {
            return encode(6, &grouped);
        }
        if is_flush {
            return encode(5, &ranks);
        }
        if let Some(top) = straight_top {
            return encode(4, &[top]);
        }
        if pattern == [3, 1, 1] {
            return encode(3, &grouped);
        }
        if pattern == [2, 2, 1] {
            return encode(2, &grouped);
        }
        if pattern == [2, 1, 1, 1] {
            return encode(1, &grouped);
        }
        encode(0, &ranks)
    }

    /// Best hand among 5..7 cards by trying every 5-card subset.
    pub fn eval_best(cards: &[Card]) -> HandValue {
        let n = cards.len();
        let mut best = 0;
        for a in 0..n {
            for b in a + 1..n {
                for c in b + 1..n {
                    for d in c + 1..n {
                        for e in d + 1..n {
                            let v = eval5([cards[a], cards[b], cards[c], cards[d], cards[e]]);
                            best = best.max(v);
                        }
                    }
                }
            }
        }
        best
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::cards_from_str;
    use rayon::prelude::*;

    fn ev(s: &str) -> HandValue {
        eval(&cards_from_str(s).unwrap())
    }

    #[test]
    fn known_orderings() {
        assert!(ev("AsKsQsJsTs2c3d") > ev("9s8s7s6s5s2c3d"));
        assert_eq!(Category::from_value(ev("As2s3s4s5sKdKc")), Category::StraightFlush);
        assert_eq!(describe(ev("As2s3s4s5sKdKc")), "five-high straight flush");
        assert!(ev("AhAdAcAs2d") > ev("KhKdKcKsAd"));
        assert!(ev("2h2d2cAsAd") < ev("3h3d3c2s2d"));
        assert!(ev("Ah2h3h4h9h") > ev("AsKdQhJcTd"));
        assert!(ev("Ad2c3h4s5d") < ev("2c3h4s5d6d"));
        assert_eq!(ev("AsKsQdJhTc9c8c"), ev("AhKdQcJsTd2c3h"));
        // two pair chooses the best kicker, ignoring a third pair
        assert_eq!(ev("KsKd7h7c5s5dAc"), ev("KhKc7s7dAd2c3h"));
        // two trips form a full house
        assert_eq!(Category::from_value(ev("7s7d7hKcKdKs2c")), Category::FullHouse);
        assert_eq!(describe(ev("7s7d7hKcKdKs2c")), "kings full of sevens");
    }

    #[test]
    fn five_card_classes_and_counts() {
        // all 2,598,960 five-card hands: 7462 distinct values, standard category counts
        let mut values = Vec::with_capacity(2_598_960);
        let mut counts = [0u64; 9];
        for a in 0..52u8 {
            for b in a + 1..52 {
                for c in b + 1..52 {
                    for d in c + 1..52 {
                        for e in d + 1..52 {
                            let cards = [a, b, c, d, e];
                            let v = eval(&cards);
                            assert_eq!(v, reference::eval5(cards));
                            counts[(v >> 20) as usize] += 1;
                            values.push(v);
                        }
                    }
                }
            }
        }
        values.sort_unstable();
        values.dedup();
        assert_eq!(values.len(), 7462);
        assert_eq!(counts, [1_302_540, 1_098_240, 123_552, 54_912, 10_200, 5_108, 3_744, 624, 40]);
    }

    /// Exhaustive check of all 133,784,560 seven-card hands against the brute-force reference,
    /// plus the published seven-card category counts. Run with
    /// `cargo test -p engine --release -- --ignored exhaustive`.
    #[test]
    #[ignore]
    fn exhaustive_seven_card() {
        let counts: Vec<[u64; 9]> = (0..52u8)
            .into_par_iter()
            .map(|a| {
                let mut counts = [0u64; 9];
                for b in a + 1..52 {
                    for c in b + 1..52 {
                        for d in c + 1..52 {
                            for e in d + 1..52 {
                                for f in e + 1..52 {
                                    for g in f + 1..52 {
                                        let cards = [a, b, c, d, e, f, g];
                                        let v = eval(&cards);
                                        let r = reference::eval_best(&cards);
                                        assert_eq!(v, r, "mismatch on {:?}", crate::card::cards_to_string(&cards));
                                        counts[(v >> 20) as usize] += 1;
                                    }
                                }
                            }
                        }
                    }
                }
                counts
            })
            .collect();
        let mut total = [0u64; 9];
        for c in counts {
            for i in 0..9 {
                total[i] += c[i];
            }
        }
        assert_eq!(
            total,
            [23_294_460, 58_627_800, 31_433_400, 6_461_620, 6_180_020, 4_047_644, 3_473_184, 224_848, 41_584]
        );
        assert_eq!(total.iter().sum::<u64>(), 133_784_560);
    }
}
