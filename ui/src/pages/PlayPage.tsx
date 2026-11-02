import { useEffect, useMemo, useRef, useState } from 'react'
import { getJson, postJson, bb } from '../api/client'
import type { BotProfile, CoachMode, SeatView, TableState } from '../api/types'
import { useTable } from '../api/useTable'
import { Cards, CardText, PlayingCard } from '../components/PlayingCard'
import { GradeCard } from '../components/GradeCard'
import { Term } from '../components/Term'
import './play.css'

const DEFAULT_LINEUP = ['gto', 'fish', 'nit', 'lag', 'station']

function Setup({ onStarted }: { onStarted: () => void }) {
  const [profiles, setProfiles] = useState<BotProfile[]>([])
  const [lineup, setLineup] = useState<string[]>(() => {
    try {
      const s = localStorage.getItem('lineup')
      if (s) return JSON.parse(s)
    } catch {
      /* ignore */
    }
    return DEFAULT_LINEUP
  })
  const [coach, setCoach] = useState<CoachMode>('decision')
  const [delay, setDelay] = useState(700)
  const [rebuy, setRebuy] = useState<'reset' | 'topup' | 'off'>('reset')
  const [rake, setRake] = useState(5)
  const [cap, setCap] = useState(3)
  const [spectate, setSpectate] = useState(false)
  const [err, setErr] = useState<string | null>(null)

  useEffect(() => {
    getJson<BotProfile[]>('/api/profiles').then(setProfiles).catch((e) => setErr(String(e)))
  }, [])

  const start = async () => {
    try {
      localStorage.setItem('lineup', JSON.stringify(lineup))
    } catch {
      /* ignore */
    }
    try {
      await postJson('/api/session/start', {
        lineup,
        coach_mode: coach,
        bot_delay_ms: delay,
        rebuy,
        rake_pct: rake / 100,
        rake_cap_bb: cap,
        hero_profile: spectate ? 'gto' : null,
      })
      onStarted()
    } catch (e) {
      setErr(String(e))
    }
  }

  return (
    <div className="page setup">
      <h1>New session</h1>
      <p className="muted">
        6-max no-limit hold'em, 100bb. You sit in seat 1; pick the five opponents. Every decision you make is graded against the solver and saved to the logbook.
      </p>
      {err && <p className="pill-bad">{err}</p>}
      <div className="panel col">
        <h3>Lineup</h3>
        {lineup.map((id, i) => {
          const p = profiles.find((x) => x.id === id)
          return (
            <div key={i} className="row lineup-row">
              <span className="faint">Seat {i + 2}</span>
              <select value={id} onChange={(e) => setLineup(lineup.map((x, j) => (j === i ? e.target.value : x)))}>
                {profiles.map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.name}
                  </option>
                ))}
              </select>
              <span className="muted small grow">{p?.description}</span>
            </div>
          )
        })}
      </div>
      <div className="grid-2">
        <div className="panel col">
          <h3>Coach</h3>
          {(['decision', 'hand', 'off'] as CoachMode[]).map((m) => (
            <label key={m} className="row radio">
              <input type="radio" checked={coach === m} onChange={() => setCoach(m)} />
              {m === 'decision' ? 'Grade after each decision' : m === 'hand' ? 'Grade after each hand' : 'Off (review later in the logbook)'}
            </label>
          ))}
          <label>
            Bot speed: {delay} ms per action
            <input type="range" min={0} max={2500} step={50} value={delay} onChange={(e) => setDelay(+e.target.value)} />
          </label>
        </div>
        <div className="panel col">
          <h3>Table</h3>
          <label>
            Rebuy
            <select value={rebuy} onChange={(e) => setRebuy(e.target.value as typeof rebuy)}>
              <option value="reset">Reset to 100bb every hand (charts and cache match exactly)</option>
              <option value="topup">Top up to 100bb when below</option>
              <option value="off">No top-up (rebuy only when busted)</option>
            </select>
          </label>
          <div className="row">
            <label>
              <Term id="rake">Rake</Term> %
              <input type="number" min={0} max={10} step={0.5} value={rake} onChange={(e) => setRake(+e.target.value)} style={{ width: 70 }} />
            </label>
            <label>
              cap (bb)
              <input type="number" min={0} max={10} step={0.5} value={cap} onChange={(e) => setCap(+e.target.value)} style={{ width: 70 }} />
            </label>
          </div>
          <label className="row radio">
            <input type="checkbox" checked={spectate} onChange={(e) => setSpectate(e.target.checked)} />
            Bot-vs-bot: a GTO bot plays seat 1 and you watch
          </label>
          <p className="faint small">The preflop charts and the solver caches assume 5% rake capped at 3bb.</p>
        </div>
      </div>
      <div className="row">
        <button className="primary" onClick={start} disabled={lineup.length !== 5}>
          Start session
        </button>
      </div>
    </div>
  )
}

