//! HTTP + WebSocket API.

use crate::state::*;
use crate::table::{start_table, DrillSpec, SessionConfig, TableCmd};
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use engine::card::*;
use engine::equity::{equity, Holding};
use engine::rules::*;
use engine::Range;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use strategy::postflop::grids::grids;
use strategy::postflop::*;
use strategy::preflop::game::POS;

type R = Result<Json<Value>, (StatusCode, String)>;

fn bad(msg: impl ToString) -> (StatusCode, String) {
    (StatusCode::BAD_REQUEST, msg.to_string())
}

pub fn router(app: App) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/profiles", get(profiles))
        .route("/api/table/state", get(table_state))
        .route("/api/session/start", post(session_start))
        .route("/api/session/stop", post(session_stop))
        .route("/api/sessions", get(sessions))
        .route("/api/hands", get(list_hands))
        .route("/api/hands/{id}", get(get_hand))
        .route("/api/hands/{id}/notes", post(set_notes))
        .route("/api/hands/{id}/hh", get(hand_hh))
        .route("/api/hands/{id}/explain", post(explain))
        .route("/api/export/hh", get(export_hh))
        .route("/api/coach/ask", post(ask))
        .route("/api/stats", get(stats))
        .route("/api/leaks", get(leaks))
        .route("/api/drill/start", post(drill_start))
        .route("/api/lab/equity", post(lab_equity))
        .route("/api/lab/range/parse", post(range_parse))
        .route("/api/lab/preflop/node", get(preflop_node))
        .route("/api/lab/postflop/solve", post(postflop_solve))
        .route("/api/lab/postflop/{id}/node", get(postflop_node))
        .route("/api/sim/start", post(sim_start))
        .route("/api/sim/{id}", get(sim_status))
        .route("/api/sim/{id}/stop", post(sim_stop))
        .route("/api/presolve/status", get(presolve_status))
        .route("/ws", get(ws))
        .with_state(app)
}

async fn health(State(app): State<App>) -> Json<Value> {
    let via = app.llm.as_ref().map(|l| l.via()).unwrap_or("none");
    Json(json!({"ok": true, "explanations": via != "none", "explanations_via": via}))
}

async fn profiles(State(app): State<App>) -> Json<Value> {
    let v: Vec<Value> = app
        .res
        .profiles
        .iter()
        .map(|p| {
            json!({
                "id": p.id, "name": p.name, "description": p.description,
                "params": p.params().into_iter().collect::<HashMap<_, _>>(),
                "expected": p.expected.iter().cloned().collect::<HashMap<_, _>>(),
                "hunter": p.hunter,
            })
        })
        .collect();
    Json(json!(v))
}

async fn table_state(State(app): State<App>) -> Json<Value> {
    let t = app.table.lock().unwrap();
    match t.as_ref().and_then(|h| h.latest.lock().unwrap().clone()) {
        Some(v) => Json(v["state"].clone()),
        None => Json(json!({"running": t.is_some(), "street": "idle", "seats": [], "board": [], "log": [], "presets": []})),
    }
}

async fn session_start(State(app): State<App>, Json(cfg): Json<SessionConfig>) -> R {
    if cfg.lineup.len() != 5 && cfg.hero_profile.is_none() {
        return Err(bad("lineup must have 5 bot profiles"));
    }
    let id = start_table(&app, cfg);
    Ok(Json(json!({"session_id": id})))
}

async fn session_stop(State(app): State<App>) -> Json<Value> {
    if let Some(t) = app.table.lock().unwrap().take() {
        let _ = t.cmd.send(TableCmd::Stop);
    }
    Json(json!({"ok": true}))
}

async fn sessions(State(app): State<App>) -> Json<Value> {
    Json(json!(app.db.sessions()))
}

#[derive(Deserialize)]
struct HandsQ {
    limit: Option<i64>,
    offset: Option<i64>,
    session: Option<i64>,
    tag: Option<String>,
    min_loss: Option<f64>,
}

