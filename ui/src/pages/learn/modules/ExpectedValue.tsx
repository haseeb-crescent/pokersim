import { useState } from 'react'
import { Term } from '../../../glossary/Term'
import { pick, randInt } from '../../lab/lib/poker'
import { Callout, Drill, Formula, Section, Slider, Stat, bbs, makeChoices, pct0, pct1, signedBb, type Question } from '../kit'

export default function ExpectedValue() {
  return (
    <>
      <p className="learn-lede">
        <Term id="ev">Expected value</Term> is the average result of a decision over many repetitions. Solvers pick actions by EV, and the coach grades you by how much EV you gave up.
      </p>

      <Section title="The idea">
        <p>List every way the decision can turn out, multiply each result by its probability, and add them up.</p>
        <Formula label="Expected value">EV = Σ probability × result</Formula>
        <p>
          Measure results <b>relative to folding</b>: chips already in the pot are gone whatever you do, so folding is worth 0 and every other action is compared with that. A positive EV means the action earns more than folding on average.
        </p>
        <h3>Calling</h3>
        <p>On the river, a call of C into a pot of P after a bet of B wins P + B when you are ahead and loses C when you are behind:</p>
        <Formula>
          EV(call) = eq × (P + B) − (1 − eq) × C
          <br />
          <span className="muted">pot 20bb, bet 10bb, 30% equity:</span> 0.30 × 30 − 0.70 × 10 = 9 − 7 = <b>+2bb</b>
        </Formula>
        <p>
          Setting EV(call) = 0 gives back the <Term id="pot-odds">pot odds</Term> formula, eq = C / (P + B + C) = 10 / 40 = 25%. Pot odds are simply the break-even point of this EV.
        </p>
        <h3>Bluffing</h3>
        <p>A pure <Term id="bluff">bluff</Term> of B into P wins the pot when villain folds (probability f) and loses the bet otherwise:</p>
        <Formula>
          EV(bluff) = f × P − (1 − f) × B
          <br />
          <span className="muted">bet 10 into 20, villain folds 40%:</span> 0.4 × 20 − 0.6 × 10 = <b>+2bb</b>
          <br />
          break-even when f = B / (P + B) = 10 / 30 = 33.3% <span className="muted">(this is </span>
          <Term id="alpha">alpha</Term>
          <span className="muted">)</span>
        </Formula>
        <h3>Betting with fold, call and raise outcomes</h3>
        <p>
          A <Term id="semi-bluff">semi-bluff</Term> combines the two: <Term id="fold-equity">fold equity</Term> now, plus equity when called. If villain folds with probability f, calls with c and raises with r (and you fold to the raise):
        </p>
        <Formula>
          EV(bet) = f × P + c × [eq × (P + 2B) − B] + r × (−B)
          <br />
          <span className="muted">P = 10, B = 7, f = 40%, c = 50%, r = 10%, 30% equity when called:</span>
          <br />= 0.4 × 10 + 0.5 × (0.3 × 24 − 7) − 0.1 × 7 = 4 + 0.1 − 0.7 = <b>+3.4bb</b>
        </Formula>
        <Callout title="Two cautions.">
          First, compare actions with each other, not just with zero: a bet worth +3.4bb is still a mistake if checking is worth +4bb. That gap is the <Term id="ev-loss">EV loss</Term> the coach reports. Second, "equity when called" before the river should be the equity you expect to realize over the remaining streets, not the raw all-in number.
        </Callout>
      </Section>

      <Section title="Try it: decision tree" kind="try">
        <EvWidget />
      </Section>

      <Section title="EV drill" kind="drill">
        <Drill slug="ev" generate={genEv} intro="Pick the EV in big blinds, relative to folding." />
      </Section>
    </>
  )
}

