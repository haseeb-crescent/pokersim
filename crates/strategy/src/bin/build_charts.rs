//! Full preflop pipeline: solve with heuristic realization, calibrate realization against
//! postflop-solver on sampled flops, re-solve, repeat; then write data/preflop/charts.json,
//! data/preflop/realization.json and data/preflop/calibration.log.
//! usage: build_charts [rounds] [flops_per_spot] [cfr_iters]
use std::io::Write;
use strategy::preflop::{calibrate::*, cfr::*, charts::*, equity_table::EquityTable, game::*};

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let rounds: u32 = args.get(1).map(|x| x.parse().unwrap()).unwrap_or(2);
    let flops: usize = args.get(2).map(|x| x.parse().unwrap()).unwrap_or(8);
    let iters: u32 = args.get(3).map(|x| x.parse().unwrap()).unwrap_or(2500);
    let data = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/preflop");
    let eq = EquityTable::load(&data.join("equity169.bin"))?;
    let tree = Tree::build();
    let mut logf = std::fs::File::create(data.join("calibration.log"))?;
    let mut log = |s: String| {
        println!("{s}");
        let _ = writeln!(logf, "{s}");
    };
    let mut real = match std::fs::read_to_string(data.join("realization.json")) {
        Ok(s) if args.iter().any(|a| a == "--resume") => serde_json::from_str(&s)?,
        _ => Realization::heuristic(),
    };
    if args.iter().any(|a| a == "--smooth") {
        strategy::preflop::calibrate::smooth(&mut real, 2, 0.4);
        log("applied neighbour smoothing to the realization factors".into());
    }
    let summary = |s: &Solver, log: &mut dyn FnMut(String)| {
        let charts = Charts::from_solver(s, &tree, dummy_meta(), false);
        for key in ["", "F", "F-F", "F-F-F", "F-F-F-F", "F-F-F-R-F", "F-F-F-F-R", "F-F-R-F-F-F"] {
            if let Some(n) = charts.nodes.get(key) {
                let mut parts = Vec::new();
                for (ai, a) in n.actions.iter().enumerate() {
                    let pct: f64 = (0..169).map(|h| n.freq[ai][h] as f64 * engine::card::class_combo_count(h) as f64).sum::<f64>() / 13.26;
                    parts.push(format!("{a} {pct:.1}%"));
                }
                log(format!("    {:<14} {:<4} {}", if key.is_empty() { "(root)" } else { key }, POS[n.player], parts.join("  ")));
            }
        }
    };
    let mut solver_state;
    for round in 0..=rounds {
        let t0 = std::time::Instant::now();
        let mut solver = Solver::new(&tree, &eq, real.clone());
        solver.run(iters, |_, _| {});
        let (ev, gain) = solver.evaluate();
        log(format!("round {round}: CFR {iters} iters in {:.1}s, EV/pos {:?} bb, BR gain {:?}", t0.elapsed().as_secs_f64(),
            ev.iter().map(|x| (x * 1000.0).round() / 1000.0).collect::<Vec<_>>(), gain.iter().map(|x| (x * 10000.0).round() / 10000.0).collect::<Vec<_>>()));
        summary(&solver, &mut log);
        if round == rounds {
            let meta = ChartMeta {
                source: "Computed locally by this repo: crates/strategy/src/preflop (see docs/PREFLOP.md)".into(),
                method: format!("Discounted CFR ({iters} iterations) over an abstract 6-max 100bb preflop game with 5% rake capped at 3bb; flop values = equity x realization, realization calibrated {rounds} times against postflop-solver on {flops} flops per spot"),
                iterations: iters,
                br_gain_bb: gain,
                ev_bb: ev,
                calibration_rounds: rounds,
                created: format!("unix:{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()),
            };
            let charts = Charts::from_solver(&solver, &tree, meta, true);
            charts.save(&data.join("charts.json"))?;
            std::fs::write(data.join("realization.json"), serde_json::to_string_pretty(&real)?)?;
            log(format!("wrote charts.json ({} nodes) and realization.json", charts.nodes.len()));
            solver_state = Some(());
            let _ = solver_state;
            break;
        }
        let (new_real, rep) = calibrate(&solver, &tree, &real, flops, &mut log);
        for l in rep.lines {
            log(format!("  {l}"));
        }
        // damped update for stability
        for (k, v) in new_real.table.iter() {
            let old = real.table.get(k).cloned().unwrap_or_else(|| v.clone());
            real.table.insert(k.clone(), old.iter().zip(v).map(|(a, b)| 0.3 * a + 0.7 * b).collect());
        }
        std::fs::write(data.join("realization.json"), serde_json::to_string_pretty(&real)?)?;
    }
    Ok(())
}

fn dummy_meta() -> ChartMeta {
    ChartMeta { source: String::new(), method: String::new(), iterations: 0, br_gain_bb: vec![], ev_bb: vec![], calibration_rounds: 0, created: String::new() }
}