async fn list_hands(State(app): State<App>, Query(q): Query<HandsQ>) -> Json<Value> {
    Json(json!(app.db.list_hands(q.limit.unwrap_or(100).min(1000), q.offset.unwrap_or(0), q.session, q.tag.as_deref(), q.min_loss)))
}

async fn get_hand(State(app): State<App>, Path(id): Path<i64>) -> R {
    let mut h = app.db.hand(id).ok_or((StatusCode::NOT_FOUND, "no such hand".into()))?;
    h["decisions"] = json!(app.db.decisions(id));
    Ok(Json(h))
}

#[derive(Deserialize)]
struct NotesBody {
    notes: String,
    tags: String,
}

async fn set_notes(State(app): State<App>, Path(id): Path<i64>, Json(b): Json<NotesBody>) -> Json<Value> {
    app.db.set_notes(id, &b.notes, &b.tags);
    Json(json!({"ok": true}))
}

fn hh_text(rec: &Value, id: i64, played_at: i64) -> Option<String> {
    let hand: Hand = serde_json::from_value(rec["hand"].clone()).ok()?;
    let names: Vec<String> = serde_json::from_value(rec["names"].clone()).ok()?;
    let ts = format_ts(played_at);
    Some(engine::hh::pokerstars(&hand, &engine::hh::HhInfo { hand_id: id as u64, table: "Solver Trainer", names: &names, timestamp: &ts, hero_seat: rec["hero_seat"].as_u64().map(|x| x as usize) }))
}

/// Civil date-time (UTC) for hand histories.
fn format_ts(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    // days since 1970-01-01 to y/m/d (Howard Hinnant's algorithm)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mo <= 2 { y + 1 } else { y };
    format!("{y:04}/{mo:02}/{d:02} {h:02}:{m:02}:{s:02}")
}

async fn hand_hh(State(app): State<App>, Path(id): Path<i64>) -> Response {
    let Some(h) = app.db.hand(id) else { return (StatusCode::NOT_FOUND, "no such hand").into_response() };
    let text = hh_text(&h["record"], id, h["played_at"].as_i64().unwrap_or(0)).unwrap_or_default();
    ([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], text).into_response()
}

#[derive(Deserialize)]
struct ExportQ {
    session: Option<i64>,
}

async fn export_hh(State(app): State<App>, Query(q): Query<ExportQ>) -> Response {
    let mut out = String::new();
    for (id, rec) in app.db.records(q.session) {
        let played = app.db.hand(id).and_then(|h| h["played_at"].as_i64()).unwrap_or(0);
        if let Some(t) = hh_text(&rec, id, played) {
            out.push_str(&t);
            out.push('\n');
        }
    }
    (
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8"), (header::CONTENT_DISPOSITION, "attachment; filename=\"solver-trainer-hands.txt\"")],
        out,
    )
        .into_response()
}

#[derive(Deserialize)]
struct ExplainBody {
    index: i64,
}

async fn explain(State(app): State<App>, Path(id): Path<i64>, Json(b): Json<ExplainBody>) -> R {
    let llm = app.llm.clone().ok_or_else(|| bad("No explanation backend: log in to the claude CLI, or set ANTHROPIC_API_KEY in .env"))?;
    let g = app.db.decision(id, b.index).ok_or((StatusCode::NOT_FOUND, "no such decision".into()))?;
    let text = llm.explain(&g).await.map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?;
    app.db.set_explanation(id, b.index, &text);
    Ok(Json(json!({"text": text})))
}

#[derive(Deserialize)]
struct AskBody {
    hand_id: i64,
    index: Option<i64>,
    messages: Vec<AskMsg>,
    /// continues a claude CLI conversation (from the previous reply)
    chat_id: Option<String>,
}

#[derive(Deserialize)]
struct AskMsg {
    role: String,
    text: String,
}

