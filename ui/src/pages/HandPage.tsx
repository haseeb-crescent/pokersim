import { useEffect, useMemo, useState } from 'react'
import { useParams, useSearchParams } from 'react-router-dom'
import { bb, getJson, postJson } from '../api/client'
import type { DecisionGrade, PreflopNode } from '../api/types'
import { GradeCard, Thinking, useExplanationsVia } from '../components/GradeCard'
import { Cards, CardText } from '../components/PlayingCard'
import { RangeGrid, StrategyBar } from '../components/RangeGrid'
import { LinkTerms } from '../components/Term'
import './hand.css'

interface Grids {
  player: 'OOP' | 'IP'
  actions: string[]
  strategy_grid: number[][]
  range_grid: [number[], number[]]
  range_freq: number[]
  equity_grid: number[] | null
  ev_grid: number[][] | null
  pot_bb: number
  precision_pct: number
}

interface ReplayNode {
  street: string
  index: number
  seat: number
  line: string[]
  took: string
  grids: Grids
}

interface EngineEvent {
  e: string
  seat?: number
  street?: string
  kind?: string
  to?: number
  added?: number
  all_in?: boolean
  cards?: number[] | [number, number]
  amount?: number
  big?: boolean
  description?: string
}

interface HandDetail {
  id: number
  hand_no: number
  hero_position: string
  hero_cards: string
  board: string
  hero_net: number
  hero_net_adj: number
  notes: string
  tags: string
  record: {
    hand: { events: EngineEvent[]; seats: { hole: [number, number]; starting_stack: number }[]; button: number }
    names: string[]
    positions: string[]
    hero_seat: number
    preflop_key: string | null
    bot_decisions: { seat: number; street: string; source: string; exploit: string | null }[]
  } | null
  replay: ReplayNode[] | null
  decisions: (DecisionGrade & { grids?: Grids; preflop_key?: string })[]
}

const RANKS = '23456789TJQKA'
const SUITS = 'cdhs'
const cardStr = (c: number) => RANKS[c >> 2] + SUITS[c & 3]

type Step = { label: string; street: string; boardLen: number; seat: number | null; eventIndex: number }

function Chat({ handId, index }: { handId: number; index: number | null }) {
  const [msgs, setMsgs] = useState<{ role: 'user' | 'assistant'; text: string }[]>([])
  const [q, setQ] = useState('')
  const [busy, setBusy] = useState(false)
  const [err, setErr] = useState<string | null>(null)
  // continues the claude CLI conversation (--resume) across follow-ups
  const [chatId, setChatId] = useState<string | null>(null)
  const via = useExplanationsVia()
  useEffect(() => {
    setMsgs([])
    setChatId(null)
  }, [handId, index])
  const ask = async () => {
    if (!q.trim() || busy) return
    const next = [...msgs, { role: 'user' as const, text: q.trim() }]
    setMsgs(next)
    setQ('')
    setBusy(true)
    setErr(null)
    try {
      const r = await postJson<{ reply: string; chat_id: string | null }>('/api/coach/ask', { hand_id: handId, index, messages: next, chat_id: chatId })
      setMsgs([...next, { role: 'assistant', text: r.reply }])
      if (r.chat_id) setChatId(r.chat_id)
    } catch (e) {
      setErr(String(e).replace(/^Error: \d+ /, ''))
    }
    setBusy(false)
  }
  return (
    <div className="panel col chat">
      <h3>Ask why</h3>
      <p className="faint small">Answers are grounded in this hand's solver facts{index !== null ? ` (focused on decision ${index + 1})` : ''}.</p>
      {msgs.map((m, i) => (
        <div key={i} className={'msg ' + m.role}>
          {m.role === 'assistant' ? <LinkTerms text={m.text} /> : m.text}
        </div>
      ))}
      {busy && (
        <div className="msg assistant">
          <Thinking />
        </div>
      )}
      {err && <p className="pill-bad small">{err}</p>}
      {via === 'none' && <p className="faint small">No coach chat backend: log in to the claude CLI, or add ANTHROPIC_API_KEY to .env.</p>}
      <div className="row">
        <input className="grow" value={q} onChange={(e) => setQ(e.target.value)} onKeyDown={(e) => e.key === 'Enter' && ask()} placeholder="Why does the solver check this flop so often?" />
        <button className="primary" onClick={ask} disabled={busy}>
          {busy ? '…' : 'Ask'}
        </button>
      </div>
    </div>
  )
}

