//! Overnight pre-solve: fills the solve cache with the most common heads-up flop spots.
//!
//! Every heads-up flop line of the preflop chart tree is weighted by how often it happens
//! under the charts, every one of the 1,755 strategically distinct flops by how often it is
//! dealt; spots are solved in order of (line frequency x flop frequency) with the same specs
//! as live play, so each one becomes an instant cache hit at the table. Resumable: cached
//! spots are skipped.
//!
//! Spots are ordered by expected cache hits per CPU-second: (line frequency x flop frequency)
//! divided by the typical solve time of the pot type (wide limped pots are slow, 3-bet pots
//! fast).
//!
//! `--migrate-v1` re-keys an existing cache from the old key format (quantized ranges, no
//! depth bucket) to the current one for every library spot, then exits.
//!
//! usage: presolve [--hours H] [--max N] [--min-reach R] [--max-secs S] [--list] [--migrate-v1]
use engine::card::*;
use engine::Range;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use strategy::postflop::*;
use strategy::preflop::charts::Charts;
use strategy::preflop::game::*;

fn arg<T: std::str::FromStr>(args: &[String], name: &str) -> Option<T> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).and_then(|v| v.parse().ok())
}

/// Probability of each heads-up flop terminal under the charts (independent ranges).
fn lines(charts: &Charts, tree: &Tree) -> Vec<(String, f64, [usize; 2], i32, i32)> {
    let mut out = Vec::new();
    for n in &tree.nodes {
        let Node::Terminal { kind: TermKind::Flop, contrib, live, key, .. } = n else { continue };
        if live.len() != 2 {
            continue;
        }
        let codes: Vec<&str> = key.split('-').collect();
        let mut prob = 1.0f64;
        let mut prefix: Vec<&str> = Vec::new();
        for c in &codes {
            let pk = prefix.join("-");
            if let Some(cn) = charts.nodes.get(&pk) {
                let r = charts.range_along(&pk, cn.player);
                let ai = cn.actions.iter().position(|a| a == c).unwrap();
                let (mut num, mut den) = (0.0, 0.0);
                for h in 0..NUM_CLASSES {
                    let w = class_combo_count(h) as f64 * r[h] as f64;
                    num += w * cn.freq[ai][h] as f64;
                    den += w;
                }
                prob *= if den > 0.0 { num / den } else { 0.0 };
            }
            prefix.push(c);
        }
        let mut pl = live.clone();
        pl.sort_by_key(|&p| postflop_rank(p));
        let pot: i32 = contrib.iter().sum();
        let stack = STACK - contrib[pl[0]].max(contrib[pl[1]]);
        out.push((key.clone(), prob, [pl[0], pl[1]], pot, stack));
    }
    out.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    out
}