async fn ask(State(app): State<App>, Json(b): Json<AskBody>) -> R {
    let llm = app.llm.clone().ok_or_else(|| bad("No coach chat backend: log in to the claude CLI, or set ANTHROPIC_API_KEY in .env"))?;
    let h = app.db.hand(b.hand_id).ok_or((StatusCode::NOT_FOUND, "no such hand".into()))?;
    let decisions = app.db.decisions(b.hand_id);
    let rec = &h["record"];
    let log: Vec<String> = serde_json::from_value::<Hand>(rec["hand"].clone())
        .map(|hand| {
            let seats: Vec<crate::view::SeatInfo> = rec["names"]
                .as_array()
                .map(|a| a.iter().map(|n| crate::view::SeatInfo { name: n.as_str().unwrap_or("").into(), profile: String::new() }).collect())
                .unwrap_or_default();
            hand.events.iter().filter_map(|e| crate::view::log_line(&hand, e, &seats)).collect()
        })
        .unwrap_or_default();
    let focus = b.index.and_then(|i| decisions.iter().find(|d| d["index"].as_i64() == Some(i)).cloned());
    let context = json!({
        "hero_cards": h["hero_cards"], "hero_position": h["hero_position"], "board": h["board"],
        "hero_result_bb": h["hero_net"].as_i64().map(|x| x as f64 / 100.0),
        "action_log": log,
        "graded_decisions": decisions.iter().map(crate::llm::explain_payload).collect::<Vec<_>>(),
        "focus_decision": focus.as_ref().map(crate::llm::explain_payload),
    });
    let hist: Vec<(String, String)> = b.messages.into_iter().map(|m| (if m.role == "assistant" { "assistant".into() } else { "user".into() }, m.text)).collect();
    if hist.is_empty() || hist.last().unwrap().0 != "user" {
        return Err(bad("the last message must be the student's question"));
    }
    let reply = llm.ask(&context, &hist, b.chat_id.as_deref()).await.map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?;
    Ok(Json(json!({"reply": reply.text, "chat_id": reply.chat_id, "via": llm.via()})))
}

#[derive(Deserialize)]
struct SessionQ {
    session: Option<i64>,
}

async fn stats(State(app): State<App>, Query(q): Query<SessionQ>) -> Json<Value> {
    let recs = app.db.records(q.session);
    let mut hero = crate::stats::Agg::default();
    let mut by_profile: HashMap<String, crate::stats::Agg> = HashMap::new();
    let mut by_pos: HashMap<String, crate::stats::Agg> = HashMap::new();
    let mut graph = Vec::new();
    let (mut cum, mut cum_adj) = (0.0, 0.0);
    for (i, (_, rec)) in recs.iter().enumerate() {
        let Ok(flags) = serde_json::from_value::<Vec<engine::stats::SeatFlags>>(rec["flags"].clone()) else { continue };
        let hs = rec["hero_seat"].as_u64().unwrap_or(0) as usize;
        let profs: Vec<String> = serde_json::from_value(rec["profiles"].clone()).unwrap_or_default();
        let pos: Vec<String> = serde_json::from_value(rec["positions"].clone()).unwrap_or_default();
        for (s, f) in flags.iter().enumerate() {
            if s == hs && profs.get(s).map(|p| p == "hero").unwrap_or(true) {
                hero.add(f);
                by_pos.entry(pos.get(s).cloned().unwrap_or_default()).or_default().add(f);
                cum += f.net as f64 / 100.0;
                cum_adj += f.net_allin_adj / 100.0;
                if i % 5 == 0 || i + 1 == recs.len() {
                    graph.push(json!({"hand": i + 1, "net": (cum * 10.0f64).round() / 10.0, "adj": (cum_adj * 10.0f64).round() / 10.0}));
                }
            } else if let Some(p) = profs.get(s) {
                by_profile.entry(p.clone()).or_default().add(f);
            }
        }
    }
    let mut pos_list: Vec<(String, Value)> = by_pos.into_iter().map(|(k, a)| (k, a.json())).collect();
    pos_list.sort_by_key(|(k, _)| POS.iter().position(|p| p == k).unwrap_or(9));
    Json(json!({
        "hero": hero.json(),
        "by_position": pos_list.into_iter().map(|(k, v)| json!({"position": k, "stats": v})).collect::<Vec<_>>(),
        "bots": by_profile.into_iter().map(|(k, a)| json!({"profile": k, "stats": a.json()})).collect::<Vec<_>>(),
        "graph": graph,
    }))
}

