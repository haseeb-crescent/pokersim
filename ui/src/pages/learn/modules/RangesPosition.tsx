import { useMemo, useState } from 'react'
import type { Position } from '../../../api/types'
import { RangeGrid } from '../../../components/RangeGrid'
import { Term } from '../../../glossary/Term'
import { CLASS_COMBOS, countCombos, gridToText, parseRange, pick, rangeToGrid, RANK_CHARS } from '../../lab/lib/poker'
import { Callout, Drill, Formula, Section, Slider, Stat, fixedChoices, pct1, type Question } from '../kit'

const PRE: Position[] = ['UTG', 'HJ', 'CO', 'BTN', 'SB', 'BB']
const POST: Position[] = ['SB', 'BB', 'UTG', 'HJ', 'CO', 'BTN']

const PRESETS = [
  { label: 'QQ+, AK', text: 'QQ+, AK' },
  { label: 'All pairs', text: '22+' },
  { label: 'Suited aces', text: 'A2s+' },
  { label: 'Broadways', text: 'TT+, AT+, KT+, QT+, JT' },
  { label: 'Suited connectors', text: 'T9s, 98s, 87s, 76s, 65s, 54s' },
]

export default function RangesPosition() {
  return (
    <>
      <p className="learn-lede">
        You never know your opponent's exact cards, so you reason about every hand they could hold: their <Term id="range">range</Term>. Where they sit, their <Term id="position">position</Term>, shapes that range more than anything else.
      </p>

      <Section title="Thinking in combos">
        <p>
          There are C(52, 2) = 52 × 51 / 2 = <b>1326</b> two-card starting hands, but only 169 strategically different <Term id="hand-class">hand classes</Term>. Count the <Term id="combos">combos</Term> in each class:
        </p>
        <Formula label="Combos per class">
          pair (77): C(4, 2) = 6 &nbsp;·&nbsp; suited (AKs): 4 &nbsp;·&nbsp; offsuit (AKo): 4 × 3 = 12 &nbsp;·&nbsp; AK in total: 16
          <br />
          13 × 6 + 78 × 4 + 78 × 12 = 78 + 312 + 936 = 1326
        </Formula>
        <p>
          Range notation compresses lists of classes. <code>TT+</code> is TT through AA (5 × 6 = 30 combos). <code>A2s+</code> is A2s through AKs (12 × 4 = 48). <code>KTo+</code> is KTo, KJo, KQo (3 × 12 = 36). <code>QQ-88</code> is five pairs (30). A weight such as <code>KQo:0.5</code> means half of those combos (6 of 12). A range's size is usually quoted as a share of 1326: "QQ+, AK" is 18 + 16 = 34 combos, 2.6% of hands.
        </p>
        <p>
          Combos matter because they are the real frequencies. AKo is "one cell" on a chart but three times as likely to be dealt as AKs, and twice as likely as any pocket pair.
        </p>
      </Section>

      <Section title="Try it: build a range" kind="try">
        <RangeBuilder />
      </Section>

      <Section title="Position">
        <p>
          Preflop, the order is fixed: <b>UTG, HJ, CO, BTN, SB, BB</b>. After the flop the blinds act first: <b>SB, BB, UTG, HJ, CO, BTN</b>. So the BTN acts last on every postflop street, and the player who acts last is <Term id="ip">in position</Term>.
        </p>
        <p>Acting last is worth a lot:</p>
        <ul>
          <li>Information: you see the opponent's check or bet before you choose.</li>
          <li>Control: you can check back to take a free card or end the betting, and you close the action.</li>
          <li>
            Realization: together these let IP hands win a larger share of their equity; <Term id="oop">OOP</Term> hands win less (<Term id="equity-realization">equity realization</Term>).
          </li>
        </ul>
        <p>
          There is also a simple counting reason to open tighter early. If each player behind has a strong hand with probability p, the chance that at least one of k players does is 1 − (1 − p)<sup>k</sup> (treating hands as independent, a fair approximation). With p = 34/1326 = 2.6% for QQ+/AK, UTG with five players behind runs into one about 1 − 0.974<sup>5</sup> ≈ 12% of the time; the BTN, with two behind, about 5%.
        </p>
        <Callout title="The blinds are special.">
          The SB and BB already have money in, so they get a better price to continue, but they are out of position after the flop against everyone except each other. That is why the BB defends wide but folds hands an in-position player would happily call with.
        </Callout>
      </Section>

      <Section title="Try it: seats and who is behind" kind="try">
        <SeatWidget />
      </Section>

      <Section title="Ranges and position drill" kind="drill">
        <Drill slug="ranges-position" generate={genRanges} />
      </Section>
    </>
  )
}

