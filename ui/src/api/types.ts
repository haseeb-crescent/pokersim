// API contract between the Rust server (crates/server) and the UI.
// Chip amounts are integers with 1bb = 100 chips unless a field name ends in _bb.
// Cards are strings like "As", "Td", "2c". Hand classes are "AKs", "T9o", "77".
// Range grids are 169 numbers laid out row-major as the usual 13x13 chart with aces top-left:
// index = row * 13 + col, row/col 0 = A ... 12 = 2; pairs on the diagonal, suited above it
// (row < col), offsuit below it.

export type Street = 'preflop' | 'flop' | 'turn' | 'river'
export type Position = 'UTG' | 'HJ' | 'CO' | 'BTN' | 'SB' | 'BB'
export type CoachMode = 'decision' | 'hand' | 'off'

export type PlayerAction =
  | { type: 'Fold' }
  | { type: 'Check' }
  | { type: 'Call' }
  | { type: 'BetTo'; to: number }

export interface Legal {
  seat: number
  can_fold: boolean
  can_check: boolean
  call_amount: number
  call_is_all_in: boolean
  min_to: number | null
  max_to: number | null
  current_bet: number
  committed: number
  pot: number
}

export interface SeatView {
  seat: number
  name: string
  profile: string // profile id, 'hero' for the user
  position: Position
  stack: number
  committed: number
  folded: boolean
  all_in: boolean
  in_hand: boolean
  cards: string[] | null // hero always; bots only when shown at showdown
  last_action: string | null
  thinking: string | null // e.g. "solving flop… 4.1s"
}

export interface BetPreset {
  label: string // "33%", "75%", "3x", "All-in"
  to: number
  key: string // keyboard key
}

export interface TableState {
  running: boolean
  paused: boolean
  session_id: number | null
  hand_id: number | null
  hand_no: number
  hero_seat: number
  button: number
  seats: SeatView[]
  board: string[]
  street: Street | 'showdown' | 'idle'
  pot: number
  to_act: number | null
  legal: Legal | null // present only when the hero is to act
  presets: BetPreset[]
  log: string[]
  coach_mode: CoachMode
  bot_delay_ms: number
  hero_net: number // chips this session
  hands_played: number
  last_result: HandResultSummary | null
}

export interface HandResultSummary {
  hand_id: number
  winners: { seat: number; amount: number; hand: string | null }[]
  hero_net: number
  showdown: { seat: number; cards: string[]; description: string }[]
  grades: DecisionGrade[]
}

export interface ActionMix {
  label: string
  freq: number // 0..1 for the hero's exact hand
  ev_bb: number | null // EV relative to folding now; null for preflop charts
  range_freq: number | null // frequency over the whole range
}

export type GradeLabel = 'Best' | 'Correct' | 'Inaccuracy' | 'Mistake' | 'Blunder' | 'Unsolved' | 'Pending'

export interface FactSheet {
  street: Street
  position: 'IP' | 'OOP' | 'multiway'
  pot_bb: number
  to_call_bb: number
  spr: number
  pot_odds_needed: number | null // equity needed to call, 0..1
  equity_vs_range: number | null // hero equity vs villain range at this node, 0..1
  hand_description: string // "top pair, good kicker"
  board_texture: string // "K-high, two-tone, disconnected"
  range_advantage: { hero: number; villain: number } | null // average equity of each range
  nut_advantage: { hero: number; villain: number } | null // share of each range that is a strong hand
  blockers: string[]
  mdf: number | null // minimum defense frequency vs the bet faced, 0..1
  bluff_ratio: number | null // bluff share a polar bettor can have for this size, 0..1
  solver_mix: string // "Check 62%, Bet 33% 38%"
  villain_profile: string | null
  notes: string[]
}

