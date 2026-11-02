import { useMemo, useState } from 'react'
import type { EquityRequest, EquityResponse } from '../../api/types'
import { Cards } from '../../components/PlayingCard'
import { RangeGrid } from '../../components/RangeGrid'
import { Term } from '../../glossary/Term'
import { describeApiError, fetchEquity, type ApiProblem } from './api'
import { CardInput, cardFieldProblem } from './CardPicker'
import { Offline } from './Offline'
import { cardStr, countCombos, fmtCombos, gridToText, gridToWeights, isExactHand, parseCards, parseRange, rangeToGrid } from './lib/poker'
import './lab.css'

const PLAYER_COLORS = ['#2dd4bf', '#f59e0b', '#a78bfa', '#f472b6', '#60a5fa', '#a3e635']

const PRESETS: { label: string; players: string[]; board: string }[] = [
  { label: 'AA vs KK', players: ['AsAh', 'KdKc'], board: '' },
  { label: 'AKo vs QQ', players: ['AsKd', 'QhQc'], board: '' },
  { label: 'Flush draw vs top pair', players: ['Ah5h', 'KsQd'], board: 'Kh8h2c' },
  { label: 'Combo draw vs AK', players: ['JhTh', 'AsKd'], board: 'Qh9h2c' },
  { label: 'AKs vs a 3-bet range', players: ['AhKh', 'QQ+,AKs,AKo,A5s,A4s'], board: '' },
  { label: 'Three-way', players: ['AsKs', 'QdQc', 'Jh9h'], board: '' },
]

interface Snapshot {
  req: EquityRequest
  res: EquityResponse
}