function GridsView({ g, heroHand, title }: { g: Grids; heroHand?: string; title: string }) {
  const [view, setView] = useState<'strategy' | 'ranges' | 'equity' | 'ev'>('strategy')
  const cls = useMemo(() => {
    if (!heroHand || heroHand.length < 4) return null
    const r1 = RANKS.indexOf(heroHand[0])
    const r2 = RANKS.indexOf(heroHand[2])
    const hi = Math.max(r1, r2)
    const lo = Math.min(r1, r2)
    const A = 'AKQJT98765432'
    const n = (r: number) => A[12 - r]
    if (hi === lo) return n(hi) + n(hi)
    return n(hi) + n(lo) + (heroHand[1] === heroHand[3] ? 's' : 'o')
  }, [heroHand])
  const weights = g.range_grid[g.player === 'OOP' ? 0 : 1]
  return (
    <div className="panel col">
      <div className="row wrap">
        <h3 className="grow">{title}</h3>
        {(['strategy', 'ranges', 'equity', 'ev'] as const).map((v) => (
          <button key={v} className={view === v ? 'primary' : ''} onClick={() => setView(v)} disabled={(v === 'equity' && !g.equity_grid) || (v === 'ev' && !g.ev_grid)}>
            {v}
          </button>
        ))}
      </div>
      <p className="faint small">
        {g.player} to act · pot {g.pot_bb}bb · solver precision {g.precision_pct < 90 ? g.precision_pct.toFixed(2) + '% pot' : '—'}
      </p>
      {g.range_freq.length > 0 && <StrategyBar labels={g.actions} freqs={g.range_freq} />}
      {view === 'strategy' && <RangeGrid strategy={g.strategy_grid} actionLabels={g.actions} weights={weights} highlight={cls} size={30} />}
      {view === 'ranges' && (
        <div className="row wrap">
          <div>
            <div className="small muted">OOP range</div>
            <RangeGrid weights={g.range_grid[0]} highlight={g.player === 'OOP' ? cls : null} size={24} color="var(--act-fold)" />
          </div>
          <div>
            <div className="small muted">IP range</div>
            <RangeGrid weights={g.range_grid[1]} highlight={g.player === 'IP' ? cls : null} size={24} color="var(--act-bet-s)" />
          </div>
        </div>
      )}
      {view === 'equity' && g.equity_grid && <RangeGrid weights={g.equity_grid} highlight={cls} size={30} color="var(--good)" />}
      {view === 'ev' && g.ev_grid && (
        <div className="col">
          {g.actions.map((a, i) => {
            const vals = g.ev_grid![i]
            const max = Math.max(1e-9, ...vals.map((x) => Math.abs(x)))
            return (
              <div key={a}>
                <div className="small muted">EV of {a} (bb, darker = higher)</div>
                <RangeGrid weights={vals.map((x) => Math.max(0, x) / max)} highlight={cls} size={20} showLabels={false} />
              </div>
            )
          })}
        </div>
      )}
    </div>
  )
}

