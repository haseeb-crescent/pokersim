// Client-side poker helpers shared by the Lab and Learn pages: cards, combos, a range-text
// parser (PioSolver syntax, same as the server), a 5-7 card evaluator and a made-hand
// classifier. The server stays the authority for equities; these are for instant, offline
// counting and teaching widgets.
//
// Card index = rank * 4 + suit, rank 0 = deuce ... 12 = ace, suit order s h d c.

import { classAt } from '../../../api/client'

export const RANK_CHARS = '23456789TJQKA'
export const SUIT_CHARS = 'shdc'
export const SUIT_NAMES = ['spades', 'hearts', 'diamonds', 'clubs']

export const rankOf = (c: number) => c >> 2
export const suitOf = (c: number) => c & 3
export const cardStr = (c: number) => RANK_CHARS[c >> 2] + SUIT_CHARS[c & 3]
export const cardsStr = (cs: number[]) => cs.map(cardStr).join('')

export function parseCard(s: string): number | null {
  if (s.length !== 2) return null
  const r = RANK_CHARS.indexOf(s[0].toUpperCase())
  const u = SUIT_CHARS.indexOf(s[1].toLowerCase())
  return r < 0 || u < 0 ? null : r * 4 + u
}

/** Parses "AsKd", "As Kd" or "As,Kd". Duplicates are an error. */
export function parseCards(text: string): { cards: number[]; error: string | null } {
  const t = text.replace(/[\s,]+/g, '')
  const cards: number[] = []
  if (t.length % 2) return { cards, error: `"${text.trim()}" is not a list of cards like AsKd` }
  for (let i = 0; i < t.length; i += 2) {
    const c = parseCard(t.slice(i, i + 2))
    if (c === null) return { cards: [], error: `"${t.slice(i, i + 2)}" is not a card (use e.g. As, Td, 2c)` }
    if (cards.includes(c)) return { cards: [], error: `${cardStr(c)} appears twice` }
    cards.push(c)
  }
  return { cards, error: null }
}

export const FULL_DECK: number[] = Array.from({ length: 52 }, (_, i) => i)

// ---- combos and grid classes ----

/** All 1326 two-card combos as [high card index, low card index]. */
export const COMBOS: [number, number][] = []
for (let a = 51; a >= 0; a--) for (let b = a - 1; b >= 0; b--) COMBOS.push([a, b])

const comboLookup = new Int16Array(52 * 52).fill(-1)
COMBOS.forEach(([a, b], i) => {
  comboLookup[a * 52 + b] = i
  comboLookup[b * 52 + a] = i
})
export const comboIndex = (a: number, b: number) => comboLookup[a * 52 + b]

/** Grid index (0..168, aces top-left, suited above the diagonal) of a two-card hand. */
export function gridIndexOf(a: number, b: number): number {
  const ra = rankOf(a)
  const rb = rankOf(b)
  const hi = 12 - Math.max(ra, rb)
  const lo = 12 - Math.min(ra, rb)
  if (ra === rb) return hi * 13 + hi
  return suitOf(a) === suitOf(b) ? hi * 13 + lo : lo * 13 + hi
}

/** Grid index of every combo, precomputed. */
export const COMBO_CLASS: number[] = COMBOS.map(([a, b]) => gridIndexOf(a, b))

/** Combos (indices into COMBOS) of each of the 169 classes. */
export const CLASS_COMBOS: number[][] = Array.from({ length: 169 }, () => [])
COMBO_CLASS.forEach((g, i) => CLASS_COMBOS[g].push(i))

export const blocked = (combo: number, dead: Set<number>) => dead.has(COMBOS[combo][0]) || dead.has(COMBOS[combo][1])

/** Live combos of a class given dead cards. */
export function liveCombos(gridIndex: number, dead: Set<number>): number[] {
  return CLASS_COMBOS[gridIndex].filter((i) => !blocked(i, dead))
}

export function comboStr(i: number): string {
  const [a, b] = COMBOS[i]
  return cardStr(a) + cardStr(b)
}

