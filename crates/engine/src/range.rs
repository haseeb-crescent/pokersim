//! Weighted hand ranges over the 1326 combos.
//!
//! The text format is the PioSolver / GTO+ one: comma-separated tokens, each optionally
//! followed by `:weight`. Tokens: `AA`, `AKs`, `AKo`, `AK`, `22+`, `A2s+`, `KTo+`, `QQ-88`,
//! `AQs-A9s`, `KTo-K7o`, and exact combos such as `AsKs`.

use crate::card::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq)]
pub struct Range {
    pub w: Vec<f32>,
}

impl Default for Range {
    fn default() -> Self {
        Range { w: vec![0.0; NUM_COMBOS] }
    }
}

impl Serialize for Range {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string_compact())
    }
}

impl<'de> Deserialize<'de> for Range {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Range::parse(&s).map_err(serde::de::Error::custom)
    }
}

impl Range {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn full() -> Self {
        Range { w: vec![1.0; NUM_COMBOS] }
    }

    pub fn get(&self, a: Card, b: Card) -> f32 {
        self.w[combo_index(a, b)]
    }

    pub fn set(&mut self, a: Card, b: Card, w: f32) {
        self.w[combo_index(a, b)] = w;
    }

    pub fn set_class(&mut self, class: usize, w: f32) {
        for (i, x) in self.w.iter_mut().enumerate() {
            let (a, b) = combo_cards(i);
            if class_of(a, b) == class {
                *x = w;
            }
        }
    }

    pub fn from_grid(grid: &[f32]) -> Self {
        let mut r = Range::empty();
        for i in 0..NUM_COMBOS {
            let (a, b) = combo_cards(i);
            r.w[i] = grid[class_of(a, b)];
        }
        r
    }

    /// Average weight of each of the 169 classes over combos not blocked by `dead`.
    pub fn to_grid(&self, dead: u64) -> Vec<f32> {
        let mut sum = vec![0f32; NUM_CLASSES];
        let mut n = vec![0f32; NUM_CLASSES];
        for i in 0..NUM_COMBOS {
            let (a, b) = combo_cards(i);
            if dead & ((1u64 << a) | (1u64 << b)) != 0 {
                continue;
            }
            let c = class_of(a, b);
            sum[c] += self.w[i];
            n[c] += 1.0;
        }
        sum.iter().zip(&n).map(|(s, n)| if *n > 0.0 { s / n } else { 0.0 }).collect()
    }

    /// Number of combos (weighted) not blocked by dead cards.
    pub fn combos(&self, dead: u64) -> f32 {
        let mut total = 0.0;
        for i in 0..NUM_COMBOS {
            let (a, b) = combo_cards(i);
            if dead & ((1u64 << a) | (1u64 << b)) == 0 {
                total += self.w[i];
            }
        }
        total
    }

    /// Fraction of all 1326 combos (the usual "x% of hands").
    pub fn percent(&self) -> f32 {
        100.0 * self.w.iter().sum::<f32>() / NUM_COMBOS as f32
    }

