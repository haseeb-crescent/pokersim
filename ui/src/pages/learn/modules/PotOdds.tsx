import { useState, type ReactNode } from 'react'
import { Term } from '../../../glossary/Term'
import { pick, randInt } from '../../lab/lib/poker'
import { Callout, Drill, Formula, Section, Slider, SplitBar, Stat, bbs, fixedChoices, makeChoices, pct1, signedBb, type Question } from '../kit'

const SIZES = [0.25, 1 / 3, 0.5, 2 / 3, 0.75, 1, 1.25, 1.5, 2]

/** Required equity to call: call / (pot before the bet + bet + call). */
const requiredEquity = (pot: number, bet: number, call: number) => call / (pot + bet + call)

export default function PotOdds() {
  return (
    <>
      <p className="learn-lede">
        Every time you face a bet, the pot offers you a price. <Term id="pot-odds">Pot odds</Term> turn that price into the minimum <Term id="equity">equity</Term> a call needs to break even.
      </p>

      <Section title="The idea">
        <p>
          Say the pot is <b>P</b> before your opponent bets, they bet <b>B</b>, and you must put in <b>C</b> to call (C = B unless you already have chips in on this street). If you call, the final pot is <b>P + B + C</b>, and you paid C of it. If there is no more betting, calling wins P + B when you win and loses C when you lose:
        </p>
        <Formula label="Break-even point">
          EV(call) = eq × (P + B) − (1 − eq) × C = 0
          <br />⇒ eq × (P + B + C) = C
          <br />⇒ <b>required equity = C / (P + B + C)</b>
        </Formula>
        <p>
          Worked example: the pot is 10bb and villain bets 5bb. You call 5bb to win the 15bb already in the middle, so the odds are 15 : 5 = <b>3 : 1</b>. As equity: 5 / (10 + 5 + 5) = 5 / 20 = <b>25%</b>. (Odds of X : 1 always convert to 1 / (X + 1).)
        </p>
        <p>
          Facing a raise works the same way once you count your own bet as part of the pot. You bet 3bb into 10bb and villain raises to 9bb. The pot before the raise is 13bb (it includes your 3bb), B = 9bb and you need C = 6bb more. Required equity = 6 / (13 + 9 + 6) = 6 / 28 = <b>21.4%</b>.
        </p>
        <p>Because the price depends only on the bet as a fraction of the pot, it is worth memorising a few sizes. For a bet of f × pot, required equity = f / (1 + 2f):</p>
        <table className="mini-table" style={{ maxWidth: 520 }}>
          <thead>
            <tr>
              <th>Bet size</th>
              <th>Odds</th>
              <th>Equity needed to call</th>
            </tr>
          </thead>
          <tbody>
            {[
              ['25% pot', '5 : 1', 0.25],
              ['33% pot', '4 : 1', 1 / 3],
              ['50% pot', '3 : 1', 0.5],
              ['66% pot', '2.5 : 1', 2 / 3],
              ['75% pot', '2.33 : 1', 0.75],
              ['Pot', '2 : 1', 1],
              ['150% pot', '1.67 : 1', 1.5],
              ['200% pot', '1.5 : 1', 2],
            ].map(([label, odds, f]) => (
              <tr key={label as string}>
                <td>{label}</td>
                <td className="mono">{odds}</td>
                <td className="mono">{pct1((f as number) / (1 + 2 * (f as number)))}</td>
              </tr>
            ))}
          </tbody>
        </table>
        <Callout title="When pot odds are not the whole story.">
          The formula is exact when no more betting can happen: on the river, or when you are all-in. Earlier in the hand, money won on later streets (<Term id="implied-odds">implied odds</Term>) can justify calls with less equity, money lost with second-best hands (<Term id="reverse-implied-odds">reverse implied odds</Term>) needs more, and hands rarely <Term id="equity-realization">realize</Term> exactly their raw equity, especially <Term id="oop">out of position</Term>.
        </Callout>
      </Section>

      <Section title="Try it: what price am I getting?" kind="try">
        <PotOddsWidget />
      </Section>

      <Section title="Pot odds drill" kind="drill">
        <Drill slug="pot-odds" generate={genPotOdds} intro="Answer with the formula, not by feel. Keys 1–4 pick an answer; Enter moves on." />
      </Section>
    </>
  )
}

