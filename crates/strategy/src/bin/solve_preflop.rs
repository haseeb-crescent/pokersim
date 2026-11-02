//! Solves the abstract 6-max 100bb preflop game and writes data/preflop/charts.json.
//! usage: solve_preflop [iterations] [realization.json]
use strategy::preflop::{cfr::*, charts::*, equity_table::EquityTable, game::*};

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let iters: u32 = args.get(1).map(|x| x.parse().unwrap()).unwrap_or(1000);
    let data = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/preflop");
    let real = match args.get(2) {
        Some(p) => serde_json::from_str(&std::fs::read_to_string(p)?)?,
        None => Realization::heuristic(),
    };
    let eq = EquityTable::load(&data.join("equity169.bin"))?;
    let tree = Tree::build();
    println!("abstract tree: {} nodes, {} decisions", tree.nodes.len(), tree.decision_count());
    let t0 = std::time::Instant::now();
    let mut solver = Solver::new(&tree, &eq, real);
    solver.run(iters, |t, s| {
        if t % 100 == 0 || t == 10 {
            let (ev, gain) = s.evaluate();
            println!("iter {t:>5} {:>6.1}s  EV {:?}  BR gain {:?}", t0.elapsed().as_secs_f64(),
                ev.iter().map(|x| (x * 1000.0).round() / 1000.0).collect::<Vec<_>>(),
                gain.iter().map(|x| (x * 1000.0).round() / 1000.0).collect::<Vec<_>>());
        }
    });
    let (ev, gain) = solver.evaluate();
    let meta = ChartMeta {
        source: "Computed locally by this repo's preflop solver (crates/strategy/src/preflop)".into(),
        method: "Discounted CFR over an abstract 6-max 100bb preflop game (169 classes); postflop value from equity x realization factors".into(),
        iterations: iters,
        br_gain_bb: gain,
        ev_bb: ev,
        calibration_rounds: 0,
        created: chrono_like_now(),
    };
    let charts = Charts::from_solver(&solver, &tree, meta, true);
    let out = args.get(3).cloned().unwrap_or_else(|| data.join("charts.json").to_string_lossy().to_string());
    charts.save(std::path::Path::new(&out))?;
    println!("wrote {out}");
    for key in ["", "F", "F-F", "F-F-F", "F-F-F-F", "F-F-F-R-F", "F-F-F-F-R"] {
        if let Some(n) = charts.nodes.get(key) {
            let total: f64 = (0..169).map(|h| (1.0 - n.freq[0][h] as f64) * engine::card::class_combo_count(h) as f64).sum::<f64>() / 1326.0;
            println!("{:<12} {:<4} plays {:>5.1}%  ({})", if key.is_empty() { "(root)" } else { key }, POS[n.player], 100.0 * total, charts.describe(key));
        }
    }
    Ok(())
}

fn chrono_like_now() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    format!("unix:{secs}")
}
