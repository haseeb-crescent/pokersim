import { useMemo, useState } from 'react'
import { Cards } from '../../../components/PlayingCard'
import { RangeGrid } from '../../../components/RangeGrid'
import { Term } from '../../../glossary/Term'
import { CardInput } from '../../lab/CardPicker'
import { BUCKETS, CLASS_COMBOS, COMBOS, RANK_CHARS, SUIT_NAMES, blocked, cardStr, choose2, classify, comboStr, deal, parseCards, parseRange, pick, rankOf, type Bucket } from '../../lab/lib/poker'
import { C, Callout, Drill, Formula, Section, Stat, type Question } from '../kit'

const NAMES = ['deuces', 'threes', 'fours', 'fives', 'sixes', 'sevens', 'eights', 'nines', 'tens', 'jacks', 'queens', 'kings', 'aces']

export default function Blockers() {
  return (
    <>
      <p className="learn-lede">
        Every card you can see is a card your opponent cannot hold. <Term id="blockers">Blockers</Term> (card removal) change how many <Term id="combos">combos</Term> of each hand are left, and good players use that to choose bluffs and bluff-catchers.
      </p>

      <Section title="Counting with card removal">
        <p>Count how many cards of each rank are still unseen (4 minus those in your hand and on the board), then:</p>
        <Formula label="Live combos">
          pair XX: C(n, 2) = n(n − 1)/2 &nbsp;→&nbsp; n = 4: 6, n = 3: 3, n = 2: 1, n ≤ 1: 0
          <br />
          XY (any suits): n<sub>X</sub> × n<sub>Y</sub> &nbsp;·&nbsp; XYs: suits where both X and Y are unseen &nbsp;·&nbsp; XYo: the rest
        </Formula>
        <p>
          Worked example: you hold <C>AsKh</C> and the board is <C>Ad7c2c</C>.
        </p>
        <ul>
          <li>
            AA: two aces unseen (<C>Ah</C>, <C>Ac</C>), so C(2, 2) = <b>1 combo</b> instead of 6.
          </li>
          <li>
            AK: 2 aces × 3 kings (<C>Ks</C>, <C>Kd</C>, <C>Kc</C>) = <b>6 combos</b> instead of 16. Suited: only <C>AcKc</C> (<C>AhKh</C> needs your king), so 1 suited and 5 offsuit.
          </li>
          <li>
            Sets: AA 1 + 77 C(3, 2) = 3 + 22 3 = <b>7 combos</b>.
          </li>
        </ul>
        <h3>Using blockers</h3>
        <ul>
          <li>
            <b>Bluff with hands that block the opponent's calls.</b> On <C>Ks9s4d2s</C>, a bluff holding the <C>As</C> means the opponent never has the nut flush, so they fold more of their range.
          </li>
          <li>
            <b>Prefer bluffs that don't block their folds.</b> Holding cards from the hands they would fold anyway (missed draws) leaves them with more strong hands, which is bad for a bluff.
          </li>
          <li>
            <b>Bluff-catch with hands that block value and unblock bluffs.</b> Holding a card from their likely value hands means they have fewer of them, so a larger share of their bets are bluffs.
          </li>
        </ul>
        <Callout title="The bunching effect.">
          Folds change the deck too. Players who fold early tend to fold low cards, so the players left to act are slightly more likely to hold high cards than a random deal suggests. The <Term id="bunching">bunching effect</Term> is small, but it is real card removal by inference.
        </Callout>
      </Section>

      <Section title="Try it: blocker counter" kind="try">
        <BlockerWidget />
      </Section>

      <Section title="Blocker drill" kind="drill">
        <Drill slug="blockers" generate={genBlockers} intro="Type the exact number of combos left." />
      </Section>
    </>
  )
}

