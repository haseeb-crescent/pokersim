import { useEffect, useState } from 'react'
import { Link } from 'react-router-dom'
import { bb, getJson } from '../api/client'
import { CardText } from '../components/PlayingCard'

interface HandRow {
  id: number
  session_id: number
  hand_no: number
  played_at: number
  hero_position: string | null
  hero_cards: string | null
  board: string | null
  hero_net: number
  ev_loss: number
  showdown: boolean
  notes: string
  tags: string
}

interface SessionRow {
  id: number
  started_at: number
  kind: string
  hands: number
  hero_net: number
}

export default function LogbookPage() {
  const [hands, setHands] = useState<HandRow[]>([])
  const [sessions, setSessions] = useState<SessionRow[]>([])
  const [session, setSession] = useState<string>('')
  const [tag, setTag] = useState('')
  const [minLoss, setMinLoss] = useState('')
  const [page, setPage] = useState(0)
  const [err, setErr] = useState<string | null>(null)

  useEffect(() => {
    getJson<SessionRow[]>('/api/sessions').then(setSessions).catch(() => {})
  }, [])
  useEffect(() => {
    const q = new URLSearchParams({ limit: '100', offset: String(page * 100) })
    if (session) q.set('session', session)
    if (tag) q.set('tag', tag)
    if (minLoss) q.set('min_loss', minLoss)
    getJson<HandRow[]>('/api/hands?' + q).then(setHands).catch((e) => setErr(String(e)))
  }, [session, tag, minLoss, page])

  return (
    <div className="page col">
      <div className="row">
        <h1 className="grow">Logbook</h1>
        <a className="btn" href={'/api/export/hh' + (session ? `?session=${session}` : '')}>
          Export PokerStars hand histories
        </a>
      </div>
      {err && <p className="pill-bad">{err}</p>}
      <div className="row wrap panel">
        <label>
          Session{' '}
          <select value={session} onChange={(e) => { setSession(e.target.value); setPage(0) }}>
            <option value="">All</option>
            {sessions.map((s) => (
              <option key={s.id} value={s.id}>
                #{s.id} · {new Date(s.started_at * 1000).toLocaleString()} · {s.kind} · {s.hands} hands · {bb(s.hero_net)}
              </option>
            ))}
          </select>
        </label>
        <label>
          Tag <input value={tag} onChange={(e) => setTag(e.target.value)} placeholder="e.g. review" style={{ width: 120 }} />
        </label>
        <label>
          Min EV loss (bb) <input value={minLoss} onChange={(e) => setMinLoss(e.target.value)} style={{ width: 70 }} />
        </label>
      </div>
      <table>
        <thead>
          <tr>
            <th>#</th>
            <th>Time</th>
            <th>Pos</th>
            <th>Hand</th>
            <th>Board</th>
            <th>Result</th>
            <th>EV lost</th>
            <th>Tags / notes</th>
          </tr>
        </thead>
        <tbody>
          {hands.map((h) => (
            <tr key={h.id}>
              <td>
                <Link to={`/hand/${h.id}`}>{h.hand_no}</Link>
              </td>
              <td className="muted">{new Date(h.played_at * 1000).toLocaleTimeString()}</td>
              <td>{h.hero_position}</td>
              <td>
                <CardText text={h.hero_cards ?? ''} />
              </td>
              <td>
                <CardText text={h.board ?? ''} />
              </td>
              <td className={h.hero_net > 0 ? 'pill-good' : h.hero_net < 0 ? 'pill-bad' : 'muted'}>{bb(h.hero_net)}</td>
              <td className={h.ev_loss > 0.5 ? 'pill-bad' : h.ev_loss > 0 ? 'pill-warn' : 'faint'}>{h.ev_loss > 0 ? h.ev_loss.toFixed(2) : '–'}</td>
              <td className="muted small">
                {h.tags && h.tags.split(',').map((t) => <span key={t} className="tag">{t}</span>)} {h.notes.slice(0, 60)}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      {hands.length === 0 && <p className="faint">No hands yet. Play a session first.</p>}
      <div className="row">
        <button disabled={page === 0} onClick={() => setPage(page - 1)}>
          Newer
        </button>
        <button disabled={hands.length < 100} onClick={() => setPage(page + 1)}>
          Older
        </button>
      </div>
    </div>
  )
}
