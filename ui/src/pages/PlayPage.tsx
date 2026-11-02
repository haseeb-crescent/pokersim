import { useEffect, useMemo, useRef, useState } from 'react'
import { getJson, postJson, bb, dollars } from '../api/client'
import type { BotProfile, CashInfo, CashSummary, CoachMode, SeatView, TableState } from '../api/types'
import { useTable } from '../api/useTable'
import { Cards, CardText, PlayingCard } from '../components/PlayingCard'
import { GradeCard } from '../components/GradeCard'
import { ResultsGraph } from '../components/ResultsGraph'
import { Term } from '../components/Term'
import './play.css'

const DEFAULT_LINEUP = ['gto', 'fish', 'nit', 'lag', 'station']
type Mode = 'training' | 'session'

function stored<T>(key: string, fallback: T): T {
  try {
    const s = localStorage.getItem(key)
    if (s) return JSON.parse(s) as T
  } catch {
    /* ignore */
  }
  return fallback
}

function store(key: string, v: unknown) {
  try {
    localStorage.setItem(key, JSON.stringify(v))
  } catch {
    /* ignore */
  }
}

const signed = (chips: number) => (chips > 0 ? '+' : '') + bb(chips)

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
  const [mode, setMode] = useState<Mode>(() => stored<Mode>('mode', 'training'))
  const [bbDollars, setBbDollars] = useState<number>(() => stored('bb_dollars', 1))
  const [dealPause, setDealPause] = useState<number>(() => stored('deal_pause_ms', 1500))
  const [cash, setCash] = useState<CashSummary | null>(null)
  const [rake, setRake] = useState(5)
  const [cap, setCap] = useState(3)
  const [spectate, setSpectate] = useState(false)
  const [err, setErr] = useState<string | null>(null)

  useEffect(() => {
    getJson<BotProfile[]>('/api/profiles').then(setProfiles).catch((e) => setErr(String(e)))
    getJson<CashSummary>('/api/cash/summary').then(setCash).catch(() => setCash(null))
  }, [])

  const open = cash?.open ?? null
  const start = async (resume: number | null = null) => {
    store('lineup', lineup)
    store('mode', mode)
    store('bb_dollars', bbDollars)
    store('deal_pause_ms', dealPause)
    try {
      await postJson('/api/session/start', {
        lineup,
        coach_mode: coach,
        bot_delay_ms: delay,
        rake_pct: rake / 100,
        rake_cap_bb: cap,
        hero_profile: spectate ? 'gto' : null,
        mode: resume !== null ? 'session' : spectate ? 'training' : mode,
        deal_pause_ms: dealPause,
        bb_dollars: bbDollars,
        resume,
      })
      onStarted()
    } catch (e) {
      setErr(String(e))
    }
  }
  const cashOut = async (id: number) => {
    if (!confirm(`Cash out session #${id}? It can no longer be resumed; its result stays in your lifetime totals.`)) return
    await postJson(`/api/cash/${id}/close`, {})
    getJson<CashSummary>('/api/cash/summary').then(setCash).catch(() => setCash(null))
  }
  const name = (id: string) => profiles.find((p) => p.id === id)?.name ?? id

  return (
    <div className="page setup">
      <h1>New session</h1>
      <p className="muted">
        6-max no-limit hold'em, 100bb buy-in. You sit in seat 1; pick the five opponents. Every decision you make is graded against the solver and saved to the logbook.
      </p>
      {err && <p className="pill-bad">{err}</p>}
      {open && (
        <div className="panel col resume">
          <h3>Open session #{open.id}</h3>
          <p className="muted">
            Stack {bb(open.stack)} · bought in for {bb(open.buyins)} ·{' '}
            <b className={open.profit >= 0 ? 'pill-good' : 'pill-bad'}>
              {signed(open.profit)} ({dollars(open.profit, open.bb_dollars ?? 1, true)})
            </b>{' '}
            over {open.hands} hands at ${(open.bb_dollars ?? 1).toFixed(2)}/bb · {(open.lineup ?? []).map(name).join(', ')}
          </p>
          <div className="row">
            <button className="primary" onClick={() => start(open.id)}>
              Resume session
            </button>
            <button onClick={() => cashOut(open.id)}>Cash out</button>
            <span className="faint small">Resuming keeps this session's lineup, stacks and stakes; the coach and speed settings below apply.</span>
          </div>
        </div>
      )}
      <div className="panel col">
        <h3>Mode</h3>
        <label className="row radio">
          <input type="radio" checked={mode === 'training'} onChange={() => setMode('training')} />
          <span>
            <b>Training</b> <span className="muted small">Every stack resets to 100bb each hand, so every spot matches the 100bb charts and solver library exactly.</span>
          </span>
        </label>
        <label className="row radio">
          <input type="radio" checked={mode === 'session'} onChange={() => setMode('session')} disabled={spectate} />
          <span>
            <b>Session</b>{' '}
            <span className="muted small">
              Stacks carry over from hand to hand and across restarts. Buy in for 100bb, rebuy when you bust, top up between hands. Bots rebuy when busted; the GTO and LAG regs (and the Hunter) top up to 100bb, the others let their stacks ride.
            </span>
          </span>
        </label>
        <div className="row">
          {mode === 'session' && !spectate && (
            <label>
              Big blind ($)
              <input type="number" min={0.01} step={0.01} value={bbDollars} onChange={(e) => setBbDollars(Math.max(0.01, +e.target.value))} style={{ width: 80 }} />
            </label>
          )}
          <label>
            Pause between hands: {(dealPause / 1000).toFixed(1)} s
            <input type="range" min={300} max={6000} step={100} value={dealPause} onChange={(e) => setDealPause(+e.target.value)} />
          </label>
        </div>
        {mode === 'session' && open && !spectate && <p className="faint small">Starting a new Session-mode session cashes out session #{open.id}.</p>}
      </div>
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
            Bot-vs-bot: a GTO bot plays seat 1 and you watch (Training mode)
          </label>
          <p className="faint small">The preflop charts and the solver caches assume 5% rake capped at 3bb.</p>
        </div>
      </div>
      <div className="row">
        <button className="primary" onClick={() => start()} disabled={lineup.length !== 5}>
          Start {spectate ? 'bot-vs-bot' : mode === 'session' ? 'Session' : 'Training'}
        </button>
      </div>
    </div>
  )
}

