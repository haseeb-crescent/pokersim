import { useState } from 'react'
import { Link } from 'react-router-dom'
import type { EquityResponse } from '../../../api/types'
import { Cards } from '../../../components/PlayingCard'
import { Term } from '../../../glossary/Term'
import { describeApiError, fetchEquity, type ApiProblem } from '../../lab/api'
import { CardInput, cardFieldProblem } from '../../lab/CardPicker'
import { Offline } from '../../lab/Offline'
import { cardStr, fmtCombos, isExactHand, parseCards, pick } from '../../lab/lib/poker'
import { C, Callout, Drill, Formula, Section, Slider, Stat, makeChoices, pct1, type Question } from '../kit'

/** Exact chance to hit at least one of `outs` with `cards` cards to come from `unseen` cards. */
function hitChance(outs: number, cards: 1 | 2, unseen: number): number {
  if (cards === 1) return outs / unseen
  return 1 - ((unseen - outs) * (unseen - 1 - outs)) / (unseen * (unseen - 1))
}

// Exact all-in equities computed by full enumeration (preflop: all 1,712,304 boards).
const MATCHUPS: { a: string; b: string; board: string; eqA: number; note: string }[] = [
  { a: 'AsAh', b: 'KdKc', board: '', eqA: 0.813, note: 'An overpair preflop is about a 4-to-1 favourite.' },
  { a: 'AsKd', b: 'QhQc', board: '', eqA: 0.428, note: 'Two overcards against a pair: the pair is a small favourite, not a coin flip.' },
  { a: 'AsKs', b: 'QhQc', board: '', eqA: 0.462, note: 'Being suited adds a few percent through flushes.' },
  { a: '2h2d', b: 'AsKc', board: '', eqA: 0.53, note: 'Even the smallest pair is a slight favourite over AK.' },
  { a: 'Ah5h', b: 'KsQd', board: 'Kh8h2c', eqA: 0.459, note: 'Flush draw plus an ace: about 12 outs twice.' },
  { a: 'JhTh', b: 'AsKd', board: 'Qh9h2c', eqA: 0.676, note: 'A 15-out combo draw that also wins by pairing its jack or ten is a big favourite over ace-high.' },
  { a: '9s8s', b: 'KdKc', board: 'Th7c2d', eqA: 0.342, note: 'An open-ender (8 outs) plus backdoors against an overpair.' },
  { a: 'AsQd', b: 'QcJc', board: 'Qh7c3s', eqA: 0.827, note: 'Top pair, better kicker: the dominated hand has about 3 outs plus backdoors.' },
]

