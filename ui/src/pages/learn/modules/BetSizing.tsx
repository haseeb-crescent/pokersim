import { useState } from 'react'
import { Term } from '../../../glossary/Term'
import { pick } from '../../lab/lib/poker'
import { C, Callout, Drill, Formula, Section, Slider, Stat, bbs, fixedChoices, makeChoices, pct0, pct1, type Question } from '../kit'

/** Fraction of pot to bet on each of n streets so the last bet is all-in. */
function geometricFraction(pot: number, stack: number, streets: number): number {
  return (Math.pow(1 + (2 * stack) / pot, 1 / streets) - 1) / 2
}

export default function BetSizing() {
  return (
    <>
      <p className="learn-lede">
        How much you bet should depend on what your betting range looks like and what the opponent's range can do about it. Size follows range shape.
      </p>

      <Section title="Range shapes">
        <p>Line up a range from strongest to weakest hand. The shape of that line tells you a lot:</p>
        <RangeShapes />
        <ul>
          <li>
            <Term id="polarized">Polarized</Term>: strong hands and bluffs, few medium hands. Typical of river bets and raises.
          </li>
          <li>
            <Term id="linear">Linear / merged</Term>: the best hands down to a cutoff, strong and medium together. Typical of small c-bets and preflop opens.
          </li>
          <li>
            <Term id="condensed">Condensed</Term>: mostly medium hands, with the strongest hands removed (they raised earlier) and the weakest folded. Typical of a caller's range. A range that can't contain the nuts is <Term id="capped">capped</Term>.
          </li>
        </ul>
      </Section>

      <Section title="Why size follows shape">
        <p>Each size asks a different question of the opponent. Against a bet of f × pot they need f / (1 + 2f) equity to call, and a balanced bettor may bluff that same share:</p>
        <table className="mini-table" style={{ maxWidth: 560 }}>
          <thead>
            <tr>
              <th>Size</th>
              <th>Caller needs</th>
              <th>Bluffs allowed</th>
              <th>Suits</th>
            </tr>
          </thead>
          <tbody>
            <tr>
              <td>25–33%</td>
              <td className="mono">16.7–20%</td>
              <td className="mono">16.7–20%</td>
              <td>linear ranges, range advantage, many weak hands to deny</td>
            </tr>
            <tr>
              <td>50–75%</td>
              <td className="mono">25–30%</td>
              <td className="mono">25–30%</td>
              <td>mixed strategies on dynamic boards</td>
            </tr>
            <tr>
              <td>100–150%</td>
              <td className="mono">33–37.5%</td>
              <td className="mono">33–37.5%</td>
              <td>polarized ranges with a nut advantage</td>
            </tr>
          </tbody>
        </table>
        <p>
          <b>Small bets</b> win with a linear range: lots of worse hands can still call a cheap bet, the bluffs risk little, and the opponent's draws pay to see the next card. When your whole range is ahead (a <Term id="range-advantage">range advantage</Term>), betting small with most of it is efficient.
        </p>
        <p>
          <b>Big bets</b> win with a polarized range. The value hands collect more per call, and the bettor can carry more bluffs. This needs a <Term id="nut-advantage">nut advantage</Term>: if the opponent can hold as many strong hands as you, a big bet just gets raised or called by better. Against a capped or condensed range, an <Term id="overbet">overbet</Term> puts every medium hand in a bad spot.
        </p>
        <Callout title="Example.">
          BTN vs BB on <C>Kh7d2c</C>: the BTN has more strong kings and overpairs, and the BB has many weak hands that can't stand a bet. Solvers mostly bet small and often here. By the river, after more betting, the hands that still want to bet are mostly strong hands or missed draws, so river bets are usually bigger and polarized.
        </Callout>
      </Section>

      <Section title="Stacks: SPR and geometric sizing">
        <p>
          The <Term id="spr">stack-to-pot ratio</Term> is the effective stack divided by the pot. It tells you how many bets fit before everyone is all-in. If you want to get all-in by the river with equal-sized bets, use <Term id="geometric-sizing">geometric sizing</Term>: after a bet of f × pot and a call, the pot is multiplied by (1 + 2f).
        </p>
        <Formula label="Geometric sizing">
          pot after n called bets = P × (1 + 2f)<sup>n</sup> = P + 2S
          <br />⇒ <b>f = ((1 + 2S / P)<sup>1/n</sup> − 1) / 2</b>
        </Formula>
        <p>
          Single-raised pot, BTN vs BB: P = 5.5bb, S = 97.5bb (SPR 17.7). Over three streets f = ((1 + 35.45)<sup>1/3</sup> − 1) / 2 = <b>116% pot</b> each street. In a 3-bet pot, P = 16.5bb and S = 92.5bb (SPR 5.6): f = ((1 + 11.21)<sup>1/3</sup> − 1) / 2 = <b>65% pot</b>. The same stacks mean very different bet sizes depending on the pot.
        </p>
      </Section>

      <Section title="Try it: geometric sizing" kind="try">
        <GeoWidget />
      </Section>

      <Section title="Bet sizing drill" kind="drill">
        <Drill slug="bet-sizing-polarization" generate={genSizing} />
      </Section>
    </>
  )
}

