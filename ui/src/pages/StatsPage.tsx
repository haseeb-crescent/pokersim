import { useEffect, useState } from 'react'
import { getJson } from '../api/client'
import { Term } from '../components/Term'

export interface StatBlock {
  hands: number
  vpip: number | null
  pfr: number | null
  three_bet: number | null
  fold_to_3bet: number | null
  cbet: number | null
  fold_to_cbet: number | null
  wtsd: number | null
  wsd: number | null
  af: number | null
  net_bb: number
  bb100: number
  bb100_adj: number
  ci95: number
  samples: Record<string, number>
}

interface StatsResp {
  hero: StatBlock
  by_position: { position: string; stats: StatBlock }[]
  bots: { profile: string; stats: StatBlock }[]
  graph: { hand: number; net: number; adj: number }[]
}

const COLS: { key: keyof StatBlock; label: string; term: string; pct?: boolean }[] = [
  { key: 'vpip', label: 'VPIP', term: 'vpip', pct: true },
  { key: 'pfr', label: 'PFR', term: 'pfr', pct: true },
  { key: 'three_bet', label: '3-bet', term: 'three-bet-pct', pct: true },
  { key: 'fold_to_3bet', label: 'Fold to 3-bet', term: 'fold-to-3bet', pct: true },
  { key: 'cbet', label: 'C-bet', term: 'cbet-pct', pct: true },
  { key: 'fold_to_cbet', label: 'Fold to c-bet', term: 'fold-to-cbet', pct: true },
  { key: 'wtsd', label: 'WTSD', term: 'wtsd', pct: true },
  { key: 'wsd', label: 'W$SD', term: 'wsd', pct: true },
  { key: 'af', label: 'AF', term: 'af' },
]

const fmt = (v: unknown, pct?: boolean) => (v === null || v === undefined ? '–' : pct ? `${v}%` : String(v))

export function StatRow({ name, s }: { name: string; s: StatBlock }) {
  return (
    <tr>
      <td>{name}</td>
      <td>{s.hands}</td>
      {COLS.map((c) => (
        <td key={c.key}>{fmt(s[c.key], c.pct)}</td>
      ))}
      <td className={s.bb100 >= 0 ? 'pill-good' : 'pill-bad'}>
        {s.bb100.toFixed(1)} <span className="faint small">±{s.ci95.toFixed(1)}</span>
      </td>
      <td>{s.bb100_adj.toFixed(1)}</td>
    </tr>
  )
}

export function StatHeader({ first }: { first: string }) {
  return (
    <thead>
      <tr>
        <th>{first}</th>
        <th>Hands</th>
        {COLS.map((c) => (
          <th key={c.key}>
            <Term id={c.term}>{c.label}</Term>
          </th>
        ))}
        <th>
          <Term id="bb-100">bb/100</Term> (95% CI)
        </th>
        <th>
          <Term id="all-in-adjusted">All-in adj.</Term>
        </th>
      </tr>
    </thead>
  )
}

function Graph({ pts }: { pts: StatsResp['graph'] }) {
  if (pts.length < 2) return <p className="faint">Play some hands to see the graph.</p>
  const W = 900
  const H = 220
  const xs = pts.map((p) => p.hand)
  const ys = pts.flatMap((p) => [p.net, p.adj])
  const x0 = Math.min(...xs)
  const x1 = Math.max(...xs)
  const y0 = Math.min(0, ...ys)
  const y1 = Math.max(0, ...ys)
  const X = (x: number) => ((x - x0) / Math.max(1, x1 - x0)) * (W - 40) + 30
  const Y = (y: number) => H - 20 - ((y - y0) / Math.max(1e-9, y1 - y0)) * (H - 40)
  const path = (k: 'net' | 'adj') => pts.map((p, i) => `${i ? 'L' : 'M'}${X(p.hand).toFixed(1)},${Y(p[k]).toFixed(1)}`).join(' ')
  return (
    <svg viewBox={`0 0 ${W} ${H}`} style={{ width: '100%', maxWidth: W }}>
      <line x1={30} x2={W - 10} y1={Y(0)} y2={Y(0)} stroke="var(--border)" />
      <path d={path('adj')} fill="none" stroke="var(--warn)" strokeWidth={1.5} strokeDasharray="4 3" />
      <path d={path('net')} fill="none" stroke="var(--accent)" strokeWidth={2} />
      <text x={34} y={14} fill="var(--text-2)" fontSize={11}>
        {y1.toFixed(0)}bb
      </text>
      <text x={34} y={H - 6} fill="var(--text-2)" fontSize={11}>
        {y0.toFixed(0)}bb · {x1} hands
      </text>
      <text x={W - 220} y={14} fill="var(--accent)" fontSize={11}>
        — net won
      </text>
      <text x={W - 130} y={14} fill="var(--warn)" fontSize={11}>
        - - all-in adjusted
      </text>
    </svg>
  )
}

export default function StatsPage() {
  const [s, setS] = useState<StatsResp | null>(null)
  const [err, setErr] = useState<string | null>(null)
  useEffect(() => {
    getJson<StatsResp>('/api/stats').then(setS).catch((e) => setErr(String(e)))
  }, [])
  if (err) return <div className="page pill-bad">{err}</div>
  if (!s) return <div className="page muted">Loading…</div>
  return (
    <div className="page col">
      <h1>Stats</h1>
      <div className="panel">
        <Graph pts={s.graph} />
      </div>
      <div className="panel">
        <table>
          <StatHeader first="" />
          <tbody>
            <StatRow name="You" s={s.hero} />
            {s.by_position.map((p) => (
              <StatRow key={p.position} name={p.position} s={p.stats} />
            ))}
          </tbody>
        </table>
        <p className="faint small">Percentages are of opportunities (e.g. fold to 3-bet: times you opened and faced a 3-bet). All-in adjusted bb/100 replaces all-in runouts by your equity at the moment of the all-in.</p>
      </div>
      <div className="panel">
        <h3>Opponents (from hands you played)</h3>
        <table>
          <StatHeader first="Profile" />
          <tbody>
            {s.bots.map((b) => (
              <StatRow key={b.profile} name={b.profile} s={b.stats} />
            ))}
          </tbody>
        </table>
      </div>
    </div>
  )
}
