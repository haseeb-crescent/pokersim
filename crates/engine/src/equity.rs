//! Equity calculation: hand or range against hand or range, heads-up or multiway.
//!
//! Heads-up uses exact enumeration whenever the work fits a budget (every opponent combo times
//! every runout), otherwise Monte Carlo. Multiway is always Monte Carlo with rejection sampling
//! for card removal.

use crate::card::*;
use crate::eval::{eval_masks, HandValue, SuitMasks};
use crate::range::Range;
use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct EquityResult {
    /// Equity (win + tie share) of each player.
    pub equity: Vec<f64>,
    pub win: Vec<f64>,
    pub tie: Vec<f64>,
    pub exact: bool,
    pub samples: u64,
}

/// One side of an equity calculation: a specific hand or a weighted range.
#[derive(Clone, Debug)]
pub enum Holding {
    Hand(Card, Card),
    Range(Range),
}

impl Holding {
    fn combos(&self, dead: u64) -> Vec<(Card, Card, f32)> {
        match self {
            Holding::Hand(a, b) => vec![(*a, *b, 1.0)],
            Holding::Range(r) => r.live_combos(dead),
        }
    }
}

fn runouts(board: &[Card], dead: u64) -> Vec<u64> {
    let need = 5 - board.len();
    let live: Vec<Card> = (0..52).filter(|&c| dead & (1u64 << c) == 0).collect();
    let mut out = Vec::new();
    match need {
        0 => out.push(0),
        1 => {
            for &a in &live {
                out.push(1u64 << a)
            }
        }
        2 => {
            for i in 0..live.len() {
                for j in i + 1..live.len() {
                    out.push((1u64 << live[i]) | (1u64 << live[j]))
                }
            }
        }
        _ => {}
    }
    out
}

/// Exact heads-up equity of a hand against a weighted list of combos on a board with at least
/// three cards. Returns (equity, win, tie) for the hand.
pub fn hand_vs_combos_exact(h: (Card, Card), opp: &[(Card, Card, f32)], board: &[Card]) -> (f64, f64, f64) {
    let bmask = mask_of(board);
    let hmask = (1u64 << h.0) | (1u64 << h.1);
    let base = SuitMasks::from_cards(board);
    let (mut eq, mut win, mut tie, mut total) = (0f64, 0f64, 0f64, 0f64);
    for &(a, b, w) in opp {
        let omask = (1u64 << a) | (1u64 << b);
        if omask & (hmask | bmask) != 0 || w <= 0.0 {
            continue;
        }
        let dead = bmask | hmask | omask;
        let (mut ww, mut tt, mut n) = (0u64, 0u64, 0u64);
        for r in runouts(board, dead) {
            let mut bm = base;
            let mut x = r;
            while x != 0 {
                bm = bm.add(x.trailing_zeros() as u8);
                x &= x - 1;
            }
            let hv = eval_masks(bm.add(h.0).add(h.1));
            let ov = eval_masks(bm.add(a).add(b));
            if hv > ov {
                ww += 1
            } else if hv == ov {
                tt += 1
            }
            n += 1;
        }
        let w = w as f64;
        win += w * ww as f64 / n as f64;
        tie += w * tt as f64 / n as f64;
        eq += w * (ww as f64 + 0.5 * tt as f64) / n as f64;
        total += w;
    }
    if total == 0.0 {
        return (0.0, 0.0, 0.0);
    }
    (eq / total, win / total, tie / total)
}

/// Heads-up or multiway equity. Exact when heads-up, the board has 3+ cards and the number of
/// evaluations is below `exact_budget`; otherwise Monte Carlo with `samples` trials.
pub fn equity(players: &[Holding], board: &[Card], dead: &[Card], samples: u64, exact_budget: u64) -> EquityResult {
    let n = players.len();
    let dead_mask = mask_of(board) | mask_of(dead);
    let lists: Vec<Vec<(Card, Card, f32)>> = players.iter().map(|p| p.combos(dead_mask)).collect();
    if lists.iter().any(|l| l.is_empty()) {
        return EquityResult { equity: vec![0.0; n], win: vec![0.0; n], tie: vec![0.0; n], exact: true, samples: 0 };
    }
    if n == 2 && board.len() >= 3 {
        let per_runout = match board.len() {
            3 => 990u64,
            4 => 44,
            _ => 1,
        };
        let work = lists[0].len() as u64 * lists[1].len() as u64 * per_runout;
        if work <= exact_budget {
            return exact_hu(&lists, board);
        }
    }
    if n == 2 && board.is_empty() && lists[0].len() * lists[1].len() <= 4 {
        // single matchup preflop: exact enumeration of all 1.7M boards
        return exact_hu_preflop(&lists);
    }
    monte_carlo(&lists, board, dead_mask, samples)
}

