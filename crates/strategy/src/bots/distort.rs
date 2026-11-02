//! Personality distortions of a base strategy.

use super::profile::Profile;
use crate::postflop::LineAction;
use crate::preflop::charts::ChartNode;
use engine::card::{class_combo_count, NUM_CLASSES};

/// A preflop choice after distortion. `Limp` only appears when a profile limps an unopened pot.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PfChoice {
    Fold,
    Check,
    Call,
    Limp,
    Raise(i32),
    AllIn,
}

/// Static preflop strength used to decide which hands a profile adds or drops at the margin:
/// all-in equity against a random hand plus small playability bonuses.
pub fn preflop_strength(eq: &crate::preflop::equity_table::EquityTable) -> Vec<f64> {
    (0..NUM_CLASSES)
        .map(|h| {
            let mut num = 0.0;
            let mut den = 0.0;
            for h2 in 0..NUM_CLASSES {
                let k = eq.k(h, h2) as f64;
                num += k * eq.eq(h, h2) as f64;
                den += k;
            }
            let (row, col) = (h / 13, h % 13);
            let bonus = if row < col {
                0.03 + if col - row <= 2 { 0.015 } else { 0.0 }
            } else if row > col && row - col <= 1 {
                0.005
            } else {
                0.0
            };
            num / den + bonus
        })
        .collect()
}

/// Distorts the chart mix at `node` for class `class`. Returns (choice, probability) pairs.
pub fn distort_preflop(p: &Profile, node: &ChartNode, class: usize, strength: &[f64]) -> Vec<(PfChoice, f64)> {
    let a = node.actions.len();
    let choices: Vec<PfChoice> = node
        .actions
        .iter()
        .zip(&node.sizes)
        .map(|(c, &s)| match c.as_str() {
            "F" => PfChoice::Fold,
            "K" => PfChoice::Check,
            "C" => PfChoice::Call,
            "A" => PfChoice::AllIn,
            _ => PfChoice::Raise(s),
        })
        .collect();
    let fold_i = choices.iter().position(|c| *c == PfChoice::Fold);
    let mut mix: Vec<f64> = (0..a).map(|i| node.freq[i][class] as f64).collect();
    let norm = mix.iter().sum::<f64>().max(1e-9);
    mix.iter_mut().for_each(|x| *x /= norm);
    if p.is_gto() || p.hunter {
        return choices.into_iter().zip(mix).collect();
    }

    if let Some(fi) = fold_i {
        if node.level <= 1 {
            mix = widen(p.range_width, node, class, strength, fi, &mix);
        } else {
            let f = mix[fi];
            let f2 = (f * p.fold_to_pressure).clamp(0.0, 1.0);
            rescale_nonfold(&mut mix, fi, 1.0 - f2);
        }
    }

    // aggression and light raises among the non-fold actions
    let weak = strength[class] < median_played_strength(node, strength, fold_i);
    let raise_mult = p.aggression * if weak { p.bluff } else { 1.0 };
    let passive: Vec<usize> = (0..a).filter(|&i| matches!(choices[i], PfChoice::Call | PfChoice::Check)).collect();
    let aggressive: Vec<usize> = (0..a).filter(|&i| matches!(choices[i], PfChoice::Raise(_) | PfChoice::AllIn)).collect();
    if !passive.is_empty() && !aggressive.is_empty() {
        let pm: f64 = passive.iter().map(|&i| mix[i]).sum();
        let am: f64 = aggressive.iter().map(|&i| mix[i]).sum();
        let total = pm + am;
        if total > 0.0 {
            let am2 = (am * raise_mult).min(total);
            let pm2 = total - am2;
            for &i in &aggressive {
                mix[i] = if am > 0.0 { mix[i] * am2 / am } else { am2 / aggressive.len() as f64 };
            }
            for &i in &passive {
                mix[i] = if pm > 0.0 { mix[i] * pm2 / pm } else { pm2 / passive.len() as f64 };
            }
        }
    }

    let mut out: Vec<(PfChoice, f64)> = choices.into_iter().zip(mix).collect();
    // open-limping instead of raising (unopened pots outside the blinds)
    if node.level == 0 && p.limp > 0.0 && node.player < 4 {
        if let Some(ri) = out.iter().position(|(c, _)| matches!(c, PfChoice::Raise(_))) {
            let r = out[ri].1;
            let limp = r * p.limp;
            out[ri].1 = r - limp;
            out.push((PfChoice::Limp, limp));
        }
    }
    out
}