/** Name ("AKs") of a grid index. */
export const className = (g: number) => classAt(Math.floor(g / 13), g % 13)

/** Grid index of a class name such as "AKs", "T9o", "77"; -1 if invalid. */
export function classIndexOf(name: string): number {
  const m = /^([2-9TJQKA])([2-9TJQKA])([so]?)$/i.exec(name.trim())
  if (!m) return -1
  const r1 = RANK_CHARS.indexOf(m[1].toUpperCase())
  const r2 = RANK_CHARS.indexOf(m[2].toUpperCase())
  const s = m[3].toLowerCase()
  const hi = 12 - Math.max(r1, r2)
  const lo = 12 - Math.min(r1, r2)
  if (r1 === r2) return s ? -1 : hi * 13 + hi
  if (s === 's') return hi * 13 + lo
  if (s === 'o') return lo * 13 + hi
  return -1 // "AK" (both) is handled by the range parser
}

// ---- range text ----

export interface ParsedRange {
  /** weight 0..1 for each of the 1326 combos */
  weights: Float64Array
  error: string | null
}

function rankIdx(ch: string) {
  return RANK_CHARS.indexOf(ch.toUpperCase())
}

/** Combos for one token without weight; throws on bad syntax. */
function tokenCombos(tok: string): number[] {
  const t = tok.trim()
  // exact combo "AsKs"
  if (/^[2-9TJQKA][shdc][2-9TJQKA][shdc]$/i.test(t)) {
    const a = parseCard(t.slice(0, 2))!
    const b = parseCard(t.slice(2, 4))!
    if (a === b) throw new Error(`"${t}" uses the same card twice`)
    return [comboIndex(a, b)]
  }
  const dash = t.split('-')
  if (dash.length === 2) {
    const A = parseClassToken(dash[0])
    const B = parseClassToken(dash[1])
    if (!A || !B || A.plus || B.plus) throw new Error(`can't read "${t}"`)
    if (A.pair && B.pair) {
      const [lo, hi] = [Math.min(A.r1, B.r1), Math.max(A.r1, B.r1)]
      return rankSpan(lo, hi).flatMap((r) => pairCombos(r))
    }
    if (!A.pair && !B.pair && A.r1 === B.r1 && A.suit === B.suit) {
      const [lo, hi] = [Math.min(A.r2, B.r2), Math.max(A.r2, B.r2)]
      return rankSpan(lo, hi).flatMap((k) => nonPairCombos(A.r1, k, A.suit))
    }
    throw new Error(`"${t}": a span needs two pairs (QQ-88) or the same top card and suitedness (AQs-A9s)`)
  }
  const c = parseClassToken(t)
  if (!c) throw new Error(`can't read "${t}"`)
  if (c.pair) return (c.plus ? rankSpan(c.r1, 12) : [c.r1]).flatMap((r) => pairCombos(r))
  const kickers = c.plus ? rankSpan(c.r2, c.r1 - 1) : [c.r2]
  return kickers.flatMap((k) => nonPairCombos(c.r1, k, c.suit))
}

function rankSpan(lo: number, hi: number) {
  const out: number[] = []
  for (let r = lo; r <= hi; r++) out.push(r)
  return out
}

function parseClassToken(t: string): { pair: boolean; r1: number; r2: number; suit: '' | 's' | 'o'; plus: boolean } | null {
  const m = /^([2-9TJQKA])([2-9TJQKA])([so]?)(\+?)$/i.exec(t.trim())
  if (!m) return null
  let r1 = rankIdx(m[1])
  let r2 = rankIdx(m[2])
  if (r2 > r1) [r1, r2] = [r2, r1]
  const suit = m[3].toLowerCase() as '' | 's' | 'o'
  if (r1 === r2 && suit) return null
  return { pair: r1 === r2, r1, r2, suit, plus: m[4] === '+' }
}

function pairCombos(r: number): number[] {
  const out: number[] = []
  for (let s1 = 0; s1 < 4; s1++) for (let s2 = s1 + 1; s2 < 4; s2++) out.push(comboIndex(r * 4 + s1, r * 4 + s2))
  return out
}