async fn leaks(State(app): State<App>, Query(q): Query<SessionQ>) -> Json<Value> {
    #[derive(Default)]
    struct L {
        n: u64,
        loss: f64,
        mistakes: u64,
        worst: Vec<(f64, i64, i64, String)>,
        spot: String,
        street: String,
        kind: String,
    }
    let mut map: HashMap<String, L> = HashMap::new();
    for (hand_id, g) in app.db.all_decisions(q.session) {
        let key = g["spot_key"].as_str().unwrap_or("").to_string();
        if key.is_empty() {
            continue;
        }
        let e = map.entry(key).or_default();
        e.n += 1;
        let loss = g["ev_loss_bb"].as_f64().unwrap_or(0.0);
        e.loss += loss;
        let grade = g["grade"].as_str().unwrap_or("");
        let off_chart = g["kind"] == "preflop_chart" && matches!(grade, "Mistake" | "Blunder" | "Inaccuracy");
        if matches!(grade, "Inaccuracy" | "Mistake" | "Blunder") {
            e.mistakes += 1;
        }
        // preflop has no EV: rank chart deviations by (1 - chosen frequency)
        let score = if g["kind"] == "preflop_chart" { if off_chart { 0.5 * (1.0 - g["chosen_freq"].as_f64().unwrap_or(1.0)) } else { 0.0 } } else { loss };
        e.worst.push((score, hand_id, g["index"].as_i64().unwrap_or(0), g["spot"].as_str().unwrap_or("").to_string()));
        e.spot = g["spot"].as_str().unwrap_or("").to_string();
        e.street = g["street"].as_str().unwrap_or("").to_string();
        e.kind = g["kind"].as_str().unwrap_or("").to_string();
    }
    let mut list: Vec<Value> = map
        .into_iter()
        .map(|(k, mut l)| {
            l.worst.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
            let parts: Vec<&str> = k.split('|').collect();
            let drill = if parts.len() == 4 && parts[0] != "MW" {
                // "BTN vs BB · ..." -> positions
                let mut it = l.spot.split(" vs ");
                let hp = it.next().unwrap_or("BTN").trim().to_string();
                let vp = it.next().and_then(|s| s.split_whitespace().next()).unwrap_or("BB").to_string();
                let pt = if parts[0] == "3BP" { "3BP" } else { "SRP" };
                Some(json!({"hero_pos": hp, "villain_pos": vp, "pot_type": pt, "texture": "any"}))
            } else {
                None
            };
            let score: f64 = if l.kind == "preflop_chart" { l.worst.iter().map(|w| w.0).sum() } else { l.loss };
            json!({
                "spot_key": k, "example": l.spot, "street": l.street, "kind": l.kind,
                "decisions": l.n, "ev_loss_bb": (l.loss * 100.0).round() / 100.0,
                "avg_loss_bb": if l.n > 0 { (l.loss / l.n as f64 * 1000.0).round() / 1000.0 } else { 0.0 },
                "mistakes": l.mistakes, "score": (score * 100.0).round() / 100.0,
                "hands": l.worst.iter().take(8).filter(|w| w.0 > 0.0).map(|w| json!({"hand_id": w.1, "index": w.2, "loss": (w.0 * 100.0).round() / 100.0})).collect::<Vec<_>>(),
                "drill": drill,
            })
        })
        .collect();
    list.sort_by(|a, b| b["score"].as_f64().unwrap_or(0.0).partial_cmp(&a["score"].as_f64().unwrap_or(0.0)).unwrap());
    Json(json!({"leaks": list, "hunter": app.db.exploits(q.session)}))
}