function BlockerWidget() {
  const [hand, setHand] = useState('AsKh')
  const [board, setBoard] = useState('Ad7c2c')
  const [mode, setMode] = useState<'range' | 'made'>('range')
  const [target, setTarget] = useState('AA, AK, 77, 22')
  const [bucket, setBucket] = useState<Bucket>('Set / trips')
  const h = parseCards(hand)
  const b = parseCards(board)
  const heroCards = h.error ? [] : h.cards.slice(0, 2)
  const boardCards = b.error ? [] : b.cards.slice(0, 5)
  const heroSet = new Set(heroCards)
  const boardSet = new Set(boardCards)
  const dead = new Set([...heroCards, ...boardCards])
  const range = useMemo(() => parseRange(target), [target])
  const deadKey = [...dead].join(',')

  const result = useMemo(() => {
    const live: number[] = []
    const byHero: number[] = []
    const byBoard: number[] = []
    let total = 0
    for (let i = 0; i < 1326; i++) {
      if (mode === 'range') {
        // any positive weight counts as "in the range"; combos are counted whole
        if (range.error || !(range.weights[i] > 0)) continue
      } else {
        // a made hand needs a board, and combos using a board card don't exist
        if (boardCards.length < 3 || blocked(i, boardSet)) continue
        if (classify(COMBOS[i], boardCards) !== bucket) continue
      }
      total++
      if (blocked(i, boardSet)) byBoard.push(i)
      else if (blocked(i, heroSet)) byHero.push(i)
      else live.push(i)
    }
    return { live, byHero, byBoard, total }
  }, [mode, range, bucket, deadKey]) // eslint-disable-line react-hooks/exhaustive-deps

  const heat = useMemo(() => CLASS_COMBOS.map((cs) => cs.filter((i) => !blocked(i, dead)).length / cs.length), [deadKey]) // eslint-disable-line react-hooks/exhaustive-deps

  return (
    <div className="widget col">
      <div className="field-row">
        <CardInput label="Your hand" value={hand} onChange={setHand} max={2} allowed={[0, 2]} used={boardSet} />
        <CardInput label="Board" value={board} onChange={setBoard} max={5} allowed={[0, 3, 4, 5]} used={heroSet} />
      </div>
      <div className="row wrap">
        <div className="seg">
          <button className={mode === 'range' ? 'on' : ''} onClick={() => setMode('range')}>
            Hands by name
          </button>
          <button className={mode === 'made' ? 'on' : ''} onClick={() => setMode('made')}>
            Made hands on this board
          </button>
        </div>
        {mode === 'range' ? (
          <input className={'mono grow' + (range.error ? ' invalid' : '')} value={target} onChange={(e) => setTarget(e.target.value)} placeholder="e.g. AA, AK, KQs" spellCheck={false} style={{ minWidth: 220 }} />
        ) : (
          <select value={bucket} onChange={(e) => setBucket(e.target.value as Bucket)}>
            {BUCKETS.map((x) => (
              <option key={x}>{x}</option>
            ))}
          </select>
        )}
      </div>
      {mode === 'range' && range.error && <span className="err">{range.error}</span>}
      {mode === 'made' && boardCards.length < 3 && <span className="muted">Enter at least a flop to count made hands.</span>}
      <div className="widget-grid">
        <div className="widget-out">
          <div className="stats">
            <Stat label={mode === 'range' ? 'Combos with no cards seen' : 'Combos that use no board card'} value={result.total} />
            {mode === 'range' && <Stat label="Blocked by the board" value={result.byBoard.length} />}
            <Stat label="Blocked by your hand" value={result.byHero.length} />
            <Stat label="Villain can hold" value={result.live.length} tone="accent" />
          </div>
          {result.live.length + result.byHero.length > 0 && result.live.length + result.byHero.length <= 120 && (
            <div className="combo-list">
              {result.live.map((i) => (
                <Cards key={i} cards={comboStr(i).match(/../g)!} size="sm" />
              ))}
              {result.byHero.map((i) => (
                <span key={i} className="blocked" title="blocked by your hand">
                  <Cards cards={comboStr(i).match(/../g)!} size="sm" />
                </span>
              ))}
            </div>
          )}
          <p className="note" style={{ margin: 0 }}>
            Faded combos are the ones your hand blocks.
          </p>
        </div>
        <div className="col" style={{ gap: 6 }}>
          <span className="note">Share of each class still possible for villain</span>
          <RangeGrid weights={heat} size={22} showLabels />
        </div>
      </div>
    </div>
  )
}

