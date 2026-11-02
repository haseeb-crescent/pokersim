//! The live table: deals hands, runs the bots, waits for the hero, grades the hero's
//! decisions in the background and writes every hand to the logbook.

use crate::coach::{grade, HeroDecision};
use crate::state::*;
use crate::view::*;
use engine::card::*;
use engine::rules::*;
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use strategy::bots::decide::{decide, BotEnv};
use strategy::bots::hunter::{spot_key, HunterModel};
use strategy::bots::profile::Profile;
use strategy::bots::view::{positions, PublicState, POS_NAMES};
use strategy::coach::facts::board_texture;
use strategy::postflop::grids::grids;
use strategy::postflop::hand::HandSolve;
use strategy::preflop::game::postflop_rank;
use tokio::sync::mpsc;

#[derive(Debug)]
pub enum TableCmd {
    Action(Act),
    NextHand,
    Pause(bool),
    SetDelay(u64),
    SetCoach(String),
    Stop,
}

fn d_coach() -> String {
    "decision".into()
}
fn d_delay() -> u64 {
    700
}
fn d_rebuy() -> String {
    "reset".into()
}
fn d_rake() -> f64 {
    0.05
}
fn d_cap() -> f64 {
    3.0
}
fn d_true() -> bool {
    true
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DrillSpec {
    pub hero_pos: String,
    pub villain_pos: String,
    /// "SRP" or "3BP"
    pub pot_type: String,
    /// any, dry, wet, monotone, paired, ahigh, low
    pub texture: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionConfig {
    pub lineup: Vec<String>,
    #[serde(default = "d_coach")]
    pub coach_mode: String,
    #[serde(default = "d_delay")]
    pub bot_delay_ms: u64,
    #[serde(default = "d_rebuy")]
    pub rebuy: String,
    #[serde(default = "d_rake")]
    pub rake_pct: f64,
    #[serde(default = "d_cap")]
    pub rake_cap_bb: f64,
    /// when set, the hero seat is played by this bot profile (bot-vs-bot spectating)
    #[serde(default)]
    pub hero_profile: Option<String>,
    #[serde(default)]
    pub drill: Option<DrillSpec>,
    #[serde(default = "d_true")]
    pub auto_deal: bool,
}

struct Controls {
    paused: bool,
    delay: u64,
    coach: String,
}

impl Controls {
    fn json(&self) -> Value {
        json!({"paused": self.paused, "bot_delay_ms": self.delay, "coach_mode": self.coach})
    }
}

pub fn start_table(app: &App, cfg: SessionConfig) -> i64 {
    if let Some(old) = app.table.lock().unwrap().take() {
        let _ = old.cmd.send(TableCmd::Stop);
    }
    let kind = if cfg.drill.is_some() { "drill" } else if cfg.hero_profile.is_some() { "botvbot" } else { "play" };
    let session_id = app.db.new_session(&serde_json::to_value(&cfg).unwrap(), kind);
    let (tx, rx) = mpsc::unbounded_channel();
    let latest = Arc::new(Mutex::new(None));
    tokio::spawn(run_table(app.clone(), cfg, session_id, rx, latest.clone()));
    *app.table.lock().unwrap() = Some(TableHandle { cmd: tx, latest, session_id });
    session_id
}

fn broadcast(app: &App, latest: &Arc<Mutex<Option<Value>>>, msg: Value) {
    if msg["type"] == "state" {
        *latest.lock().unwrap() = Some(msg.clone());
    }
    let _ = app.tx.send(msg.to_string());
}

const HERO: usize = 0;

/// Position index (0 UTG .. 5 BB) to the button seat that puts the hero there.
fn button_for(pos: usize) -> usize {
    (12 + 3 - pos) % 6
}

fn seat_of_pos(button: usize, pos: usize) -> usize {
    (button + 3 + pos) % 6
}

fn pos_index(name: &str) -> Option<usize> {
    POS_NAMES.iter().position(|p| p.eq_ignore_ascii_case(name))
}

/// Deck that deals the given hole cards and flop (burns and the rest random).
fn stacked_deck(button: usize, holes: &[Option<[Card; 2]>; 6], flop: Option<[Card; 3]>, rng: &mut StdRng) -> Vec<Card> {
    let mut used: Vec<Card> = holes.iter().flatten().flat_map(|h| h.to_vec()).collect();
    if let Some(f) = flop {
        used.extend_from_slice(&f);
    }
    let mut rest: Vec<Card> = (0..52).filter(|c| !used.contains(c)).collect();
    rest.shuffle(rng);
    let mut take = || rest.pop().unwrap();
    let order: Vec<usize> = (1..=6).map(|k| (button + k) % 6).collect();
    let mut deck = Vec::with_capacity(52);
    let mut second = Vec::new();
    for &s in &order {
        match holes[s] {
            Some(h) => {
                deck.push(h[0]);
                second.push(Some(h[1]));
            }
            None => {
                deck.push(take());
                second.push(None);
            }
        }
    }
    for c in second {
        deck.push(c.unwrap_or_else(&mut take));
    }
    deck.push(take());
    match flop {
        Some(f) => deck.extend_from_slice(&f),
        None => {
            for _ in 0..3 {
                deck.push(take());
            }
        }
    }
    while deck.len() < 52 {
        deck.push(take());
    }
    deck
}

fn texture_ok(flop: &[Card], texture: &str) -> bool {
    let t = board_texture(flop);
    match texture {
        "dry" => t.contains("(dry)"),
        "wet" => t.contains("(wet)"),
        "monotone" => t.contains("monotone"),
        "paired" => t.contains("paired"),
        "ahigh" => flop.iter().any(|&c| rank(c) == 12),
        "low" => flop.iter().all(|&c| rank(c) <= 7),
        _ => true,
    }
}

/// Scripted preflop action for a drill.
fn drill_action(h: &Hand, spec: &DrillSpec, app: &App) -> Act {
    let pos = positions(h);
    let seat = h.to_act.unwrap();
    let hp = pos_index(&spec.hero_pos).unwrap_or(3);
    let vp = pos_index(&spec.villain_pos).unwrap_or(5);
    let (opener, other) = if hp < vp { (hp, vp) } else { (vp, hp) };
    let raises = h.events.iter().filter(|e| matches!(e, Event::Act { street: Street::Preflop, kind: ActKind::Raise | ActKind::Bet, .. })).count();
    let st = PublicState::from_hand(h);
    let chart_raise = || -> Act {
        let codes = st.preflop_codes();
        let size = app
            .res
            .charts
            .lookup(&codes)
            .and_then(|(n, _)| n.actions.iter().position(|a| a == "R").map(|i| n.sizes[i] as i64))
            .unwrap_or(250);
        Act::BetTo(size)
    };
    let p = pos[seat];
    if p == opener && raises == 0 {
        chart_raise()
    } else if p == other && raises == 1 {
        if spec.pot_type == "3BP" { chart_raise() } else { Act::Call }
    } else if p == opener && raises == 2 {
        Act::Call
    } else if h.legal().map(|l| l.can_check).unwrap_or(false) {
        Act::Check
    } else {
        Act::Fold
    }
}

fn sample_from_grid(grid: &[f32], dead: u64, rng: &mut StdRng) -> Option<[Card; 2]> {
    let mut items = Vec::new();
    let mut total = 0.0;
    for i in 0..NUM_COMBOS {
        let (a, b) = combo_cards(i);
        if dead & ((1u64 << a) | (1u64 << b)) != 0 {
            continue;
        }
        let w = grid[class_of(a, b)];
        if w > 0.001 {
            total += w;
            items.push((total, [a, b]));
        }
    }
    if items.is_empty() {
        return None;
    }
    let x = rng.gen::<f32>() * total;
    items.iter().find(|(c, _)| *c >= x).map(|(_, h)| *h).or(items.last().map(|x| x.1))
}

/// Builds the deck for one drill hand: both players' hands from their chart ranges for the
/// line, and a flop of the requested texture.
fn drill_deck(app: &App, spec: &DrillSpec, cfg: &TableConfig, button: usize, rng: &mut StdRng) -> Vec<Card> {
    // dry run to learn the preflop line
    let mut probe: Vec<Card> = (0..52).collect();
    probe.shuffle(rng);
    let mut h = Hand::start(cfg.clone(), &[Some(10_000); 6], button, probe);
    while h.street == Street::Preflop && h.to_act.is_some() {
        let a = drill_action(&h, spec, app);
        if h.apply(h.to_act.unwrap(), a).is_err() {
            break;
        }
    }
    let st = PublicState::from_hand(&h);
    let codes = st.preflop_codes();
    let key = app.res.charts.map_key(&codes).map(|k| k.0).unwrap_or_default();
    let hp = pos_index(&spec.hero_pos).unwrap_or(3);
    let vp = pos_index(&spec.villain_pos).unwrap_or(5);
    for _ in 0..200 {
        let hero = sample_from_grid(&app.res.charts.range_along(&key, hp), 0, rng);
        let Some(hero) = hero else { break };
        let dead = (1u64 << hero[0]) | (1u64 << hero[1]);
        let Some(vil) = sample_from_grid(&app.res.charts.range_along(&key, vp), dead, rng) else { continue };
        let dead = dead | (1u64 << vil[0]) | (1u64 << vil[1]);
        for _ in 0..500 {
            let mut live: Vec<Card> = (0..52).filter(|&c| dead & (1u64 << c) == 0).collect();
            live.shuffle(rng);
            let flop = [live[0], live[1], live[2]];
            if texture_ok(&flop, &spec.texture) {
                let mut holes: [Option<[Card; 2]>; 6] = [None; 6];
                holes[seat_of_pos(button, hp)] = Some(hero);
                holes[seat_of_pos(button, vp)] = Some(vil);
                return stacked_deck(button, &holes, Some(flop), rng);
            }
        }
    }
    let mut d: Vec<Card> = (0..52).collect();
    d.shuffle(rng);
    d
}

struct HandCtx {
    hs: Option<Arc<HandSolve>>,
    last_board: usize,
    decisions: usize,
    grade_tasks: Vec<tokio::task::JoinHandle<Option<Value>>>,
    bot_log: Vec<Value>,
    thinking: Option<(usize, String)>,
}

fn sync_streets(h: &Hand, ctx: &mut HandCtx, app: &App, rake: (f64, f64)) {
    let blen = h.board.len();
    if blen == ctx.last_board {
        return;
    }
    let prev_len = ctx.last_board;
    ctx.last_board = blen;
    if h.to_act.is_none() {
        return;
    }
    let st = PublicState::from_hand(h);
    let live = st.live_seats();
    if live.len() != 2 {
        return;
    }
    let mut pair = live.clone();
    pair.sort_by_key(|&s| postflop_rank(st.positions[s]));
    let (oop, ip) = (pair[0], pair[1]);
    let pot = h.pot_total() as i32;
    let stack = h.seats[oop].stack.min(h.seats[ip].stack) as i32;
    if blen == 3 && ctx.hs.is_none() {
        let codes = st.preflop_codes();
        let Some((key, changed)) = app.res.charts.map_key(&codes) else { return };
        if changed.iter().any(|&c| c) {
            return;
        }
        let g_oop = app.res.charts.range_along(&key, st.positions[oop]);
        let g_ip = app.res.charts.range_along(&key, st.positions[ip]);
        if g_oop.iter().all(|&x| x <= 0.0) || g_ip.iter().all(|&x| x <= 0.0) {
            return;
        }
        let ranges = [engine::Range::from_grid(&g_oop), engine::Range::from_grid(&g_ip)];
        // fold ranges of the preflop folders for the bunching effect
        let mut folds = Vec::new();
        for (i, &c) in codes.iter().enumerate() {
            if c != 'F' {
                continue;
            }
            let Some((k, _)) = app.res.charts.map_key(&codes[..i]) else { continue };
            if let Some(n) = app.res.charts.nodes.get(&k) {
                let reach = app.res.charts.range_along(&k, n.player);
                if let Some(fi) = n.actions.iter().position(|a| a == "F") {
                    let g: Vec<f32> = (0..NUM_CLASSES).map(|h| reach[h] * n.freq[fi][h]).collect();
                    folds.push(engine::Range::from_grid(&g));
                }
            }
        }
        ctx.hs = Some(Arc::new(HandSolve::start_flop([oop, ip], h.board.clone(), ranges, pot, stack, app.res.cache.clone(), rake, folds)));
    } else if blen > 3 {
        if let Some(hs) = &ctx.hs {
            let prev = match prev_len {
                3 => Street::Flop,
                4 => Street::Turn,
                _ => return,
            };
            let line = st.street_line(prev);
            if let Err(e) = hs.advance(h.board.clone(), &line, pot, stack) {
                eprintln!("advance failed: {e}");
                ctx.hs = None;
            }
        }
    }
}

fn seat_names(profiles: &[Option<Profile>], spectate: bool) -> Vec<SeatInfo> {
    let mut out = Vec::new();
    let mut counts = std::collections::HashMap::new();
    for (i, p) in profiles.iter().enumerate() {
        if i == HERO && !spectate {
            out.push(SeatInfo { name: "Hero".into(), profile: "hero".into() });
            continue;
        }
        let p = p.as_ref().unwrap();
        let n = counts.entry(p.id.clone()).or_insert(0);
        *n += 1;
        let name = if *n > 1 { format!("{} {}", p.name, n) } else { p.name.clone() };
        out.push(SeatInfo { name, profile: p.id.clone() });
    }
    out
}

async fn run_table(app: App, cfg: SessionConfig, session_id: i64, mut rx: mpsc::UnboundedReceiver<TableCmd>, latest: Arc<Mutex<Option<Value>>>) {
    let res = app.res.clone();
    let spectate = cfg.hero_profile.is_some();
    let mut profiles: Vec<Option<Profile>> = vec![None; 6];
    if let Some(hp) = &cfg.hero_profile {
        profiles[HERO] = res.profile(hp).or_else(|| res.profile("gto"));
    }
    for i in 1..6 {
        let id = cfg.lineup.get(i - 1).cloned().unwrap_or_else(|| "gto".into());
        profiles[i] = res.profile(&id).or_else(|| res.profile("gto"));
    }
    let seats = seat_names(&profiles, spectate);
    let mut ctl = Controls { paused: false, delay: cfg.bot_delay_ms, coach: cfg.coach_mode.clone() };
    let table_cfg = TableConfig {
        seats: 6,
        small_blind: 50,
        big_blind: 100,
        ante: 0,
        rake: RakeConfig { percent: cfg.rake_pct, cap: (cfg.rake_cap_bb * 100.0).round() as i64, no_flop_no_drop: true },
    };
    let rake = (cfg.rake_pct, cfg.rake_cap_bb * 100.0);
    let mut stacks = vec![10_000i64; 6];
    let mut rng = StdRng::from_entropy();
    let mut button = rng.gen_range(0..6);
    let hunter = Arc::new(HunterModel::new());
    let mut hero_net: i64 = 0;
    let mut hands_played: i64 = 0;
    let mut last_result = Value::Null;
    let mut exploits_saved = 0usize;

    'session: loop {
        // rebuys: bots always top up to 100bb; the hero per the session setting
        for s in 0..6 {
            let hero = s == HERO && !spectate;
            let mode = if hero { cfg.rebuy.as_str() } else if cfg.rebuy == "reset" { "reset" } else { "topup" };
            stacks[s] = match mode {
                "reset" => 10_000,
                "topup" => stacks[s].max(10_000),
                _ => if stacks[s] < table_cfg.big_blind { 10_000 } else { stacks[s] },
            };
        }
        if let Some(d) = &cfg.drill {
            button = button_for(pos_index(&d.hero_pos).unwrap_or(3));
            stacks = vec![10_000; 6];
        }
        let hand_no = app.db.next_hand_no();
        let deck = match &cfg.drill {
            Some(d) => drill_deck(&app, d, &table_cfg, button, &mut rng),
            None => {
                let mut d: Vec<Card> = (0..52).collect();
                d.shuffle(&mut rng);
                d
            }
        };
        let st_opts: Vec<Option<Chips>> = stacks.iter().map(|&s| Some(s)).collect();
        let mut hand = Hand::start(table_cfg.clone(), &st_opts, button, deck);
        let hero_pos = POS_NAMES[positions(&hand)[HERO]].to_string();
        let hero_hole = hand.seats[HERO].hole;
        let hand_id = app.db.new_hand(session_id, hand_no, HERO as i64, &hero_pos, &cards_to_string(&hero_hole));
        *hunter.hand_no.lock().unwrap() = hand_no as u64;
        let mut ctx = HandCtx { hs: None, last_board: 0, decisions: 0, grade_tasks: Vec::new(), bot_log: Vec::new(), thinking: None };

        if let Some(d) = &cfg.drill {
            while hand.street == Street::Preflop && hand.to_act.is_some() {
                let a = drill_action(&hand, d, &app);
                let s = hand.to_act.unwrap();
                if hand.apply(s, a).is_err() {
                    let _ = hand.apply(s, Act::Fold);
                }
            }
        }

        let has_hunter = profiles.iter().flatten().any(|p| p.hunter);
        let hunter_read = |h: &HunterModel| -> Value {
            if !has_hunter {
                return Value::Null;
            }
            let c = h.counts.lock().unwrap();
            json!(c.iter().map(|(k, v)| json!({"spot": k, "n": v.n, "fold": v.fold, "call": v.call, "raise": v.raise, "check": v.check, "bet": v.bet})).collect::<Vec<_>>())
        };
        let extra = |hero_net: i64, hands_played: i64, last_result: &Value| {
            json!({"session_id": session_id, "hand_id": hand_id, "hand_no": hand_no, "hero_net": hero_net, "hands_played": hands_played, "last_result": last_result, "hunter_read": hunter_read(&hunter)})
        };

        while !hand.is_over() {
            sync_streets(&hand, &mut ctx, &app, rake);
            let seat = hand.to_act.unwrap();
            let st = PublicState::from_hand(&hand);
            if seat == HERO && !spectate {
                let legal = hand.legal().unwrap();
                let node = ctx.hs.as_ref().and_then(|hs| {
                    let p = hs.player_of(HERO)?;
                    let v = hs.current().public.node(&st.street_line(st.street), false).ok()?;
                    (v.player == p && !v.terminal && !v.chance).then_some(v)
                });
                let p = presets(&hand, &legal, node.as_ref());
                let msg = json!({"type": "state", "state": table_state(Some(&hand), &seats, HERO, spectate, None, Some((legal.clone(), p)), &ctl.json(), extra(hero_net, hands_played, &last_result))});
                broadcast(&app, &latest, msg);
                let act = loop {
                    match rx.recv().await {
                        Some(TableCmd::Action(a)) => {
                            let mut test = hand.clone();
                            match test.apply(HERO, a) {
                                Ok(()) => break a,
                                Err(e) => broadcast(&app, &latest, json!({"type": "error", "message": e.to_string()})),
                            }
                        }
                        Some(TableCmd::Pause(p)) => ctl.paused = p,
                        Some(TableCmd::SetDelay(ms)) => ctl.delay = ms,
                        Some(TableCmd::SetCoach(m)) => ctl.coach = m,
                        Some(TableCmd::NextHand) => {}
                        Some(TableCmd::Stop) | None => break 'session,
                    }
                };
                if let Some(k) = spot_key(&st, HERO) {
                    hunter.observe(&k, act, legal.can_check);
                }
                let villain = {
                    let live = st.live_seats();
                    if live.len() == 2 {
                        let v = *live.iter().find(|&&s| s != HERO).unwrap();
                        profiles[v].clone().map(|p| (v, p))
                    } else {
                        None
                    }
                };
                let d = HeroDecision {
                    hand_id,
                    index: ctx.decisions,
                    public: st.clone(),
                    hero_seat: HERO,
                    hole: hero_hole,
                    act,
                    hand_solve: ctx.hs.clone(),
                    street_solve: ctx.hs.as_ref().map(|h| h.current()),
                    villain,
                };
                ctx.decisions += 1;
                let app2 = app.clone();
                let mode = ctl.coach.clone();
                let task = tokio::spawn(async move {
                    let res = app2.res.clone();
                    let g = tokio::task::spawn_blocking(move || grade(&res, &d, true)).await.ok()?;
                    app2.db.save_decision(g["hand_id"].as_i64().unwrap_or(0), g["index"].as_i64().unwrap_or(0), &g);
                    if mode == "decision" {
                        let _ = app2.tx.send(json!({"type": "grade", "grade": g}).to_string());
                    }
                    if mode != "off" {
                        spawn_explanation(app2.clone(), g.clone(), mode == "decision");
                    }
                    Some(g)
                });
                ctx.grade_tasks.push(task);
                hand.apply(HERO, act).unwrap();
            } else {
                let profile = profiles[seat].clone().unwrap();
                ctx.thinking = Some((seat, "thinking…".into()));
                let msg = json!({"type": "state", "state": table_state(Some(&hand), &seats, HERO, spectate, ctx.thinking.clone(), None, &ctl.json(), extra(hero_net, hands_played, &last_result))});
                broadcast(&app, &latest, msg);
                let t0 = Instant::now();
                let res2 = res.clone();
                let hs2 = ctx.hs.clone();
                let hunter2 = hunter.clone();
                let hole = hand.seats[seat].hole;
                let st2 = st.clone();
                let seed: u64 = rng.gen();
                let hero_seat = if spectate { None } else { Some(HERO) };
                let decision = tokio::task::spawn_blocking(move || {
                    let env = BotEnv {
                        charts: &res2.charts,
                        strength: &res2.strength,
                        hand_solve: hs2.as_deref(),
                        hunter: Some(&hunter2),
                        hero_seat,
                        max_wait: Duration::from_secs(6),
                        act_precision: 3.0,
                        mc_samples: 3000,
                        fast: false,
                    };
                    let mut r = StdRng::seed_from_u64(seed);
                    decide(&profile, &st2, seat, hole, &env, &mut r)
                })
                .await;
                let elapsed = t0.elapsed().as_millis() as u64;
                // honour pause / speed changes while "thinking"
                let mut wait = ctl.delay.saturating_sub(elapsed);
                loop {
                    while let Ok(cmd) = rx.try_recv() {
                        match cmd {
                            TableCmd::Pause(p) => ctl.paused = p,
                            TableCmd::SetDelay(ms) => ctl.delay = ms,
                            TableCmd::SetCoach(m) => ctl.coach = m,
                            TableCmd::Stop => break 'session,
                            _ => {}
                        }
                    }
                    if !ctl.paused && wait == 0 {
                        break;
                    }
                    let step = if ctl.paused { 100 } else { wait.min(100) };
                    tokio::time::sleep(Duration::from_millis(step)).await;
                    wait = wait.saturating_sub(step);
                }
                let (act, info) = match decision {
                    Ok(d) => (d.act, json!({"seat": seat, "street": hand.street.name(), "source": d.source, "mix": d.mix, "exploit": d.exploit})),
                    Err(e) => (Act::Fold, json!({"seat": seat, "source": format!("error: {e}")})),
                };
                if hand.apply(seat, act).is_err() {
                    let l = hand.legal().unwrap();
                    let fallback = if l.can_check { Act::Check } else { Act::Fold };
                    hand.apply(seat, fallback).unwrap();
                }
                ctx.bot_log.push(info);
                ctx.thinking = None;
            }
        }

        // ---- hand complete ----
        let r = hand.result.clone().unwrap();
        let flags = engine::stats::seat_flags(&hand);
        let net = r.net[HERO];
        hero_net += net;
        hands_played += 1;
        let names: Vec<String> = seats.iter().map(|s| s.name.clone()).collect();
        let pos: Vec<&str> = positions(&hand).iter().map(|&p| POS_NAMES.get(p).copied().unwrap_or("")).collect();
        let st_end = PublicState::from_hand(&hand);
        let preflop_key = res.charts.map_key(&st_end.preflop_codes()).map(|k| k.0);
        let record = json!({
            "hand": hand,
            "names": names,
            "profiles": seats.iter().map(|s| s.profile.clone()).collect::<Vec<_>>(),
            "positions": pos,
            "hero_seat": HERO,
            "flags": flags,
            "bot_decisions": ctx.bot_log,
            "preflop_key": preflop_key,
            "solved": ctx.hs.is_some(),
            "session_id": session_id,
        });
        app.db.finish_hand(hand_id, &cards_to_string(&hand.board), net, flags[HERO].net_allin_adj, hand.pot_total(), r.showdown, &record);

        // replay snapshots of every solved postflop node
        if let Some(hs) = ctx.hs.clone() {
            let app2 = app.clone();
            let h2 = hand.clone();
            tokio::task::spawn_blocking(move || {
                let snaps = replay_snapshots(&h2, &hs);
                app2.db.set_replay(hand_id, &json!(snaps));
            });
        }
        {
            let ex = hunter.exploits.lock().unwrap();
            for e in ex.iter().skip(exploits_saved) {
                app.db.add_exploit(session_id, e);
            }
            exploits_saved = ex.len();
        }

        let mut grades = Vec::new();
        if ctl.coach == "hand" {
            for t in ctx.grade_tasks.drain(..) {
                if let Ok(Ok(Some(g))) = tokio::time::timeout(Duration::from_secs(150), t).await {
                    grades.push(g);
                }
            }
        }
        let winners: Vec<Value> = (0..6)
            .filter(|&i| r.won[i] > 0)
            .map(|i| json!({"seat": i, "amount": r.won[i], "hand": r.values[i].map(engine::eval::describe)}))
            .collect();
        let showdown: Vec<Value> = r
            .shown
            .iter()
            .map(|&i| json!({"seat": i, "cards": [card_to_string(hand.seats[i].hole[0]), card_to_string(hand.seats[i].hole[1])], "description": r.values[i].map(engine::eval::describe).unwrap_or_default()}))
            .collect();
        last_result = json!({"hand_id": hand_id, "winners": winners, "hero_net": net, "showdown": showdown, "grades": grades});
        broadcast(&app, &latest, json!({"type": "hand_complete", "summary": last_result}));
        let msg = json!({"type": "state", "state": table_state(Some(&hand), &seats, HERO, spectate, None, None, &ctl.json(), extra(hero_net, hands_played, &last_result))});
        broadcast(&app, &latest, msg);

        stacks = hand.stacks();
        button = hand.next_in_hand(button);

        // between hands
        let wait_for_click = !cfg.auto_deal || ctl.coach == "hand";
        let hero_folded_early = hand.seats[HERO].folded && !spectate;
        let mut pause_ms = if hero_folded_early { 400 } else { (ctl.delay * 2).max(900) };
        loop {
            let cmd = if wait_for_click || ctl.paused {
                rx.recv().await
            } else {
                match tokio::time::timeout(Duration::from_millis(pause_ms), rx.recv()).await {
                    Ok(c) => c,
                    Err(_) => break,
                }
            };
            match cmd {
                Some(TableCmd::NextHand) => break,
                Some(TableCmd::Pause(p)) => {
                    ctl.paused = p;
                    pause_ms = 300;
                }
                Some(TableCmd::SetDelay(ms)) => ctl.delay = ms,
                Some(TableCmd::SetCoach(m)) => ctl.coach = m,
                Some(TableCmd::Action(_)) => {}
                Some(TableCmd::Stop) | None => break 'session,
            }
            if !wait_for_click && !ctl.paused {
                continue;
            }
        }
    }
    let _ = app.tx.send(json!({"type": "state", "state": {"running": false, "street": "idle", "seats": [], "board": [], "log": [], "presets": []}}).to_string());
}

/// Grids for every postflop decision of a heads-up solved hand (the public solves).
fn replay_snapshots(h: &Hand, hs: &HandSolve) -> Vec<Value> {
    let st = PublicState::from_hand(h);
    let mut out = Vec::new();
    for (si, street) in [Street::Flop, Street::Turn, Street::River].iter().enumerate() {
        let Some(ss) = hs.street(si) else { break };
        ss.public.wait_final(Duration::from_secs(60));
        let line = st.street_line(*street);
        let acts: Vec<_> = st.actions.iter().filter(|a| a.street == *street).collect();
        for k in 0..line.len() {
            if let Ok(v) = ss.public.node(&line[..k], true) {
                if v.terminal || v.chance {
                    continue;
                }
                out.push(json!({
                    "street": street.name(),
                    "index": k,
                    "seat": acts[k].seat,
                    "line": line[..k].iter().map(|a| a.label()).collect::<Vec<_>>(),
                    "took": line[k].label(),
                    "grids": grids(&v),
                }));
            }
        }
    }
    out
}

pub fn spawn_explanation(app: App, g: Value, push: bool) {
    let Some(llm) = app.llm.clone() else { return };
    tokio::spawn(async move {
        match llm.explain(&g).await {
            Ok(text) => {
                let (hid, idx) = (g["hand_id"].as_i64().unwrap_or(0), g["index"].as_i64().unwrap_or(0));
                app.db.set_explanation(hid, idx, &text);
                if push {
                    let _ = app.tx.send(json!({"type": "explanation", "hand_id": hid, "index": idx, "text": text}).to_string());
                }
            }
            Err(e) => eprintln!("explanation failed: {e}"),
        }
    });
}