// seat positions around the oval, hero (seat 0) bottom centre, clockwise
const SEAT_POS = [
  { left: '50%', top: '93%' },
  { left: '9%', top: '74%' },
  { left: '9%', top: '24%' },
  { left: '50%', top: '5%' },
  { left: '91%', top: '24%' },
  { left: '91%', top: '74%' },
]
const BET_POS = [
  { left: '50%', top: '72%' },
  { left: '27%', top: '64%' },
  { left: '27%', top: '35%' },
  { left: '62%', top: '27%' },
  { left: '73%', top: '35%' },
  { left: '73%', top: '64%' },
]

function Seat({ s, isButton, toAct, heroSeat }: { s: SeatView; isButton: boolean; toAct: boolean; heroSeat: number }) {
  const pos = SEAT_POS[(s.seat - heroSeat + 6) % 6]
  return (
    <div className={'seat' + (s.folded ? ' folded' : '') + (toAct ? ' to-act' : '')} style={pos}>
      <div className="seat-cards">
        {s.cards ? <Cards cards={s.cards} size="md" /> : s.in_hand && !s.folded ? <><PlayingCard card={null} /><PlayingCard card={null} /></> : null}
      </div>
      <div className="seat-box">
        <div className="seat-name">
          <span className="pos">{s.position}</span> {s.name}
          {isButton && <span className="dealer">D</span>}
        </div>
        <div className="seat-stack">{bb(s.stack)}{s.all_in && <span className="allin"> all-in</span>}</div>
        {s.thinking ? <div className="seat-act thinking">{s.thinking}</div> : s.last_action && <div className="seat-act">{s.last_action}</div>}
      </div>
    </div>
  )
}

function ActionBar({ t, send }: { t: TableState; send: ReturnType<typeof useTable>['send'] }) {
  const legal = t.legal!
  const [amount, setAmount] = useState<number>(legal.min_to ?? 0)
  const input = useRef<HTMLInputElement>(null)
  useEffect(() => {
    setAmount(legal.min_to ?? 0)
  }, [legal.min_to, t.hand_id, t.street])

  const fold = () => send({ type: 'action', action: { type: 'Fold' } })
  const checkCall = () => send({ type: 'action', action: legal.can_check ? { type: 'Check' } : { type: 'Call' } })
  const bet = (to: number) => send({ type: 'action', action: { type: 'BetTo', to } })

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (document.activeElement === input.current) {
        if (e.key === 'Enter') {
          e.preventDefault()
          if (legal.min_to !== null) bet(Math.round(amount))
        }
        if (e.key === 'Escape') input.current?.blur()
        return
      }
      if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement || e.target instanceof HTMLSelectElement) return
      const k = e.key.toLowerCase()
      if (k === 'f') fold()
      else if (k === 'c' || k === 'k' || k === ' ') {
        e.preventDefault()
        checkCall()
      } else if (k === 'r' || k === 'b') {
        e.preventDefault()
        input.current?.focus()
        input.current?.select()
      } else if (k === 'a' && legal.max_to !== null) bet(legal.max_to)
      else {
        const p = t.presets.find((x) => x.key === k)
        if (p) bet(p.to)
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  })

  const facing = !legal.can_check
  return (
    <div className="action-bar">
      <button className="act fold" onClick={fold}>
        Fold <span className="kbd">F</span>
      </button>
      <button className="act call" onClick={checkCall}>
        {legal.can_check ? 'Check' : `Call ${bb(legal.call_amount)}${legal.call_is_all_in ? ' (all-in)' : ''}`} <span className="kbd">C</span>
      </button>
      {legal.min_to !== null && legal.max_to !== null && (
        <>
          <div className="presets">
            {t.presets.map((p) => (
              <button key={p.key} onClick={() => bet(p.to)} title={bb(p.to)}>
                {p.label} <span className="kbd">{p.key}</span>
              </button>
            ))}
          </div>
          <div className="custom">
            <input
              type="range"
              min={legal.min_to}
              max={legal.max_to}
              step={5}
              value={amount}
              onChange={(e) => setAmount(+e.target.value)}
            />
            <input
              ref={input}
              type="number"
              step={0.5}
              value={(amount / 100).toFixed(1)}
              onChange={(e) => setAmount(Math.round(+e.target.value * 100))}
              className="amount"
            />
            <button className="act raise" onClick={() => bet(Math.round(amount))}>
              {facing || t.street === 'preflop' ? 'Raise to' : 'Bet'} {bb(amount)} <span className="kbd">R</span>
            </button>
          </div>
        </>
      )}
    </div>
  )
}