fn exact_hu(lists: &[Vec<(Card, Card, f32)>], board: &[Card]) -> EquityResult {
    let results: Vec<(f64, f64, f64, f64)> = lists[0]
        .par_iter()
        .map(|&(a, b, w)| {
            let (eq, win, tie) = hand_vs_combos_exact((a, b), &lists[1], board);
            // weight by the opponent mass that does not conflict with this hand
            let m = (1u64 << a) | (1u64 << b);
            let opp_w: f64 =
                lists[1].iter().filter(|(x, y, _)| ((1u64 << x) | (1u64 << y)) & m == 0).map(|t| t.2 as f64).sum();
            let ww = w as f64 * opp_w;
            (ww * eq, ww * win, ww * tie, ww)
        })
        .collect();
    let (mut eq, mut win, mut tie, mut tot) = (0.0, 0.0, 0.0, 0.0);
    for r in results {
        eq += r.0;
        win += r.1;
        tie += r.2;
        tot += r.3;
    }
    let e0 = eq / tot;
    let w0 = win / tot;
    let t0 = tie / tot;
    EquityResult {
        equity: vec![e0, 1.0 - e0],
        win: vec![w0, 1.0 - w0 - t0],
        tie: vec![t0, t0],
        exact: true,
        samples: 0,
    }
}

fn exact_hu_preflop(lists: &[Vec<(Card, Card, f32)>]) -> EquityResult {
    let mut acc = [0f64; 4];
    for &(a, b, w0) in &lists[0] {
        for &(c, d, w1) in &lists[1] {
            let m = (1u64 << a) | (1u64 << b) | (1u64 << c) | (1u64 << d);
            if m.count_ones() < 4 {
                continue;
            }
            let (w, t, n) = preflop_matchup_exact((a, b), (c, d));
            let ww = (w0 * w1) as f64;
            acc[0] += ww * (w as f64 + 0.5 * t as f64) / n as f64;
            acc[1] += ww * w as f64 / n as f64;
            acc[2] += ww * t as f64 / n as f64;
            acc[3] += ww;
        }
    }
    let e0 = acc[0] / acc[3];
    let w0 = acc[1] / acc[3];
    let t0 = acc[2] / acc[3];
    EquityResult { equity: vec![e0, 1.0 - e0], win: vec![w0, 1.0 - w0 - t0], tie: vec![t0, t0], exact: true, samples: 0 }
}

/// Exact preflop all-in result for two specific hands: (wins, ties, boards) for hand one.
pub fn preflop_matchup_exact(h1: (Card, Card), h2: (Card, Card)) -> (u64, u64, u64) {
    let dead = (1u64 << h1.0) | (1u64 << h1.1) | (1u64 << h2.0) | (1u64 << h2.1);
    let live: Vec<Card> = (0..52).filter(|&c| dead & (1u64 << c) == 0).collect();
    let m1 = SuitMasks::default().add(h1.0).add(h1.1);
    let m2 = SuitMasks::default().add(h2.0).add(h2.1);
    let n = live.len();
    let per_first: Vec<(u64, u64, u64)> = (0..n)
        .into_par_iter()
        .map(|i| {
            let (mut w, mut t, mut tot) = (0u64, 0u64, 0u64);
            let bi = SuitMasks::default().add(live[i]);
            for j in i + 1..n {
                let bj = bi.add(live[j]);
                for k in j + 1..n {
                    let bk = bj.add(live[k]);
                    for l in k + 1..n {
                        let bl = bk.add(live[l]);
                        for m in l + 1..n {
                            let bm = bl.add(live[m]);
                            let v1 = eval_masks(merge(bm, m1));
                            let v2 = eval_masks(merge(bm, m2));
                            if v1 > v2 {
                                w += 1
                            } else if v1 == v2 {
                                t += 1
                            }
                            tot += 1;
                        }
                    }
                }
            }
            (w, t, tot)
        })
        .collect();
    per_first.iter().fold((0, 0, 0), |acc, x| (acc.0 + x.0, acc.1 + x.1, acc.2 + x.2))
}

#[inline]
fn merge(a: SuitMasks, b: SuitMasks) -> SuitMasks {
    SuitMasks([a.0[0] | b.0[0], a.0[1] | b.0[1], a.0[2] | b.0[2], a.0[3] | b.0[3]])
}

fn sample_weighted<R: Rng>(rng: &mut R, list: &[(Card, Card, f32)], cum: &[f32]) -> (Card, Card) {
    let total = *cum.last().unwrap();
    let x = rng.gen::<f32>() * total;
    let idx = cum.partition_point(|&c| c <= x).min(list.len() - 1);
    (list[idx].0, list[idx].1)
}

