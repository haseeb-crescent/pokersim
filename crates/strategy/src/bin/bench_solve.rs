//! Measures postflop-solver solve times on this machine for candidate trees.
//! usage: bench_solve <spot> <tree> [max_secs] [compress 0|1]
use postflop_solver::*;
use std::time::Instant;

const BTN_OPEN: &str = "22+,A2s+,A2o+,K2s+,K8o+,Q4s+,Q9o+,J6s+,J9o+,T6s+,T9o,95s+,98o,85s+,74s+,64s+,53s+,43s";
const BB_VS_BTN: &str = "QQ-22:0.7,AQs-A2s,AJo-A2o,K2s+,K7o+,Q3s+,Q8o+,J5s+,J8o+,T6s+,T8o+,96s+,98o,85s+,87o,74s+,64s+,53s+,43s";
const CO_3B: &str = "TT+,AJs+,AQo+,KQs,A5s-A4s,K9s,QJs:0.5,JTs:0.5,76s:0.4,65s:0.4";
const BTN_CALL_3B: &str = "QQ-22,AJs-ATs,AQo,KJs+,QJs,JTs,T9s,98s,87s,76s,65s,54s";

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let spot = a[1].as_str();
    let tree = a[2].as_str();
    let max_secs: f64 = a.get(3).map(|x| x.parse().unwrap()).unwrap_or(40.0);
    let compress = a.get(4).map(|x| x == "1").unwrap_or(true);
    let (oop, ip, flop, turn, river, pot, stack) = match spot {
        "k72" => (BB_VS_BTN, BTN_OPEN, "Kh7d2c", "", "", 550, 9750),
        "t98" => (BB_VS_BTN, BTN_OPEN, "Ts9s8d", "", "", 550, 9750),
        "q63_3b" => (CO_3B, BTN_CALL_3B, "Qd6c3h", "", "", 2050, 8850),
        "k72_turn" => (BB_VS_BTN, BTN_OPEN, "Kh7d2c", "5s", "", 1100, 9475),
        "k72_river" => (BB_VS_BTN, BTN_OPEN, "Kh7d2c", "5s", "Jd", 2200, 8925),
        _ => panic!("spot"),
    };
    // (flop, turn, river) bet strings, raise string, caps
    let (sizes, caps): ([&str; 3], [i32; 3]) = match tree {
        "one" => (["33%", "66%", "75%"], [2, 2, 2]),
        "one_c1" => (["33%", "66%", "75%"], [2, 1, 1]),
        "flop2" => (["33%,75%", "66%", "75%"], [2, 2, 2]),
        "flop2_c1" => (["33%,75%", "66%", "75%"], [2, 1, 1]),
        "flop3_c1" => (["25%,50%,100%", "66%", "75%"], [2, 1, 1]),
        "street3" => (["33%,75%,125%", "33%,75%,125%", "33%,75%,125%"], [3, 3, 3]),
        _ => panic!("tree"),
    };
    let s = |b: &str| BetSizeOptions::try_from((b, "3x")).unwrap();
    let floor: f32 = std::env::var("FLOOR").ok().map(|x| x.parse().unwrap()).unwrap_or(0.0);
    let fl = |r: &str| { let mut r: Range = r.parse().unwrap(); if floor > 0.0 { for c1 in 0..52u8 { for c2 in c1+1..52u8 { if r.get_weight_by_cards(c1, c2) < floor { r.set_weight_by_cards(c1, c2, floor); } } } } r };
    let card_config = CardConfig {
        range: [fl(oop), fl(ip)],
        flop: flop_from_str(flop).unwrap(),
        turn: if turn.is_empty() { NOT_DEALT } else { card_from_str(turn).unwrap() },
        river: if river.is_empty() { NOT_DEALT } else { card_from_str(river).unwrap() },
    };
    let state = if !river.is_empty() { BoardState::River } else if !turn.is_empty() { BoardState::Turn } else { BoardState::Flop };
    let tree_config = TreeConfig {
        initial_state: state, starting_pot: pot, effective_stack: stack,
        rake_rate: 0.05, rake_cap: 300.0,
        flop_bet_sizes: [s(sizes[0]), s(sizes[0])],
        turn_bet_sizes: [s(sizes[1]), s(sizes[1])],
        river_bet_sizes: [s(sizes[2]), s(sizes[2])],
        turn_donk_sizes: None, river_donk_sizes: None,
        add_allin_threshold: 1.5, force_allin_threshold: 0.15, merging_threshold: 0.1,
        max_bets_per_street: caps,
    };
    let t0 = Instant::now();
    let mut game = PostFlopGame::with_config(card_config, ActionTree::new(tree_config).unwrap()).unwrap();
    let (mem, memc) = game.memory_usage();
    game.allocate_memory(compress);
    let t_build = t0.elapsed().as_secs_f64();
    let t1 = Instant::now();
    let (mut it, mut expl, mut marks, mut th) = (0u32, f32::MAX, Vec::new(), vec![2.0f32, 1.0, 0.5, 0.3]);
    while it < 3000 && !th.is_empty() && t1.elapsed().as_secs_f64() < max_secs {
        solve_step(&game, it);
        it += 1;
        if it % 5 == 0 {
            expl = 100.0 * compute_exploitability(&game) / pot as f32;
            while let Some(&t) = th.first() {
                if expl <= t { marks.push(format!("{t}%@{:.1}s", t1.elapsed().as_secs_f64())); th.remove(0); } else { break; }
            }
        }
    }
    finalize(&mut game);
    let path = std::env::temp_dir().join("bench_flop_only.bin");
    let mut size = 0;
    if state == BoardState::Flop {
        game.set_target_storage_mode(BoardState::Flop).unwrap();
        save_data_to_file(&game, "", &path, Some(3)).unwrap();
        size = std::fs::metadata(&path).unwrap().len();
    }
    println!("{spot:<10} {tree:<9} thr {:>2} cmp {} mem {:>6.0}MB build {:.2}s | {} | it {it} final {expl:.2}% | flop-only file {:.1}MB",
        rayon::current_num_threads(), compress as u8, (if compress { memc } else { mem }) as f64 / 1e6, t_build, marks.join(" "), size as f64 / 1e6);
}
