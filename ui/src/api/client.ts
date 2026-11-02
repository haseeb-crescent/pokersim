// Thin fetch helpers for the local server.

export async function getJson<T>(path: string): Promise<T> {
  const r = await fetch(path)
  if (!r.ok) throw new Error(`${r.status} ${await r.text()}`)
  return r.json() as Promise<T>
}

export async function postJson<T>(path: string, body: unknown): Promise<T> {
  const r = await fetch(path, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (!r.ok) throw new Error(`${r.status} ${await r.text()}`)
  return r.json() as Promise<T>
}

export const RANKS = ['A', 'K', 'Q', 'J', 'T', '9', '8', '7', '6', '5', '4', '3', '2']

/** Name of a grid cell: row/col 0 = ace. */
export function classAt(row: number, col: number): string {
  if (row === col) return RANKS[row] + RANKS[row]
  if (row < col) return RANKS[row] + RANKS[col] + 's'
  return RANKS[col] + RANKS[row] + 'o'
}

export function classIndex(name: string): number {
  const r1 = RANKS.indexOf(name[0])
  const r2 = RANKS.indexOf(name[1])
  const hi = Math.min(r1, r2)
  const lo = Math.max(r1, r2)
  if (hi === lo) return hi * 13 + hi
  if (name[2] === 's') return hi * 13 + lo
  return lo * 13 + hi
}

export function comboCount(index: number): number {
  const row = Math.floor(index / 13)
  const col = index % 13
  return row === col ? 6 : row < col ? 4 : 12
}

export const bb = (chips: number) => {
  const v = chips / 100
  return (Math.abs(v - Math.round(v)) < 1e-9 ? v.toFixed(0) : v.toFixed(1)) + 'bb'
}

export const pct = (x: number | null | undefined, digits = 0) =>
  x === null || x === undefined ? '–' : (100 * x).toFixed(digits) + '%'

/** Dollars for a chip amount at `bbDollars` per big blind, signed when `sign` is set. */
export function dollars(chips: number, bbDollars: number, sign = false): string {
  const v = (chips / 100) * bbDollars
  const s = Math.abs(v).toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 })
  return (v < 0 ? '-' : sign && v > 0 ? '+' : '') + '$' + s
}