#[derive(Deserialize)]
struct DrillBody {
    #[serde(flatten)]
    spec: DrillSpec,
    lineup: Option<Vec<String>>,
    villain_profile: Option<String>,
    bot_delay_ms: Option<u64>,
}

async fn drill_start(State(app): State<App>, Json(b): Json<DrillBody>) -> R {
    let vp = b.villain_profile.unwrap_or_else(|| "gto".into());
    let lineup = b.lineup.unwrap_or_else(|| vec![vp.clone(); 5]);
    let cfg = SessionConfig {
        lineup,
        coach_mode: "decision".into(),
        bot_delay_ms: b.bot_delay_ms.unwrap_or(400),
        rebuy: "reset".into(),
        rake_pct: 0.05,
        rake_cap_bb: 3.0,
        hero_profile: None,
        drill: Some(b.spec),
        auto_deal: false,
    };
    let id = start_table(&app, cfg);
    Ok(Json(json!({"session_id": id})))
}

#[derive(Deserialize)]
struct EqBody {
    players: Vec<String>,
    board: String,
    #[serde(default)]
    dead: String,
    samples: Option<u64>,
}

fn holding(s: &str) -> Result<Holding, String> {
    let t = s.trim();
    if let Some(c) = cards_from_str(t).filter(|c| c.len() == 2 && t.len() == 4) {
        return Ok(Holding::Hand(c[0], c[1]));
    }
    Range::parse(t).map(Holding::Range)
}

async fn lab_equity(Json(b): Json<EqBody>) -> R {
    if b.players.len() < 2 || b.players.len() > 6 {
        return Err(bad("between 2 and 6 players"));
    }
    let board = if b.board.trim().is_empty() { vec![] } else { cards_from_str(&b.board).ok_or_else(|| bad("bad board"))? };
    if board.len() > 5 || board.len() == 1 || board.len() == 2 {
        return Err(bad("board must have 0, 3, 4 or 5 cards"));
    }
    let dead = if b.dead.trim().is_empty() { vec![] } else { cards_from_str(&b.dead).ok_or_else(|| bad("bad dead cards"))? };
    let players: Vec<Holding> = b.players.iter().map(|p| holding(p)).collect::<Result<_, _>>().map_err(bad)?;
    let dm = mask_of(&board) | mask_of(&dead);
    let combos: Vec<f32> = players
        .iter()
        .map(|p| match p {
            Holding::Hand(..) => 1.0,
            Holding::Range(r) => r.combos(dm),
        })
        .collect();
    let samples = b.samples.unwrap_or(300_000).min(5_000_000);
    let t0 = Instant::now();
    let r = tokio::task::spawn_blocking(move || equity(&players, &board, &dead, samples, 60_000_000)).await.map_err(|e| bad(e))?;
    Ok(Json(json!({"equity": r.equity, "win": r.win, "tie": r.tie, "exact": r.exact, "samples": r.samples, "combos": combos, "ms": t0.elapsed().as_millis()})))
}

#[derive(Deserialize)]
struct TextBody {
    text: String,
}

async fn range_parse(Json(b): Json<TextBody>) -> Json<Value> {
    match Range::parse(&b.text) {
        Ok(r) => Json(json!({"ok": true, "error": null, "grid": r.to_grid(0), "combos": r.combos(0), "percent": r.percent(), "text": r.to_string_compact()})),
        Err(e) => Json(json!({"ok": false, "error": e, "grid": vec![0.0; 169], "combos": 0, "percent": 0, "text": b.text})),
    }
}

#[derive(Deserialize)]
struct KeyQ {
    key: Option<String>,
}

