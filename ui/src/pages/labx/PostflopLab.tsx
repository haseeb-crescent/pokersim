import { useEffect, useRef, useState } from 'react'
import { classAt, getJson, postJson } from '../../api/client'
import type { PreflopNode } from '../../api/types'
import { RangeGrid, StrategyBar } from '../../components/RangeGrid'
import { Cards } from '../../components/PlayingCard'

type LineAction = { kind: string; amount?: number }

interface NodeResp {
  solve_id: string
  status: { iterations: number; exploitability_pct: number; finalized: boolean; from_cache: boolean; elapsed_ms: number }
  terminal: boolean
  player: 'OOP' | 'IP'
  pot_bb: number
  actions: string[]
  action_values: LineAction[]
  strategy_grid: number[][]
  range_grid: { OOP: number[]; IP: number[] }
  range_freq: number[]
  equity_grid: number[] | null
  ev_grid: number[][] | null
}

const PRESETS: { label: string; key: string; oop: [string, string]; ip: [string, string]; pot: number; stack: number }[] = [
  { label: 'BTN vs BB, single-raised', key: 'F-F-F-R-F', oop: ['BB', 'C'], ip: ['BTN', ''], pot: 5.5, stack: 97.5 },
  { label: 'CO vs BB, single-raised', key: 'F-F-R-F-F', oop: ['BB', 'C'], ip: ['CO', ''], pot: 5.5, stack: 97.5 },
  { label: 'SB vs BB, single-raised', key: 'F-F-F-F-R', oop: ['SB', ''], ip: ['BB', 'C'], pot: 6, stack: 97 },
]

/** Grid (169 weights) to range text "AKs:0.53,..." */
function gridToText(g: number[]): string {
  const out: string[] = []
  for (let r = 0; r < 13; r++)
    for (let c = 0; c < 13; c++) {
      const w = g[r * 13 + c]
      if (w > 0.005) out.push(w > 0.995 ? classAt(r, c) : `${classAt(r, c)}:${w.toFixed(3)}`)
    }
  return out.join(',')
}

