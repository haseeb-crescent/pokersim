import { useEffect, useRef, useState } from 'react'
import { getJson, postJson } from '../../api/client'
import type { BotProfile } from '../../api/types'

interface SimResult {
  seat: number
  profile: string
  name: string
  bb100: number
  ci95: number
  stats: Record<string, number | null | Record<string, number>>
  expected: Record<string, number>
}
interface Sim {
  id: string
  done: boolean
  hands: number
  target: number
  hands_per_sec: number
  results: SimResult[]
  series: { hand: number; cum_bb: number[] }[]
}

const COLORS = ['#2dd4bf', '#60a5fa', '#f472b6', '#facc15', '#a78bfa', '#fb923c']
const KEYS: [string, string][] = [
  ['vpip', 'VPIP'],
  ['pfr', 'PFR'],
  ['three_bet', '3-bet'],
  ['fold_to_3bet', 'F3B'],
  ['cbet', 'C-bet'],
  ['fold_to_cbet', 'FCB'],
  ['wtsd', 'WTSD'],
  ['wsd', 'W$SD'],
  ['af', 'AF'],
]

function Swings({ sim }: { sim: Sim }) {
  const pts = sim.series
  if (pts.length < 2) return null
  const W = 900
  const H = 260
  const x1 = pts[pts.length - 1].hand || 1
  const all = pts.flatMap((p) => p.cum_bb)
  const y0 = Math.min(0, ...all)
  const y1 = Math.max(0, ...all)
  const X = (x: number) => 40 + (x / x1) * (W - 50)
  const Y = (y: number) => H - 20 - ((y - y0) / Math.max(1e-9, y1 - y0)) * (H - 40)
  return (
    <svg viewBox={`0 0 ${W} ${H}`} style={{ width: '100%', maxWidth: W }}>
      <line x1={40} x2={W - 10} y1={Y(0)} y2={Y(0)} stroke="var(--border)" />
      {sim.results.map((_, s) => (
        <path key={s} d={pts.map((p, i) => `${i ? 'L' : 'M'}${X(p.hand).toFixed(1)},${Y(p.cum_bb[s]).toFixed(1)}`).join(' ')} fill="none" stroke={COLORS[s]} strokeWidth={1.6} />
      ))}
      <text x={42} y={14} fill="var(--text-2)" fontSize={11}>
        {Math.round(y1)}bb
      </text>
      <text x={42} y={H - 6} fill="var(--text-2)" fontSize={11}>
        {Math.round(y0)}bb · {x1.toLocaleString()} hands
      </text>
    </svg>
  )
}

/** Bot-vs-bot simulation: win rates with confidence intervals, swing graph, stat signatures. */
export default function SimLab() {
  const [profiles, setProfiles] = useState<BotProfile[]>([])
  const [lineup, setLineup] = useState(['gto', 'nit', 'station', 'maniac', 'fish', 'lag'])
  const [hands, setHands] = useState(200000)
  const [sim, setSim] = useState<Sim | null>(null)
  const [err, setErr] = useState<string | null>(null)
  const timer = useRef<ReturnType<typeof setInterval> | null>(null)
  useEffect(() => {
    getJson<BotProfile[]>('/api/profiles').then(setProfiles).catch(() => setErr('Start the server to run simulations.'))
    return () => {
      if (timer.current) clearInterval(timer.current)
    }
  }, [])
  const start = async () => {
    setErr(null)
    try {
      const { id } = await postJson<{ id: string }>('/api/sim/start', { lineup, hands })
      if (timer.current) clearInterval(timer.current)
      timer.current = setInterval(async () => {
        const s = await getJson<Sim>(`/api/sim/${id}`)
        setSim(s)
        if (s.done && timer.current) clearInterval(timer.current)
      }, 700)
    } catch (e) {
      setErr(String(e))
    }
  }
  const stop = () => sim && postJson(`/api/sim/${sim.id}/stop`, {})
  return (
    <div className="col">
      <h2>Bot-vs-bot simulator</h2>
      <p className="muted">
        Six bots play each other with stacks reset to 100bb every hand. Preflop uses the charts with each profile's distortions exactly as at the table; postflop uses the
        fast equity-vs-estimated-range policy (solving every flop is too slow for millions of hands), so simulated postflop stats approximate table play.
      </p>
      {err && <p className="pill-bad">{err}</p>}
      <div className="panel row wrap">
        {lineup.map((id, i) => (
          <select key={i} value={id} onChange={(e) => setLineup(lineup.map((x, j) => (j === i ? e.target.value : x)))} style={{ borderColor: COLORS[i] }}>
            {profiles.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            ))}
          </select>
        ))}
        <select value={hands} onChange={(e) => setHands(+e.target.value)}>
          {[20000, 200000, 1000000, 5000000].map((h) => (
            <option key={h} value={h}>
              {h.toLocaleString()} hands
            </option>
          ))}
        </select>
        <button className="primary" onClick={start}>
          Run
        </button>
        {sim && !sim.done && <button onClick={stop}>Stop</button>}
      </div>
      {sim && (
        <>
          <div className="muted">
            {sim.hands.toLocaleString()} / {sim.target.toLocaleString()} hands · {sim.hands_per_sec.toLocaleString()} hands/s {sim.done ? '· done' : '…'}
          </div>
          <div className="panel">
            <Swings sim={sim} />
          </div>
          <div className="panel">
            <table>
              <thead>
                <tr>
                  <th>Seat</th>
                  <th>bb/100 (95% CI)</th>
                  {KEYS.map(([, l]) => (
                    <th key={l}>{l}</th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {sim.results.map((r) => (
                  <tr key={r.seat}>
                    <td>
                      <span style={{ color: COLORS[r.seat] }}>●</span> {r.name}
                    </td>
                    <td className={r.bb100 >= 0 ? 'pill-good' : 'pill-bad'}>
                      {r.bb100.toFixed(1)} <span className="faint small">±{r.ci95.toFixed(1)}</span>
                    </td>
                    {KEYS.map(([k]) => (
                      <td key={k}>
                        {String(r.stats[k] ?? '–')}
                        {r.expected[k] !== undefined && <div className="faint small">target {r.expected[k]}</div>}
                      </td>
                    ))}
                  </tr>
                ))}
              </tbody>
            </table>
            <p className="faint small">Targets are typical stat signatures of each real-player archetype; the GTO reg's targets come from all-GTO self-play.</p>
          </div>
        </>
      )}
    </div>
  )
}
