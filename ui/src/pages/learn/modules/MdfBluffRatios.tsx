import { useState, type MouseEvent } from 'react'
import { Term } from '../../../glossary/Term'
import { pick, randInt } from '../../lab/lib/poker'
import { Callout, Drill, Formula, Section, Slider, SplitBar, Stat, bbs, makeChoices, pct1, type Question } from '../kit'

const mdf = (P: number, B: number) => P / (P + B)
const alpha = (P: number, B: number) => B / (P + B)
const bluffShare = (P: number, B: number) => B / (P + 2 * B)

const ROWS = [0.25, 1 / 3, 0.5, 0.75, 1, 1.5, 2]
const SIZE_LABEL = (f: number) => (Math.abs(f - 1 / 3) < 1e-9 ? '33%' : `${Math.round(f * 100)}%`)

export default function MdfBluffRatios() {
  return (
    <>
      <p className="learn-lede">
        Bet size alone fixes three numbers: how often a bluff must work, how often the defender must continue, and how many bluffs a balanced bettor can have. They all come from the same pot arithmetic.
      </p>

      <Section title="The bettor: alpha">
        <p>
          A pure <Term id="bluff">bluff</Term> of B into a pot P risks B to win P. It breaks even when the opponent folds often enough:
        </p>
        <Formula label="Alpha (break-even fold frequency)">
          f × P − (1 − f) × B = 0 ⇒ <b>alpha = B / (P + B)</b>
          <br />
          <span className="muted">half-pot bet, 5 into 10:</span> 5 / 15 = 33.3%
        </Formula>
      </Section>

      <Section title="The defender: MDF">
        <p>
          If the defender folds more than alpha, the bettor profits by bluffing with any two cards. So the defender must continue with at least the rest of their range, the <Term id="mdf">minimum defense frequency</Term>:
        </p>
        <Formula label="Minimum defense frequency">
          <b>MDF = 1 − alpha = P / (P + B)</b>
          <br />
          <span className="muted">5 into 10:</span> 10 / 15 = 66.7% &nbsp;·&nbsp; <span className="muted">pot-sized bet:</span> 50%
        </Formula>
      </Section>

      <Section title="The balanced bettor: bluff ratio">
        <p>
          On the river a <Term id="polarized">polarized</Term> bettor has value hands and bluffs. The defender's <Term id="bluff-catcher">bluff-catchers</Term> beat only the bluffs. Calling B to win P + B needs equity B / (P + 2B), the <Term id="pot-odds">pot odds</Term>. A bluff-catcher's equity is exactly the share of bluffs in the betting range, so the bettor makes it <Term id="indifference">indifferent</Term> by bluffing exactly that share:
        </p>
        <Formula label="Bluff share of a balanced river bet">
          <b>bluffs / (value + bluffs) = B / (P + 2B)</b> &nbsp;⇔&nbsp; value : bluffs = (P + B) : B
          <br />
          <span className="muted">pot-sized bet:</span> 1 / 3 bluffs, 2 : 1 value to bluffs &nbsp;·&nbsp; <span className="muted">half pot:</span> 25%, 3 : 1
          <br />
          <span className="muted">check:</span> calling a pot bet with 1/3 bluffs: (1/3) × 2P − (2/3) × P = 0
        </Formula>
        <table className="mini-table" style={{ maxWidth: 640 }}>
          <thead>
            <tr>
              <th>Bet (% pot)</th>
              <th>Alpha</th>
              <th>MDF</th>
              <th>Bluff share</th>
              <th>Value : bluff</th>
            </tr>
          </thead>
          <tbody>
            {ROWS.map((f) => (
              <tr key={f}>
                <td>{SIZE_LABEL(f)}</td>
                <td className="mono">{pct1(alpha(1, f))}</td>
                <td className="mono">{pct1(mdf(1, f))}</td>
                <td className="mono">{pct1(bluffShare(1, f))}</td>
                <td className="mono">{((1 + f) / f).toFixed(2)} : 1</td>
              </tr>
            ))}
          </tbody>
        </table>
        <p>Bigger bets demand fewer folds from the defender's side (lower MDF) but allow the bettor more bluffs. The bluff share never passes 50%, approached only as the bet grows without limit.</p>
        <Callout title="Where MDF bends.">
          MDF assumes the bettor could be holding pure air and that nothing happens later. In practice: if an opponent bluffs less than the balanced share, folding more than MDF is correct (see <Term id="exploit">exploitation</Term>); on the flop and turn the bettor's "bluffs" still have equity, and solvers often defend somewhat less than MDF; and in <Term id="multiway">multiway</Term> pots the defence is shared between players.
        </Callout>
      </Section>

      <Section title="Try it: one bet size, three numbers" kind="try">
        <MdfWidget />
      </Section>

      <Section title="MDF and bluff ratio drill" kind="drill">
        <Drill slug="mdf-bluff-ratios" generate={genMdf} />
      </Section>
    </>
  )
}