    pub fn remove_dead(&mut self, dead: u64) {
        for i in 0..NUM_COMBOS {
            let (a, b) = combo_cards(i);
            if dead & ((1u64 << a) | (1u64 << b)) != 0 {
                self.w[i] = 0.0;
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.w.iter().all(|&x| x <= 0.0)
    }

    /// Combos with positive weight, excluding dead cards.
    pub fn live_combos(&self, dead: u64) -> Vec<(Card, Card, f32)> {
        let mut out = Vec::new();
        for i in 0..NUM_COMBOS {
            let (a, b) = combo_cards(i);
            if self.w[i] > 0.0 && dead & ((1u64 << a) | (1u64 << b)) == 0 {
                out.push((a, b, self.w[i]));
            }
        }
        out
    }

    pub fn parse(text: &str) -> Result<Range, String> {
        let mut r = Range::empty();
        for raw in text.split(',') {
            let tok = raw.trim();
            if tok.is_empty() {
                continue;
            }
            let (body, weight) = match tok.split_once(':') {
                Some((b, w)) => (b.trim(), w.trim().parse::<f32>().map_err(|_| format!("bad weight in {tok}"))?),
                None => (tok, 1.0),
            };
            if !(0.0..=1.0).contains(&weight) {
                return Err(format!("weight out of range in {tok}"));
            }
            for (a, b) in expand_token(body)? {
                r.set(a, b, weight);
            }
        }
        Ok(r)
    }

    /// Compact text using class tokens with weights, exact combos when a class is mixed by suit.
    pub fn to_string_compact(&self) -> String {
        let cc = class_combos();
        let mut parts = Vec::new();
        for class in 0..NUM_CLASSES {
            let ws: Vec<f32> = cc[class].iter().map(|&i| self.w[i]).collect();
            let first = ws[0];
            if ws.iter().all(|&x| (x - first).abs() < 1e-4) {
                if first > 0.0 {
                    parts.push(fmt_weight(&class_name(class), first));
                }
            } else {
                for &i in &cc[class] {
                    if self.w[i] > 0.0 {
                        let (a, b) = combo_cards(i);
                        let s = format!("{}{}", card_to_string(b), card_to_string(a));
                        parts.push(fmt_weight(&s, self.w[i]));
                    }
                }
            }
        }
        parts.join(",")
    }
}

fn fmt_weight(name: &str, w: f32) -> String {
    if (w - 1.0).abs() < 1e-4 {
        name.to_string()
    } else {
        let s = format!("{w:.3}");
        let s = s.trim_end_matches('0').trim_end_matches('.');
        format!("{name}:{s}")
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Suited {
    Pair,
    Suited,
    Offsuit,
    Both,
}

fn class_combos_of(hi: u8, lo: u8, s: Suited) -> Vec<(Card, Card)> {
    let mut out = Vec::new();
    for s1 in 0..4u8 {
        for s2 in 0..4u8 {
            if hi == lo {
                if s1 < s2 {
                    out.push((make_card(hi, s1), make_card(lo, s2)));
                }
                continue;
            }
            let ok = match s {
                Suited::Suited => s1 == s2,
                Suited::Offsuit => s1 != s2,
                _ => true,
            };
            if ok {
                out.push((make_card(hi, s1), make_card(lo, s2)));
            }
        }
    }
    out
}

fn parse_class(tok: &str) -> Result<(u8, u8, Suited), String> {
    let ch: Vec<char> = tok.chars().collect();
    if ch.len() < 2 || ch.len() > 3 {
        return Err(format!("bad hand {tok}"));
    }
    let r1 = rank_from_char(ch[0]).ok_or(format!("bad rank in {tok}"))?;
    let r2 = rank_from_char(ch[1]).ok_or(format!("bad rank in {tok}"))?;
    let (hi, lo) = if r1 >= r2 { (r1, r2) } else { (r2, r1) };
    let s = match ch.get(2) {
        None if hi == lo => Suited::Pair,
        None => Suited::Both,
        Some('s') if hi != lo => Suited::Suited,
        Some('o') if hi != lo => Suited::Offsuit,
        _ => return Err(format!("bad suitedness in {tok}")),
    };
    Ok((hi, lo, s))
}

fn expand_token(tok: &str) -> Result<Vec<(Card, Card)>, String> {
    // exact combo
    if tok.len() == 4 && tok.chars().nth(1).map(|c| suit_from_char(c).is_some()).unwrap_or(false) {
        let cards = cards_from_str(tok).ok_or(format!("bad combo {tok}"))?;
        return Ok(vec![(cards[0], cards[1])]);
    }
    let mut out = Vec::new();
    if let Some(base) = tok.strip_suffix('+') {
        let (hi, lo, s) = parse_class(base)?;
        if s == Suited::Pair {
            for r in lo..=12 {
                out.extend(class_combos_of(r, r, s));
            }
        } else {
            for k in lo..hi {
                out.extend(class_combos_of(hi, k, s));
            }
        }
        return Ok(out);
    }
    if let Some((a, b)) = tok.split_once('-') {
        let (h1, l1, s1) = parse_class(a)?;
        let (h2, l2, s2) = parse_class(b)?;
        if s1 != s2 {
            return Err(format!("mismatched range {tok}"));
        }
        if s1 == Suited::Pair {
            let (lo, hi) = if h1 <= h2 { (h1, h2) } else { (h2, h1) };
            for r in lo..=hi {
                out.extend(class_combos_of(r, r, s1));
            }
        } else {
            if h1 != h2 {
                return Err(format!("range must share the top card: {tok}"));
            }
            let (lo, hi) = if l1 <= l2 { (l1, l2) } else { (l2, l1) };
            for k in lo..=hi {
                out.extend(class_combos_of(h1, k, s1));
            }
        }
        return Ok(out);
    }
    let (hi, lo, s) = parse_class(tok)?;
    Ok(class_combos_of(hi, lo, s))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_counts() {
        assert_eq!(Range::parse("AA").unwrap().combos(0), 6.0);
        assert_eq!(Range::parse("AKs").unwrap().combos(0), 4.0);
        assert_eq!(Range::parse("AKo").unwrap().combos(0), 12.0);
        assert_eq!(Range::parse("AK").unwrap().combos(0), 16.0);
        assert_eq!(Range::parse("22+").unwrap().combos(0), 78.0);
        assert_eq!(Range::parse("A2s+").unwrap().combos(0), 48.0);
        assert_eq!(Range::parse("QQ-88").unwrap().combos(0), 30.0);
        assert_eq!(Range::parse("KTo-K7o").unwrap().combos(0), 48.0);
        assert_eq!(Range::parse("AsKs,AhKh:0.5").unwrap().combos(0), 1.5);
        let r = Range::parse("AA,KK:0.5,AKs,T9s:0.25").unwrap();
        let back = Range::parse(&r.to_string_compact()).unwrap();
        assert_eq!(r, back);
        assert_eq!(r.to_string_compact(), "AA,AKs,KK:0.5,T9s:0.25");
    }

    #[test]
    fn grid_roundtrip() {
        let r = Range::parse("22+,A2s+,KTo+").unwrap();
        let g = r.to_grid(0);
        assert_eq!(Range::from_grid(&g), r);
    }
}