function EvWidget() {
  const [mode, setMode] = useState<'call' | 'bet'>('bet')
  const [pot, setPot] = useState(10)
  const [bet, setBet] = useState(7)
  const [eq, setEq] = useState(30)
  const [fold, setFold] = useState(40)
  const [raise, setRaise] = useState(10)
  const e = eq / 100
  const call = Math.max(0, 100 - fold - raise)
  const evCall = e * (pot + bet) - (1 - e) * bet
  const branchFold = (fold / 100) * pot
  const whenCalled = e * (pot + 2 * bet) - bet
  const branchCall = (call / 100) * whenCalled
  const branchRaise = (raise / 100) * -bet
  const evBet = branchFold + branchCall + branchRaise
  return (
    <div className="widget col">
      <div className="seg">
        <button className={mode === 'bet' ? 'on' : ''} onClick={() => setMode('bet')}>
          I bet: villain folds, calls or raises
        </button>
        <button className={mode === 'call' ? 'on' : ''} onClick={() => setMode('call')}>
          I face a river bet: call or fold
        </button>
      </div>
      <div className="widget-grid">
        <div className="widget-controls">
          <Slider label="Pot before the bet (bb)" value={pot} onChange={setPot} min={1} max={150} step={0.5} />
          <Slider label={mode === 'bet' ? 'My bet (bb)' : "Villain's bet (bb)"} value={bet} onChange={setBet} min={0.5} max={300} step={0.5} format={(v) => `${Math.round((100 * v) / pot)}% pot`} />
          <Slider label={mode === 'bet' ? 'My equity when called' : 'My equity when I call'} value={eq} onChange={setEq} min={0} max={100} unit="%" />
          {mode === 'bet' && (
            <>
              <Slider label="Villain folds" value={fold} onChange={(v) => setFold(Math.min(v, 100 - raise))} min={0} max={100} unit="%" />
              <Slider label="Villain raises (I fold)" value={raise} onChange={(v) => setRaise(Math.min(v, 100 - fold))} min={0} max={100} unit="%" />
              <div className="note">Villain calls the remaining {call}%.</div>
            </>
          )}
        </div>
        <div className="widget-out">
          {mode === 'call' ? (
            <>
              <div className="stats">
                <Stat label="EV(fold)" value="0bb" />
                <Stat label="EV(call)" value={signedBb(evCall, 2)} tone={evCall >= 0 ? 'good' : 'bad'} />
                <Stat label="Break-even equity" value={pct1(bet / (pot + 2 * bet))} />
              </div>
              <div className="work">
                EV(call) = {eq}% × ({bbs(pot)} + {bbs(bet)}) − {100 - eq}% × {bbs(bet)}
                <br />= {bbs(e * (pot + bet), 2)} − {bbs((1 - e) * bet, 2)} = <b>{signedBb(evCall, 2)}</b>
              </div>
              <p className="note">Best action: {evCall > 0 ? 'call' : evCall < 0 ? 'fold' : 'either (indifferent)'}. {evCall < 0 && `Folding saves ${bbs(-evCall, 2)} on average.`}</p>
            </>
          ) : (
            <>
              <table className="mini-table">
                <thead>
                  <tr>
                    <th>Branch</th>
                    <th>Probability</th>
                    <th>Result</th>
                    <th>Contribution</th>
                  </tr>
                </thead>
                <tbody>
                  <tr>
                    <td>Villain folds</td>
                    <td className="mono">{fold}%</td>
                    <td className="mono">{signedBb(pot)}</td>
                    <td className="mono">{signedBb(branchFold, 2)}</td>
                  </tr>
                  <tr>
                    <td>Villain calls</td>
                    <td className="mono">{call}%</td>
                    <td className="mono" title="eq × (P + 2B) − B">
                      {signedBb(whenCalled, 2)}
                    </td>
                    <td className="mono">{signedBb(branchCall, 2)}</td>
                  </tr>
                  <tr>
                    <td>Villain raises, I fold</td>
                    <td className="mono">{raise}%</td>
                    <td className="mono">{signedBb(-bet)}</td>
                    <td className="mono">{signedBb(branchRaise, 2)}</td>
                  </tr>
                </tbody>
              </table>
              <div className="stats">
                <Stat label="EV(bet)" value={signedBb(evBet, 2)} tone={evBet >= 0 ? 'good' : 'bad'} sub="relative to giving up" />
                <Stat label="When called" value={signedBb(whenCalled, 2)} sub={`${eq}% × ${bbs(pot + 2 * bet)} − ${bbs(bet)}`} />
                <Stat label="Pure-bluff break-even" value={pct0(bet / (pot + bet))} sub="folds needed with 0% equity, no raises" />
              </div>
              <p className="note">“When called” assumes the hand is checked down after the call; on earlier streets use the equity you expect to realize.</p>
            </>
          )}
        </div>
      </div>
    </div>
  )
}

