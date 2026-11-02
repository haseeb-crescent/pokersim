//! Preflop charts at other stack depths, for Session mode (stacks that drift from 100bb).
//!
//! Solves the same abstract preflop game at each depth (5-bets become non-all-in raises deeper
//! than 100bb) starting from the 100bb realization factors, optionally recalibrating them
//! against postflop-solver at that depth, and writes data/preflop/charts_<N>bb.json.
//! Run it on its own, not during the overnight pre-solve; the 100bb flop library does not
//! depend on these charts.
//!
//! usage: build_depth_charts [--depths 40,60,150,200] [--rounds 0] [--flops 10] [--iters 3000] [--out DIR]
use std::io::Write;
use strategy::preflop::{calibrate::*, cfr::*, charts::*, equity_table::EquityTable, game::*};

fn arg<T: std::str::FromStr>(args: &[String], name: &str) -> Option<T> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).and_then(|v| v.parse().ok())
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let depths: Vec<u32> = args
        .iter()
        .position(|a| a == "--depths")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.split(',').filter_map(|x| x.trim().parse().ok()).collect())
        .unwrap_or_else(|| vec![40, 60, 150, 200]);
    let rounds: u32 = arg(&args, "--rounds").unwrap_or(0);
    let flops: usize = arg(&args, "--flops").unwrap_or(10);
    let iters: u32 = arg(&args, "--iters").unwrap_or(3000);
    let data = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/preflop");
    // inputs always come from data/preflop; outputs go to --out (default: the same folder)
    let out_dir = args.iter().position(|a| a == "--out").and_then(|i| args.get(i + 1)).map(std::path::PathBuf::from).unwrap_or_else(|| data.clone());
    std::fs::create_dir_all(&out_dir)?;
    let eq = EquityTable::load(&data.join("equity169.bin"))?;
    let base: Realization = serde_json::from_str(&std::fs::read_to_string(data.join("realization.json"))?)?;
    let mut logf = std::fs::OpenOptions::new().create(true).append(true).open(out_dir.join("depth_charts.log"))?;
    for depth in depths {
        let mut log = |s: String| {
            println!("{s}");
            let _ = writeln!(logf, "{s}");
        };
        let tree = Tree::build_with_stack(depth as i32 * 100);
        log(format!("{depth}bb: abstract tree {} decisions", tree.decision_count()));
        let mut real = base.clone();
        for round in 0..=rounds {
            let t0 = std::time::Instant::now();
            let mut solver = Solver::new(&tree, &eq, real.clone());
            solver.run(iters, |_, _| {});
            let (ev, gain) = solver.evaluate();
            log(format!("  round {round}: CFR {iters} iters in {:.1}s, BR gain {:?}", t0.elapsed().as_secs_f64(), gain.iter().map(|x| (x * 10000.0).round() / 10000.0).collect::<Vec<_>>()));
            if round == rounds {
                let meta = ChartMeta {
                    source: "Computed locally by this repo: crates/strategy/src/bin/build_depth_charts.rs (see docs/PREFLOP.md)".into(),
                    method: format!(
                        "Discounted CFR ({iters} iterations) over the abstract 6-max preflop game at {depth}bb; realization factors from the 100bb charts{}",
                        if rounds > 0 { format!(", recalibrated {rounds} times at {depth}bb on {flops} flops per spot") } else { String::new() }
                    ),
                    iterations: iters,
                    br_gain_bb: gain,
                    ev_bb: ev,
                    calibration_rounds: rounds,
                    created: format!("unix:{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()),
                    stack_bb: depth as f64,
                };
                let charts = Charts::from_solver(&solver, &tree, meta, true);
                let out = out_dir.join(format!("charts_{depth}bb.json"));
                charts.save(&out)?;
                for key in ["", "F-F-F", "F-F-F-F", "F-F-F-R-F"] {
                    if let Some(n) = charts.nodes.get(key) {
                        let parts: Vec<String> = n
                            .actions
                            .iter()
                            .enumerate()
                            .map(|(ai, a)| {
                                let pct: f64 = (0..169).map(|h| n.freq[ai][h] as f64 * engine::card::class_combo_count(h) as f64).sum::<f64>() / 13.26;
                                format!("{a} {pct:.1}%")
                            })
                            .collect();
                        log(format!("    {:<12} {:<4} {}", if key.is_empty() { "(root)" } else { key }, POS[n.player], parts.join("  ")));
                    }
                }
                log(format!("  wrote {}", out.display()));
                break;
            }
            let (new_real, rep) = calibrate(&solver, &tree, &real, flops, &mut |s| println!("{s}"));
            for l in rep.lines {
                log(format!("    {l}"));
            }
            for (k, v) in new_real.table.iter() {
                let old = real.table.get(k).cloned().unwrap_or_else(|| v.clone());
                real.table.insert(k.clone(), old.iter().zip(v).map(|(a, b)| 0.3 * a + 0.7 * b).collect());
            }
            std::fs::write(out_dir.join(format!("realization_{depth}bb.json")), serde_json::to_string_pretty(&real)?)?;
        }
    }
    Ok(())
}
