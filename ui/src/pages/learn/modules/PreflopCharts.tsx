import { useCallback, useEffect, useState } from 'react'
import { Link } from 'react-router-dom'
import { classAt, comboCount } from '../../../api/client'
import type { PreflopNode } from '../../../api/types'
import { Cards } from '../../../components/PlayingCard'
import { RangeGrid, StrategyBar } from '../../../components/RangeGrid'
import { Term } from '../../../glossary/Term'
import { describeApiError, fetchPreflopNode, type ApiProblem } from '../../lab/api'
import { Offline } from '../../lab/Offline'
import { PreflopChartView, actingRange, actionLabel, actionTotals, usePreflopNode } from '../../lab/PreflopChart'
import { CLASS_COMBOS, comboStr, pick, randInt } from '../../lab/lib/poker'
import { Callout, Drill, Section, fixedChoices, pct0, type Question } from '../kit'

const SPOTS = [
  { key: '', label: 'UTG open', who: 'UTG', situation: 'You are first to act (UTG).' },
  { key: 'F', label: 'HJ open', who: 'HJ', situation: 'UTG folds to you in the HJ.' },
  { key: 'F-F', label: 'CO open', who: 'CO', situation: 'It folds to you in the CO.' },
  { key: 'F-F-F', label: 'BTN open', who: 'BTN', situation: 'It folds to you on the BTN.' },
  { key: 'F-F-F-F', label: 'SB first in', who: 'SB', situation: 'It folds to you in the SB.' },
  { key: 'F-F-F-R-F', label: 'BB vs BTN open', who: 'BB', situation: 'The BTN opens, the SB folds, and you are in the BB.' },
]

export default function PreflopCharts() {
  return (
    <>
      <p className="learn-lede">
        A <Term id="preflop-chart">preflop chart</Term> answers one question for one spot: with each of the 169 starting hands, how often should you fold, call or raise?
      </p>

      <Section title="Reading the grid">
        <p>
          The 13 × 13 grid has aces top-left. <b>Pairs</b> run down the diagonal, <b>suited</b> hands sit above it (AKs is just right of AA) and <b>offsuit</b> hands below it (AKo is just under AA). Each cell is a <Term id="hand-class">hand class</Term>, but the classes are not equally likely: an offsuit cell holds 12 <Term id="combos">combos</Term>, a pair 6 and a suited cell only 4. In the grid below, shading shows exactly that.
        </p>
        <div className="row wrap" style={{ alignItems: 'flex-start', gap: 20 }}>
          <RangeGrid weights={Array.from({ length: 169 }, (_, i) => comboCount(i) / 12)} size={28} color="#64748b" />
          <div className="col" style={{ maxWidth: 360 }}>
            <p className="note" style={{ margin: 0 }}>
              Full cells: offsuit (12 combos). Half: pairs (6). A third: suited (4). Totals: 78 offsuit classes × 12 = 936 combos, 78 suited × 4 = 312, 13 pairs × 6 = 78.
            </p>
            <p className="note" style={{ margin: 0 }}>
              So "the BTN opens 40% of hands" counts combos, not cells: opening every suited hand adds far less to that number than opening every offsuit broadway.
            </p>
          </div>
        </div>
        <h3 style={{ marginTop: 16 }}>Colours and mixes</h3>
        <p>
          In a chart, each colour is an action (blue fold, green call, warm colours raises, dark red all-in). A cell split into colours is a <Term id="mixed-strategy">mixed strategy</Term>: K9o coloured 60% raise / 40% fold means raise it about 6 times in 10. At equilibrium a hand only mixes when its options are worth about the same, so getting a mixed hand "wrong" costs little; getting a pure cell wrong costs more.
        </p>
        <h3>Where these charts come from</h3>
        <p>
          The charts in this app are computed by the app's own preflop <Term id="solver">solver</Term> (<Term id="cfr">CFR</Term>) on a simplified 6-max, 100bb game with one size per decision; the sizes are shown on the action buttons (opens are 2.5bb, 3bb from the SB). The solver estimates postflop play rather than solving it, so the charts give <b>frequencies, not true EVs</b>. That is why the coach grades preflop decisions by how often the chart takes your action, and says that charts carry no EVs.
        </p>
        <Callout title="Patterns to look for.">
          Opening ranges widen from UTG to the BTN as fewer players remain behind. The SB can complete as well as raise. The BB defends many hands against a late open because it already has 1bb in: against a 2.5bb BTN open with the SB folding it calls 1.5bb into a 5.5bb final pot, needing only 27% equity.
        </Callout>
      </Section>

      <Section title="Try it: chart viewer (server)" kind="try">
        <ChartViewer />
      </Section>

      <Section title="Chart drill" kind="drill">
        <ChartDrill />
      </Section>
    </>
  )
}