// Seats sit around the oval by their position relative to the hero (s0 = hero, bottom centre,
// then clockwise); the coordinates live in play.css so they can change with the table's width.
const SHORT: Record<string, string> = { gto: 'GTO Reg', lag: 'LAG Reg', fish: 'Fish', station: 'Station', nit: 'Nit', maniac: 'Maniac', hunter: 'Hunter' }

function shortName(s: SeatView): string {
  const base = SHORT[s.profile]
  if (!base) return s.name
  const n = s.name.match(/ (\d+)$/)
  return n ? `${base} ${n[1]}` : base
}

function Seat({ s, isButton, toAct, heroSeat }: { s: SeatView; isButton: boolean; toAct: boolean; heroSeat: number }) {
  const rel = (s.seat - heroSeat + 6) % 6
  return (
    <div className={`seat s${rel}` + (s.folded ? ' folded' : '') + (toAct ? ' to-act' : '')}>
      <div className="seat-cards">
        {s.cards ? <Cards cards={s.cards} size="md" /> : s.in_hand && !s.folded ? <><PlayingCard card={null} /><PlayingCard card={null} /></> : null}
      </div>
      <div className="seat-box">
        <div className="seat-name" title={s.name}>
          <span className="pos">{s.position}</span> <span className="name-long">{s.name}</span>
          <span className="name-short">{shortName(s)}</span>
          {isButton && <span className="dealer">D</span>}
        </div>
        <div className="seat-stack">{bb(s.stack)}{s.all_in && <span className="allin"> all-in</span>}</div>
        {s.thinking ? <div className="seat-act thinking">{s.thinking}</div> : s.last_action && <div className="seat-act">{s.last_action}</div>}
      </div>
    </div>
  )
}