async fn preflop_node(State(app): State<App>, Query(q): Query<KeyQ>) -> R {
    let key = q.key.unwrap_or_default();
    let charts = &app.res.charts;
    let n = charts.nodes.get(&key).ok_or((StatusCode::NOT_FOUND, format!("no chart node '{key}'")))?;
    let actions: Vec<Value> = n
        .actions
        .iter()
        .zip(&n.sizes)
        .map(|(a, &s)| {
            let label = match a.as_str() {
                "F" => "Fold".to_string(),
                "K" => "Check".to_string(),
                "C" => if n.level == 0 { "Limp".to_string() } else { "Call".to_string() },
                "A" => "All-in".to_string(),
                _ => format!("Raise to {}", fmt_bb(s)),
            };
            json!({"code": a, "label": label, "size_bb": if s > 0 { json!(s as f64 / 100.0) } else { Value::Null }})
        })
        .collect();
    let children: Vec<Value> = n
        .actions
        .iter()
        .map(|a| {
            let k = if key.is_empty() { a.clone() } else { format!("{key}-{a}") };
            json!({"code": a, "key": if charts.nodes.contains_key(&k) { json!(k) } else { Value::Null }})
        })
        .collect();
    // ranges of every position still in the hand at this node
    let codes: Vec<&str> = if key.is_empty() { vec![] } else { key.split('-').collect() };
    let mut folded = [false; 6];
    let mut prefix: Vec<&str> = Vec::new();
    for c in &codes {
        if let Some(pn) = charts.nodes.get(&prefix.join("-")) {
            if *c == "F" {
                folded[pn.player] = true;
            }
        }
        prefix.push(c);
    }
    let mut ranges = serde_json::Map::new();
    for p in 0..6 {
        if !folded[p] {
            ranges.insert(POS[p].to_string(), json!(charts.range_along(&key, p)));
        }
    }
    Ok(Json(json!({
        "key": key, "description": charts.describe(&key), "player": POS[n.player], "actions": actions,
        "freq": n.freq, "ev_bb": n.ev, "reach": n.reach, "children": children, "ranges": ranges, "level": n.level,
        "meta": {"source": charts.meta.source, "method": charts.meta.method},
    })))
}

#[derive(Deserialize)]
struct SolveBody {
    board: String,
    oop: String,
    ip: String,
    pot_bb: f64,
    stack_bb: f64,
}

async fn postflop_solve(State(app): State<App>, Json(b): Json<SolveBody>) -> R {
    let board = cards_from_str(&b.board).ok_or_else(|| bad("bad board"))?;
    if !(3..=5).contains(&board.len()) {
        return Err(bad("board must have 3-5 cards"));
    }
    let mut oop = Range::parse(&b.oop).map_err(bad)?;
    let mut ip = Range::parse(&b.ip).map_err(bad)?;
    oop.remove_dead(mask_of(&board));
    ip.remove_dead(mask_of(&board));
    let preset = match board.len() {
        3 => TreePreset::Flop,
        4 => TreePreset::Turn,
        _ => TreePreset::River,
    };
    let spec = SpotSpec {
        board,
        ranges: [oop, ip],
        pot: (b.pot_bb * 100.0).round() as i32,
        stack: (b.stack_bb * 100.0).round() as i32,
        preset,
        rake_pct: 0.05,
        rake_cap: 300.0,
        extra_lines: vec![],
        locks: vec![],
        bunching: None,
    };
    let spot = Spot::start(spec, app.res.cache.clone(), SolveTargets::for_preset(preset));
    let id = spot.key.clone();
    app.lab_solves.lock().unwrap().insert(id.clone(), spot);
    Ok(Json(json!({"solve_id": id})))
}

#[derive(Deserialize)]
struct LineQ {
    line: Option<String>,
}