export default function EquityPage() {
  const [players, setPlayers] = useState<string[]>(['AsKs', 'QQ+,AKo'])
  const [board, setBoard] = useState('')
  const [dead, setDead] = useState('')
  const [samples, setSamples] = useState('')
  const [gridFor, setGridFor] = useState<number | null>(null)
  const [busy, setBusy] = useState(false)
  const [problem, setProblem] = useState<ApiProblem | null>(null)
  const [result, setResult] = useState<Snapshot | null>(null)

  const boardCards = parseCards(board).cards
  const deadCards = parseCards(dead).cards
  const exactCards = players.flatMap((p) => (isExactHand(p) ? parseCards(p).cards : []))
  const tableDead = new Set([...boardCards, ...deadCards])

  const boardErr = cardFieldProblem(board, 5, [0, 3, 4, 5], new Set([...deadCards, ...exactCards]))
  const deadErr = cardFieldProblem(dead, 12, undefined, new Set(exactCards))
  const exactClash = (() => {
    const seen = new Set<number>()
    for (const c of exactCards) {
      if (seen.has(c)) return `${cardStr(c)} is in two players' hands`
      seen.add(c)
    }
    return null
  })()
  const emptyPlayer = players.findIndex((p) => !p.trim())
  const blocking = boardErr ? `Board: ${boardErr}` : deadErr ? `Dead cards: ${deadErr}` : exactClash ?? (emptyPlayer >= 0 ? `Player ${emptyPlayer + 1} needs a hand or range` : null)

  const current: EquityRequest = {
    players: players.map((p) => p.trim()),
    board: boardCards.map(cardStr).join(''),
    dead: deadCards.map(cardStr).join(''),
    ...(samples.trim() ? { samples: Math.max(1000, Math.round(Number(samples) || 0)) } : {}),
  }
  const stale = result && JSON.stringify(result.req) !== JSON.stringify(current)

  async function run() {
    if (blocking) return
    setBusy(true)
    setProblem(null)
    try {
      const res = await fetchEquity(current)
      setResult({ req: current, res })
    } catch (e) {
      setProblem(describeApiError(e))
    } finally {
      setBusy(false)
    }
  }

  const setPlayer = (i: number, v: string) => setPlayers((ps) => ps.map((p, j) => (j === i ? v : p)))

  return (
    <div>
      <p className="lab-sub">
        <Term id="equity">Equity</Term> is each player's share of the pot if all cards are dealt out with no more betting. Enter exact hands (AsKs) or <Term id="range">ranges</Term> (TT+, AKs, KQo:0.5); the server enumerates every runout when it can and uses Monte Carlo sampling otherwise.
      </p>
      <div className="lab-grid">
        <div className="panel col">
          <div className="row wrap" style={{ justifyContent: 'space-between' }}>
            <h2 style={{ margin: 0 }}>Players</h2>
            <div className="btnrow">
              {PRESETS.map((p) => (
                <button
                  key={p.label}
                  type="button"
                  onClick={() => {
                    setPlayers(p.players)
                    setBoard(p.board)
                    setDead('')
                    setGridFor(null)
                  }}
                >
                  {p.label}
                </button>
              ))}
            </div>
          </div>
          <form
            onSubmit={(e) => {
              e.preventDefault()
              run()
            }}
          >
            {players.map((p, i) => (
              <PlayerRow
                key={i}
                index={i}
                value={p}
                onChange={(v) => setPlayer(i, v)}
                onRemove={players.length > 2 ? () => setPlayers((ps) => ps.filter((_, j) => j !== i)) : undefined}
                gridOpen={gridFor === i}
                onToggleGrid={() => setGridFor(gridFor === i ? null : i)}
                dead={tableDead}
              />
            ))}
            <div className="row wrap" style={{ marginTop: 8 }}>
              <button type="button" disabled={players.length >= 6} onClick={() => setPlayers((ps) => [...ps, ''])}>
                + Add player
              </button>
              <span className="note">2–6 players</span>
            </div>
            <div className="col" style={{ marginTop: 14 }}>
              <CardInput label="Board (0, 3, 4 or 5 cards)" value={board} onChange={setBoard} max={5} allowed={[0, 3, 4, 5]} used={new Set([...deadCards, ...exactCards])} placeholder="e.g. Td9d6h" />
              <CardInput label="Dead cards (removed from the deck)" value={dead} onChange={setDead} max={12} used={new Set([...boardCards, ...exactCards])} placeholder="optional" />
              <div className="field" style={{ maxWidth: 220 }}>
                <label>Monte Carlo samples (optional)</label>
                <input type="number" min={1000} step={10000} value={samples} placeholder="server default" onChange={(e) => setSamples(e.target.value)} />
              </div>
            </div>
            <div className="row wrap" style={{ marginTop: 14 }}>
              <button className="primary" type="submit" disabled={busy || !!blocking}>
                {busy ? 'Calculating…' : 'Calculate equity'}
              </button>
              {blocking && <span className="err">{blocking}</span>}
            </div>
          </form>
        </div>

        <div className="panel col">
          <h2 style={{ margin: 0 }}>Result</h2>
          {problem && <Offline problem={problem} onRetry={run} />}
          {!result && !problem && <p className="muted">Press “Calculate equity”.</p>}
          {result && <Results snap={result} stale={!!stale} />}
        </div>
      </div>
    </div>
  )
}

function PlayerRow({
  index,
  value,
  onChange,
  onRemove,
  gridOpen,
  onToggleGrid,
  dead,
}: {
  index: number
  value: string
  onChange: (v: string) => void
  onRemove?: () => void
  gridOpen: boolean
  onToggleGrid: () => void
  dead: Set<number>
}) {
  const exact = isExactHand(value)
  const info = useMemo(() => {
    if (!value.trim()) return null
    if (exact) {
      const { cards, error } = parseCards(value)
      if (error) return { kind: 'err' as const, text: error }
      const hit = cards.find((c) => dead.has(c))
      if (hit !== undefined) return { kind: 'err' as const, text: `${cardStr(hit)} is on the board or dead` }
      return { kind: 'hand' as const, cards: cards.map(cardStr) }
    }
    const r = parseRange(value)
    if (r.error) return { kind: 'warn' as const, text: `Local check: ${r.error} (the server decides)` }
    const n = countCombos(r.weights, dead)
    return { kind: 'range' as const, text: `${n.toFixed(n % 1 ? 1 : 0)} combos${dead.size ? ' after card removal' : ''}`, grid: rangeToGrid(r.weights) }
  }, [value, exact, dead])

  const grid = info && info.kind === 'range' ? info.grid : new Array(169).fill(0)
  return (
    <div className="eq-player">
      <div className="who">
        <span className="eq-swatch" style={{ background: PLAYER_COLORS[index] }} />P{index + 1}
      </div>
      <div>
        <input className={'mono' + (info?.kind === 'err' ? ' invalid' : '')} style={{ width: '100%' }} value={value} onChange={(e) => onChange(e.target.value)} placeholder="AsKs or TT+, AKs" spellCheck={false} />
        <div className="meta">
          {info?.kind === 'hand' && <Cards cards={info.cards} size="sm" />}
          {info?.kind === 'range' && info.text}
          {info?.kind === 'warn' && <span className="warn">{info.text}</span>}
          {info?.kind === 'err' && <span className="err">{info.text}</span>}
        </div>
        {gridOpen && (
          <div className="gridpop">
            <div className="note" style={{ marginBottom: 6 }}>
              Click cells to add or remove hand classes. Partial weights show as faded cells.
            </div>
            <div className="rg-click">
              <RangeGrid
                weights={grid}
                size={26}
                onCellClick={(_, i) => {
                  const next = [...grid]
                  next[i] = next[i] > 0 ? 0 : 1
                  onChange(next.some((x) => x > 0) ? gridToText(next) : '')
                }}
              />
            </div>
            <div className="note" style={{ marginTop: 6 }}>
              {countCombos(gridToWeights(grid)).toFixed(0)} combos selected
            </div>
          </div>
        )}
      </div>
      <div className="row" style={{ gap: 4 }}>
        <button type="button" onClick={onToggleGrid} title="Pick a range from the grid" aria-expanded={gridOpen}>
          Grid
        </button>
        {onRemove && (
          <button type="button" className="ghost" onClick={onRemove} title="Remove player">
            ✕
          </button>
        )}
      </div>
    </div>
  )
}

