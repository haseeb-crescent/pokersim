import { useEffect, useState } from 'react'
import type { PreflopNode, RangeParseResponse } from '../../api/types'
import { RangeGrid, actionColor } from '../../components/RangeGrid'
import { Term } from '../../glossary/Term'
import { describeApiError, fetchPreflopNode, parseRangeOnServer, type ApiProblem } from './api'
import { Offline } from './Offline'
import { PreflopChartView, actionLabel, usePreflopNode } from './PreflopChart'
import { countCombos, parseRange, rangeToGrid } from './lib/poker'
import './lab.css'

const SPOTS: { label: string; key: string }[] = [
  { label: 'UTG open', key: '' },
  { label: 'HJ open', key: 'F' },
  { label: 'CO open', key: 'F-F' },
  { label: 'BTN open', key: 'F-F-F' },
  { label: 'SB open', key: 'F-F-F-F' },
  { label: 'BTN vs CO open', key: 'F-F-R' },
  { label: 'BB vs BTN open', key: 'F-F-F-R-F' },
  { label: 'BB vs SB open', key: 'F-F-F-F-R' },
]

const VERB: Record<string, string> = { F: 'folds', K: 'checks', C: 'calls', R: 'raises', A: 'all-in' }

export default function RangesPage() {
  return (
    <div className="col" style={{ gap: 16 }}>
      <ChartBrowser />
      <CustomRange />
    </div>
  )
}

function ChartBrowser() {
  const [path, setPath] = useState<string[]>([])
  const key = path.join('-')
  const { node, error, loading, reload } = usePreflopNode(key)
  const [seen, setSeen] = useState<Record<string, PreflopNode>>({})
  const [sel, setSel] = useState<number | null>(null)

  useEffect(() => {
    if (node) setSeen((s) => (s[key] ? s : { ...s, [key]: node }))
  }, [node, key])

  // Make sure every node on the path is known, so the breadcrumb can name each step.
  useEffect(() => {
    let live = true
    path.forEach((_, i) => {
      const k = path.slice(0, i).join('-')
      if (!seen[k]) fetchPreflopNode(k).then((n) => live && setSeen((s) => ({ ...s, [k]: n })), () => {})
    })
    return () => {
      live = false
    }
  }, [path, seen])

  const go = (p: string[]) => {
    setPath(p)
    setSel(null)
  }

  return (
    <div className="panel">
      <div className="spot-head">
        <h2>Preflop charts</h2>
        <span className="note">6-max, 100bb. From the app's own preflop solver: action frequencies only, no true EVs.</span>
      </div>
      <div className="btnrow" style={{ margin: '8px 0 12px' }}>
        {SPOTS.map((s) => (
          <button key={s.key} type="button" className={key === s.key ? 'chip-on' : ''} onClick={() => go(s.key ? s.key.split('-') : [])}>
            {s.label}
          </button>
        ))}
      </div>

      <div className="crumbs">
        <button type="button" onClick={() => go([])} disabled={!path.length}>
          Start
        </button>
        {path.map((code, i) => {
          const parent = seen[path.slice(0, i).join('-')]
          const a = parent?.actions.find((x) => x.code === code)
          const verb = VERB[code] ?? code
          const size = code === 'R' && a?.size_bb ? ` to ${a.size_bb}bb` : ''
          const text = parent ? `${parent.player} ${verb}${size}` : verb
          return (
            <span key={i} className="row" style={{ gap: 4 }}>
              <span className="sep">›</span>
              <button type="button" onClick={() => go(path.slice(0, i + 1))} disabled={i === path.length - 1}>
                {text}
              </button>
            </span>
          )
        })}
      </div>

      {error && <Offline problem={error} onRetry={reload} />}
      {loading && !error && <p className="muted">Loading chart…</p>}
      {node && (
        <>
          <div className="spot-head">
            <h2 style={{ fontSize: 17 }}>{node.player} to act</h2>
            <span className="muted">{node.description}</span>
            <span className="tag" title="How often play reaches this spot when everyone follows the charts">
              reached {(100 * node.reach).toFixed(node.reach < 0.01 ? 2 : 1)}% of hands
            </span>
          </div>
          <div className="acts">
            {node.actions.map((a, i) => {
              const child = node.children.find((c) => c.code === a.code)
              const label = actionLabel(a)
              return (
                <button
                  key={a.code}
                  type="button"
                  disabled={!child}
                  title={child ? `Go to the spot after ${node.player} ${VERB[a.code] ?? label}` : 'No further preflop decision after this action'}
                  onClick={() => child && go(child.key ? child.key.split('-') : [...path, a.code])}
                >
                  <i className="swatch" style={{ background: actionColor(label, i, node.actions.length) }} />
                  {label}
                  {child ? ' →' : ''}
                </button>
              )
            })}
          </div>
          <PreflopChartView node={node} selected={sel} onSelect={setSel} />
          <p className="note" style={{ marginTop: 10 }}>
            Cell colours show how often each hand takes each action; cell height shows how much of that hand class is still in {node.player}'s range here. Totals are weighted by <Term id="combos">combos</Term> (pairs 6, suited 4, offsuit 12).
          </p>
        </>
      )}
    </div>
  )
}

