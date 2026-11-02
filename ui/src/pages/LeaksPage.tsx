import { useEffect, useState } from 'react'
import { Link, useNavigate } from 'react-router-dom'
import { getJson, postJson, pct } from '../api/client'
import { Term } from '../components/Term'

interface Leak {
  spot_key: string
  example: string
  street: string
  kind: string
  decisions: number
  ev_loss_bb: number
  avg_loss_bb: number
  mistakes: number
  score: number
  hands: { hand_id: number; index: number; loss: number }[]
  drill: { hero_pos: string; villain_pos: string; pot_type: string; texture: string } | null
}

interface Exploit {
  session_id: number
  hand_no: number
  spot: string
  observed: number
  solver: number
  samples: number
  note: string
}

function describeKey(k: string): string {
  const p = k.split('|')
  if (p[0] === 'PF') {
    const m: Record<string, string> = { RFI: 'first in (open or fold)', vs_open: 'facing an open', vs_3bet: 'facing a 3-bet', vs_3bet_cold: 'facing a 3-bet cold', vs_4bet: 'facing a 4-bet', vs_5bet: 'facing a 5-bet', squeeze: 'squeeze spot', vs_limp: 'vs a limp', offchart: 'off-chart line' }
    return `Preflop · ${m[p[1]] ?? p[1]}${p[2] ? ' · ' + p[2] : ''}`
  }
  if (p[0] === 'MW') return `Multiway · ${p[1]}`
  const pot: Record<string, string> = { SRP: 'single-raised pot', '3BP': '3-bet pot', '4BP': '4-bet pot', Limped: 'limped pot' }
  const node: Record<string, string> = { first: 'first to act', vs_check: 'checked to', vs_bet_small: 'facing a small bet', vs_bet_big: 'facing a big bet', vs_raise: 'facing a raise' }
  return `${pot[p[0]] ?? p[0]} · ${p[1]} · ${p[2]} · ${node[p[3]] ?? p[3]}`
}

export default function LeaksPage() {
  const [data, setData] = useState<{ leaks: Leak[]; hunter: Exploit[] } | null>(null)
  const nav = useNavigate()
  useEffect(() => {
    getJson<{ leaks: Leak[]; hunter: Exploit[] }>('/api/leaks').then(setData)
  }, [])
  const drill = async (d: NonNullable<Leak['drill']>) => {
    await postJson('/api/drill/start', d)
    nav('/play')
  }
  if (!data) return <div className="page muted">Loading…</div>
  const solverLeaks = data.leaks.filter((l) => l.kind !== 'unsolved')
  return (
    <div className="page col">
      <h1>Leak report</h1>
      <p className="muted">
        <Term id="ev-loss">EV loss</Term> against the solver, aggregated by spot type and ranked. Preflop spots have no EVs, so they are ranked by how far your choices were from the chart frequencies.
      </p>
      <table>
        <thead>
          <tr>
            <th>Spot</th>
            <th>Decisions</th>
            <th>Mistakes</th>
            <th>EV lost (bb)</th>
            <th>Per decision</th>
            <th>Worst hands</th>
            <th />
          </tr>
        </thead>
        <tbody>
          {solverLeaks.map((l) => (
            <tr key={l.spot_key}>
              <td>
                <div>{describeKey(l.spot_key)}</div>
                <div className="faint small">e.g. {l.example}</div>
              </td>
              <td>{l.decisions}</td>
              <td>
                {l.mistakes} <span className="faint small">({pct(l.mistakes / Math.max(1, l.decisions))})</span>
              </td>
              <td className={l.ev_loss_bb > 0 ? 'pill-bad' : 'faint'}>{l.kind === 'preflop_chart' ? '—' : l.ev_loss_bb.toFixed(2)}</td>
              <td>{l.kind === 'preflop_chart' ? '—' : l.avg_loss_bb.toFixed(3)}</td>
              <td className="small">
                {l.hands.map((h) => (
                  <Link key={`${h.hand_id}-${h.index}`} to={`/hand/${h.hand_id}?d=${h.index}`} style={{ marginRight: 6 }}>
                    #{h.hand_id}
                  </Link>
                ))}
              </td>
              <td>{l.drill && <button onClick={() => drill(l.drill!)}>Drill this</button>}</td>
            </tr>
          ))}
        </tbody>
      </table>
      {solverLeaks.length === 0 && <p className="faint">No graded decisions yet.</p>}
      <div className="panel col">
        <h3>What the Hunter exploited</h3>
        {data.hunter.length === 0 ? (
          <p className="faint small">Nothing yet. Add the Hunter to your lineup; it starts exploiting a spot after about 12–15 observations of you there.</p>
        ) : (
          <table>
            <thead>
              <tr>
                <th>Hand</th>
                <th>Spot</th>
                <th>You</th>
                <th>Solver</th>
                <th>Samples</th>
                <th>What it did</th>
              </tr>
            </thead>
            <tbody>
              {data.hunter.map((e, i) => (
                <tr key={i}>
                  <td>{e.hand_no}</td>
                  <td className="mono small">{e.spot}</td>
                  <td>{pct(e.observed)}</td>
                  <td>{pct(e.solver)}</td>
                  <td>{e.samples}</td>
                  <td className="small">{e.note}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </div>
  )
}