/** Schematic: how many hands sit at each strength level for three range shapes. */
function RangeShapes() {
  const W = 640
  const rowH = 46
  const shapes = [
    { label: 'Polarized', f: (x: number) => (x < 0.25 ? 1 : x > 0.7 ? 0.8 : 0.08) },
    { label: 'Linear / merged', f: (x: number) => (x < 0.6 ? 1 - x * 0.6 : 0.04) },
    { label: 'Condensed', f: (x: number) => (x < 0.12 ? 0.08 : x < 0.7 ? 0.9 : 0.12) },
  ]
  const bins = 24
  const bw = (W - 150) / bins
  return (
    <svg className="lchart" viewBox={`0 0 ${W} ${rowH * shapes.length + 24}`} role="img" aria-label="Schematic range shapes">
      {shapes.map((s, r) => (
        <g key={s.label} transform={`translate(0 ${r * rowH})`}>
          <text x={0} y={rowH / 2 + 4} className="axis-label">
            {s.label}
          </text>
          {Array.from({ length: bins }, (_, i) => {
            const v = s.f((i + 0.5) / bins)
            const h = v * (rowH - 12)
            return <rect key={i} x={140 + i * bw} y={rowH - 6 - h} width={bw - 2} height={h} rx={2} fill={i / bins < 0.5 ? 'var(--accent-2)' : '#64748b'} opacity={0.85} />
          })}
        </g>
      ))}
      <text x={140} y={rowH * shapes.length + 16}>
        strongest
      </text>
      <text x={W - 10} y={rowH * shapes.length + 16} textAnchor="end">
        weakest
      </text>
    </svg>
  )
}

function GeoWidget() {
  const [pot, setPot] = useState(5.5)
  const [stack, setStack] = useState(97.5)
  const [streets, setStreets] = useState(3)
  const f = geometricFraction(pot, stack, streets)
  const rows: { street: string; potBefore: number; bet: number; stackAfter: number }[] = []
  let p = pot
  let s = stack
  for (let i = 0; i < streets; i++) {
    const bet = i === streets - 1 ? s : f * p
    rows.push({ street: ['Flop', 'Turn', 'River'][3 - streets + i], potBefore: p, bet, stackAfter: s - bet })
    p += 2 * bet
    s -= bet
  }
  return (
    <div className="widget widget-grid">
      <div className="widget-controls">
        <Slider label="Pot now (bb)" value={pot} onChange={setPot} min={1} max={100} step={0.5} />
        <Slider label="Effective stack behind (bb)" value={stack} onChange={setStack} min={1} max={300} step={0.5} />
        <div className="seg">
          {[1, 2, 3].map((n) => (
            <button key={n} className={streets === n ? 'on' : ''} onClick={() => setStreets(n)}>
              {n} street{n > 1 ? 's' : ''} left
            </button>
          ))}
        </div>
        <div className="btnrow">
          <button onClick={() => (setPot(5.5), setStack(97.5))}>SRP BTN vs BB</button>
          <button onClick={() => (setPot(16.5), setStack(92.5))}>3-bet pot</button>
          <button onClick={() => (setPot(40), setStack(80))}>4-bet-ish</button>
        </div>
      </div>
      <div className="widget-out">
        <div className="stats">
          <Stat label="SPR" value={(stack / pot).toFixed(2)} />
          <Stat label="Geometric bet" value={pct0(f)} tone="accent" sub="of the pot, every street" />
          <Stat label="Pot multiplier" value={`× ${(1 + 2 * f).toFixed(3)}`} sub="per called bet" />
        </div>
        <table className="mini-table">
          <thead>
            <tr>
              <th>Street</th>
              <th>Pot before</th>
              <th>Bet</th>
              <th>Stack left</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((r) => (
              <tr key={r.street}>
                <td>{r.street}</td>
                <td className="mono">{bbs(r.potBefore)}</td>
                <td className="mono">
                  {bbs(r.bet)} <span className="faint">({pct0(r.bet / r.potBefore)})</span>
                </td>
                <td className="mono">{bbs(Math.max(0, r.stackAfter))}</td>
              </tr>
            ))}
          </tbody>
        </table>
        <div className="work">
          f = ((1 + 2 × {bbs(stack)} / {bbs(pot)})^(1/{streets}) − 1) / 2 = (({(1 + (2 * stack) / pot).toFixed(3)})^(1/{streets}) − 1) / 2 = <b>{pct1(f)}</b>
        </div>
      </div>
    </div>
  )
}