function nonPairCombos(hi: number, lo: number, suit: '' | 's' | 'o'): number[] {
  const out: number[] = []
  for (let s1 = 0; s1 < 4; s1++)
    for (let s2 = 0; s2 < 4; s2++) {
      if (suit === 's' && s1 !== s2) continue
      if (suit === 'o' && s1 === s2) continue
      out.push(comboIndex(hi * 4 + s1, lo * 4 + s2))
    }
  return out
}

/** Parses PioSolver-style range text: "TT+, AKs, A5s-A2s, KQo:0.5, AsKs". Later tokens override. */
export function parseRange(text: string): ParsedRange {
  const weights = new Float64Array(1326)
  const tokens = text.split(',').map((s) => s.trim()).filter(Boolean)
  if (!tokens.length) return { weights, error: 'empty range' }
  try {
    for (const raw of tokens) {
      const [body, w, extra] = raw.split(':')
      if (extra !== undefined) throw new Error(`"${raw}" has two weights`)
      let weight = 1
      if (w !== undefined) {
        weight = Number(w)
        if (!Number.isFinite(weight) || weight < 0 || weight > 1) throw new Error(`weight in "${raw}" must be between 0 and 1`)
      }
      for (const i of tokenCombos(body.replace(/\s+/g, ''))) weights[i] = weight
    }
  } catch (e) {
    return { weights, error: (e as Error).message }
  }
  return { weights, error: null }
}

/** True if text looks like one exact hand ("AsKs", "As Ks"). */
export function isExactHand(text: string): boolean {
  return /^\s*[2-9TJQKA][shdc]\s*[2-9TJQKA][shdc]\s*$/i.test(text)
}

/** Average weight per class (169), all combos counted. */
export function rangeToGrid(weights: ArrayLike<number>): number[] {
  return CLASS_COMBOS.map((cs) => cs.reduce((s, i) => s + weights[i], 0) / cs.length)
}

export function gridToWeights(grid: ArrayLike<number>): Float64Array {
  const w = new Float64Array(1326)
  for (let i = 0; i < 1326; i++) w[i] = grid[COMBO_CLASS[i]]
  return w
}

/** Weighted combo count, skipping combos that touch a dead card. */
export function countCombos(weights: ArrayLike<number>, dead: Set<number> = new Set()): number {
  let n = 0
  for (let i = 0; i < 1326; i++) if (weights[i] > 0 && !blocked(i, dead)) n += weights[i]
  return n
}

/** Compresses a 169-class grid (weights 0..1) into range text, e.g. "TT+, AQs+, KQo:0.5". */
export function gridToText(grid: ArrayLike<number>, digits = 2): string {
  const round = (x: number) => Math.round(x * 10 ** digits) / 10 ** digits
  const byWeight = new Map<number, number[]>()
  for (let g = 0; g < 169; g++) {
    const w = round(Math.max(0, Math.min(1, grid[g])))
    if (w <= 0) continue
    if (!byWeight.has(w)) byWeight.set(w, [])
    byWeight.get(w)!.push(g)
  }
  const parts: string[] = []
  for (const w of [...byWeight.keys()].sort((a, b) => b - a)) {
    const set = new Set(byWeight.get(w)!)
    const suffix = w < 1 ? `:${w}` : ''
    const R = (r: number) => RANK_CHARS[r]
    // pairs, rank 12 (A) down to 0
    const pairs = rankSpan(0, 12).reverse().filter((r) => set.has((12 - r) * 14))
    for (const run of runs(pairs)) {
      const [top, bottom] = [run[0], run[run.length - 1]]
      if (top === 12 && run.length > 1) parts.push(`${R(bottom)}${R(bottom)}+${suffix}`)
      else if (run.length > 1) parts.push(`${R(top)}${R(top)}-${R(bottom)}${R(bottom)}${suffix}`)
      else parts.push(`${R(top)}${R(top)}${suffix}`)
    }
    for (const s of ['s', 'o'] as const) {
      for (let hi = 12; hi >= 1; hi--) {
        const ks = rankSpan(0, hi - 1).reverse().filter((lo) => set.has(s === 's' ? (12 - hi) * 13 + (12 - lo) : (12 - lo) * 13 + (12 - hi)))
        for (const run of runs(ks)) {
          const [top, bottom] = [run[0], run[run.length - 1]]
          if (top === hi - 1 && run.length > 1) parts.push(`${R(hi)}${R(bottom)}${s}+${suffix}`)
          else if (run.length > 1) parts.push(`${R(hi)}${R(top)}${s}-${R(hi)}${R(bottom)}${s}${suffix}`)
          else parts.push(`${R(hi)}${R(top)}${s}${suffix}`)
        }
      }
    }
  }
  return parts.join(', ')
}