fn monte_carlo(lists: &[Vec<(Card, Card, f32)>], board: &[Card], dead_mask: u64, samples: u64) -> EquityResult {
    let n = lists.len();
    let cums: Vec<Vec<f32>> = lists
        .iter()
        .map(|l| {
            let mut acc = 0.0;
            l.iter()
                .map(|x| {
                    acc += x.2;
                    acc
                })
                .collect()
        })
        .collect();
    let chunks = 64u64;
    let per = samples.div_ceil(chunks);
    let partial: Vec<(Vec<f64>, Vec<f64>, Vec<f64>, u64)> = (0..chunks)
        .into_par_iter()
        .map(|chunk| {
            let mut rng = SmallRng::seed_from_u64(0x9E37_79B9_7F4A_7C15 ^ (chunk * 7919 + samples));
            let mut eq = vec![0f64; n];
            let mut win = vec![0f64; n];
            let mut tie = vec![0f64; n];
            let mut done = 0u64;
            let mut attempts = 0u64;
            let base = SuitMasks::from_cards(board);
            let mut vals = vec![0 as HandValue; n];
            while done < per && attempts < per * 50 {
                attempts += 1;
                let mut used = dead_mask;
                let mut hands = Vec::with_capacity(n);
                let mut ok = true;
                for p in 0..n {
                    let (a, b) = sample_weighted(&mut rng, &lists[p], &cums[p]);
                    let m = (1u64 << a) | (1u64 << b);
                    if used & m != 0 {
                        ok = false;
                        break;
                    }
                    used |= m;
                    hands.push((a, b));
                }
                if !ok {
                    continue;
                }
                let mut bm = base;
                for _ in board.len()..5 {
                    loop {
                        let c = rng.gen_range(0..52u8);
                        if used & (1u64 << c) == 0 {
                            used |= 1u64 << c;
                            bm = bm.add(c);
                            break;
                        }
                    }
                }
                let mut best = 0;
                for p in 0..n {
                    vals[p] = eval_masks(bm.add(hands[p].0).add(hands[p].1));
                    best = best.max(vals[p]);
                }
                let winners = vals.iter().filter(|&&v| v == best).count();
                for p in 0..n {
                    if vals[p] == best {
                        if winners == 1 {
                            win[p] += 1.0;
                        } else {
                            tie[p] += 1.0;
                        }
                        eq[p] += 1.0 / winners as f64;
                    }
                }
                done += 1;
            }
            (eq, win, tie, done)
        })
        .collect();
    let mut eq = vec![0f64; n];
    let mut win = vec![0f64; n];
    let mut tie = vec![0f64; n];
    let mut total = 0u64;
    for (e, w, t, d) in partial {
        for p in 0..n {
            eq[p] += e[p];
            win[p] += w[p];
            tie[p] += t[p];
        }
        total += d;
    }
    let t = total.max(1) as f64;
    EquityResult {
        equity: eq.iter().map(|x| x / t).collect(),
        win: win.iter().map(|x| x / t).collect(),
        tie: tie.iter().map(|x| x / t).collect(),
        exact: false,
        samples: total,
    }
}

/// Equity of every combo of `range` against `opp` on a 3-5 card board (exact). Used for
/// range-advantage and nut-advantage facts. Returns per-combo equity (NaN where dead/absent).
pub fn range_equities(range: &Range, opp: &Range, board: &[Card]) -> Vec<f64> {
    let dead = mask_of(board);
    let opp_list = opp.live_combos(dead);
    (0..NUM_COMBOS)
        .into_par_iter()
        .map(|i| {
            let (a, b) = combo_cards(i);
            if range.w[i] <= 0.0 || dead & ((1u64 << a) | (1u64 << b)) != 0 {
                return f64::NAN;
            }
            hand_vs_combos_exact((a, b), &opp_list, board).0
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hand(s: &str) -> Holding {
        let c = cards_from_str(s).unwrap();
        Holding::Hand(c[0], c[1])
    }

    #[test]
    fn aa_vs_kk_preflop() {
        let r = equity(&[hand("AsAh"), hand("KsKh")], &[], &[], 0, 0);
        assert!(r.exact);
        // well-known exact value: 82.64% (AsAh vs KsKh)
        assert!((r.equity[0] - 0.8264).abs() < 0.0015, "{:?}", r.equity);
    }

    #[test]
    fn flop_exact_vs_mc() {
        let board = cards_from_str("Td9d6h").unwrap();
        let villain = Holding::Range(Range::parse("TT+,AK,KQs,QJs,J8s+,87s").unwrap());
        let ex = equity(&[hand("AdKd"), villain.clone()], &board, &[], 0, u64::MAX);
        let mc = equity(&[hand("AdKd"), villain], &board, &[], 400_000, 0);
        assert!(ex.exact && !mc.exact);
        assert!((ex.equity[0] - mc.equity[0]).abs() < 0.01, "{} vs {}", ex.equity[0], mc.equity[0]);
    }

    #[test]
    fn river_split() {
        let board = cards_from_str("AsKsQsJsTs").unwrap();
        let r = equity(&[hand("2c3c"), hand("4d5d")], &board, &[], 0, u64::MAX);
        assert!((r.equity[0] - 0.5).abs() < 1e-9);
    }
}