fn median_played_strength(node: &ChartNode, strength: &[f64], fold_i: Option<usize>) -> f64 {
    let mut v: Vec<(f64, f64)> = (0..NUM_CLASSES)
        .map(|h| {
            let play = match fold_i {
                Some(fi) => 1.0 - node.freq[fi][h] as f64,
                None => 1.0,
            };
            (strength[h], play * class_combo_count(h) as f64)
        })
        .filter(|x| x.1 > 0.0)
        .collect();
    v.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let total: f64 = v.iter().map(|x| x.1).sum();
    let mut acc = 0.0;
    for (s, w) in v {
        acc += w;
        if acc >= total / 2.0 {
            return s;
        }
    }
    0.5
}

fn rescale_nonfold(mix: &mut [f64], fi: usize, target: f64) {
    let nf: f64 = mix.iter().enumerate().filter(|(i, _)| *i != fi).map(|(_, x)| x).sum();
    let n = mix.len() - 1;
    for (i, x) in mix.iter_mut().enumerate() {
        if i == fi {
            *x = 1.0 - target;
        } else if nf > 0.0 {
            *x *= target / nf;
        } else {
            *x = target / n as f64;
        }
    }
}

/// Scales the number of hands played at a node by `width`, adding the strongest unplayed
/// hands first or dropping the weakest played hands first. Hands that gain play copy the
/// non-fold mix of the weakest hands GTO already plays.
fn widen(width: f64, node: &ChartNode, class: usize, strength: &[f64], fi: usize, mix: &[f64]) -> Vec<f64> {
    let play: Vec<f64> = (0..NUM_CLASSES).map(|h| 1.0 - node.freq[fi][h] as f64).collect();
    let n: Vec<f64> = (0..NUM_CLASSES).map(|h| class_combo_count(h) as f64).collect();
    let mass: f64 = (0..NUM_CLASSES).map(|h| play[h] * n[h]).sum();
    let target = (mass * width).clamp(0.0, 1326.0 * 0.97);
    let mut order: Vec<usize> = (0..NUM_CLASSES).collect();
    let mut newplay = play.clone();
    if target > mass {
        order.sort_by(|&a, &b| strength[b].partial_cmp(&strength[a]).unwrap());
        let mut extra = target - mass;
        for &h in &order {
            if extra <= 0.0 {
                break;
            }
            let room = (1.0 - newplay[h]) * n[h];
            let take = room.min(extra);
            newplay[h] += take / n[h];
            extra -= take;
        }
    } else {
        order.sort_by(|&a, &b| strength[a].partial_cmp(&strength[b]).unwrap());
        let mut cut = mass - target;
        for &h in &order {
            if cut <= 0.0 {
                break;
            }
            let have = newplay[h] * n[h];
            let take = have.min(cut);
            newplay[h] -= take / n[h];
            cut -= take;
        }
    }
    let p_old = play[class];
    let p_new = newplay[class].clamp(0.0, 1.0);
    let a = mix.len();
    let mut out = vec![0.0; a];
    out[fi] = 1.0 - p_new;
    if p_old > 1e-6 {
        for i in 0..a {
            if i != fi {
                out[i] = mix[i] / p_old * p_new;
            }
        }
    } else {
        // template: the non-fold mix of the weakest played hands
        let mut weakest: Vec<usize> = (0..NUM_CLASSES).filter(|&h| play[h] > 0.2).collect();
        weakest.sort_by(|&x, &y| strength[x].partial_cmp(&strength[y]).unwrap());
        weakest.truncate(8);
        let mut t = vec![0.0; a];
        for &h in &weakest {
            for i in 0..a {
                if i != fi {
                    t[i] += node.freq[i][h] as f64;
                }
            }
        }
        let s: f64 = t.iter().sum();
        for i in 0..a {
            if i != fi {
                out[i] = if s > 0.0 { t[i] / s * p_new } else { p_new / (a - 1) as f64 };
            }
        }
    }
    out
}