/** Bot speed, pause between hands and coach mode, tucked into a popover so the toolbar is one row. */
function TableSettings({ t, send }: { t: TableState; send: ReturnType<typeof useTable>['send'] }) {
  const ref = useRef<HTMLDetailsElement>(null)
  useEffect(() => {
    const close = (e: MouseEvent) => {
      if (ref.current?.open && !ref.current.contains(e.target as Node)) ref.current.open = false
    }
    document.addEventListener('mousedown', close)
    return () => document.removeEventListener('mousedown', close)
  }, [])
  return (
    <details className="table-settings" ref={ref}>
      <summary className="btn">Settings</summary>
      <div className="settings-panel col">
        <label className="col">
          Coach
          <select value={t.coach_mode} onChange={(e) => send({ type: 'set_coach', mode: e.target.value as CoachMode })}>
            <option value="decision">Grade each decision</option>
            <option value="hand">Grade each hand</option>
            <option value="off">Coach off</option>
          </select>
        </label>
        <label className="col">
          Bot speed: {t.bot_delay_ms} ms per action
          <input type="range" min={0} max={2500} step={50} value={t.bot_delay_ms} onChange={(e) => send({ type: 'set_delay', ms: +e.target.value })} />
        </label>
        <label className="col">
          Pause between hands: {((t.deal_pause_ms ?? 1500) / 1000).toFixed(1)} s
          <input type="range" min={300} max={6000} step={100} value={t.deal_pause_ms ?? 1500} onChange={(e) => send({ type: 'set_deal_pause', ms: +e.target.value })} />
        </label>
      </div>
    </details>
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

/** Session mode: buy-ins, profit in bb and dollars, top-up and rebuy. */
function CashBar({ c, liveStack, send }: { c: CashInfo; liveStack: number; send: ReturnType<typeof useTable>['send'] }) {
  const rate = c.bb_dollars ?? 1
  const profit = c.session_profit ?? 0
  const life = c.lifetime_profit ?? 0
  const canTopUp = !c.busted && liveStack > 0 && (c.stack ?? liveStack) < 10_000
  return (
    <div className="cash-bar row">
      <span>
        Bought in <b>{bb(c.buyins ?? 0)}</b>
      </span>
      <span>
        Session{' '}
        <b className={profit >= 0 ? 'pill-good' : 'pill-bad'}>
          {signed(profit)} ({dollars(profit, rate, true)})
        </b>
      </span>
      <span>
        Lifetime{' '}
        <b className={life >= 0 ? 'pill-good' : 'pill-bad'}>
          {signed(life)} ({dollars(life, rate, true)})
        </b>
      </span>
      <span className="faint small">${rate.toFixed(2)}/bb</span>
      <span className="grow" />
      {canTopUp && (
        <button onClick={() => send({ type: 'top_up' })} disabled={c.pending_topup} title="Adds chips up to 100bb before the next hand">
          {c.pending_topup ? 'Top-up before next hand' : `Top up to 100bb (+${bb(10_000 - (c.stack ?? liveStack))})`}
        </button>
      )}
    </div>
  )
}

/** Session mode: this session's results graph, refreshed after every hand. */
function SessionResults({ sessionId, handId, rate }: { sessionId: number; handId: number | null; rate: number }) {
  const [pts, setPts] = useState<{ hand: number; net: number; adj: number }[]>([])
  useEffect(() => {
    getJson<CashSummary>(`/api/cash/summary?session=${sessionId}`)
      .then((c) => setPts(c.graph.map((p) => ({ hand: p.i, net: p.net / 100, adj: p.ev / 100 }))))
      .catch(() => setPts([]))
  }, [sessionId, handId])
  return (
    <div className="panel results">
      <ResultsGraph pts={pts} height={150} bbDollars={rate} empty="The results graph appears after two hands." />
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
  const cash = t.cash?.mode === 'session' ? t.cash : null
  const hero = t.seats[t.hero_seat]
  const stop = async (close = false) => {
    if (close && !confirm('Cash out this session? It can no longer be resumed; its result stays in your lifetime totals.')) return
    await postJson('/api/session/stop', { close })
    setShowSetup(true)
  }
  return (
    <div className="play">
      <div className="play-main">
        <div className="toolbar row">
          <span className="muted status">
            {cash ? 'Session' : 'Training'} · Hand #{t.hand_no} · {t.hands_played} played
            {!cash && (
              <>
                {' '}
                · net{' '}
                <b className={t.hero_net >= 0 ? 'pill-good' : 'pill-bad'}>{signed(t.hero_net)}</b>
              </>
            )}
            {t.hero_chart_bb && t.hero_chart_bb !== 100 ? <span className="faint small"> · {t.hero_chart_bb}bb charts</span> : null}
          </span>
          <span className="grow" />
          <button onClick={() => send({ type: 'pause', paused: !t.paused })}>
            {t.paused ? 'Resume' : 'Pause'} <span className="kbd">P</span>
          </button>
          <button onClick={() => send({ type: 'next_hand' })}>
            Next hand <span className="kbd">N</span>
          </button>
          <TableSettings t={t} send={send} />
          {cash ? (
            <>
              <button className="ghost" onClick={() => stop(false)} title="Stacks are saved; resume from the setup screen">
                Leave
              </button>
              <button className="ghost" onClick={() => stop(true)}>
                Cash out
              </button>
            </>
          ) : (
            <button className="ghost" onClick={() => stop(false)}>
              End session
            </button>
          )}
        </div>
        {cash && <CashBar c={cash} liveStack={(hero?.stack ?? 0) + (hero?.committed ?? 0)} send={send} />}
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
                <div key={'b' + s.seat} className={`bet-chip b${(s.seat - t.hero_seat + 6) % 6}`}>
                  {bb(s.committed)}
                </div>
              ))}
          </div>
        </div>
        {error && <div className="toast pill-bad">{error}</div>}
        {heroTurn ? (
          <ActionBar t={t} send={send} />
        ) : cash?.busted ? (
          <div className="action-bar idle">
            <span>You're out of chips.</span>
            <button className="primary" onClick={() => send({ type: 'rebuy' })}>
              Rebuy for 100bb{cash.bb_dollars ? ` (${dollars(10_000, cash.bb_dollars)})` : ''}
            </button>
          </div>
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
                <span className="faint">{cash && !t.paused ? ' — next hand shortly (N deals now)' : t.paused ? ' — paused (P to resume)' : ' — press N for the next hand'}</span>
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
        {cash?.session_id && <SessionResults sessionId={cash.session_id} handId={t.last_result?.hand_id ?? null} rate={cash.bb_dollars ?? 1} />}
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