/** Splits a descending list of ranks into runs of consecutive ranks. */
function runs(desc: number[]): number[][] {
  const out: number[][] = []
  for (const r of desc) {
    const last = out[out.length - 1]
    if (last && last[last.length - 1] === r + 1) last.push(r)
    else out.push([r])
  }
  return out
}

// ---- evaluator ----

export const CAT_NAMES = ['High card', 'Pair', 'Two pair', 'Three of a kind', 'Straight', 'Flush', 'Full house', 'Four of a kind', 'Straight flush']

function popcount(x: number) {
  let n = 0
  while (x) {
    x &= x - 1
    n++
  }
  return n
}

/** High rank of the best straight in a 13-bit rank mask, or -1. The wheel (A-5) has high 3. */
function straightHigh(mask: number): number {
  for (let h = 12; h >= 4; h--) {
    const w = 0x1f << (h - 4)
    if ((mask & w) === w) return h
  }
  return (mask & 0x100f) === 0x100f ? 3 : -1
}

const score = (cat: number, ranks: number[]) => {
  let v = cat
  for (let i = 0; i < 5; i++) v = v * 13 + (ranks[i] ?? 0)
  return v
}
const CAT_BASE = 13 ** 5
export const categoryOf = (s: number) => Math.floor(s / CAT_BASE)

/** Strength of the best 5-card hand from 1-7 cards; higher is better. Category = categoryOf(score). */
export function evaluate(cards: number[]): number {
  const cnt = new Array(13).fill(0)
  const sm = [0, 0, 0, 0]
  let rm = 0
  for (const c of cards) {
    const r = c >> 2
    cnt[r]++
    sm[c & 3] |= 1 << r
    rm |= 1 << r
  }
  let fs = -1
  for (let s = 0; s < 4; s++) if (popcount(sm[s]) >= 5) fs = s
  if (fs >= 0) {
    const h = straightHigh(sm[fs])
    if (h >= 0) return score(8, [h])
  }
  const q: number[] = []
  const t: number[] = []
  const p: number[] = []
  const k: number[] = []
  for (let r = 12; r >= 0; r--) {
    if (cnt[r] === 4) q.push(r)
    else if (cnt[r] === 3) t.push(r)
    else if (cnt[r] === 2) p.push(r)
    else if (cnt[r] === 1) k.push(r)
  }
  if (q.length) return score(7, [q[0], [...t, ...p, ...k].sort((a, b) => b - a)[0]])
  if (t.length && (t.length > 1 || p.length)) return score(6, [t[0], Math.max(t[1] ?? -1, p[0] ?? -1)])
  if (fs >= 0) {
    const a: number[] = []
    for (let r = 12; r >= 0 && a.length < 5; r--) if ((sm[fs] >> r) & 1) a.push(r)
    return score(5, a)
  }
  const st = straightHigh(rm)
  if (st >= 0) return score(4, [st])
  if (t.length) return score(3, [t[0], ...[...p, ...k].sort((a, b) => b - a)])
  if (p.length >= 2) return score(2, [p[0], p[1], [...p.slice(2), ...k].sort((a, b) => b - a)[0]])
  if (p.length) return score(1, [p[0], ...k])
  return score(0, k)
}