export default function PlayPage() {
  const { state: t, grades, lastHand, error, connected, send } = useTable()
  const [showSetup, setShowSetup] = useState(false)
  const handGrades = useMemo(() => [...grades].reverse(), [grades])

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement || e.target instanceof HTMLSelectElement) return
      if (e.key.toLowerCase() === 'n') send({ type: 'next_hand' })
      if (e.key.toLowerCase() === 'p' && t) send({ type: 'pause', paused: !t.paused })
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [send, t])

  if (!connected && !t) return <div className="page muted">Connecting to the server…</div>
  if (showSetup || !t || !t.running || t.street === 'idle' && !t.seats.length) {
    return <Setup onStarted={() => setShowSetup(false)} />
  }
  const heroTurn = t.legal !== null && t.to_act === t.hero_seat
  const stop = async () => {
    await postJson('/api/session/stop', {})
    setShowSetup(true)
  }
  return (
    <div className="play">
      <div className="play-main">
        <div className="toolbar row">
          <span className="muted">
            Hand #{t.hand_no} · {t.hands_played} played · session{' '}
            <b className={t.hero_net >= 0 ? 'pill-good' : 'pill-bad'}>
              {t.hero_net >= 0 ? '+' : ''}
              {bb(t.hero_net)}
            </b>
          </span>
          <span className="grow" />
          <label className="row small">
            Speed
            <input type="range" min={0} max={2500} step={50} value={t.bot_delay_ms} onChange={(e) => send({ type: 'set_delay', ms: +e.target.value })} />
          </label>
          <select value={t.coach_mode} onChange={(e) => send({ type: 'set_coach', mode: e.target.value as CoachMode })}>
            <option value="decision">Grade each decision</option>
            <option value="hand">Grade each hand</option>
            <option value="off">Coach off</option>
          </select>
          <button onClick={() => send({ type: 'pause', paused: !t.paused })}>
            {t.paused ? 'Resume' : 'Pause'} <span className="kbd">P</span>
          </button>
          <button onClick={() => send({ type: 'next_hand' })}>
            Next hand <span className="kbd">N</span>
          </button>
          <button className="ghost" onClick={stop}>
            End session
          </button>
        </div>
        <div className="felt-wrap">
          <div className="felt">
            <div className="board">
              <div className="pot">
                Pot {bb(t.pot)}
              </div>
              <div className="board-cards">
                {t.board.length ? <Cards cards={t.board} size="lg" /> : <span className="faint">{t.street === 'preflop' ? 'Preflop' : ''}</span>}
              </div>
            </div>
            {t.seats.map((s) => (
              <Seat key={s.seat} s={s} isButton={s.seat === t.button} toAct={t.to_act === s.seat} heroSeat={t.hero_seat} />
            ))}
            {t.seats
              .filter((s) => s.committed > 0)
              .map((s) => (
                <div key={'b' + s.seat} className="bet-chip" style={BET_POS[(s.seat - t.hero_seat + 6) % 6]}>
                  {bb(s.committed)}
                </div>
              ))}
          </div>
        </div>
        {error && <div className="toast pill-bad">{error}</div>}
        {heroTurn ? (
          <ActionBar t={t} send={send} />
        ) : (
          <div className="action-bar idle muted">
            {t.street === 'showdown' && lastHand ? (
              <span>
                {lastHand.hero_net > 0 ? <b className="pill-good">You won {bb(lastHand.hero_net)}</b> : lastHand.hero_net < 0 ? <b className="pill-bad">You lost {bb(-lastHand.hero_net)}</b> : 'Hand over'}
                {lastHand.showdown.map((s) => (
                  <span key={s.seat}>
                    {' '}
                    · {t.seats[s.seat]?.name}: <CardText text={s.cards.join('')} /> {s.description}
                  </span>
                ))}
                <span className="faint"> — press N for the next hand</span>
              </span>
            ) : (
              <span>Waiting for {t.to_act !== null ? t.seats[t.to_act]?.name : '…'}</span>
            )}
          </div>
        )}
        <div className="hand-log">
          {t.log.slice(-14).map((l, i) => (
            <div key={i}>
              <CardText text={l} />
            </div>
          ))}
        </div>
        <div className="faint small shortcuts">
          Keys: <span className="kbd">F</span> fold · <span className="kbd">C</span>/<span className="kbd">Space</span> check/call · <span className="kbd">1</span>–<span className="kbd">5</span> bet sizes · <span className="kbd">R</span> custom size then <span className="kbd">Enter</span> · <span className="kbd">A</span> all-in · <span className="kbd">N</span> next hand · <span className="kbd">P</span> pause
        </div>
      </div>
      <aside className="coach">
        <h3>Coach</h3>
        {t.coach_mode === 'off' && <p className="faint small">Coach is off; decisions are still graded and saved for review in the logbook.</p>}
        {handGrades.length === 0 && <p className="faint small">Your graded decisions appear here.</p>}
        {handGrades.map((g, i) => (
          <GradeCard key={`${g.hand_id}-${g.index}`} g={g} compact={i > 0} />
        ))}
      </aside>
    </div>
  )
}