function PotOddsWidget() {
  const [pot, setPot] = useState(10)
  const [bet, setBet] = useState(5)
  const [already, setAlready] = useState(0)
  const [eq, setEq] = useState(30)
  const call = Math.max(0, bet - already)
  const final = pot + bet + call
  const need = final > 0 ? call / final : 0
  const ev = (eq / 100) * (pot + bet) - (1 - eq / 100) * call
  const odds = call > 0 ? (pot + bet) / call : Infinity
  const alreadyBad = already >= bet
  return (
    <div className="widget widget-grid">
      <div className="widget-controls">
        <Slider label="Pot before villain's bet (bb)" value={pot} onChange={setPot} min={1} max={200} step={0.5} />
        <Slider label="Villain's bet, total this street (bb)" value={bet} onChange={setBet} min={0.5} max={400} step={0.5} format={(v) => `${Math.round((100 * v) / Math.max(pot, 0.01))}% pot`} />
        <Slider label="You already bet this street (if facing a raise)" value={already} onChange={setAlready} min={0} max={Math.max(0, bet)} step={0.5} />
        <Slider label="Your equity when you call" value={eq} onChange={setEq} min={0} max={100} step={1} unit="%" />
      </div>
      <div className="widget-out">
        {alreadyBad ? (
          <p className="err">Villain's bet must be bigger than what you already put in.</p>
        ) : (
          <>
            <div className="stats">
              <Stat label="To call" value={bbs(call)} />
              <Stat label="Pot odds" value={`${odds.toFixed(2)} : 1`} sub={`win ${bbs(pot + bet)} for ${bbs(call)}`} />
              <Stat label="Equity needed" value={pct1(need)} tone="accent" />
              <Stat label="EV of calling" value={signedBb(ev, 2)} tone={ev >= 0 ? 'good' : 'bad'} sub={ev >= 0 ? 'call beats fold' : 'fold beats call'} />
            </div>
            <SplitBar
              parts={[
                { label: 'Your call', value: call, color: 'var(--accent-2)' },
                { label: 'Pot + villain bet', value: pot + bet, color: '#475569' },
              ]}
            />
            <div className="work">
              required = C / (P + B + C) = {bbs(call)} / ({bbs(pot)} + {bbs(bet)} + {bbs(call)}) = {bbs(call)} / {bbs(final)} = <b>{pct1(need)}</b>
              <br />
              EV(call) = {eq}% × {bbs(pot + bet)} − {100 - eq}% × {bbs(call)} = <b>{signedBb(ev, 2)}</b>
            </div>
            <p className="note">EV here assumes no more betting after your call (true on the river). With {eq}% equity you {eq / 100 >= need ? 'have enough' : 'are short'} by {Math.abs(eq - 100 * need).toFixed(1)} percentage points.</p>
          </>
        )}
      </div>
    </div>
  )
}

function situation() {
  const pot = randInt(4, 60)
  const f = pick(SIZES)
  const bet = Math.max(1, Math.round(pot * f * 2) / 2)
  return { pot, bet, need: requiredEquity(pot, bet, bet) }
}

function genPotOdds(): Question {
  const r = Math.random()
  const { pot, bet, need } = situation()
  if (r < 0.4) {
    const wrong = [bet / (pot + bet), bet / (pot + 3 * bet), pot / (pot + bet), bet / pot].filter((x) => x < 1)
    const { choices, correct } = makeChoices(pct1(need), wrong.map(pct1))
    return {
      key: `req-${pot}-${bet}`,
      prompt: (
        <>
          The pot is <b>{bbs(pot)}</b> and villain bets <b>{bbs(bet)}</b>. What equity do you need to call?
        </>
      ),
      choices,
      correct,
      explanation: (
        <>
          You call {bbs(bet)} into a final pot of {bbs(pot)} + {bbs(bet)} + {bbs(bet)} = {bbs(pot + 2 * bet)}. Required equity = {bbs(bet)} / {bbs(pot + 2 * bet)} = <b>{pct1(need)}</b>. ({pct1(bet / (pot + bet))} is alpha, what a bluff of this size needs in folds, a different question.)
        </>
      ),
    }
  }
  if (r < 0.65) {
    const odds = (pot + bet) / bet
    return {
      key: `odds-${pot}-${bet}`,
      prompt: (
        <>
          The pot is <b>{bbs(pot)}</b> and villain bets <b>{bbs(bet)}</b>. Your pot odds are X : 1. What is X? <span className="faint">(within 0.1)</span>
        </>
      ),
      numeric: { answer: Math.round(odds * 100) / 100, tolerance: 0.1, unit: ' : 1' },
      explanation: (
        <>
          You risk {bbs(bet)} to win {bbs(pot)} + {bbs(bet)} = {bbs(pot + bet)}. Odds = {bbs(pot + bet)} / {bbs(bet)} = <b>{odds.toFixed(2)} : 1</b>, which is 1 / ({odds.toFixed(2)} + 1) = {pct1(need)} required equity.
        </>
      ),
    }
  }
  // decision with a draw on the turn or a made hand on the river
  const draw = Math.random() < 0.5
  let eq: number
  let what: ReactNode
  if (draw) {
    const outs = pick([4, 8, 9, 12, 15])
    eq = outs / 46
    what = (
      <>
        It is the turn and you have <b>{outs} clean outs</b> ({outs}/46 = {pct1(eq)} to hit the river). Ignore implied odds: you will fold the river if you miss.
      </>
    )
  } else {
    eq = randInt(8, 60) / 100
    what = (
      <>
        It is the river and you expect to win <b>{pct1(eq)}</b> of the time when you call.
      </>
    )
  }
  if (Math.abs(eq - need) < 0.025) return genPotOdds()
  const ok = eq > need
  const ev = eq * (pot + bet) - (1 - eq) * bet
  const { choices, correct } = fixedChoices(['Call', 'Fold'], [ok ? 'Call' : 'Fold'])
  return {
    key: `dec-${pot}-${bet}-${eq.toFixed(3)}`,
    prompt: (
      <>
        Pot <b>{bbs(pot)}</b>, villain bets <b>{bbs(bet)}</b>. {what} Call or fold?
      </>
    ),
    choices,
    correct,
    explanation: (
      <>
        You need {bbs(bet)} / {bbs(pot + 2 * bet)} = <b>{pct1(need)}</b> and you have {pct1(eq)}, so <b>{ok ? 'call' : 'fold'}</b>. EV(call) = {pct1(eq)} × {bbs(pot + bet)} − {pct1(1 - eq)} × {bbs(bet)} = {signedBb(ev, 2)}.
      </>
    ),
  }
}