function MdfWidget() {
  const [pot, setPot] = useState(10)
  const [bet, setBet] = useState(7.5)
  const [value, setValue] = useState(20)
  const P = Math.max(0.01, pot)
  const B = Math.max(0.01, bet)
  const bluffs = value * (B / (P + B))
  return (
    <div className="widget col">
      <div className="widget-grid">
        <div className="widget-controls">
          <Slider label="Pot (bb)" value={pot} onChange={setPot} min={1} max={100} step={0.5} />
          <Slider label="Bet (bb)" value={bet} onChange={setBet} min={0.5} max={300} step={0.5} format={(v) => `${Math.round((100 * v) / P)}% pot`} />
          <Slider label="Value combos in the betting range" value={value} onChange={setValue} min={1} max={100} />
        </div>
        <div className="widget-out">
          <div className="stats">
            <Stat label="Alpha (bluff needs folds)" value={pct1(alpha(P, B))} sub={`${bbs(B)} / ${bbs(P + B)}`} />
            <Stat label="MDF" value={pct1(mdf(P, B))} tone="accent" sub={`${bbs(P)} / ${bbs(P + B)}`} />
            <Stat label="Bluff share" value={pct1(bluffShare(P, B))} sub={`${bbs(B)} / ${bbs(P + 2 * B)}`} />
            <Stat label="Bluffs to add" value={bluffs.toFixed(1)} sub={`${value} × ${bbs(B)} / ${bbs(P + B)}`} />
          </div>
          <div>
            <div className="note" style={{ marginBottom: 4 }}>
              Defender's range
            </div>
            <SplitBar
              parts={[
                { label: 'Must continue (MDF)', value: mdf(P, B), color: 'var(--act-call)' },
                { label: 'May fold', value: alpha(P, B), color: 'var(--act-fold)' },
              ]}
            />
          </div>
          <div>
            <div className="note" style={{ marginBottom: 4 }}>
              Balanced bettor's range ({value} value + {bluffs.toFixed(1)} bluffs)
            </div>
            <SplitBar
              parts={[
                { label: 'Value', value: value, color: 'var(--accent-2)' },
                { label: 'Bluffs', value: bluffs, color: 'var(--act-bet-m)' },
              ]}
            />
          </div>
        </div>
      </div>
      <SizeCurves f={B / P} />
    </div>
  )
}

const CW = 720
const CH = 252
const PAD = { l: 44, r: 110, t: 12, b: 42 }
const MAXF = 3

function SizeCurves({ f }: { f: number }) {
  const [hover, setHover] = useState<number | null>(null)
  const x = (s: number) => PAD.l + (s / MAXF) * (CW - PAD.l - PAD.r)
  const y = (v: number) => PAD.t + (1 - v) * (CH - PAD.t - PAD.b)
  const series = [
    { label: 'MDF', fn: (s: number) => 1 / (1 + s), color: 'var(--act-call)' },
    { label: 'Alpha', fn: (s: number) => s / (1 + s), color: 'var(--act-fold)' },
    { label: 'Bluff share', fn: (s: number) => s / (1 + 2 * s), color: 'var(--act-bet-m)' },
  ]
  const path = (fn: (s: number) => number) => Array.from({ length: 121 }, (_, i) => (i * MAXF) / 120).map((s, i) => `${i ? 'L' : 'M'}${x(s).toFixed(1)},${y(fn(s)).toFixed(1)}`).join('')
  const fx = Math.min(MAXF, f)
  const hs = hover ?? fx
  const onMove = (e: MouseEvent<SVGSVGElement>) => {
    const r = e.currentTarget.getBoundingClientRect()
    const vx = ((e.clientX - r.left) / r.width) * CW
    const s = ((vx - PAD.l) / (CW - PAD.l - PAD.r)) * MAXF
    setHover(s >= 0 && s <= MAXF ? Math.round(s * 20) / 20 : null)
  }
  return (
    <div>
      <svg className="lchart" viewBox={`0 0 ${CW} ${CH}`} onMouseMove={onMove} onMouseLeave={() => setHover(null)} role="img" aria-label="MDF, alpha and bluff share against bet size">
        {[0, 0.25, 0.5, 0.75, 1].map((v) => (
          <g key={v}>
            <line x1={PAD.l} x2={CW - PAD.r} y1={y(v)} y2={y(v)} stroke="var(--border)" />
            <text x={PAD.l - 6} y={y(v) + 4} textAnchor="end">
              {v * 100}%
            </text>
          </g>
        ))}
        {[0, 0.5, 1, 1.5, 2, 2.5, 3].map((s) => (
          <text key={s} x={x(s)} y={CH - PAD.b + 16} textAnchor="middle">
            {s * 100}%
          </text>
        ))}
        <text x={CW - PAD.r} y={CH - 2} textAnchor="end" className="axis-label">
          bet size (% of pot)
        </text>
        <line x1={x(hs)} x2={x(hs)} y1={PAD.t} y2={CH - PAD.b} stroke="var(--text-3)" />
        {series.map((s) => (
          <g key={s.label}>
            <path d={path(s.fn)} fill="none" stroke={s.color} strokeWidth={2} strokeLinecap="round" />
            <text x={CW - PAD.r + 6} y={y(s.fn(MAXF)) + 4} className="axis-label">
              {s.label}
            </text>
            <circle cx={x(hs)} cy={y(s.fn(hs))} r={4} fill={s.color} stroke="var(--panel)" strokeWidth={2} />
          </g>
        ))}
      </svg>
      <div className="note">
        At {Math.round(hs * 100)}% pot: MDF {pct1(1 / (1 + hs))} · alpha {pct1(hs / (1 + hs))} · bluff share {pct1(hs / (1 + 2 * hs))}
        {hover === null ? ' (your bet; hover the chart to explore)' : ''}
      </div>
    </div>
  )
}

