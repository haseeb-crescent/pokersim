import { useMemo, useState } from 'react'
import type { EquityResponse } from '../../../api/types'
import { Cards } from '../../../components/PlayingCard'
import { RichText, Term } from '../../../glossary/Term'
import { describeApiError, fetchEquity, fetchPreflopNode, type ApiProblem } from '../../lab/api'
import { CardInput } from '../../lab/CardPicker'
import { Offline } from '../../lab/Offline'
import { BUCKETS, COMBOS, STRONG_BUCKETS, blocked, cardStr, classify, deal, gridToText, parseCards, parseRange, pick, type Bucket } from '../../lab/lib/poker'
import { C, Callout, Drill, Section, SplitBar, fixedChoices, pct1, type Question } from '../kit'

// Hand-typed example ranges (not from the solver), used offline and in the drill.
const RAISER = '22+, A2s+, K8s+, Q9s+, J9s+, T8s+, 97s+, 86s+, 76s, 65s, A9o+, KTo+, QTo+, JTo'
const CALLER = 'JJ-22, AJs-A2s, KTs-K6s, Q8s+, J8s+, T7s+, 96s+, 85s+, 74s+, 63s+, 53s+, 43s, AJo-A8o, KJo-K9o, QTo+, J9o+, T9o, 98o'

const BUCKET_COLORS: Record<Bucket, string> = {
  'Straight or better': '#99f6e4',
  'Set / trips': '#5eead4',
  'Two pair': '#2dd4bf',
  Overpair: '#14b8a6',
  'Top pair': '#0d9488',
  'Weaker pair': '#115e59',
  'Strong draw': '#f59e0b',
  Nothing: '#334155',
}

const BOARDS = ['AsKd4c', 'Kh8s3d', '7s6s5d', '6h5h4c', 'JhTh8c', '8s8d3c', 'Qd8d3d']

interface RangeProfile {
  total: number
  buckets: Record<Bucket, number>
  strong: number
}

/** Weighted bucket counts of a range on a board (combos touching the board removed). */
function profileRange(weights: ArrayLike<number>, board: number[]): RangeProfile {
  const dead = new Set(board)
  const buckets = Object.fromEntries(BUCKETS.map((b) => [b, 0])) as Record<Bucket, number>
  let total = 0
  for (let i = 0; i < 1326; i++) {
    const w = weights[i]
    if (w <= 0 || blocked(i, dead)) continue
    buckets[classify(COMBOS[i], board)] += w
    total += w
  }
  const strong = STRONG_BUCKETS.reduce((s, b) => s + buckets[b], 0)
  return { total, buckets, strong }
}

export default function RangeNutAdvantage() {
  return (
    <>
      <p className="learn-lede">
        Before thinking about your own hand, ask whose <Term id="range">range</Term> the board favours. Two different questions hide in that: who has more equity overall, and who has more of the very best hands.
      </p>

      <Section title="The idea">
        <p>
          <b>
            <Term id="range-advantage">Range advantage</Term>
          </b>{' '}
          is about averages: if every hand in your range played against every hand in the opponent's, how much equity would yours have? 57% against 43% is a range advantage.
        </p>
        <p>
          <b>
            <Term id="nut-advantage">Nut advantage</Term>
          </b>{' '}
          is about the top: what share of each range is very strong (here: two pair or better, including sets and straights)? 12% against 5% is a nut advantage, even if the averages are close.
        </p>
        <p>The two often point the same way, but not always. A range full of overpairs can have the higher average equity while the other range holds more sets and straights. They also call for different strategies:</p>
        <ul>
          <li>Range advantage without much nut advantage: bet often, mostly small. Worse hands and draws still call, and the opponent cannot raise much.</li>
          <li>
            Nut advantage: bet bigger, even <Term id="overbet">overbet</Term>, with a <Term id="polarized">polarized</Term> range, especially when the opponent is <Term id="capped">capped</Term>.
          </li>
          <li>Neither: check more and let the opponent bet.</li>
        </ul>
        <h3>How preflop action shapes this</h3>
        <p>
          Ranges carry their history. When the BTN opens and the BB just calls, the BB has 3-bet most of AA, KK and AK, so on <C>AsKd4c</C> the BTN has both more strong aces and kings and the higher average equity. On a low, connected <Term id="board-texture">board</Term> like <C>7s6s5d</C> the picture changes: the BB defends many small pairs and suited connectors that make sets, two pair and straights, while the BTN's extra preflop strength (AA, KK, AK) is only overpairs and overcards. The BTN's lead in very strong hands shrinks sharply, and on some low boards (try <C>6h5h4c</C>) it flips to the BB. Load the app's charts below and compare the boards.
        </p>
        <Callout title="How this page measures it.">
          Range advantage uses the server's range-vs-range equity. Nut advantage is counted in your browser: each live combo is classified on the board (combos using a board card are removed) and weighted by its range weight.
        </Callout>
      </Section>

      <Section title="Try it: compare two ranges on a board" kind="try">
        <AdvantageWidget />
      </Section>

      <Section title="Advantage drill" kind="drill">
        <Drill slug="range-nut-advantage" generate={genAdvantage} />
      </Section>
    </>
  )
}