/// Distorts a postflop solver mix for one hand. `equity` is the hand's equity against the
/// opponent's range at this node.
pub fn distort_postflop(p: &Profile, actions: &[LineAction], mix: &[f32], equity: f32) -> Vec<f64> {
    let mut m: Vec<f64> = mix.iter().map(|&x| x as f64).collect();
    let s: f64 = m.iter().sum::<f64>().max(1e-9);
    m.iter_mut().for_each(|x| *x /= s);
    if p.is_gto() || p.hunter {
        return m;
    }
    let fold_i = actions.iter().position(|a| *a == LineAction::Fold);
    let call_i = actions.iter().position(|a| *a == LineAction::Call);
    let check_i = actions.iter().position(|a| *a == LineAction::Check);
    let aggr: Vec<usize> = (0..actions.len()).filter(|&i| actions[i].is_aggressive()).collect();
    let eq = equity as f64;

    // 1. fold to pressure (and calling-station stickiness with medium hands)
    if let Some(fi) = fold_i {
        let f = m[fi];
        let mut f2 = (f * p.fold_to_pressure).clamp(0.0, 1.0);
        if (0.2..0.6).contains(&eq) {
            f2 *= 1.0 - p.postflop_call.clamp(0.0, 1.0);
        }
        let delta = f - f2;
        m[fi] = f2;
        if delta > 0.0 {
            match call_i {
                Some(ci) => m[ci] += delta,
                None => spread(&mut m, &aggr, delta),
            }
        } else if delta < 0.0 {
            let others: Vec<usize> = (0..m.len()).filter(|&i| i != fi).collect();
            let tot: f64 = others.iter().map(|&i| m[i]).sum();
            if tot > 0.0 {
                for &i in &others {
                    m[i] *= (tot + delta) / tot;
                }
            }
        }
    }

    // 2. aggression, bluffing
    let passive = call_i.or(check_i);
    if let (Some(pi), false) = (passive, aggr.is_empty()) {
        let am: f64 = aggr.iter().map(|&i| m[i]).sum();
        let pool = am + m[pi];
        let bluffing = eq < 0.35;
        let mult = p.aggression * if bluffing { p.bluff } else { 1.0 };
        let mut am2 = (am * mult).min(pool);
        if am < 1e-3 && bluffing && p.bluff > 1.0 {
            am2 = (0.12 * (p.bluff - 1.0)).min(pool * 0.6);
        }
        if am < 1e-3 && eq > 0.7 && p.aggression > 1.0 {
            am2 = (0.3 * (p.aggression - 1.0)).min(pool);
        }
        m[pi] = pool - am2;
        if am > 0.0 {
            for &i in &aggr {
                m[i] *= am2 / am;
            }
        } else {
            spread(&mut m, &aggr, am2);
        }
    }

    // 3. sizing preference among aggressive actions
    if aggr.len() > 1 && p.size_bias != 0.0 {
        let am: f64 = aggr.iter().map(|&i| m[i]).sum();
        if am > 0.0 {
            let k = aggr.len() as f64 - 1.0;
            let mut w: Vec<f64> = aggr
                .iter()
                .enumerate()
                .map(|(j, &i)| {
                    let z = if k > 0.0 { 2.0 * j as f64 / k - 1.0 } else { 0.0 };
                    (m[i] + 0.15 * am / aggr.len() as f64) * (p.size_bias * 1.5 * z).exp()
                })
                .collect();
            let ws: f64 = w.iter().sum();
            w.iter_mut().for_each(|x| *x *= am / ws);
            for (j, &i) in aggr.iter().enumerate() {
                m[i] = w[j];
            }
        }
    }
    let s: f64 = m.iter().sum::<f64>().max(1e-9);
    m.iter_mut().for_each(|x| *x = (*x / s).max(0.0));
    m
}

fn spread(m: &mut [f64], idx: &[usize], amount: f64) {
    if idx.is_empty() {
        return;
    }
    for &i in idx {
        m[i] += amount / idx.len() as f64;
    }
}

pub fn sample<R: rand::Rng>(rng: &mut R, probs: &[f64]) -> usize {
    let total: f64 = probs.iter().sum();
    let mut x = rng.gen::<f64>() * total;
    for (i, &p) in probs.iter().enumerate() {
        if x < p {
            return i;
        }
        x -= p;
    }
    probs.len() - 1
}
