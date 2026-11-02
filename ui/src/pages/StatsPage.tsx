import { useEffect, useState } from 'react'
import { bb, dollars, getJson } from '../api/client'
import type { CashSummary } from '../api/types'
import { ResultsGraph } from '../components/ResultsGraph'
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

/** Session mode: lifetime money results across every cash session. */
function CashResults() {
  const [c, setC] = useState<CashSummary | null>(null)
  useEffect(() => {
    getJson<CashSummary>('/api/cash/summary').then(setC).catch(() => setC(null))
  }, [])
  if (!c || c.sessions.length === 0) return null
  // dollars per session at that session's stakes
  const lifeDollars = c.sessions.reduce((a, x) => a + x.profit * (x.bb_dollars ?? 1), 0)
  const pts = c.graph.map((p) => ({ hand: p.i, net: p.net / 100, adj: p.ev / 100 }))
  const L = c.lifetime
  return (
    <div className="panel col">
      <h3>Session mode results</h3>
      <div className="row">
        <span>
          Lifetime{' '}
          <b className={L.profit >= 0 ? 'pill-good' : 'pill-bad'}>
            {L.profit >= 0 ? '+' : ''}
            {bb(L.profit)} ({dollars(lifeDollars, 1, true)})
          </b>{' '}
          over {L.hands} hands in {L.sessions} session{L.sessions === 1 ? '' : 's'} · bought in for {bb(L.buyins)} · all-in adjusted {L.ev_profit >= 0 ? '+' : ''}
          {bb(Math.round(L.ev_profit))}
        </span>
      </div>
      <ResultsGraph pts={pts} />
      <table>
        <thead>
          <tr>
            <th>Session</th>
            <th>Started</th>
            <th>Hands</th>
            <th>Stakes</th>
            <th>Bought in</th>
            <th>Stack</th>
            <th>Profit</th>
            <th>All-in adj.</th>
            <th />
          </tr>
        </thead>
        <tbody>
          {c.sessions.map((x) => (
            <tr key={x.id}>
              <td>#{x.id}</td>
              <td>{new Date(x.started_at * 1000).toLocaleString()}</td>
              <td>{x.hands}</td>
              <td>${(x.bb_dollars ?? 1).toFixed(2)}/bb</td>
              <td>{bb(x.buyins)}</td>
              <td>{bb(x.stack)}</td>
              <td className={x.profit >= 0 ? 'pill-good' : 'pill-bad'}>
                {x.profit >= 0 ? '+' : ''}
                {bb(x.profit)} ({dollars(x.profit, x.bb_dollars ?? 1, true)})
              </td>
              <td>
                {x.ev_profit >= 0 ? '+' : ''}
                {bb(Math.round(x.ev_profit))}
              </td>
              <td className="faint small">{x.closed ? 'cashed out' : c.running === x.id ? 'playing' : 'open'}</td>
            </tr>
          ))}
        </tbody>
      </table>
      <p className="faint small">
        Profit is the hero's stack minus buy-ins (rebuys and top-ups included). Dollars use each session's stakes. The all-in adjusted line replaces all-in runouts by your equity at the moment of the all-in.
      </p>
    </div>
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
        <ResultsGraph pts={s.graph} />
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
      <CashResults />
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