function AdvantageWidget() {
  const [a, setA] = useState({ label: 'Opener (example)', text: RAISER })
  const [b, setB] = useState({ label: 'Caller (example)', text: CALLER })
  const [board, setBoard] = useState('7s6s5d')
  const [chartProblem, setChartProblem] = useState<ApiProblem | null>(null)
  const [eq, setEq] = useState<{ key: string; res: EquityResponse } | null>(null)
  const [eqProblem, setEqProblem] = useState<ApiProblem | null>(null)
  const [busy, setBusy] = useState(false)

  const bp = parseCards(board)
  const boardOk = !bp.error && bp.cards.length >= 3 && bp.cards.length <= 5
  const pa = useMemo(() => parseRange(a.text), [a.text])
  const pb = useMemo(() => parseRange(b.text), [b.text])
  const boardKey = bp.cards.join(',')
  const profA = useMemo(() => (boardOk && !pa.error ? profileRange(pa.weights, bp.cards) : null), [pa, boardKey, boardOk]) // eslint-disable-line react-hooks/exhaustive-deps
  const profB = useMemo(() => (boardOk && !pb.error ? profileRange(pb.weights, bp.cards) : null), [pb, boardKey, boardOk]) // eslint-disable-line react-hooks/exhaustive-deps
  const eqKey = `${a.text}|${b.text}|${boardKey}`

  async function loadCharts() {
    setChartProblem(null)
    try {
      const [btn, bb] = await Promise.all([fetchPreflopNode('F-F-F'), fetchPreflopNode('F-F-F-R-F')])
      const r = btn.actions.findIndex((x) => x.code === 'R')
      const c = bb.actions.findIndex((x) => x.code === 'C')
      if (r < 0 || c < 0) throw new Error('500 chart nodes do not have the expected raise/call actions')
      setA({ label: 'BTN open (app chart)', text: gridToText(btn.freq[r]) })
      setB({ label: 'BB call vs BTN (app chart)', text: gridToText(bb.freq[c]) })
    } catch (e) {
      setChartProblem(describeApiError(e))
    }
  }

  async function runEquity() {
    setBusy(true)
    setEqProblem(null)
    try {
      const res = await fetchEquity({ players: [a.text, b.text], board: bp.cards.map(cardStr).join(''), dead: '' })
      setEq({ key: eqKey, res })
    } catch (e) {
      setEqProblem(describeApiError(e))
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="widget col">
      <div className="row wrap">
        <button onClick={loadCharts}>Load BTN open vs BB call from the app's charts</button>
        <button
          onClick={() => {
            setA({ label: 'Opener (example)', text: RAISER })
            setB({ label: 'Caller (example)', text: CALLER })
          }}
        >
          Use the hand-typed example ranges
        </button>
      </div>
      {chartProblem && <Offline problem={chartProblem} onRetry={loadCharts} />}
      <div className="field-row">
        <div className="field">
          <label>Range A: {a.label}</label>
          <textarea className={'mono' + (pa.error ? ' invalid' : '')} rows={3} value={a.text} onChange={(e) => setA({ label: 'Range A (edited)', text: e.target.value })} spellCheck={false} />
          {pa.error && <span className="err">{pa.error}</span>}
        </div>
        <div className="field">
          <label>Range B: {b.label}</label>
          <textarea className={'mono' + (pb.error ? ' invalid' : '')} rows={3} value={b.text} onChange={(e) => setB({ label: 'Range B (edited)', text: e.target.value })} spellCheck={false} />
          {pb.error && <span className="err">{pb.error}</span>}
        </div>
      </div>
      <div className="row wrap" style={{ alignItems: 'flex-end' }}>
        <div style={{ flex: '1 1 320px' }}>
          <CardInput label="Board (3–5 cards)" value={board} onChange={setBoard} max={5} allowed={[3, 4, 5]} />
        </div>
        <div className="btnrow">
          {BOARDS.map((x) => (
            <button key={x} onClick={() => setBoard(x)} className={x === board ? 'chip-on' : ''}>
              <Cards cards={x.match(/../g)!} size="sm" />
            </button>
          ))}
        </div>
      </div>

      {profA && profB && (
        <div className="col" style={{ gap: 14 }}>
          <h3 style={{ margin: '6px 0 0' }}>Nut advantage: share of each range by hand strength</h3>
          {[
            { label: a.label, p: profA },
            { label: b.label, p: profB },
          ].map(({ label, p }) => (
            <div key={label}>
              <div className="row" style={{ justifyContent: 'space-between', fontSize: 13, marginBottom: 4 }}>
                <b>{label}</b>
                <span className="muted">
                  {p.total.toFixed(0)} live combos · two pair or better <b style={{ color: 'var(--text)' }}>{pct1(p.total ? p.strong / p.total : 0)}</b> ({p.strong.toFixed(1)} combos)
                </span>
              </div>
              <SplitBar height={16} parts={BUCKETS.map((k) => ({ label: k, value: p.buckets[k], color: BUCKET_COLORS[k] }))} />
            </div>
          ))}
          <p className="note" style={{ margin: 0 }}>
            {(() => {
              const sa = profA.total ? profA.strong / profA.total : 0
              const sb = profB.total ? profB.strong / profB.total : 0
              if (Math.abs(sa - sb) < 0.01) return 'The strong-hand shares are about equal: no clear nut advantage.'
              return `${sa > sb ? a.label : b.label} has the nut advantage: ${pct1(Math.max(sa, sb))} of its range is two pair or better against ${pct1(Math.min(sa, sb))}.`
            })()}
          </p>

          <h3 style={{ margin: '6px 0 0' }}>Range advantage: range vs range equity</h3>
          <div className="row">
            <button className="primary" onClick={runEquity} disabled={busy}>
              {busy ? 'Calculating…' : 'Calculate with the server'}
            </button>
            {eq && eq.key !== eqKey && <span className="warn">Inputs changed: calculate again.</span>}
          </div>
          {eqProblem && <Offline problem={eqProblem} onRetry={runEquity} />}
          {eq && (
            <SplitBar
              height={20}
              parts={[
                { label: a.label, value: eq.res.equity[0], color: 'var(--accent-2)' },
                { label: b.label, value: eq.res.equity[1], color: '#f59e0b' },
              ]}
            />
          )}
        </div>
      )}
      {!boardOk && <p className="muted">Enter a flop, turn or river board to compare the ranges.</p>}
    </div>
  )
}

const RAISER_W = parseRange(RAISER).weights
const CALLER_W = parseRange(CALLER).weights

const CONCEPTS: { q: string; choices: string[]; right: string; why: string }[] = [
  {
    q: 'BTN opens, BB calls. The flop is [AsKd4c]. Who usually has the range advantage?',
    choices: ['The BTN (preflop raiser)', 'The BB (caller)', 'Neither'],
    right: 'The BTN (preflop raiser)',
    why: 'The BB would have 3-bet most AA, KK and AK, so the BTN keeps more strong aces and kings. Solvers c-bet boards like this very often, usually small.',
  },
  {
    q: 'You hold a clear nut advantage and the opponent\'s range is capped. Which sizing fits best?',
    choices: ['Large bets or overbets', 'Small bets', 'Checking'],
    right: 'Large bets or overbets',
    why: 'When only you can have the strongest hands, big bets make the opponent pay the most with hands they cannot fold profitably, and allow more bluffs (the bluff share B / (P + 2B) grows with B).',
  },
  {
    q: 'Your range has more equity overall, mostly from medium-strength hands, but not many more very strong hands than the opponent. Which plan fits?',
    choices: ['Bet small with a high frequency', 'Overbet with a polarized range', 'Check everything'],
    right: 'Bet small with a high frequency',
    why: 'A range advantage without a nut advantage is cashed in with cheap bets that worse hands and draws still call. Big bets would fold out worse hands and get called only by better ones.',
  },
  {
    q: 'Compared with an A-K-4 flop, who gains nut advantage on [7s6s5d] in a BTN vs BB single-raised pot?',
    choices: ['The BB (caller)', 'The BTN (preflop raiser)', 'Nobody: it is unchanged'],
    right: 'The BB (caller)',
    why: 'The BB defends many small pairs and suited connectors that flop sets, two pair and straights here; the BTN\'s extra preflop strength (AA, KK, AK) is only overpairs and overcards on this board.',
  },
  {
    q: 'Which statement is correct?',
    choices: ['Range advantage is average equity; nut advantage is the share of very strong hands', 'They are two names for the same thing', 'Nut advantage means holding the single best possible hand'],
    right: 'Range advantage is average equity; nut advantage is the share of very strong hands',
    why: 'They measure different parts of the range: the average, and the top. They usually agree, but low connected boards and paired boards often split them.',
  },
]

function genAdvantage(): Question {
  if (Math.random() < 0.4) {
    const c = pick(CONCEPTS)
    const { choices, correct } = fixedChoices(c.choices, [c.right])
    return { key: `c-${c.q}`, prompt: <RichText text={c.q} />, choices, correct, explanation: c.why }
  }
  for (let tries = 0; tries < 20; tries++) {
    const board = deal(3)
    const pa = profileRange(RAISER_W, board)
    const pb = profileRange(CALLER_W, board)
    const sa = pa.strong / pa.total
    const sb = pb.strong / pb.total
    if (Math.abs(sa - sb) < 0.015) continue
    const right = sa > sb ? 'Opener' : 'Caller'
    const { choices, correct } = fixedChoices(['Opener', 'Caller'], [right])
    const strs = board.map(cardStr)
    return {
      key: `n-${strs.join('')}`,
      prompt: (
        <>
          Flop <Cards cards={strs} size="sm" />. With the example ranges on this page (an opener and a caller that 3-bet its premiums), which range has the bigger share of two pair or better?
        </>
      ),
      choices,
      correct,
      explanation: (
        <>
          Opener: {pa.strong.toFixed(1)} of {pa.total.toFixed(0)} live combos = <b>{pct1(sa)}</b>. Caller: {pb.strong.toFixed(1)} of {pb.total.toFixed(0)} = <b>{pct1(sb)}</b>. Sets, two pair and straights come from pairs and connected hands that match this board; the caller has more small pairs and suited connectors, the opener more big pairs and broadways.
        </>
      ),
    }
  }
  const c = CONCEPTS[0]
  const { choices, correct } = fixedChoices(c.choices, [c.right])
  return { key: `c-${c.q}`, prompt: <RichText text={c.q} />, choices, correct, explanation: c.why }
}