/** Solve any heads-up spot and browse every node: strategy, ranges, equity and EV grids. */
export default function PostflopLab() {
  const [board, setBoard] = useState('Kh7d2c')
  const [oop, setOop] = useState('')
  const [ip, setIp] = useState('')
  const [pot, setPot] = useState(5.5)
  const [stack, setStack] = useState(97.5)
  const [id, setId] = useState<string | null>(null)
  const [line, setLine] = useState<LineAction[]>([])
  const [labels, setLabels] = useState<string[]>([])
  const [node, setNode] = useState<NodeResp | null>(null)
  const [view, setView] = useState<'strategy' | 'ranges' | 'equity'>('strategy')
  const [err, setErr] = useState<string | null>(null)
  const poll = useRef<ReturnType<typeof setTimeout> | null>(null)

  const loadPreset = async (p: (typeof PRESETS)[number]) => {
    try {
      const n = await getJson<PreflopNode>(`/api/lab/preflop/node?key=${p.key}`)
      const pick = ([pos, act]: [string, string]) => {
        const r = n.ranges[pos]
        if (!act) return r
        const ai = n.actions.findIndex((a) => a.code === act)
        return r.map((w, i) => w * (n.freq[ai]?.[i] ?? 0))
      }
      // the acting player's continuing range at this node; the other side's range so far
      setOop(gridToText(p.oop[1] ? pick(p.oop) : n.ranges[p.oop[0]]))
      setIp(gridToText(p.ip[1] ? pick(p.ip) : n.ranges[p.ip[0]]))
      setPot(p.pot)
      setStack(p.stack)
    } catch (e) {
      setErr('Start the server to load chart ranges. ' + String(e))
    }
  }
  useEffect(() => {
    loadPreset(PRESETS[0])
    return () => {
      if (poll.current) clearTimeout(poll.current)
    }
  }, [])

  const fetchNode = async (sid: string, l: LineAction[]) => {
    try {
      const n = await getJson<NodeResp>(`/api/lab/postflop/${sid}/node?line=${encodeURIComponent(JSON.stringify(l))}`)
      setNode(n)
      if (!n.status.finalized) poll.current = setTimeout(() => fetchNode(sid, l), 1500)
    } catch (e) {
      setErr(String(e))
    }
  }
  const solve = async () => {
    setErr(null)
    setNode(null)
    try {
      const r = await postJson<{ solve_id: string }>('/api/lab/postflop/solve', { board, oop, ip, pot_bb: pot, stack_bb: stack })
      setId(r.solve_id)
      setLine([])
      setLabels([])
      fetchNode(r.solve_id, [])
    } catch (e) {
      setErr(String(e))
    }
  }
  const go = (i: number) => {
    if (!node || !id) return
    const l = [...line, node.action_values[i]]
    setLine(l)
    setLabels([...labels, `${node.player} ${node.actions[i]}`])
    if (poll.current) clearTimeout(poll.current)
    fetchNode(id, l)
  }
  const back = (k: number) => {
    if (!id) return
    const l = line.slice(0, k)
    setLine(l)
    setLabels(labels.slice(0, k))
    fetchNode(id, l)
  }
  const cards = board.match(/[2-9TJQKA][shdc]/gi) ?? []
  return (
    <div className="col">
      <h2>Postflop explorer</h2>
      <p className="muted">Solve any heads-up spot with postflop-solver and browse every node. Flop solves take 10-40 s unless cached; turn and river solves take about a second.</p>
      {err && <p className="pill-bad small">{err}</p>}
      <div className="panel col">
        <div className="row wrap">
          {PRESETS.map((p) => (
            <button key={p.key} onClick={() => loadPreset(p)}>
              {p.label}
            </button>
          ))}
        </div>
        <div className="row wrap">
          <label>
            Board <input value={board} onChange={(e) => setBoard(e.target.value)} style={{ width: 140 }} />
          </label>
          <label>
            Pot (bb) <input type="number" value={pot} onChange={(e) => setPot(+e.target.value)} style={{ width: 80 }} />
          </label>
          <label>
            Effective stack (bb) <input type="number" value={stack} onChange={(e) => setStack(+e.target.value)} style={{ width: 80 }} />
          </label>
          {cards.length >= 3 && <Cards cards={cards} size="sm" />}
        </div>
        <label>
          OOP range
          <textarea rows={2} value={oop} onChange={(e) => setOop(e.target.value)} style={{ width: '100%' }} />
        </label>
        <label>
          IP range
          <textarea rows={2} value={ip} onChange={(e) => setIp(e.target.value)} style={{ width: '100%' }} />
        </label>
        <div className="row">
          <button className="primary" onClick={solve}>
            Solve
          </button>
        </div>
      </div>
      {node && (
        <div className="panel col">
          <div className="row wrap">
            <button className="ghost" onClick={() => back(0)}>
              root
            </button>
            {labels.map((l, k) => (
              <button key={k} className="ghost" onClick={() => back(k + 1)}>
                › {l}
              </button>
            ))}
            <span className="grow" />
            <span className="faint small">
              {node.status.from_cache ? 'cached · ' : ''}
              {node.status.iterations} iterations · {node.status.exploitability_pct < 90 ? node.status.exploitability_pct.toFixed(2) + '% pot' : 'measuring'} ·{' '}
              {node.status.finalized ? 'final' : 'solving…'}
            </span>
          </div>
          {node.terminal ? (
            <p className="muted">End of the street (or of the hand). Go back to explore another line.</p>
          ) : (
            <>
              <div className="row wrap">
                <b>
                  {node.player} to act · pot {node.pot_bb}bb
                </b>
                {node.actions.map((a, i) => (
                  <button key={a} onClick={() => go(i)}>
                    {a}
                  </button>
                ))}
                <span className="grow" />
                {(['strategy', 'ranges', 'equity'] as const).map((v) => (
                  <button key={v} className={view === v ? 'primary' : ''} onClick={() => setView(v)} disabled={v === 'equity' && !node.equity_grid}>
                    {v}
                  </button>
                ))}
              </div>
              <StrategyBar labels={node.actions} freqs={node.range_freq} />
              {view === 'strategy' && <RangeGrid strategy={node.strategy_grid} actionLabels={node.actions} weights={node.range_grid[node.player]} size={34} />}
              {view === 'ranges' && (
                <div className="row wrap">
                  <div>
                    <div className="small muted">OOP range here</div>
                    <RangeGrid weights={node.range_grid.OOP} size={26} color="var(--act-fold)" />
                  </div>
                  <div>
                    <div className="small muted">IP range here</div>
                    <RangeGrid weights={node.range_grid.IP} size={26} color="var(--act-bet-s)" />
                  </div>
                </div>
              )}
              {view === 'equity' && node.equity_grid && <RangeGrid weights={node.equity_grid} size={34} color="var(--good)" />}
            </>
          )}
        </div>
      )}
    </div>
  )
}