function Results({ snap, stale }: { snap: Snapshot; stale: boolean }) {
  const { req, res } = snap
  // Rough 95% margin for Monte Carlo: 1.96 * sqrt(p(1-p)/n), using the largest equity.
  const maxEq = Math.max(...res.equity)
  const moe = res.exact ? 0 : 1.96 * Math.sqrt((maxEq * (1 - maxEq)) / Math.max(1, res.samples))
  return (
    <div>
      {stale && <div className="warn" style={{ marginBottom: 6 }}>Inputs changed since this result. Calculate again to update.</div>}
      {req.board && (
        <div className="row" style={{ marginBottom: 8 }}>
          <span className="muted" style={{ fontSize: 12 }}>
            Board
          </span>
          <Cards cards={req.board.match(/../g) ?? []} size="sm" />
        </div>
      )}
      {req.players.map((p, i) => (
        <div key={i} className="eq-result">
          <div className="lbl" title={p}>
            <span className="eq-swatch" style={{ background: PLAYER_COLORS[i] }} />
            {isExactHand(p) ? <Cards cards={parseCards(p).cards.map(cardStr)} size="sm" /> : <span className="mono">{p}</span>}
          </div>
          <div className="eq-bar" title={`equity ${(100 * res.equity[i]).toFixed(2)}%`}>
            <span className="win" style={{ width: `${100 * (res.win[i] ?? 0)}%`, background: PLAYER_COLORS[i] }} />
            <span className="tie" style={{ left: `${100 * (res.win[i] ?? 0)}%`, width: `${100 * (res.tie[i] ?? 0)}%`, background: PLAYER_COLORS[i] }} />
          </div>
          <div className="eq-val">{(100 * res.equity[i]).toFixed(1)}%</div>
          <div className="eq-sub">
            <span>win {(100 * res.win[i]).toFixed(2)}%</span>
            <span>tie {(100 * res.tie[i]).toFixed(2)}%</span>
            <span>{fmtCombos(res.combos[i])} combos</span>
          </div>
        </div>
      ))}
      <div className="eq-metaline">
        <span className="tag">{res.exact ? 'Exact enumeration' : 'Monte Carlo'}</span>
        <span className="tag">{res.samples.toLocaleString()} {res.exact ? 'runouts' : 'samples'}</span>
        {!res.exact && <span className="tag">±{(100 * moe).toFixed(2)}% (≈95%)</span>}
        <span className="tag">{res.ms.toFixed(0)} ms</span>
      </div>
      <p className="note" style={{ marginTop: 10 }}>
        Bars show wins (solid) and ties (faded). Equity = win% plus your share of tied pots (a tie between k players counts 1/k). Combos are the hand combinations each player can hold after card removal.
      </p>
    </div>
  )
}