function CustomRange() {
  const [text, setText] = useState('22+, A2s+, KTs+, QTs+, JTs, ATo+, KQo')
  const [res, setRes] = useState<RangeParseResponse | null>(null)
  const [problem, setProblem] = useState<ApiProblem | null>(null)
  const [busy, setBusy] = useState(false)

  async function parse() {
    setBusy(true)
    setProblem(null)
    try {
      setRes(await parseRangeOnServer(text))
    } catch (e) {
      setRes(null)
      setProblem(describeApiError(e))
    } finally {
      setBusy(false)
    }
  }

  // Local parse as a fallback when the server is offline.
  const local = problem?.offline ? parseRange(text) : null

  return (
    <div className="panel">
      <h2>Custom range</h2>
      <p className="muted" style={{ fontSize: 13 }}>
        Type a <Term id="range">range</Term> in the usual notation: <code>QQ+</code>, <code>A2s+</code>, <code>KTo-K7o</code>, <code>QQ-88</code>, exact combos like <code>AsKs</code>, and weights like <code>KQo:0.5</code>.
      </p>
      <form
        className="row wrap"
        onSubmit={(e) => {
          e.preventDefault()
          parse()
        }}
      >
        <input className="grow mono" value={text} onChange={(e) => setText(e.target.value)} spellCheck={false} style={{ minWidth: 260 }} />
        <button className="primary" type="submit" disabled={busy || !text.trim()}>
          {busy ? 'Parsing…' : 'Show range'}
        </button>
      </form>
      <div style={{ marginTop: 12 }}>
        {problem && <Offline problem={problem} onRetry={parse} />}
        {res && !res.ok && <p className="err">{res.error ?? 'Could not parse that range.'}</p>}
        {res && res.ok && <RangeResult grid={res.grid} combos={res.combos} percent={res.percent} text={res.text} source="server" />}
        {local && !local.error && (
          <RangeResult grid={rangeToGrid(local.weights)} combos={countCombos(local.weights)} percent={(100 * countCombos(local.weights)) / 1326} text={text} source="local" />
        )}
        {local && local.error && <p className="err">Local check: {local.error}</p>}
      </div>
    </div>
  )
}

function RangeResult({ grid, combos, percent, text, source }: { grid: number[]; combos: number; percent: number; text: string; source: 'server' | 'local' }) {
  return (
    <div className="row wrap" style={{ alignItems: 'flex-start', gap: 16, marginTop: 8 }}>
      <RangeGrid weights={grid} size={28} />
      <div className="col" style={{ minWidth: 220, flex: 1 }}>
        <div className="stat-row">
          <div className="stat">
            <div className="k">Combos</div>
            <div className="v">{combos.toFixed(combos % 1 ? 1 : 0)}</div>
            <div className="s">of 1326</div>
          </div>
          <div className="stat">
            <div className="k">Share of all hands</div>
            <div className="v">{percent.toFixed(1)}%</div>
          </div>
        </div>
        <div className="note">
          {source === 'server' ? 'Normalised: ' : 'Parsed locally because the server is offline: '}
          <code>{text}</code>
        </div>
      </div>
    </div>
  )
}