// ---- made-hand classification on a board ----

export const BUCKETS = ['Straight or better', 'Set / trips', 'Two pair', 'Overpair', 'Top pair', 'Weaker pair', 'Strong draw', 'Nothing'] as const
export type Bucket = (typeof BUCKETS)[number]
/** Buckets that count as "strong" (nut advantage share). */
export const STRONG_BUCKETS: Bucket[] = ['Straight or better', 'Set / trips', 'Two pair']

/** Number of ranks that would complete a straight for this rank mask (open-ender = 2, gutshot = 1). */
function straightOutRanks(mask: number): Set<number> {
  const out = new Set<number>()
  if (straightHigh(mask) >= 0) return out
  for (let r = 0; r < 13; r++) if (!((mask >> r) & 1) && straightHigh(mask | (1 << r)) >= 0) out.add(r)
  return out
}

/**
 * Classifies a two-card hand on a 3-5 card board into a coarse bucket, using only pairs the
 * hole cards take part in (so A-K on K-7-7 is top pair, not two pair).
 */
export function classify(hole: [number, number], board: number[]): Bucket {
  const all = [...hole, ...board]
  const s = evaluate(all)
  const cat = categoryOf(s)
  const boardScore = evaluate(board)
  const boardRanks = board.map(rankOf)
  const distinct = [...new Set(boardRanks)].sort((a, b) => b - a)
  const top = distinct[0]
  const [h1, h2] = hole.map(rankOf)
  const playsBoard = board.length === 5 && s === boardScore
  if (cat >= 4 && !playsBoard) {
    // a straight or flush only counts if the hole cards help make it
    if (board.length < 5 || s > boardScore) return 'Straight or better'
  }
  const onBoard = (r: number) => boardRanks.filter((x) => x === r).length
  if (h1 === h2) {
    if (onBoard(h1) >= 1) return 'Set / trips'
    if (h1 > top) return 'Overpair'
    return 'Weaker pair'
  }
  const hits = [h1, h2].filter((r) => onBoard(r) > 0)
  if (hits.some((r) => onBoard(r) >= 2)) return 'Set / trips'
  if (hits.length === 2) return 'Two pair'
  if (hits.length === 1) return hits[0] === top ? 'Top pair' : 'Weaker pair'
  if (board.length < 5) {
    // draws: four to a flush with a hole card, or two+ ranks that complete a straight
    for (let suit = 0; suit < 4; suit++) {
      const n = all.filter((c) => suitOf(c) === suit).length
      if (n === 4 && hole.some((c) => suitOf(c) === suit)) return 'Strong draw'
    }
    const mask = (cs: number[]) => cs.reduce((m, c) => m | (1 << rankOf(c)), 0)
    const withHole = straightOutRanks(mask(all))
    const boardOnly = straightOutRanks(mask(board))
    const gained = [...withHole].filter((r) => !boardOnly.has(r))
    if (gained.length >= 2) return 'Strong draw'
  }
  return 'Nothing'
}

// ---- small random helpers ----

export function randInt(lo: number, hi: number) {
  return lo + Math.floor(Math.random() * (hi - lo + 1))
}
export function pick<T>(xs: readonly T[]): T {
  return xs[Math.floor(Math.random() * xs.length)]
}
export function shuffle<T>(xs: T[]): T[] {
  const a = [...xs]
  for (let i = a.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1))
    ;[a[i], a[j]] = [a[j], a[i]]
  }
  return a
}
/** Deals n distinct cards not in `dead`. */
export function deal(n: number, dead: Iterable<number> = []): number[] {
  const d = new Set(dead)
  return shuffle(FULL_DECK.filter((c) => !d.has(c))).slice(0, n)
}

/** Combo counts can be fractional for weighted ranges: "12" or "469.5". */
export const fmtCombos = (n: number) => (Math.abs(n - Math.round(n)) < 0.05 ? String(Math.round(n)) : n.toFixed(1))

/** C(n, 2). */
export const choose2 = (n: number) => (n * (n - 1)) / 2