async fn postflop_node(State(app): State<App>, Path(id): Path<String>, Query(q): Query<LineQ>) -> R {
    let spot = app.lab_solves.lock().unwrap().get(&id).cloned().ok_or((StatusCode::NOT_FOUND, "no such solve".into()))?;
    let line: Vec<LineAction> = match q.line {
        Some(s) if !s.is_empty() => serde_json::from_str(&s).map_err(bad)?,
        _ => vec![],
    };
    let v = tokio::task::spawn_blocking(move || {
        spot.wait_quality(5.0, Duration::from_secs(2));
        spot.node(&line, true).map(|v| (v, line))
    })
    .await
    .map_err(bad)?
    .map_err(bad)?;
    let (view, line) = v;
    let g = grids(&view);
    Ok(Json(json!({
        "solve_id": id, "status": view.status, "terminal": view.terminal || view.chance, "player": g.player,
        "pot_bb": g.pot_bb, "actions": g.actions, "action_values": view.actions.iter().map(|a| a.action).collect::<Vec<_>>(),
        "line": line, "strategy_grid": g.strategy_grid, "range_grid": {"OOP": g.range_grid[0], "IP": g.range_grid[1]},
        "range_freq": g.range_freq, "equity_grid": g.equity_grid, "ev_grid": g.ev_grid,
    })))
}

#[derive(Deserialize)]
struct SimBody {
    lineup: Vec<String>,
    hands: u64,
}

async fn sim_start(State(app): State<App>, Json(b): Json<SimBody>) -> R {
    if b.lineup.len() != 6 {
        return Err(bad("lineup must have 6 profiles"));
    }
    let id = crate::sim::start_sim(&app, b.lineup, b.hands.clamp(1000, 20_000_000));
    Ok(Json(json!({"id": id})))
}

async fn sim_status(State(app): State<App>, Path(id): Path<String>) -> R {
    let s = app.sims.lock().unwrap().get(&id).cloned().ok_or((StatusCode::NOT_FOUND, "no such sim".into()))?;
    let v = s.lock().unwrap().json(&app.res.profiles);
    Ok(Json(v))
}

async fn sim_stop(State(app): State<App>, Path(id): Path<String>) -> Json<Value> {
    if let Some(s) = app.sims.lock().unwrap().get(&id) {
        s.lock().unwrap().stop = true;
    }
    Json(json!({"ok": true}))
}

async fn presolve_status(State(app): State<App>) -> Json<Value> {
    let (n, bytes) = app.res.cache.stats();
    Json(json!({"cached_solves": n, "bytes": bytes, "dir": app.res.cache.dir}))
}

async fn ws(State(app): State<App>, upgrade: WebSocketUpgrade) -> Response {
    upgrade.on_upgrade(move |socket| ws_session(app, socket))
}

async fn ws_session(app: App, socket: WebSocket) {
    let (mut tx, mut rx) = socket.split();
    let mut sub = app.tx.subscribe();
    let first = app.table.lock().unwrap().as_ref().and_then(|t| t.latest.lock().unwrap().clone());
    if let Some(v) = first {
        let _ = tx.send(Message::Text(v.to_string().into())).await;
    }
    let app2 = app.clone();
    let reader = tokio::spawn(async move {
        while let Some(Ok(msg)) = rx.next().await {
            let Message::Text(t) = msg else { continue };
            let Ok(v) = serde_json::from_str::<Value>(&t) else { continue };
            let cmd = match v["type"].as_str() {
                Some("action") => serde_json::from_value::<Act>(v["action"].clone()).ok().map(TableCmd::Action),
                Some("next_hand") => Some(TableCmd::NextHand),
                Some("pause") => Some(TableCmd::Pause(v["paused"].as_bool().unwrap_or(false))),
                Some("set_delay") => v["ms"].as_u64().map(TableCmd::SetDelay),
                Some("set_coach") => v["mode"].as_str().map(|m| TableCmd::SetCoach(m.to_string())),
                _ => None,
            };
            if let Some(c) = cmd {
                if let Some(t) = app2.table.lock().unwrap().as_ref() {
                    let _ = t.cmd.send(c);
                }
            }
        }
    });
    loop {
        match sub.recv().await {
            Ok(m) => {
                if tx.send(Message::Text(m.into())).await.is_err() {
                    break;
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
            Err(_) => break,
        }
    }
    reader.abort();
}

pub fn _unused(_: Arc<()>) {}