function ChartViewer() {
  const [key, setKey] = useState('F-F-F')
  const { node, error, loading, reload } = usePreflopNode(key)
  return (
    <div className="widget col">
      <div className="btnrow">
        {SPOTS.map((s) => (
          <button key={s.key} className={s.key === key ? 'chip-on' : ''} onClick={() => setKey(s.key)}>
            {s.label}
          </button>
        ))}
      </div>
      {error && <Offline problem={error} onRetry={reload} />}
      {loading && !error && <p className="muted">Loading chart…</p>}
      {node && (
        <>
          <div className="muted" style={{ fontSize: 13 }}>
            <b style={{ color: 'var(--text)' }}>{node.player} to act</b> · {node.description}
          </div>
          <PreflopChartView node={node} size={28} />
          <Link className="note" to="/lab/ranges">
            Browse every spot, including 3-bets and 4-bets, in Lab → Ranges & charts
          </Link>
        </>
      )}
    </div>
  )
}

function useSpotNodes() {
  const [state, setState] = useState<{ nodes: Record<string, PreflopNode>; error: ApiProblem | null; done: boolean }>({ nodes: {}, error: null, done: false })
  const [nonce, setNonce] = useState(0)
  useEffect(() => {
    let live = true
    Promise.allSettled(SPOTS.map((s) => fetchPreflopNode(s.key))).then((rs) => {
      if (!live) return
      const nodes: Record<string, PreflopNode> = {}
      let error: ApiProblem | null = null
      rs.forEach((r, i) => {
        if (r.status === 'fulfilled') nodes[SPOTS[i].key] = r.value
        else error = describeApiError(r.reason)
      })
      setState({ nodes, error, done: true })
    })
    return () => {
      live = false
    }
  }, [nonce])
  return { ...state, retry: () => setNonce((n) => n + 1) }
}

function ChartDrill() {
  const { nodes, error, done, retry } = useSpotNodes()
  const have = Object.keys(nodes).length > 0
  const [mode, setMode] = useState<'chart' | 'basics' | null>(null)
  const active = mode ?? (have ? 'chart' : 'basics')
  const genChart = useCallback(() => chartQuestion(nodes), [nodes])
  if (!done) return <p className="muted">Loading charts…</p>
  return (
    <div className="col">
      <div className="seg" style={{ alignSelf: 'flex-start' }}>
        <button className={active === 'chart' ? 'on' : ''} onClick={() => setMode('chart')} disabled={!have}>
          Chart quiz (uses server charts)
        </button>
        <button className={active === 'basics' ? 'on' : ''} onClick={() => setMode('basics')}>
          Grid basics (offline)
        </button>
      </div>
      {!have && error && <Offline problem={error} onRetry={retry} />}
      {active === 'chart' && have ? (
        <Drill key="chart" slug="preflop-charts" generate={genChart} intro="Answer with the action the chart takes most often. For a mixed hand, any action it takes at least about a third of the time counts." />
      ) : (
        <Drill key="basics" slug="preflop-charts" generate={basicsQuestion} />
      )}
    </div>
  )
}

/** Classes the acting player can hold here that are played, or folded right next to played ones. */
function interestingClasses(node: PreflopNode): number[] {
  const reach = actingRange(node)
  const foldIdx = node.actions.findIndex((a) => a.code === 'F')
  const played = new Set<number>()
  for (let i = 0; i < 169; i++) {
    if (reach[i] < 0.5) continue
    const fold = foldIdx >= 0 ? node.freq[foldIdx][i] ?? 0 : 0
    if (fold < 0.98) played.add(i)
  }
  const out = new Set(played)
  for (const i of played) {
    const r = Math.floor(i / 13)
    const c = i % 13
    for (const [dr, dc] of [
      [0, 1],
      [1, 0],
      [0, -1],
      [-1, 0],
    ]) {
      const rr = r + dr
      const cc = c + dc
      const j = rr * 13 + cc
      if (rr < 0 || rr > 12 || cc < 0 || cc > 12) continue
      if ((rr === cc) !== (r === c) || (rr < cc) !== (r < c)) continue // stay in the same region
      if (reach[j] >= 0.5) out.add(j)
    }
  }
  return [...out]
}