export default function Equity() {
  return (
    <>
      <p className="learn-lede">
        <Term id="equity">Equity</Term> is your share of the pot if all the remaining cards were dealt and nobody bet again. It is the number you compare against <Term id="pot-odds">pot odds</Term>.
      </p>

      <Section title="The idea">
        <p>
          Run the hand out every possible way and count: equity = P(win) + your share of ties. A two-way tie is worth half the pot, so it counts half.
        </p>
        <Formula label="Definition">equity = P(win) + P(two-way tie) / 2 + P(three-way tie) / 3 + …</Formula>
        <p>
          Example: all-in preflop with <C>AsAh</C> against <C>KdKc</C>, the aces win on 81.1% of the 1,712,304 possible boards and tie on 0.4%, so their equity is 81.1% + 0.4% / 2 ≈ <b>81.3%</b>. In a 200bb pot that is worth 0.813 × 200 ≈ 163bb on average.
        </p>
        <h3>Counting outs</h3>
        <p>
          With a draw, count your <Term id="outs">outs</Term>: unseen cards that make your hand best. After the flop there are 47 unseen cards (52 minus your 2 and the board's 3; the opponents' cards count as unseen because you can't see them), after the turn 46.
        </p>
        <Formula label="Chance to hit">
          one card to come: outs / unseen
          <br />
          two cards to come (flop → river): 1 − (47 − outs)(46 − outs) / (47 × 46)
        </Formula>
        <p>
          A <Term id="flush-draw">flush draw</Term> on the flop has 9 outs: 9/47 = 19.1% on the turn, and 1 − (38 × 37)/(47 × 46) = 1 − 1406/2162 = <b>35.0%</b> by the river. The <Term id="rule-of-2-and-4">rule of 2 and 4</Term> (outs × 4 = 36%) is close for small draws but overstates big ones: 15 outs × 4 = 60%, against an exact 54.1%.
        </p>
        <h3>Against a range</h3>
        <p>
          You rarely know the exact hand. Equity against a <Term id="range">range</Term> is the average of your equity against each combo in it, weighted by how many combos remain after <Term id="blockers">card removal</Term>. With <C>AsKd</C> against a range of QQ (6 combos, you have about 43% against each) and 22 (6 combos, about 47%): (6 × 43% + 6 × 47%) / 12 = <b>45%</b>. If the range also had AA, your ace would leave only 3 of its 6 combos.
        </p>
        <Callout title="Equity is not EV.">
          Equity assumes the cards are simply dealt out. Real hands keep betting, so position, initiative and playability decide how much of it you actually win: your <Term id="equity-realization">equity realization</Term>. A 40% hand out of position may realize much less than 40% of the pot.
        </Callout>
      </Section>

      <Section title="Try it: outs to percentages" kind="try">
        <OutsWidget />
      </Section>

      <Section title="Try it: hand and range equity (server)" kind="try">
        <ServerEquityWidget />
      </Section>

      <Section title="Equity drill" kind="drill">
        <Drill slug="equity" generate={genEquity} />
      </Section>
    </>
  )
}

function OutsWidget() {
  const [outs, setOuts] = useState(9)
  const [street, setStreet] = useState<'flop' | 'turn'>('flop')
  const exactTurn = hitChance(outs, 1, 47)
  const exactRiver = street === 'flop' ? hitChance(outs, 2, 47) : hitChance(outs, 1, 46)
  const rule = street === 'flop' ? outs * 4 : outs * 2
  return (
    <div className="widget widget-grid">
      <div className="widget-controls">
        <Slider label="Outs" value={outs} onChange={(v) => setOuts(Math.max(0, Math.min(20, Math.round(v))))} min={0} max={20} />
        <div className="seg">
          <button className={street === 'flop' ? 'on' : ''} onClick={() => setStreet('flop')}>
            On the flop (2 cards to come)
          </button>
          <button className={street === 'turn' ? 'on' : ''} onClick={() => setStreet('turn')}>
            On the turn (1 card)
          </button>
        </div>
        <p className="note">Common draws: gutshot 4, open-ender 8, flush draw 9, flush draw + gutshot 12, flush draw + open-ender 15.</p>
      </div>
      <div className="widget-out">
        <div className="stats">
          {street === 'flop' && <Stat label="Hit on the turn" value={pct1(exactTurn)} sub={`${outs} / 47`} />}
          <Stat label={street === 'flop' ? 'Hit by the river (exact)' : 'Hit on the river (exact)'} value={pct1(exactRiver)} tone="accent" sub={street === 'flop' ? `1 − ${47 - outs}·${46 - outs} / 2162` : `${outs} / 46`} />
          <Stat label={`Rule of ${street === 'flop' ? 4 : 2}`} value={`${rule}%`} sub={`error ${(rule - 100 * exactRiver).toFixed(1)} pts`} />
        </div>
        <table className="mini-table">
          <thead>
            <tr>
              <th>Outs</th>
              <th>Flop → turn</th>
              <th>Turn → river</th>
              <th>Flop → river</th>
            </tr>
          </thead>
          <tbody>
            {[4, 6, 8, 9, 12, 15].map((o) => (
              <tr key={o} style={o === outs ? { color: 'var(--accent)' } : undefined}>
                <td className="mono">{o}</td>
                <td className="mono">{pct1(hitChance(o, 1, 47))}</td>
                <td className="mono">{pct1(hitChance(o, 1, 46))}</td>
                <td className="mono">{pct1(hitChance(o, 2, 47))}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  )
}

function ServerEquityWidget() {
  const [a, setA] = useState('AsKd')
  const [b, setB] = useState('QQ, 22')
  const [board, setBoard] = useState('')
  const [res, setRes] = useState<{ res: EquityResponse; a: string; b: string; board: string } | null>(null)
  const [problem, setProblem] = useState<ApiProblem | null>(null)
  const [busy, setBusy] = useState(false)
  const boardProblem = cardFieldProblem(board, 5, [0, 3, 4, 5])
  const run = async (pa = a, pb = b, bd = board) => {
    setBusy(true)
    setProblem(null)
    try {
      const boardText = parseCards(bd).cards.map(cardStr).join('')
      const r = await fetchEquity({ players: [pa.trim(), pb.trim()], board: boardText, dead: '' })
      setRes({ res: r, a: pa, b: pb, board: boardText })
    } catch (e) {
      setProblem(describeApiError(e))
    } finally {
      setBusy(false)
    }
  }
  const presets = [
    { label: 'AKo vs {QQ, 22}', a: 'AsKd', b: 'QQ, 22', board: '' },
    ...MATCHUPS.slice(0, 5).map((m) => ({ label: `${m.a} vs ${m.b}${m.board ? ' on ' + m.board : ''}`, a: m.a, b: m.b, board: m.board })),
    { label: 'Top pair vs a calling range', a: 'AhKd', b: 'KQ, KJ, QJ, JT, T9s, 88-22', board: 'Kc8h3s' },
  ]
  return (
    <div className="widget col">
      <div className="btnrow">
        {presets.map((p) => (
          <button
            key={p.label}
            onClick={() => {
              setA(p.a)
              setB(p.b)
              setBoard(p.board)
              run(p.a, p.b, p.board)
            }}
          >
            {p.label}
          </button>
        ))}
      </div>
      <form
        className="col"
        onSubmit={(e) => {
          e.preventDefault()
          run()
        }}
      >
        <div className="field-row">
          <div className="field">
            <label>Hand or range 1</label>
            <input className="mono" value={a} onChange={(e) => setA(e.target.value)} spellCheck={false} />
          </div>
          <div className="field">
            <label>Hand or range 2</label>
            <input className="mono" value={b} onChange={(e) => setB(e.target.value)} spellCheck={false} />
          </div>
        </div>
        <CardInput label="Board (optional)" value={board} onChange={setBoard} max={5} allowed={[0, 3, 4, 5]} placeholder="e.g. Kh8h2c" />
        <div className="row">
          <button className="primary" type="submit" disabled={busy || !a.trim() || !b.trim() || !!boardProblem}>
            {busy ? 'Calculating…' : 'Calculate'}
          </button>
          <Link to="/lab/equity" className="note">
            Full calculator with up to 6 players in the Lab →
          </Link>
        </div>
      </form>
      {problem && <Offline problem={problem} onRetry={() => run()} />}
      {res && (
        <div className="col" style={{ gap: 6 }}>
          {res.board && (
            <div className="row">
              <span className="note">Board</span> <Cards cards={res.board.match(/../g) ?? []} size="sm" />
            </div>
          )}
          {[res.a, res.b].map((p, i) => (
            <div key={i} className="row" style={{ gap: 12 }}>
              <span style={{ width: 200 }} className="mono">
                {isExactHand(p) ? <Cards cards={parseCards(p).cards.map(cardStr)} size="sm" /> : p}
              </span>
              <div className="grow" style={{ height: 16, background: 'var(--bg-2)', borderRadius: 4, overflow: 'hidden' }}>
                <div style={{ width: `${100 * res.res.equity[i]}%`, height: '100%', background: i ? '#f59e0b' : 'var(--accent-2)', borderRadius: '0 4px 4px 0' }} />
              </div>
              <b className="mono" style={{ width: 64, textAlign: 'right' }}>
                {pct1(res.res.equity[i])}
              </b>
              <span className="note" style={{ width: 90 }}>
                {fmtCombos(res.res.combos[i])} combos
              </span>
            </div>
          ))}
          <span className="note">
            {res.res.exact ? 'Exact enumeration' : `Monte Carlo, ${res.res.samples.toLocaleString()} samples`} · {res.res.ms.toFixed(0)} ms
          </span>
        </div>
      )}
    </div>
  )
}

const OUT_SPOTS: { hand: string; board: string; outs: number; what: string; why: string }[] = [
  { hand: 'Ah5h', board: 'Jh8h2c', outs: 9, what: 'a flush', why: '13 hearts minus the 4 you can see = 9.' },
  { hand: '9s8s', board: 'Th7c2d', outs: 8, what: 'a straight', why: 'Any jack (J-T-9-8-7) or six (T-9-8-7-6): 4 + 4 = 8.' },
  { hand: '9s8s', board: 'QhTc2d', outs: 4, what: 'a straight', why: 'Only a jack fills Q-J-T-9-8: 4 outs (a gutshot).' },
  { hand: 'JhTh', board: 'Qh9h2c', outs: 15, what: 'a flush or a straight', why: '9 hearts, plus the 3 non-heart kings and 3 non-heart eights for the straight: 9 + 6 = 15.' },
  { hand: '9h8h', board: 'QhTh3c', outs: 12, what: 'a flush or a straight', why: '9 hearts plus the 3 non-heart jacks (Q-J-T-9-8): 12.' },
  { hand: 'AsKd', board: '9h6c2s', outs: 6, what: 'top pair (assume a pair of aces or kings wins)', why: '3 aces + 3 kings = 6.' },
  { hand: '7s7d', board: 'Kh7c2s', outs: 7, what: 'a full house or quads on the turn', why: 'Pair the king (3) or the deuce (3), or the last seven (1): 7.' },
  { hand: '5s5d', board: 'Kh9c2s', outs: 2, what: 'a set', why: 'Only two fives are left.' },
]

function genEquity(): Question {
  const r = Math.random()
  if (r < 0.4) {
    const s = pick(OUT_SPOTS)
    const { choices, correct } = makeChoices(String(s.outs), [s.outs + 1, s.outs - 1, s.outs + 3, s.outs * 2, s.outs - 2].filter((x) => x > 0).map(String))
    return {
      key: `outs-${s.hand}-${s.board}`,
      prompt: (
        <>
          You hold <Cards cards={s.hand.match(/../g)!} size="sm" /> on <Cards cards={s.board.match(/../g)!} size="sm" />. How many outs to make {s.what}?
        </>
      ),
      choices,
      correct,
      explanation: <>{s.why}</>,
    }
  }
  if (r < 0.7) {
    const outs = pick([4, 6, 8, 9, 12, 15])
    const flop = Math.random() < 0.6
    const exact = flop ? hitChance(outs, 2, 47) : hitChance(outs, 1, 46)
    const wrong = flop ? [hitChance(outs, 1, 47), outs / 52, Math.min(0.99, (outs * 4.6) / 100), 1 - Math.pow(1 - outs / 47, 3)] : [hitChance(outs, 2, 47), outs / 52, outs / 37]
    const { choices, correct } = makeChoices(pct1(exact), wrong.map(pct1))
    return {
      key: `hit-${outs}-${flop}`,
      prompt: (
        <>
          You have <b>{outs} outs</b> {flop ? 'on the flop. What is the exact chance to hit by the river (both cards)?' : 'on the turn. What is the exact chance to hit on the river?'}
        </>
      ),
      choices,
      correct,
      explanation: flop ? (
        <>
          Miss twice: ({47 - outs}/47) × ({46 - outs}/46) = {pct1(1 - exact)}. Hit at least once: 1 − {pct1(1 - exact)} = <b>{pct1(exact)}</b>. Rule of 4 says {outs * 4}%.
        </>
      ) : (
        <>
          46 unseen cards, {outs} of them help: {outs}/46 = <b>{pct1(exact)}</b>. Rule of 2 says {outs * 2}%.
        </>
      ),
    }
  }
  const m = pick(MATCHUPS)
  const right = Math.round(100 * m.eqA)
  const offsets = [-24, -15, -9, 9, 15, 24].filter((o) => right + o > 2 && right + o < 98)
  const wrongs = offsets.sort(() => Math.random() - 0.5).slice(0, 3).map((o) => `${right + o}%`)
  const { choices, correct } = makeChoices(`${right}%`, wrongs)
  return {
    key: `mu-${m.a}-${m.b}`,
    prompt: (
      <>
        All-in{m.board ? ' on ' : ' preflop'}
        {m.board && <Cards cards={m.board.match(/../g)!} size="sm" />}: <Cards cards={m.a.match(/../g)!} size="sm" /> vs <Cards cards={m.b.match(/../g)!} size="sm" />. About how much equity does the first hand have?
      </>
    ),
    choices,
    correct,
    explanation: (
      <>
        Exact enumeration gives <b>{pct1(m.eqA)}</b> against {pct1(1 - m.eqA)} (ties split). {m.note}
      </>
    ),
  }
}