function RangeBuilder() {
  const [text, setText] = useState('QQ+, AK')
  const parsed = useMemo(() => parseRange(text), [text])
  const grid = useMemo(() => (parsed.error ? new Array(169).fill(0) : rangeToGrid(parsed.weights)), [parsed])
  const total = parsed.error ? 0 : countCombos(parsed.weights)
  let pairs = 0
  let suited = 0
  let offsuit = 0
  if (!parsed.error)
    for (let g = 0; g < 169; g++) {
      const n = CLASS_COMBOS[g].reduce((s, i) => s + parsed.weights[i], 0)
      const r = Math.floor(g / 13)
      const c = g % 13
      if (r === c) pairs += n
      else if (r < c) suited += n
      else offsuit += n
    }
  return (
    <div className="widget widget-grid">
      <div className="widget-controls">
        <div className="field">
          <label>Range text (or click cells)</label>
          <input className={'mono' + (parsed.error && text.trim() ? ' invalid' : '')} value={text} onChange={(e) => setText(e.target.value)} spellCheck={false} />
          {parsed.error && text.trim() && <span className="err">{parsed.error}</span>}
        </div>
        <div className="btnrow">
          {PRESETS.map((p) => (
            <button key={p.label} onClick={() => setText(p.text)}>
              {p.label}
            </button>
          ))}
          <button onClick={() => setText('')}>Clear</button>
        </div>
        <div className="stats">
          <Stat label="Combos" value={total.toFixed(total % 1 ? 1 : 0)} sub="of 1326" tone="accent" />
          <Stat label="Share of hands" value={pct1(total / 1326)} />
        </div>
        <table className="mini-table">
          <tbody>
            <tr>
              <td>Pairs</td>
              <td className="mono">{pairs.toFixed(0)}</td>
            </tr>
            <tr>
              <td>Suited</td>
              <td className="mono">{suited.toFixed(0)}</td>
            </tr>
            <tr>
              <td>Offsuit</td>
              <td className="mono">{offsuit.toFixed(0)}</td>
            </tr>
          </tbody>
        </table>
      </div>
      <div className="widget-out rg-click" style={{ overflowX: 'auto' }}>
        <RangeGrid
          weights={grid}
          size={32}
          onCellClick={(_, i) => {
            const next = [...grid]
            next[i] = next[i] > 0 ? 0 : 1
            setText(gridToText(next))
          }}
        />
        <span className="note">Pairs on the diagonal, suited hands above it, offsuit below. {grid.some((w) => w > 0 && w < 1) ? 'Faded cells are only partly in the range (weights or specific combos).' : ''}</span>
      </div>
    </div>
  )
}

function SeatWidget() {
  const [seat, setSeat] = useState<Position>('CO')
  const [top, setTop] = useState(3)
  const behindPre = PRE.length - 1 - PRE.indexOf(seat)
  const p = top / 100
  const atLeastOne = 1 - Math.pow(1 - p, behindPre)
  const postRank = POST.indexOf(seat)
  const ipAgainst = POST.filter((_, i) => i < postRank)
  const oopAgainst = POST.filter((_, i) => i > postRank)
  return (
    <div className="widget col">
      <div className="seat-map">
        {PRE.map((s) => (
          <button key={s} className={'seat' + (s === seat ? ' on' : '')} onClick={() => setSeat(s)}>
            <b>{s}</b>
            <span className="faint">
              pre #{PRE.indexOf(s) + 1} · post #{POST.indexOf(s) + 1}
            </span>
          </button>
        ))}
      </div>
      <div className="widget-grid">
        <div className="widget-controls">
          <Slider label="“Strong hand” = top … of hands" value={top} onChange={setTop} min={0.5} max={20} step={0.5} unit="%" />
          <p className="note">For scale: QQ+, AK, AQs is 38 combos, 2.9% of hands. Each player behind is treated as independently holding such a hand with this probability.</p>
        </div>
        <div className="widget-out">
          <div className="stats">
            <Stat label="Players left to act preflop" value={behindPre} />
            <Stat label="P(at least one strong hand behind)" value={pct1(atLeastOne)} tone="accent" sub={`1 − (1 − ${top}%)^${behindPre}`} />
          </div>
          <p style={{ margin: 0 }}>
            After the flop {seat} is <b>in position</b> against {ipAgainst.length ? ipAgainst.join(', ') : 'nobody'} and <b>out of position</b> against {oopAgainst.length ? oopAgainst.join(', ') : 'nobody'}.
          </p>
        </div>
      </div>
    </div>
  )
}