const SHAPES: { desc: string; right: 'Polarized' | 'Linear / merged' | 'Condensed'; why: string }[] = [
  { desc: 'A river bet with the nut straight, sets, and missed flush draws.', right: 'Polarized', why: 'Very strong hands plus air, with nothing in between: polarized.' },
  { desc: 'A 3-bet range of QQ+ and AK plus A5s–A4s.', right: 'Polarized', why: 'Premiums plus hands that play as bluffs (suited wheel aces), skipping the medium hands: polarized.' },
  { desc: 'A 33% c-bet with every top pair, overpairs, sets and some second pairs.', right: 'Linear / merged', why: 'The best hands down to a cutoff, strong and medium together: linear (merged).' },
  { desc: 'A 3-bet range of the top hands in order: TT+, AQ+, KQs.', right: 'Linear / merged', why: 'Simply the best hands down to a cutoff: linear.' },
  { desc: 'A big blind range after calling a flop bet: many middle pairs and weak top pairs; its sets mostly check-raised.', right: 'Condensed', why: 'Mostly medium-strength hands with the top and bottom removed: condensed (and partly capped).' },
  { desc: 'A cold-calling range that 3-bets its premiums: 99–66, AJs, KQs, QJs, JTs.', right: 'Condensed', why: 'No very strong hands and no trash, just medium hands: condensed and capped.' },
]

function genSizing(): Question {
  const r = Math.random()
  if (r < 0.35) {
    const s = pick(SHAPES)
    const { choices, correct } = fixedChoices(['Polarized', 'Linear / merged', 'Condensed'], [s.right])
    return { key: `shape-${s.desc}`, prompt: <>What shape is this range? {s.desc}</>, choices, correct, explanation: s.why }
  }
  if (r < 0.7) {
    const pot = pick([5.5, 6, 10, 16.5, 20, 24, 30])
    const stack = pick([60, 80, 92.5, 97.5, 100, 150])
    const n = pick([2, 3])
    const f = geometricFraction(pot, stack, n)
    const { choices, correct } = makeChoices(pct0(f), [pct0(stack / pot / n), pct0(Math.pow(1 + stack / pot, 1 / n) - 1), pct0(Math.pow((2 * stack) / pot, 1 / n) / 2), pct0(f * 1.6), pct0(f * 0.6)])
    return {
      key: `geo-${pot}-${stack}-${n}`,
      prompt: (
        <>
          Pot <b>{bbs(pot)}</b>, effective stack <b>{bbs(stack)}</b>, {n} streets to go. What fraction of the pot should you bet each street to be all-in by the river with equal-sized bets?
        </>
      ),
      choices,
      correct,
      explanation: (
        <>
          f = ((1 + 2S/P)^(1/n) − 1) / 2 = ((1 + {((2 * stack) / pot).toFixed(2)})^(1/{n}) − 1) / 2 = <b>{pct1(f)}</b>. Each called bet multiplies the pot by 1 + 2f = {(1 + 2 * f).toFixed(3)}; after {n} streets the pot is {bbs(pot)} × {Math.pow(1 + 2 * f, n).toFixed(2)} = {bbs(pot + 2 * stack)}, i.e. all-in.
        </>
      ),
    }
  }
  if (r < 0.85) {
    const pot = pick([5.5, 6.5, 11, 16.5, 22, 25])
    const stack = pick([47.5, 77.5, 92.5, 97.5, 140])
    return {
      key: `spr-${pot}-${stack}`,
      prompt: (
        <>
          The flop pot is <b>{bbs(pot)}</b> and the effective stack behind is <b>{bbs(stack)}</b>. What is the SPR? <span className="faint">(to one decimal)</span>
        </>
      ),
      numeric: { answer: Math.round((stack / pot) * 10) / 10, tolerance: 0.1 },
      explanation: (
        <>
          SPR = stack / pot = {bbs(stack)} / {bbs(pot)} = <b>{(stack / pot).toFixed(2)}</b>.
        </>
      ),
    }
  }
  const nut = Math.random() < 0.5
  const { choices, correct } = fixedChoices(['Small bet, high frequency', 'Large bet or overbet, polarized'], [nut ? 'Large bet or overbet, polarized' : 'Small bet, high frequency'])
  return {
    key: `size-${nut}`,
    prompt: nut ? 'River. Your range has many nut hands that the opponent can\'t have, plus missed draws; their range is capped at one pair. Which sizing strategy fits?' : 'Flop. Your range has more equity overall (lots of overpairs and top pairs) but the opponent has about as many sets and two pairs as you. Which sizing strategy fits?',
    choices,
    correct,
    explanation: nut
      ? 'A polarized range with a nut advantage against a capped range wants the biggest bet the opponent\'s bluff-catchers must still call sometimes: large bets or overbets.'
      : 'A range advantage without a nut advantage is best used with cheap bets: worse hands keep calling, and you avoid big pots against their strongest hands.',
  }
}