export default function HandPage() {
  const { id } = useParams()
  const [search] = useSearchParams()
  const [h, setH] = useState<HandDetail | null>(null)
  const [step, setStep] = useState(0)
  const [notes, setNotes] = useState('')
  const [tags, setTags] = useState('')
  const [saved, setSaved] = useState(false)
  const [hh, setHh] = useState<string | null>(null)
  const [pfNode, setPfNode] = useState<PreflopNode | null>(null)
  const [focus, setFocus] = useState<number | null>(search.get('d') !== null ? Number(search.get('d')) : null)

  useEffect(() => {
    getJson<HandDetail>(`/api/hands/${id}`).then((x) => {
      setH(x)
      setNotes(x.notes)
      setTags(x.tags)
    })
  }, [id])

  const steps: Step[] = useMemo(() => {
    if (!h?.record) return []
    const out: Step[] = [{ label: 'Deal', street: 'preflop', boardLen: 0, seat: null, eventIndex: -1 }]
    let boardLen = 0
    h.record.hand.events.forEach((e, i) => {
      if (e.e === 'Board') {
        boardLen += (e.cards as number[]).length
        out.push({ label: `${e.street}`, street: e.street!, boardLen, seat: null, eventIndex: i })
      } else if (e.e === 'Act') {
        const name = h.record!.names[e.seat!]
        const verb = e.kind === 'Fold' ? 'folds' : e.kind === 'Check' ? 'checks' : e.kind === 'Call' ? `calls ${bb(e.added!)}` : e.kind === 'Bet' ? `bets ${bb(e.to!)}` : `raises to ${bb(e.to!)}`
        out.push({ label: `${name} ${verb}`, street: e.street!, boardLen, seat: e.seat!, eventIndex: i })
      }
    })
    out.push({ label: 'Showdown / result', street: 'end', boardLen, seat: null, eventIndex: h.record.hand.events.length })
    return out
  }, [h])

  const cur = steps[step]
  // preflop chart view for preflop steps
  const preflopKey = useMemo(() => {
    if (!h?.record || !cur || cur.street !== 'preflop' || cur.seat === null) return null
    const codes: string[] = []
    for (let i = 0; i < cur.eventIndex; i++) {
      const e = h.record.hand.events[i]
      if (e.e === 'Act' && e.street === 'preflop') codes.push(e.kind === 'Fold' ? 'F' : e.kind === 'Check' ? 'K' : e.kind === 'Call' ? 'C' : e.all_in ? 'A' : 'R')
    }
    return codes.join('-')
  }, [h, cur])
  useEffect(() => {
    if (preflopKey === null) {
      setPfNode(null)
      return
    }
    getJson<PreflopNode>(`/api/lab/preflop/node?key=${encodeURIComponent(preflopKey)}`).then(setPfNode).catch(() => setPfNode(null))
  }, [preflopKey])

  if (!h) return <div className="page muted">Loading…</div>
  if (!h.record) return <div className="page muted">This hand was not completed.</div>
  const rec = h.record
  const board = h.board ? h.board.match(/../g) ?? [] : []
  const shownBoard = board.slice(0, cur?.boardLen ?? 0)
  // postflop node for the current step
  const streetIdx: Record<string, number> = { flop: 0, turn: 1, river: 2 }
  let node: ReplayNode | undefined
  if (cur && cur.seat !== null && cur.street in streetIdx && h.replay) {
    const before = steps.slice(0, step).filter((s) => s.street === cur.street && s.seat !== null).length
    node = h.replay.find((r) => r.street === cur.street && r.index === before)
  }
  const heroDecision = h.decisions.find((d) => {
    if (!cur || cur.seat !== rec.hero_seat) return false
    const heroStepsBefore = steps.slice(0, step).filter((s) => s.seat === rec.hero_seat).length
    return d.index === heroStepsBefore
  })

  const saveNotes = async () => {
    await postJson(`/api/hands/${id}/notes`, { notes, tags })
    setSaved(true)
    setTimeout(() => setSaved(false), 1500)
  }
  const showHH = async () => {
    const r = await fetch(`/api/hands/${id}/hh`)
    setHh(await r.text())
  }

  return (
    <div className="page col hand-page">
      <div className="row wrap">
        <h1 className="grow">
          Hand #{h.hand_no} · {h.hero_position} · <CardText text={h.hero_cards} />
        </h1>
        <span className={h.hero_net >= 0 ? 'pill-good' : 'pill-bad'}>
          {bb(h.hero_net)} {Math.abs(h.hero_net_adj - h.hero_net) > 1 && <span className="faint small">(all-in adj. {bb(Math.round(h.hero_net_adj))})</span>}
        </span>
        <button onClick={showHH}>PokerStars HH</button>
      </div>
      <div className="replayer panel">
        <div className="row wrap">
          <button onClick={() => setStep(0)}>⏮</button>
          <button onClick={() => setStep(Math.max(0, step - 1))}>◀ Prev</button>
          <button onClick={() => setStep(Math.min(steps.length - 1, step + 1))}>Next ▶</button>
          {['preflop', 'flop', 'turn', 'river'].map((s) => {
            const i = steps.findIndex((x) => x.street === s && (s === 'preflop' || x.seat === null))
            return i >= 0 ? (
              <button key={s} onClick={() => setStep(i)} className="ghost">
                {s}
              </button>
            ) : null
          })}
          <span className="grow" />
          <span className="muted">
            {step + 1}/{steps.length}: <b>{cur?.label}</b>
          </span>
        </div>
        <div className="replay-table">
          <div className="replay-board">{shownBoard.length ? <Cards cards={shownBoard} size="lg" /> : <span className="faint">preflop</span>}</div>
          <div className="replay-seats">
            {rec.names.map((n, i) => (
              <div key={i} className={'replay-seat' + (cur?.seat === i ? ' acting' : '')}>
                <b>{rec.positions[i]}</b> {n} <Cards cards={rec.hand.seats[i].hole.map(cardStr)} size="sm" />
              </div>
            ))}
          </div>
        </div>
      </div>
      <div className="grid-2">
        <div className="col">
          {node && <GridsView g={node.grids} title={`Solver view: ${rec.names[node.seat]} to act (${node.took})`} heroHand={node.seat === rec.hero_seat ? h.hero_cards : undefined} />}
          {pfNode && cur?.seat !== null && (
            <div className="panel col">
              <h3>
                Chart: {pfNode.player} — {pfNode.description}
              </h3>
              <StrategyBar
                labels={pfNode.actions.map((a) => a.label)}
                freqs={pfNode.actions.map((_, ai) => {
                  let num = 0
                  let den = 0
                  for (let c = 0; c < 169; c++) {
                    const w = (Math.floor(c / 13) === c % 13 ? 6 : Math.floor(c / 13) < c % 13 ? 4 : 12) * (pfNode.ranges[pfNode.player]?.[c] ?? 1)
                    num += w * pfNode.freq[ai][c]
                    den += w
                  }
                  return den ? num / den : 0
                })}
              />
              <RangeGrid strategy={pfNode.freq} actionLabels={pfNode.actions.map((a) => a.label)} weights={pfNode.ranges[pfNode.player]} size={30} />
              <p className="faint small">Preflop charts carry no EVs; grades use chart frequencies.</p>
            </div>
          )}
          {cur?.seat !== null && cur && cur.seat !== rec.hero_seat && (
            <p className="faint small">
              Bot logic: {rec.bot_decisions.filter((b) => b.seat === cur.seat)[steps.slice(0, step).filter((s) => s.seat === cur.seat).length]?.source ?? '—'}
            </p>
          )}
          {!node && cur && cur.street in streetIdx && cur.seat !== null && <p className="faint small">No solver view for this node (multiway or unsolved pot).</p>}
        </div>
        <div className="col">
          {heroDecision ? <GradeCard g={heroDecision} /> : <p className="faint small">Step to one of your decisions to see its grade.</p>}
          {h.decisions.length > 0 && (
            <div className="panel col">
              <h3>All your decisions</h3>
              {h.decisions.map((d) => (
                <div key={d.index} className="row small decision-row" onClick={() => setFocus(d.index)}>
                  <span className="tag">{d.street}</span>
                  <span className="grow">{d.spot}</span>
                  <b>{d.grade}</b>
                  {d.ev_loss_bb ? <span className="pill-bad">−{d.ev_loss_bb.toFixed(2)}bb</span> : null}
                </div>
              ))}
            </div>
          )}
          <Chat handId={h.id} index={focus} />
          <div className="panel col">
            <h3>Notes & tags</h3>
            <textarea rows={3} value={notes} onChange={(e) => setNotes(e.target.value)} />
            <input value={tags} onChange={(e) => setTags(e.target.value)} placeholder="comma-separated tags" />
            <button onClick={saveNotes}>{saved ? 'Saved' : 'Save'}</button>
          </div>
        </div>
      </div>
      {hh && (
        <div className="panel col">
          <div className="row">
            <h3 className="grow">PokerStars hand history</h3>
            <button onClick={() => navigator.clipboard?.writeText(hh)}>Copy</button>
          </div>
          <pre className="hh">{hh}</pre>
        </div>
      )}
    </div>
  )
}
