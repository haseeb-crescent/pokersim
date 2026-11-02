//! Exact preflop all-in equity between the 169 hand classes, with card-removal weights.
//!
//! `eq[a][b]` is the average equity of a combo of class `a` against a combo of class `b`,
//! averaged over all non-conflicting combo pairs, each enumerated over all 1,712,304 boards.
//! `k[a][b]` is the average number of class-`b` combos compatible with one class-`a` combo.
//! Suit isomorphism reduces the 812k unordered combo pairs to a few tens of thousands.

use engine::card::*;
use engine::eval::{eval_masks, SuitMasks};
use rayon::prelude::*;
use std::collections::HashMap;

pub struct EquityTable {
    pub eq: Vec<f32>,
    pub k: Vec<f32>,
}

impl EquityTable {
    #[inline]
    pub fn eq(&self, a: usize, b: usize) -> f32 {
        self.eq[a * NUM_CLASSES + b]
    }
    #[inline]
    pub fn k(&self, a: usize, b: usize) -> f32 {
        self.k[a * NUM_CLASSES + b]
    }

    pub fn save(&self, path: &std::path::Path) -> std::io::Result<()> {
        let mut bytes = Vec::with_capacity(8 * NUM_CLASSES * NUM_CLASSES);
        for x in self.eq.iter().chain(self.k.iter()) {
            bytes.extend_from_slice(&x.to_le_bytes());
        }
        std::fs::write(path, bytes)
    }

    pub fn load(path: &std::path::Path) -> std::io::Result<Self> {
        let bytes = std::fs::read(path)?;
        let n = NUM_CLASSES * NUM_CLASSES;
        let f: Vec<f32> = bytes.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
        if f.len() != 2 * n {
            return Err(std::io::Error::other("bad equity table size"));
        }
        Ok(EquityTable { eq: f[..n].to_vec(), k: f[n..].to_vec() })
    }
}

fn canonical(h1: (Card, Card), h2: (Card, Card)) -> u32 {
    let mut best = u32::MAX;
    for perm in PERMS.iter() {
        let m = |c: Card| make_card(rank(c), perm[suit(c) as usize]);
        let (a, b) = (m(h1.0), m(h1.1));
        let (c, d) = (m(h2.0), m(h2.1));
        let (a, b) = if a < b { (a, b) } else { (b, a) };
        let (c, d) = if c < d { (c, d) } else { (d, c) };
        let key = (a as u32) << 18 | (b as u32) << 12 | (c as u32) << 6 | d as u32;
        best = best.min(key);
    }
    best
}

static PERMS: std::sync::LazyLock<Vec<[u8; 4]>> = std::sync::LazyLock::new(|| {
    let mut out = Vec::new();
    for a in 0..4u8 {
        for b in 0..4u8 {
            for c in 0..4u8 {
                for d in 0..4u8 {
                    let p = [a, b, c, d];
                    let mut s = p;
                    s.sort();
                    if s == [0, 1, 2, 3] {
                        out.push(p);
                    }
                }
            }
        }
    }
    out
});

/// Exact (equity of hand one) over all boards, single-threaded.
fn matchup_equity(key: u32) -> f64 {
    let (a, b, c, d) = ((key >> 18) as u8, ((key >> 12) & 63) as u8, ((key >> 6) & 63) as u8, (key & 63) as u8);
    let dead = (1u64 << a) | (1u64 << b) | (1u64 << c) | (1u64 << d);
    let live: Vec<Card> = (0..52).filter(|&x| dead & (1u64 << x) == 0).collect();
    let m1 = SuitMasks::default().add(a).add(b);
    let m2 = SuitMasks::default().add(c).add(d);
    let merge = |x: SuitMasks, y: SuitMasks| SuitMasks([x.0[0] | y.0[0], x.0[1] | y.0[1], x.0[2] | y.0[2], x.0[3] | y.0[3]]);
    let n = live.len();
    let (mut w, mut t, mut tot) = (0u64, 0u64, 0u64);
    for i in 0..n {
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
                        w += (v1 > v2) as u64;
                        t += (v1 == v2) as u64;
                        tot += 1;
                    }
                }
            }
        }
    }
    (w as f64 + 0.5 * t as f64) / tot as f64
}

pub fn generate() -> EquityTable {
    let combos = all_combos();
    let mut keys = std::collections::HashSet::new();
    for (i, &h1) in combos.iter().enumerate() {
        for &h2 in &combos[i + 1..] {
            let m1 = (1u64 << h1.0) | (1u64 << h1.1);
            let m2 = (1u64 << h2.0) | (1u64 << h2.1);
            if m1 & m2 == 0 {
                keys.insert(canonical(h1, h2));
            }
        }
    }
    let keys: Vec<u32> = keys.into_iter().collect();
    eprintln!("{} distinct matchups up to suit isomorphism", keys.len());
    let done = std::sync::atomic::AtomicUsize::new(0);
    let results: HashMap<u32, f64> = keys
        .par_iter()
        .map(|&k| {
            let e = matchup_equity(k);
            let d = done.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if d % 5000 == 0 {
                eprintln!("  {d}/{}", keys.len());
            }
            (k, e)
        })
        .collect();
    let n = NUM_CLASSES;
    let mut sum = vec![0f64; n * n];
    let mut cnt = vec![0f64; n * n];
    for &h1 in &combos {
        for &h2 in &combos {
            let m1 = (1u64 << h1.0) | (1u64 << h1.1);
            let m2 = (1u64 << h2.0) | (1u64 << h2.1);
            if m1 & m2 != 0 {
                continue;
            }
            // canonical() orders the hands, so look up from hand one's perspective
            let k12 = canonical(h1, h2);
            let e = match results.get(&k12) {
                Some(&e) if is_first(h1, h2, k12) => e,
                Some(&e) => 1.0 - e,
                None => 1.0 - results[&canonical(h2, h1)],
            };
            let (c1, c2) = (class_of(h1.0, h1.1), class_of(h2.0, h2.1));
            sum[c1 * n + c2] += e;
            cnt[c1 * n + c2] += 1.0;
        }
    }
    let eq = (0..n * n).map(|i| if cnt[i] > 0.0 { (sum[i] / cnt[i]) as f32 } else { 0.5 }).collect();
    let k = (0..n * n).map(|i| (cnt[i] / class_combo_count(i / n) as f64) as f32).collect();
    EquityTable { eq, k }
}

/// Whether the canonical key was produced with `h1` as the first hand (some permutation maps
/// (h1, h2) onto the key in that order).
fn is_first(h1: (Card, Card), h2: (Card, Card), key: u32) -> bool {
    for perm in PERMS.iter() {
        let m = |c: Card| make_card(rank(c), perm[suit(c) as usize]);
        let (a, b) = (m(h1.0), m(h1.1));
        let (c, d) = (m(h2.0), m(h2.1));
        let (a, b) = if a < b { (a, b) } else { (b, a) };
        let (c, d) = if c < d { (c, d) } else { (d, c) };
        if (a as u32) << 18 | (b as u32) << 12 | (c as u32) << 6 | d as u32 == key {
            return true;
        }
    }
    false
}