export interface DecisionGrade {
  hand_id: number
  index: number // decision number within the hand
  street: Street
  spot: string // "BTN vs BB · single-raised pot · flop · IP vs check"
  spot_key: string // aggregation key for leaks, e.g. "SRP|IP|flop|vs_check"
  kind: 'solver' | 'preflop_chart' | 'unsolved'
  hero_hand: string
  board: string[]
  actions: ActionMix[]
  chosen: string
  chosen_freq: number | null
  best: string | null
  ev_loss_bb: number | null
  grade: GradeLabel
  precision_pct: number | null // solver exploitability in % of pot
  note: string | null // off-tree mapping, unsolved reasons, "charts carry no EVs"
  facts: FactSheet | null
  explanation: string | null
  exploit: ExploitView | null
  /** set when your action is not in the preflop chart at all */
  off_chart?: { action: string; chart: string; message: string; leaves_chart: boolean } | null
  /** why an unsolved decision has no grade */
  unsolved_reason?: string | null
  /** 'pending' while the coach is writing the explanation */
  explanation_status?: 'pending' | 'none'
  explanation_error?: string | null
}

export interface ExploitView {
  profile: string
  actions: ActionMix[]
  best: string
  note: string
}

export type ServerMsg =
  | { type: 'state'; state: TableState }
  | { type: 'grade'; grade: DecisionGrade }
  | { type: 'explanation'; hand_id: number; index: number; text: string | null; error?: string }
  | { type: 'hand_complete'; summary: HandResultSummary }
  | { type: 'error'; message: string }

export type ClientMsg =
  | { type: 'action'; action: PlayerAction }
  | { type: 'next_hand' }
  | { type: 'pause'; paused: boolean }
  | { type: 'set_delay'; ms: number }
  | { type: 'set_coach'; mode: CoachMode }

export interface BotProfile {
  id: string
  name: string
  description: string
  params: Record<string, number>
  expected: Record<string, number> // target stat signature, e.g. vpip, pfr, three_bet
}

export interface SessionConfig {
  lineup: string[] // 5 profile ids, seats after the hero
  coach_mode: CoachMode
  bot_delay_ms: number
  rebuy: 'reset' | 'topup' | 'off'
  rake_pct: number
  rake_cap_bb: number
}

// ---- lab ----

export interface EquityRequest {
  players: string[] // each a hand "AsKs" or a range "TT+,AKs"
  board: string // "Td9d6h" or ""
  dead: string
  samples?: number
}

export interface EquityResponse {
  equity: number[]
  win: number[]
  tie: number[]
  exact: boolean
  samples: number
  combos: number[]
  ms: number
}

export interface RangeParseResponse {
  ok: boolean
  error: string | null
  grid: number[] // 169
  combos: number
  percent: number
  text: string // normalised text
}

export interface PreflopNode {
  key: string
  description: string
  player: Position
  actions: { code: string; label: string; size_bb: number | null }[]
  freq: number[][] // per action, 169
  ev_bb: number[][] // per action, 169 (model estimate, may be empty)
  reach: number
  children: { code: string; key: string }[]
  ranges: Record<string, number[]> // range of every player still in, by position, at this node
}

export interface PostflopNodeView {
  solve_id: string
  status: { iterations: number; exploitability_pct: number; finalized: boolean; from_cache: boolean; elapsed_ms: number }
  terminal: boolean
  player: 'OOP' | 'IP'
  pot_bb: number
  actions: string[]
  line: string[]
  strategy_grid: number[][] // per action, 169 (range-weighted average within each class)
  range_grid: { OOP: number[]; IP: number[] } // reach weights per class (0..1)
  range_freq: number[] // per action over the acting range
  equity_grid: number[] | null // acting player's average equity per class
  ev_grid: number[][] | null // per action, 169, bb
}

export interface SimRequest {
  lineup: string[] // 6 profile ids
  hands: number
}

export interface SimStatus {
  id: string
  done: boolean
  hands: number
  target: number
  hands_per_sec: number
  results: {
    seat: number
    profile: string
    bb100: number
    ci95: number // +/- bb/100
    stats: Record<string, number>
  }[]
  series: { hand: number; cum_bb: number[] }[] // cumulative bb per seat, sampled
}
