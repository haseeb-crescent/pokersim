//! Bot-vs-bot simulation for the lab: many hands quickly across all cores.
//!
//! Preflop uses the charts with each profile's distortions exactly as at the table. Postflop
//! uses the fast heuristic policy (equity against estimated ranges, pot odds, profile) for all
//! pots: running the solver for millions of hands is not feasible, so simulated postflop play
//! is an approximation of the bots' table play. Stacks reset to 100bb every hand.

use crate::state::*;
use crate::stats::Agg;
use engine::rules::*;
use rand::rngs::SmallRng;
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
use rayon::prelude::*;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use strategy::bots::decide::{decide, BotEnv};
use strategy::bots::profile::Profile;
use strategy::bots::view::PublicState;

pub struct SimState {
    pub id: String,
    pub lineup: Vec<String>,
    pub target: u64,
    pub hands: u64,
    pub done: bool,
    pub stop: bool,
    pub started: Instant,
    pub agg: Vec<Agg>,
    pub series: Vec<(u64, Vec<f64>)>,
}

impl SimState {
    pub fn json(&self, profiles: &[Profile]) -> Value {
        let secs = self.started.elapsed().as_secs_f64().max(1e-3);
        let results: Vec<Value> = (0..6)
            .map(|s| {
                let p = profiles.iter().find(|p| p.id == self.lineup[s]);
                json!({
                    "seat": s,
                    "profile": self.lineup[s],
                    "name": p.map(|p| p.name.clone()).unwrap_or_default(),
                    "bb100": (self.agg[s].bb100() * 100.0).round() / 100.0,
                    "ci95": (self.agg[s].ci95() * 100.0).round() / 100.0,
                    "stats": self.agg[s].json(),
                    "expected": p.map(|p| p.expected.iter().map(|(k, v)| (k.clone(), json!(v))).collect::<serde_json::Map<_, _>>()).unwrap_or_default(),
                })
            })
            .collect();
        json!({
            "id": self.id,
            "done": self.done,
            "hands": self.hands,
            "target": self.target,
            "hands_per_sec": (self.hands as f64 / secs).round(),
            "results": results,
            "series": self.series.iter().map(|(h, c)| json!({"hand": h, "cum_bb": c})).collect::<Vec<_>>(),
        })
    }
}

pub fn start_sim(app: &App, lineup: Vec<String>, hands: u64) -> String {
    let id = format!("sim{}", rand::thread_rng().gen::<u32>());
    let st = Arc::new(Mutex::new(SimState {
        id: id.clone(),
        lineup: lineup.clone(),
        target: hands,
        hands: 0,
        done: false,
        stop: false,
        started: Instant::now(),
        agg: vec![Agg::default(); 6],
        series: vec![(0, vec![0.0; 6])],
    }));
    app.sims.lock().unwrap().insert(id.clone(), st.clone());
    let res = app.res.clone();
    std::thread::spawn(move || run(res, lineup, hands, st));
    id
}

fn run(res: Arc<Resources>, lineup: Vec<String>, hands: u64, st: Arc<Mutex<SimState>>) {
    let profiles: Vec<Profile> = lineup.iter().map(|id| res.profile(id).unwrap_or_else(|| res.profile("gto").unwrap())).collect();
    let chunk = 2_000u64;
    let chunks = hands.div_ceil(chunk);
    let mut cum = vec![0.0f64; 6];
    // process in waves so progress and the swing graph update while running
    let wave = (rayon::current_num_threads() as u64 * 2).max(1);
    let mut c0 = 0;
    while c0 < chunks {
        if st.lock().unwrap().stop {
            break;
        }
        let c1 = (c0 + wave).min(chunks);
        let results: Vec<(Vec<Agg>, Vec<Vec<f64>>)> = (c0..c1)
            .into_par_iter()
            .map(|c| {
                let n = chunk.min(hands - c * chunk);
                play_chunk(&res, &profiles, n, c)
            })
            .collect();
        let mut s = st.lock().unwrap();
        for (agg, path) in results {
            for (k, a) in agg.iter().enumerate() {
                s.agg[k].merge(a);
            }
            for step in path {
                for k in 0..6 {
                    cum[k] += step[k];
                }
                s.hands += 200;
                let h = s.hands;
                s.series.push((h, cum.iter().map(|x| (x * 10.0).round() / 10.0).collect()));
            }
            let total: u64 = s.agg[0].hands;
            s.hands = total;
        }
        if s.series.len() > 2000 {
            let keep: Vec<(u64, Vec<f64>)> = s.series.iter().step_by(2).cloned().collect();
            s.series = keep;
        }
        drop(s);
        c0 = c1;
    }
    st.lock().unwrap().done = true;
}

fn play_chunk(res: &Resources, profiles: &[Profile], n: u64, seed: u64) -> (Vec<Agg>, Vec<Vec<f64>>) {
    let mut rng = SmallRng::seed_from_u64(0xC0FFEE ^ seed.wrapping_mul(0x9E3779B97F4A7C15));
    let mut agg = vec![Agg::default(); 6];
    let mut path = Vec::new();
    let mut step = vec![0.0f64; 6];
    let cfg = TableConfig::six_max();
    let mut button = (seed % 6) as usize;
    let env = BotEnv {
        charts: &res.charts,
        strength: &res.strength,
        hand_solve: None,
        hunter: None,
        hero_seat: None,
        max_wait: Duration::from_millis(0),
        act_precision: 99.0,
        mc_samples: 120,
        fast: true,
    };
    for i in 0..n {
        let mut deck: Vec<u8> = (0..52).collect();
        deck.shuffle(&mut rng);
        let mut h = Hand::start(cfg.clone(), &[Some(10_000); 6], button, deck);
        let mut guard = 0;
        while let Some(seat) = h.to_act {
            guard += 1;
            if guard > 200 {
                break;
            }
            let st = PublicState::from_hand(&h);
            let d = decide(&profiles[seat], &st, seat, h.seats[seat].hole, &env, &mut rng);
            if h.apply(seat, d.act).is_err() {
                let l = h.legal().unwrap();
                let _ = h.apply(seat, if l.can_check { Act::Check } else { Act::Fold });
            }
        }
        if h.result.is_some() {
            let flags = engine::stats::seat_flags(&h);
            for k in 0..6 {
                agg[k].add(&flags[k]);
                step[k] += flags[k].net as f64 / 100.0;
            }
        }
        if (i + 1) % 200 == 0 {
            path.push(std::mem::replace(&mut step, vec![0.0; 6]));
        }
        button = (button + 1) % 6;
    }
    (agg, path)
}