function genMdf(): Question {
  const pot = randInt(4, 40)
  const f = pick([0.25, 1 / 3, 0.5, 2 / 3, 0.75, 1, 1.25, 1.5, 2])
  const bet = Math.max(1, Math.round(pot * f * 2) / 2)
  const P = pot
  const B = bet
  const kind = pick(['mdf', 'alpha', 'bluff', 'ratio', 'count'] as const)
  const vals = { mdf: mdf(P, B), alpha: alpha(P, B), bluff: bluffShare(P, B), potodds: B / (P + 2 * B) }
  const setup = (
    <>
      Pot <b>{bbs(P)}</b>, bet <b>{bbs(B)}</b> ({Math.round((100 * B) / P)}% pot).
    </>
  )
  if (kind === 'mdf' || kind === 'alpha' || kind === 'bluff') {
    const right = vals[kind]
    const others = [vals.mdf, vals.alpha, vals.bluff, (P + B) / (P + 2 * B), B / P].filter((v) => Math.abs(v - right) > 1e-9)
    const { choices, correct } = makeChoices(pct1(right), others.map(pct1))
    const ask = {
      mdf: 'What is the defender\'s minimum defense frequency?',
      alpha: 'How often must a pure bluff of this size work to break even?',
      bluff: 'On the river, what share of a balanced, polarized betting range should be bluffs?',
    }[kind]
    const how = {
      mdf: `MDF = P / (P + B) = ${bbs(P)} / ${bbs(P + B)} = ${pct1(right)}.`,
      alpha: `Alpha = B / (P + B) = ${bbs(B)} / ${bbs(P + B)} = ${pct1(right)}.`,
      bluff: `Bluff share = B / (P + 2B) = ${bbs(B)} / ${bbs(P + 2 * B)} = ${pct1(right)}, the same as the caller's pot odds.`,
    }[kind]
    return { key: `${kind}-${P}-${B}`, prompt: <>{setup} {ask}</>, choices, correct, explanation: how }
  }
  if (kind === 'ratio') {
    const r = (P + B) / B
    const fmt = (x: number) => `${x.toFixed(2)} : 1`
    const { choices, correct } = makeChoices(fmt(r), [fmt(P / B), fmt((P + 2 * B) / B), fmt(B / (P + B) + 1), fmt(r + 1)])
    return {
      key: `ratio-${P}-${B}`,
      prompt: <>{setup} For a balanced river bet, what is the value-to-bluff ratio?</>,
      choices,
      correct,
      explanation: (
        <>
          Bluffs are B / (P + 2B) of the range, so value is (P + B) / (P + 2B), and value : bluffs = (P + B) : B = {bbs(P + B)} : {bbs(B)} = <b>{fmt(r)}</b>.
        </>
      ),
    }
  }
  const value = randInt(4, 30)
  const n = value * (B / (P + B))
  return {
    key: `count-${P}-${B}-${value}`,
    prompt: (
      <>
        {setup} Your river betting range has <b>{value} value combos</b>. How many bluff combos make it balanced? <span className="faint">(to one decimal)</span>
      </>
    ),
    numeric: { answer: Math.round(n * 10) / 10, tolerance: 0.15 },
    explanation: (
      <>
        bluffs / value = B / (P + B) = {bbs(B)} / {bbs(P + B)}, so bluffs = {value} × {(B / (P + B)).toFixed(3)} = <b>{n.toFixed(1)}</b>. Check: {n.toFixed(1)} / {(value + n).toFixed(1)} = {pct1(n / (value + n))} = B / (P + 2B).
      </>
    ),
  }
}