function genEv(): Question {
  const r = Math.random()
  const pot = randInt(4, 40) * 1
  const bet = Math.max(1, Math.round(pot * pick([0.33, 0.5, 0.75, 1, 1.5])))
  if (r < 0.45) {
    const eq = randInt(1, 12) * 5 / 100
    const ev = eq * (pot + bet) - (1 - eq) * bet
    const { choices, correct } = makeChoices(signedBb(ev), [signedBb(eq * (pot + 2 * bet) ), signedBb(eq * pot - (1 - eq) * bet), signedBb(eq * (pot + bet)), signedBb(-ev), signedBb(eq * (pot + 2 * bet) - (1 - eq) * bet)])
    return {
      key: `call-${pot}-${bet}-${eq}`,
      prompt: (
        <>
          River. The pot is <b>{bbs(pot)}</b>, villain bets <b>{bbs(bet)}</b>, and you win <b>{pct0(eq)}</b> of the time when you call. What is the EV of calling?
        </>
      ),
      choices,
      correct,
      explanation: (
        <>
          EV = {pct0(eq)} × ({bbs(pot)} + {bbs(bet)}) − {pct0(1 - eq)} × {bbs(bet)} = {bbs(eq * (pot + bet), 2)} − {bbs((1 - eq) * bet, 2)} = <b>{signedBb(ev)}</b>. When you win you collect the pot and villain's bet; when you lose you lose only your call.
        </>
      ),
    }
  }
  if (r < 0.8) {
    const f = randInt(2, 14) * 5 / 100
    const ev = f * pot - (1 - f) * bet
    const { choices, correct } = makeChoices(signedBb(ev), [signedBb(f * (pot + bet) - (1 - f) * bet), signedBb(f * pot - bet), signedBb(f * pot), signedBb(f * pot - (1 - f) * (pot + bet))])
    return {
      key: `bluff-${pot}-${bet}-${f}`,
      prompt: (
        <>
          You bluff <b>{bbs(bet)}</b> into <b>{bbs(pot)}</b> with a hand that never wins when called. Villain folds <b>{pct0(f)}</b>. What is the EV of the bluff?
        </>
      ),
      choices,
      correct,
      explanation: (
        <>
          EV = {pct0(f)} × {bbs(pot)} − {pct0(1 - f)} × {bbs(bet)} = <b>{signedBb(ev)}</b>. Break-even needs {bbs(bet)} / ({bbs(pot)} + {bbs(bet)}) = {pct1(bet / (pot + bet))} folds, so this bluff is {ev >= 0 ? 'profitable' : 'losing'}.
        </>
      ),
    }
  }
  const f = randInt(2, 8) * 5 / 100
  const rz = randInt(0, 3) * 5 / 100
  const c = 1 - f - rz
  const eq = randInt(2, 8) * 5 / 100
  const whenCalled = eq * (pot + 2 * bet) - bet
  const ev = f * pot + c * whenCalled - rz * bet
  const { choices, correct } = makeChoices(signedBb(ev), [signedBb(f * pot + c * eq * (pot + 2 * bet)), signedBb(f * pot + c * (eq * (pot + bet) - bet) - rz * bet), signedBb(f * pot - rz * bet), signedBb(f * pot + c * whenCalled)])
  return {
    key: `semi-${pot}-${bet}-${f}-${rz}-${eq}`,
    prompt: (
      <>
        You semi-bluff <b>{bbs(bet)}</b> into <b>{bbs(pot)}</b>. Villain folds {pct0(f)}, calls {pct0(c)} and raises {pct0(rz)} (you fold to a raise). When called you have <b>{pct0(eq)}</b> equity and the hand checks down. EV of the bet?
      </>
    ),
    choices,
    correct,
    explanation: (
      <>
        Fold: {pct0(f)} × {bbs(pot)} = {signedBb(f * pot, 2)}. Call: {pct0(c)} × ({pct0(eq)} × {bbs(pot + 2 * bet)} − {bbs(bet)}) = {signedBb(c * whenCalled, 2)}. Raise: −{pct0(rz)} × {bbs(bet)} = {signedBb(-rz * bet, 2)}. Total <b>{signedBb(ev)}</b>.
      </>
    ),
  }
}