function comboQuestion(): Question {
  const R = RANK_CHARS
  const hi = 1 + Math.floor(Math.random() * 12)
  const lo = Math.floor(Math.random() * hi)
  const kind = pick(['pair', 's', 'o', 'any', 'pair+', 's+', 'span'] as const)
  let token: string
  if (kind === 'pair') token = R[hi] + R[hi]
  else if (kind === 's' || kind === 'o') token = R[hi] + R[lo] + kind
  else if (kind === 'any') token = R[hi] + R[lo]
  else if (kind === 'pair+') token = `${R[lo]}${R[lo]}+`
  else if (kind === 's+') token = `${R[hi]}${R[lo]}s+`
  else token = `${R[hi]}${R[hi]}-${R[lo]}${R[lo]}`
  const n = countCombos(parseRange(token).weights)
  return {
    key: `combo-${token}`,
    prompt: (
      <>
        How many combos are in <code>{token}</code>?
      </>
    ),
    numeric: { answer: n, tolerance: 0 },
    explanation: <>{explainToken(token, n)}</>,
  }
}

function explainToken(token: string, n: number) {
  if (/^(.)\1$/.test(token)) return `A pair has C(4,2) = 6 combos.`
  if (/^..s$/.test(token)) return `One suited class: one combo per suit = 4.`
  if (/^..o$/.test(token)) return `One offsuit class: 4 × 3 = 12 combos.`
  if (/^..$/.test(token)) return `Suited (4) plus offsuit (12) = 16.`
  if (/^(.)\1\+$/.test(token)) return `${n / 6} pairs × 6 = ${n}.`
  if (/^..s\+$/.test(token)) return `${n / 4} suited classes (the kicker climbs up to one below the top card) × 4 = ${n}.`
  return `${n / 6} pairs × 6 = ${n}.`
}

function genRanges(): Question {
  const r = Math.random()
  if (r < 0.5) return comboQuestion()
  if (r < 0.8) {
    const [a, b] = pickTwo(PRE)
    const first = POST.indexOf(a) < POST.indexOf(b) ? a : b
    const ask = Math.random() < 0.5 ? 'first' : 'ip'
    const answer = ask === 'first' ? first : first === a ? b : a
    const { choices, correct } = fixedChoices([a, b], [answer])
    return {
      key: `pos-${a}-${b}-${ask}`,
      prompt: (
        <>
          {a} and {b} see a flop heads-up. {ask === 'first' ? 'Who acts first on the flop?' : 'Who is in position?'}
        </>
      ),
      choices,
      correct,
      explanation: (
        <>
          Postflop order is SB, BB, UTG, HJ, CO, BTN. {first} comes first, so {first} is out of position and {first === a ? b : a} acts last (in position).
        </>
      ),
    }
  }
  const s = pick(PRE.slice(0, 5))
  const behind = PRE.length - 1 - PRE.indexOf(s)
  const { choices, correct } = fixedChoices(['1', '2', '3', '4', '5'], [String(behind)])
  return {
    key: `behind-${s}`,
    prompt: <>It is folded to {s}, who opens. How many players can still act behind {s} preflop?</>,
    choices,
    correct,
    explanation: (
      <>
        Preflop order is UTG, HJ, CO, BTN, SB, BB. After {s}: {PRE.slice(PRE.indexOf(s) + 1).join(', ')}, so {behind}.
      </>
    ),
  }
}

function pickTwo<T>(xs: T[]): [T, T] {
  const i = Math.floor(Math.random() * xs.length)
  let j = Math.floor(Math.random() * (xs.length - 1))
  if (j >= i) j++
  return [xs[i], xs[j]]
}
