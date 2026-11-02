//! A solve running (or finished) for one spot.
//!
//! The solve runs on its own thread, one DCFR iteration at a time. Between iterations readers
//! may lock the game, navigate to a node and read the current average strategy, so a bot can
//! act on a partially converged solution after a short budget while the solve keeps
//! improving. EVs need the finalized game. Finished flop solves are saved to the disk cache
//! with flop-only storage (a few hundred KB), and reloaded in that compact form.

use super::cache::SolveCache;
use super::spot::*;
use engine::card::*;
use postflop_solver as ps;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

/// Number of live (high-priority) solves running. Background solves pause between iterations
/// while any live solve is running, so what a bot is waiting for gets the whole machine.
static LIVE_SOLVES: AtomicUsize = AtomicUsize::new(0);

pub fn live_solves() -> usize {
    LIVE_SOLVES.load(Ordering::Relaxed)
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SpotStatus {
    pub iterations: u32,
    /// exploitability as % of the starting pot (lower is better), f32::MAX before measured
    pub exploitability_pct: f32,
    pub finalized: bool,
    pub from_cache: bool,
    pub elapsed_ms: u64,
    pub error: Option<String>,
    pub memory_mb: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct SolveTargets {
    /// stop when exploitability reaches this % of the pot
    pub target_pct: f32,
    /// stop after this long regardless (time spent paused for live solves does not count)
    pub max_secs: f64,
    /// background solves yield to live ones
    pub background: bool,
}

impl SolveTargets {
    pub fn for_preset(p: TreePreset) -> SolveTargets {
        match p {
            TreePreset::Flop => SolveTargets { target_pct: 0.75, max_secs: 45.0, background: false },
            TreePreset::Turn => SolveTargets { target_pct: 0.4, max_secs: 12.0, background: false },
            TreePreset::River => SolveTargets { target_pct: 0.25, max_secs: 4.0, background: false },
            TreePreset::Calibration => SolveTargets { target_pct: 2.0, max_secs: 30.0, background: false },
        }
    }

    pub fn background(mut self) -> Self {
        self.background = true;
        self
    }
}

struct Inner {
    game: Option<ps::PostFlopGame>,
    status: SpotStatus,
    stop: bool,
}

pub struct Spot {
    pub key: String,
    /// the spec in real suits
    pub spec: SpotSpec,
    /// real suit -> canonical suit
    pub perm: SuitPerm,
    inner: Mutex<Inner>,
    cv: Condvar,
    background: AtomicBool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlayerValues {
    pub hands: Vec<(Card, Card)>,
    pub weights: Vec<f32>,
    pub ev: Vec<f32>,
    pub equity: Vec<f32>,
}

/// A solver action as shown to the rest of the app.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeAction {
    pub action: LineAction,
    pub label: String,
}

/// Everything known about one node, in real card space.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeView {
    pub terminal: bool,
    pub chance: bool,
    /// acting player: 0 = OOP, 1 = IP
    pub player: usize,
    pub actions: Vec<NodeAction>,
    /// private hands of [OOP, IP] (real cards)
    pub hands: [Vec<(Card, Card)>; 2],
    /// reach weights of [OOP, IP] at this node, aligned with `hands`
    pub weights: [Vec<f32>; 2],
    /// strategy of the acting player: actions x hands
    pub strategy: Vec<f32>,
    /// EV (chips, relative to folding now) per action per hand of the acting player
    pub ev: Option<Vec<f32>>,
    /// equity of each hand of the acting player against the opponent's range here
    pub equity: Option<Vec<f32>>,
    /// equity of each opponent hand against the acting player's range
    pub opp_equity: Option<Vec<f32>>,
    /// actions in the requested line that had to be mapped to the nearest tree action
    pub mapped: Vec<(LineAction, LineAction)>,
    /// chips committed this street by [OOP, IP]
    pub committed: [i32; 2],
    pub pot: i32,
    pub status: SpotStatus,
}

impl NodeView {
    pub fn hand_index(&self, player: usize, hand: (Card, Card)) -> Option<usize> {
        let (a, b) = hand;
        self.hands[player].iter().position(|&(x, y)| (x == a && y == b) || (x == b && y == a))
    }

    /// Strategy (one probability per action) for a specific hand of the acting player.
    pub fn hand_strategy(&self, hand: (Card, Card)) -> Option<Vec<f32>> {
        let i = self.hand_index(self.player, hand)?;
        let n = self.hands[self.player].len();
        Some((0..self.actions.len()).map(|a| self.strategy[a * n + i]).collect())
    }

    pub fn hand_ev(&self, hand: (Card, Card)) -> Option<Vec<f32>> {
        let ev = self.ev.as_ref()?;
        let i = self.hand_index(self.player, hand)?;
        let n = self.hands[self.player].len();
        Some((0..self.actions.len()).map(|a| ev[a * n + i]).collect())
    }

    /// Aggregate frequency of each action over the acting player's range.
    pub fn range_frequencies(&self) -> Vec<f32> {
        let p = self.player;
        let n = self.hands[p].len();
        let w = &self.weights[p];
        let total: f32 = w.iter().sum();
        (0..self.actions.len())
            .map(|a| if total > 0.0 { (0..n).map(|i| w[i] * self.strategy[a * n + i]).sum::<f32>() / total } else { 0.0 })
            .collect()
    }
}

impl Spot {
    /// Starts (or loads from cache) the solve for `spec`.
    pub fn start(spec: SpotSpec, cache: Arc<SolveCache>, targets: SolveTargets) -> Arc<Spot> {
        let perm = spec.canonical_perm();
        let canon = spec.permuted(perm);
        let key = canon.key();
        let spot = Arc::new(Spot {
            key: key.clone(),
            spec,
            perm,
            inner: Mutex::new(Inner { game: None, status: SpotStatus { exploitability_pct: f32::MAX, ..Default::default() }, stop: false }),
            cv: Condvar::new(),
            background: AtomicBool::new(targets.background),
        });
        if let Some((game, status)) = cache.load(&key) {
            let mut inner = spot.inner.lock().unwrap();
            inner.game = Some(game);
            inner.status = SpotStatus { from_cache: true, finalized: true, ..status };
            drop(inner);
            return spot;
        }
        let s2 = spot.clone();
        std::thread::Builder::new()
            .name(format!("solve-{}", &key[..key.len().min(24)]))
            .spawn(move || s2.run(canon, cache, targets))
            .expect("spawn solver thread");
        spot
    }

    fn run(self: Arc<Self>, canon: SpotSpec, cache: Arc<SolveCache>, targets: SolveTargets) {
        let t0 = Instant::now();
        let mut game = match canon.build_game() {
            Ok(g) => g,
            Err(e) => {
                let mut inner = self.inner.lock().unwrap();
                inner.status.error = Some(e);
                inner.status.finalized = true;
                self.cv.notify_all();
                return;
            }
        };
        if let Some(b) = &canon.bunching {
            if let Err(e) = game.set_bunching_effect(&b.0) {
                eprintln!("bunching not applied: {e}");
            }
        }
        let (_, mem_compressed) = game.memory_usage();
        game.allocate_memory(true);
        if let Err(e) = apply_locks(&mut game, &canon.locks) {
            let mut inner = self.inner.lock().unwrap();
            inner.status.error = Some(e);
            inner.status.finalized = true;
            self.cv.notify_all();
            return;
        }
        {
            let mut inner = self.inner.lock().unwrap();
            inner.status.memory_mb = mem_compressed as f64 / 1e6;
            inner.game = Some(game);
        }
        let pot = canon.pot as f32;
        let mut it = 0u32;
        let mut counted = false;
        let mut paused = Duration::ZERO;
        loop {
            let bg = self.background.load(Ordering::Relaxed);
            if !bg && !counted {
                LIVE_SOLVES.fetch_add(1, Ordering::Relaxed);
                counted = true;
            } else if bg && counted {
                LIVE_SOLVES.fetch_sub(1, Ordering::Relaxed);
                counted = false;
            }
            if bg {
                let p0 = Instant::now();
                while LIVE_SOLVES.load(Ordering::Relaxed) > 0 && !self.inner.lock().unwrap().stop {
                    std::thread::sleep(Duration::from_millis(40));
                }
                paused += p0.elapsed();
            }
            let mut inner = self.inner.lock().unwrap();
            if inner.stop {
                break;
            }
            let game = inner.game.as_mut().unwrap();
            ps::solve_step(game, it);
            it += 1;
            inner.status.iterations = it;
            inner.status.elapsed_ms = t0.elapsed().as_millis() as u64;
            if it % 8 == 0 || it == 3 {
                let e = 100.0 * ps::compute_exploitability(inner.game.as_ref().unwrap()) / pot;
                inner.status.exploitability_pct = e;
            }
            let done = inner.status.exploitability_pct <= targets.target_pct
                || (t0.elapsed() - paused).as_secs_f64() >= targets.max_secs
                || it >= 3000;
            drop(inner);
            self.cv.notify_all();
            if done {
                break;
            }
        }
        if counted {
            LIVE_SOLVES.fetch_sub(1, Ordering::Relaxed);
        }
        let mut inner = self.inner.lock().unwrap();
        {
            let game = inner.game.as_mut().unwrap();
            let e = 100.0 * ps::compute_exploitability(game) / pot;
            ps::finalize(game);
            inner.status.exploitability_pct = e;
        }
        inner.status.finalized = true;
        inner.status.elapsed_ms = t0.elapsed().as_millis() as u64;
        // persist; flop solves keep only the flop street. Locked solves are not cached.
        let status = inner.status.clone();
        let street = canon.street();
        let cacheable = canon.locks.is_empty() && canon.bunching.is_none();
        if let (Some(game), true) = (inner.game.as_mut(), cacheable) {
            let _ = cache.save(&self.key, game, street, &status, &canon);
        }
        // replace a big flop game by its compact flop-only copy
        if street == ps::BoardState::Flop && cacheable {
            if let Some((g, _)) = cache.load(&self.key) {
                inner.game = Some(g);
            }
        }
        drop(inner);
        self.cv.notify_all();
    }

    pub fn status(&self) -> SpotStatus {
        self.inner.lock().unwrap().status.clone()
    }

    /// Blocks until the solve is finalized, or exploitability <= `pct`, or `max_wait` passes.
    pub fn wait_quality(&self, pct: f32, max_wait: Duration) -> SpotStatus {
        let deadline = Instant::now() + max_wait;
        let mut inner = self.inner.lock().unwrap();
        loop {
            let s = &inner.status;
            if s.finalized || s.exploitability_pct <= pct || s.error.is_some() {
                return s.clone();
            }
            let now = Instant::now();
            if now >= deadline {
                // must have at least one iteration to read a strategy
                if s.iterations > 0 {
                    return s.clone();
                }
            }
            let wait = if now >= deadline { Duration::from_millis(50) } else { deadline - now };
            inner = self.cv.wait_timeout(inner, wait).unwrap().0;
        }
    }

    pub fn wait_final(&self, max_wait: Duration) -> SpotStatus {
        self.wait_quality(-1.0, max_wait)
    }

    /// Lowers (or raises) the priority of a running solve.
    pub fn set_background(&self, bg: bool) {
        self.background.store(bg, Ordering::Relaxed);
    }

    /// Asks a running solve to stop and finalize at the next iteration boundary.
    pub fn stop(&self) {
        self.inner.lock().unwrap().stop = true;
    }

    /// Navigates to the node reached by `line` (real chip amounts, chance cards as board
    /// cards are not part of a line: every spot covers one street's decisions) and returns
    /// everything known about it.
    pub fn node(&self, line: &[LineAction], with_equity: bool) -> Result<NodeView, String> {
        let mut inner = self.inner.lock().unwrap();
        let status = inner.status.clone();
        if let Some(e) = &status.error {
            return Err(e.clone());
        }
        let game = inner.game.as_mut().ok_or("solve not started")?;
        game.back_to_root();
        let mut mapped = Vec::new();
        for &a in line {
            if game.is_terminal_node() || game.is_chance_node() {
                return Err("line goes past the end of the street".into());
            }
            let actions = game.available_actions();
            let (idx, used) = match_action(&actions, a, game.tree_config().starting_pot + 2 * game.total_bet_amount()[0].max(game.total_bet_amount()[1]))
                .ok_or_else(|| format!("no matching action for {a:?} among {actions:?}"))?;
            if used != a {
                mapped.push((a, used));
            }
            game.play(idx);
        }
        let inv = self.perm.inverse();
        let real = |h: &(Card, Card)| (inv.card(h.0), inv.card(h.1));
        let hands = [
            game.private_cards(0).iter().map(real).collect::<Vec<_>>(),
            game.private_cards(1).iter().map(real).collect::<Vec<_>>(),
        ];
        let weights = [game.weights(0).to_vec(), game.weights(1).to_vec()];
        let bets = game.total_bet_amount();
        let pot = game.tree_config().starting_pot + bets[0] + bets[1];
        let terminal = game.is_terminal_node();
        let chance = game.is_chance_node();
        if terminal || chance {
            return Ok(NodeView {
                terminal,
                chance,
                player: 0,
                actions: vec![],
                hands,
                weights,
                strategy: vec![],
                ev: None,
                equity: None,
                opp_equity: None,
                mapped,
                committed: bets,
                pot,
                status,
            });
        }
        let player = game.current_player();
        let actions: Vec<NodeAction> = game
            .available_actions()
            .into_iter()
            .filter_map(LineAction::from_ps)
            .map(|a| NodeAction { action: a, label: a.label() })
            .collect();
        let strategy = game.strategy();
        let (ev, equity, opp_equity) = if status.finalized {
            game.cache_normalized_weights();
            let ev = game.expected_values_detail(player);
            let (eq, oeq) = if with_equity { (Some(game.equity(player)), Some(game.equity(player ^ 1))) } else { (None, None) };
            (Some(ev), eq, oeq)
        } else if with_equity {
            game.cache_normalized_weights();
            (None, Some(game.equity(player)), Some(game.equity(player ^ 1)))
        } else {
            (None, None, None)
        };
        Ok(NodeView { terminal, chance, player, actions, hands, weights, strategy, ev, equity, opp_equity, mapped, committed: bets, pot, status })
    }

    /// Overall EV (chips of pot share) and equity of every hand of both players at the node
    /// after `line`. Requires a finalized solve.
    pub fn values(&self, line: &[LineAction]) -> Result<[PlayerValues; 2], String> {
        let mut inner = self.inner.lock().unwrap();
        if !inner.status.finalized {
            return Err("solve not finalized".into());
        }
        let game = inner.game.as_mut().ok_or("no game")?;
        game.back_to_root();
        for &a in line {
            let actions = game.available_actions();
            let (idx, _) = match_action(&actions, a, game.tree_config().starting_pot).ok_or("no matching action")?;
            game.play(idx);
        }
        game.cache_normalized_weights();
        let inv = self.perm.inverse();
        let mut out: Vec<PlayerValues> = Vec::new();
        for p in 0..2 {
            out.push(PlayerValues {
                hands: game.private_cards(p).iter().map(|h| (inv.card(h.0), inv.card(h.1))).collect(),
                weights: game.weights(p).to_vec(),
                ev: game.expected_values(p),
                equity: game.equity(p),
            });
        }
        let b = out.pop().unwrap();
        let a = out.pop().unwrap();
        Ok([a, b])
    }

    /// Reach-weighted ranges of [OOP, IP] after `line`, as engine ranges in real suits. Used to
    /// start the next street's re-solve.
    pub fn ranges_after(&self, line: &[LineAction]) -> Result<[engine::Range; 2], String> {
        let v = self.node(line, false)?;
        let mut out = [engine::Range::empty(), engine::Range::empty()];
        for p in 0..2 {
            for (i, &(a, b)) in v.hands[p].iter().enumerate() {
                out[p].set(a, b, v.weights[p][i]);
            }
        }
        Ok(out)
    }

    /// Locks a strategy at the node reached by `line`. Must be called before solving, so this
    /// is only used on games built for node-locked solves (see `locked.rs`).
    pub fn with_game<R>(&self, f: impl FnOnce(&mut ps::PostFlopGame) -> R) -> Option<R> {
        let mut inner = self.inner.lock().unwrap();
        inner.game.as_mut().map(f)
    }
}

/// Applies node locks (canonical cards) to a game whose memory is allocated.
fn apply_locks(game: &mut ps::PostFlopGame, locks: &[LockSpec]) -> Result<(), String> {
    for lock in locks {
        game.back_to_root();
        for &a in &lock.line {
            let actions = game.available_actions();
            let pot = game.tree_config().starting_pot;
            let (idx, _) = match_action(&actions, a, pot).ok_or_else(|| format!("lock line {a:?} not in tree"))?;
            game.play(idx);
        }
        if game.is_terminal_node() || game.is_chance_node() {
            return Err("lock line ends at a terminal or chance node".into());
        }
        let player = game.current_player();
        let na = game.available_actions().len();
        let hands = game.private_cards(player).to_vec();
        let n = hands.len();
        let k = lock.hands.len();
        if k == 0 || lock.strategy.len() != na * k {
            return Err(format!("lock has {} entries for {na} actions x {k} hands", lock.strategy.len()));
        }
        let mut full = vec![0.0f32; na * n];
        for (li, &(a, b)) in lock.hands.iter().enumerate() {
            let (lo, hi) = if a < b { (a, b) } else { (b, a) };
            if let Some(j) = hands.iter().position(|&(x, y)| x == lo && y == hi) {
                let mut sum = 0.0;
                for i in 0..na {
                    sum += lock.strategy[i * k + li].max(0.0);
                }
                for i in 0..na {
                    let v = lock.strategy[i * k + li].max(0.0);
                    // keep every locked hand strictly positive somewhere so it stays locked
                    full[i * n + j] = if sum > 0.0 { v / sum + 1e-6 } else { 1.0 / na as f32 };
                }
            }
        }
        game.lock_current_strategy(&full);
    }
    game.back_to_root();
    Ok(())
}

/// Finds the tree action matching a real action: exact for fold/check/call, nearest amount
/// (in pot fractions) for bets and raises; an all-in matches the tree's all-in.
pub fn match_action(actions: &[ps::Action], a: LineAction, pot_now: i32) -> Option<(usize, LineAction)> {
    let lines: Vec<LineAction> = actions.iter().filter_map(|&x| LineAction::from_ps(x)).collect();
    if let Some(i) = lines.iter().position(|&x| x == a) {
        return Some((i, a));
    }
    match a {
        LineAction::Fold | LineAction::Check | LineAction::Call => None,
        _ => {
            let want = a.amount().unwrap();
            if matches!(a, LineAction::AllIn(_)) {
                if let Some(i) = lines.iter().position(|x| matches!(x, LineAction::AllIn(_))) {
                    return Some((i, lines[i]));
                }
            }
            let pot = pot_now.max(1) as f64;
            let mut best: Option<(usize, f64)> = None;
            for (i, x) in lines.iter().enumerate() {
                if let Some(amt) = x.amount() {
                    let d = ((1.0 + want as f64 / pot).ln() - (1.0 + amt as f64 / pot).ln()).abs();
                    if best.map_or(true, |(_, bd)| d < bd) {
                        best = Some((i, d));
                    }
                }
            }
            best.map(|(i, _)| (i, lines[i]))
        }
    }
}