/// The 1,755 canonical flops with their share of all 22,100 flops.
fn flops() -> Vec<([Card; 3], f64)> {
    let mut map: HashMap<Vec<Card>, ([Card; 3], u32)> = HashMap::new();
    for a in 0..52u8 {
        for b in a + 1..52 {
            for c in b + 1..52 {
                let mut best: Option<Vec<Card>> = None;
                for p in SuitPerm::all() {
                    let mut f = vec![p.card(a), p.card(b), p.card(c)];
                    f.sort();
                    if best.as_ref().map_or(true, |x| f < *x) {
                        best = Some(f);
                    }
                }
                let k = best.unwrap();
                map.entry(k.clone()).or_insert(([k[0], k[1], k[2]], 0)).1 += 1;
            }
        }
    }
    let mut v: Vec<([Card; 3], f64)> = map.into_values().map(|(f, n)| (f, n as f64 / 22100.0)).collect();
    v.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap().then(a.0.cmp(&b.0)));
    v
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let hours: f64 = arg(&args, "--hours").unwrap_or(8.0);
    let max: usize = arg(&args, "--max").unwrap_or(usize::MAX);
    let min_reach: f64 = arg(&args, "--min-reach").unwrap_or(0.002);
    let max_secs: f64 = arg(&args, "--max-secs").unwrap_or(150.0);
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let charts = Charts::load(&root.join("data/preflop/charts.json"))?;
    let cache = Arc::new(SolveCache::new(root.join("data/cache/solves")));
    let tree = Tree::build();
    let all_lines = lines(&charts, &tree);
    let flops = flops();
    if args.iter().any(|a| a == "--migrate-v1") {
        let (mut checked, mut moved) = (0usize, 0usize);
        for (key, _, pl, pot, stack) in &all_lines {
            let base = [Range::from_grid(&charts.range_along(key, pl[0])), Range::from_grid(&charts.range_along(key, pl[1]))];
            for (f, _) in &flops {
                let board = f.to_vec();
                let mut ranges = base.clone();
                ranges[0].remove_dead(mask_of(&board));
                ranges[1].remove_dead(mask_of(&board));
                let spec = SpotSpec { board, ranges, pot: *pot, stack: *stack, depth: STACK, preset: TreePreset::Flop, rake_pct: 0.05, rake_cap: 300.0, extra_lines: vec![], locks: vec![], bunching: None };
                let canon = spec.permuted(spec.canonical_perm());
                checked += 1;
                if cache.rename(&canon.legacy_key_v1(), &canon.key()) {
                    moved += 1;
                }
            }
        }
        let (n, _) = cache.stats();
        println!("checked {checked} library spots, re-keyed {moved} cached solves; {n} solves in the cache");
        return Ok(());
    }
    let lines: Vec<_> = all_lines.into_iter().filter(|l| l.1 >= min_reach).collect();
    println!("{} heads-up flop lines with reach >= {min_reach}, {} canonical flops", lines.len(), flops.len());
    for (k, p, pl, pot, stack) in lines.iter().take(15) {
        println!("  {:<22} {:>6.2}%  {} vs {}  pot {}bb  stack {}bb", k, 100.0 * p, POS[pl[0]], POS[pl[1]], *pot as f64 / 100.0, *stack as f64 / 100.0);
    }
    if args.iter().any(|a| a == "--list") {
        return Ok(());
    }
    // jobs ordered by line reach x flop frequency
    let mut jobs: Vec<(f64, usize, usize)> = Vec::new();
    // typical solve time (s) by starting pot: limped, single-raised, 3-bet, 4-bet
    let cost = |pot: i32| -> f64 {
        match pot {
            p if p <= 300 => 100.0,
            p if p <= 900 => 30.0,
            p if p <= 3000 => 7.0,
            _ => 3.0,
        }
    };
    for (li, l) in lines.iter().enumerate() {
        for (fi, f) in flops.iter().enumerate() {
            jobs.push((l.1 * f.1 / cost(l.3), li, fi));
        }
    }
    jobs.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
    let total_mass: f64 = jobs.iter().map(|j| lines[j.1].1 * flops[j.2].1).sum();
    let t0 = Instant::now();
    let deadline = t0 + Duration::from_secs_f64(hours * 3600.0);
    let (mut solved, mut skipped, mut covered) = (0usize, 0usize, 0.0f64);
    let mut grids: HashMap<usize, [Range; 2]> = HashMap::new();
    for (_, li, fi) in jobs {
        let mass = lines[li].1 * flops[fi].1;
        if Instant::now() > deadline || solved >= max {
            break;
        }
        let (key, _, pl, pot, stack) = &lines[li];
        let board = flops[fi].0.to_vec();
        let base = grids.entry(li).or_insert_with(|| {
            [Range::from_grid(&charts.range_along(key, pl[0])), Range::from_grid(&charts.range_along(key, pl[1]))]
        });
        let mut ranges = base.clone();
        ranges[0].remove_dead(mask_of(&board));
        ranges[1].remove_dead(mask_of(&board));
        let spec = SpotSpec { board: board.clone(), ranges, pot: *pot, stack: *stack, depth: STACK, preset: TreePreset::Flop, rake_pct: 0.05, rake_cap: 300.0, extra_lines: vec![], locks: vec![], bunching: None };
        let k = spec.permuted(spec.canonical_perm()).key();
        covered += mass;
        if cache.contains(&k) {
            skipped += 1;
            continue;
        }
        let ts = Instant::now();
        let mut targets = SolveTargets::for_preset(TreePreset::Flop);
        targets.max_secs = max_secs;
        let spot = Spot::start(spec, cache.clone(), targets);
        let st = spot.wait_final(Duration::from_secs(600));
        solved += 1;
        let elapsed = t0.elapsed().as_secs_f64();
        println!(
            "[{solved:>5}] {:<20} {}  {:>5.1}s  {:.2}% pot  {} it  | coverage {:.2}% of HU flop play  ({:.1} solves/h, {skipped} cached)",
            key,
            cards_to_string(&board),
            ts.elapsed().as_secs_f64(),
            st.exploitability_pct,
            st.iterations,
            100.0 * covered / total_mass,
            solved as f64 / elapsed * 3600.0
        );
    }
    println!("done: {solved} solved, {skipped} already cached, coverage {:.2}% of heads-up flop situations", 100.0 * covered / total_mass);
    Ok(())
}