function chartQuestion(nodes: Record<string, PreflopNode>): Question {
  const spot = pick(SPOTS.filter((s) => nodes[s.key]))
  const node = nodes[spot.key]
  const pool = interestingClasses(node)
  const cls = pick(pool.length ? pool : Array.from({ length: 169 }, (_, i) => i))
  const name = classAt(Math.floor(cls / 13), cls % 13)
  const combo = comboStr(pick(CLASS_COMBOS[cls]))
  const labels = node.actions.map(actionLabel)
  const mix = node.freq.map((f) => f[cls] ?? 0)
  const best = mix.indexOf(Math.max(...mix))
  const ok = mix[best] >= 0.65 ? [best] : mix.map((f, i) => (f >= 0.33 || i === best ? i : -1)).filter((i) => i >= 0)
  const { choices, correct } = fixedChoices(labels, ok.map((i) => labels[i]))
  const totals = actionTotals(node)
  return {
    key: `${spot.key}-${name}`,
    prompt: (
      <>
        {spot.situation} You hold <Cards cards={combo.match(/../g)!} size="sm" /> ({name}). What does the chart do?
      </>
    ),
    choices,
    correct,
    explanation: (
      <>
        <div style={{ maxWidth: 420, margin: '4px 0 8px' }}>
          <StrategyBar labels={labels} freqs={mix} height={12} />
        </div>
        {mix[best] >= 0.9
          ? `${name} is close to pure "${labels[best]}" here (${pct0(mix[best])}).`
          : mix[best] >= 0.65
            ? `${name} mostly takes "${labels[best]}" (${pct0(mix[best])}), with some mixing.`
            : `${name} is a genuinely mixed hand here: its options are close in value, so either choice costs little as long as your frequencies are right over time.`}{' '}
        Across the whole {node.player} range in this spot: {labels.map((l, i) => `${l} ${pct0(totals[i])}`).join(', ')}.
      </>
    ),
  }
}

function basicsQuestion(): Question {
  const r = Math.random()
  const g = randInt(0, 168)
  const row = Math.floor(g / 13)
  const col = g % 13
  const name = classAt(row, col)
  const region = row === col ? 'On the diagonal' : row < col ? 'Above the diagonal' : 'Below the diagonal'
  if (r < 0.4) {
    const { choices, correct } = fixedChoices(['On the diagonal', 'Above the diagonal', 'Below the diagonal'], [region])
    return {
      key: `where-${name}`,
      prompt: <>Where is {name} on a standard 13 × 13 chart?</>,
      choices,
      correct,
      explanation: <>Pairs sit on the diagonal, suited hands above it and offsuit hands below it. {name} is {row === col ? 'a pair' : row < col ? 'suited' : 'offsuit'}.</>,
    }
  }
  if (r < 0.75) {
    const n = comboCount(g)
    const { choices, correct } = fixedChoices(['4', '6', '12', '16'], [String(n)])
    return {
      key: `n-${name}`,
      prompt: <>How many combos does the {name} cell stand for?</>,
      choices,
      correct,
      explanation: <>{row === col ? 'A pair: C(4,2) = 6.' : row < col ? 'Suited: one combo per suit, 4.' : 'Offsuit: 4 × 3 = 12.'} (16 would be both suited and offsuit together.)</>,
    }
  }
  const f = randInt(2, 8) / 10
  const n = pick([10, 20, 30, 50])
  return {
    key: `mix-${f}-${n}`,
    prompt: (
      <>
        A cell shows <b>{pct0(f)} raise</b> / {pct0(1 - f)} fold. Over {n} times you are dealt that hand, how many times should you raise?
      </>
    ),
    numeric: { answer: Math.round(f * n), tolerance: 0 },
    explanation: (
      <>
        {pct0(f)} × {n} = {Math.round(f * n)}. A mixed frequency is a long-run rate; a randomiser (such as the seconds on a clock) helps you hit it without patterns.
      </>
    ),
  }
}