function unseen(rank: number, seen: number[]) {
  return 4 - seen.filter((c) => rankOf(c) === rank).length
}

function seenList(seen: number[], rank: number) {
  const s = seen.filter((c) => rankOf(c) === rank)
  return s.length ? s.map(cardStr).join(', ') : 'none'
}

function genBlockers(): Question {
  const r = Math.random()
  // an unpaired flop for the set question; any flop otherwise
  let hero: number[]
  let board: number[]
  do {
    hero = deal(2).sort((a, b) => b - a)
    board = deal(3, hero)
  } while (r >= 0.7 && new Set(board.map(rankOf)).size < 3)
  const seen = [...hero, ...board]
  const seenRanks = [...new Set(seen.map(rankOf))]
  const rankPick = () => (Math.random() < 0.75 ? pick(seenRanks) : Math.floor(Math.random() * 13))
  const show = (
    <>
      You hold <Cards cards={hero.map(cardStr)} size="sm" /> and the board is <Cards cards={board.map(cardStr)} size="sm" />.
    </>
  )
  if (r < 0.35) {
    const x = rankPick()
    const n = unseen(x, seen)
    const ans = choose2(n)
    const name = RANK_CHARS[x] + RANK_CHARS[x]
    return {
      key: `p-${name}-${seen.join()}`,
      prompt: (
        <>
          {show} How many combos of <b>{name}</b> can villain hold?
        </>
      ),
      numeric: { answer: ans, tolerance: 0 },
      explanation: (
        <>
          Seen {NAMES[x]}: {seenList(seen, x)}. Unseen: {n}, so C({n}, 2) = <b>{ans}</b>.
        </>
      ),
    }
  }
  if (r < 0.7) {
    let x = rankPick()
    let y = rankPick()
    while (y === x) y = Math.floor(Math.random() * 13)
    if (y > x) [x, y] = [y, x]
    const suitedOnly = Math.random() < 0.35
    const nx = unseen(x, seen)
    const ny = unseen(y, seen)
    const suits = [0, 1, 2, 3].filter((s) => !seen.includes(x * 4 + s) && !seen.includes(y * 4 + s))
    const ans = suitedOnly ? suits.length : nx * ny
    const name = RANK_CHARS[x] + RANK_CHARS[y] + (suitedOnly ? 's' : '')
    return {
      key: `u-${name}-${seen.join()}`,
      prompt: (
        <>
          {show} How many combos of <b>{name}</b>
          {suitedOnly ? '' : ' (suited and offsuit)'} can villain hold?
        </>
      ),
      numeric: { answer: ans, tolerance: 0 },
      explanation: suitedOnly ? (
        <>
          A suited combo needs both cards of one suit unseen. Suits that still work: {suits.length ? suits.map((s) => SUIT_NAMES[s]).join(', ') : 'none'}. So <b>{ans}</b>.
        </>
      ) : (
        <>
          Unseen {NAMES[x]}: {nx} (seen: {seenList(seen, x)}). Unseen {NAMES[y]}: {ny} (seen: {seenList(seen, y)}). {nx} × {ny} = <b>{ans}</b>.
        </>
      ),
    }
  }
  const parts = board.map((c) => {
    const rk = rankOf(c)
    const n = unseen(rk, seen)
    return { rk, n, c: choose2(n) }
  })
  const ans = parts.reduce((s, p) => s + p.c, 0)
  return {
    key: `s-${seen.join()}`,
    prompt: <>{show} How many set combos (a pocket pair matching one board card) can villain hold?</>,
    numeric: { answer: ans, tolerance: 0 },
    explanation: (
      <>
        {parts.map((p, i) => (
          <span key={i}>
            {i ? ' + ' : ''}
            {RANK_CHARS[p.rk]}
            {RANK_CHARS[p.rk]}: C({p.n}, 2) = {p.c}
          </span>
        ))}{' '}
        = <b>{ans}</b>. Each board card leaves 3 of its rank, and any you hold cut that further.
      </>
    ),
  }
}
