import { useEffect, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { getJson, postJson } from '../../api/client'
import type { BotProfile } from '../../api/types'

const POSITIONS = ['UTG', 'HJ', 'CO', 'BTN', 'SB', 'BB']
const TEXTURES: [string, string][] = [
  ['any', 'Any flop'],
  ['dry', 'Dry (rainbow, disconnected)'],
  ['wet', 'Wet (draw-heavy)'],
  ['monotone', 'Monotone'],
  ['paired', 'Paired'],
  ['ahigh', 'Ace-high'],
  ['low', 'Low (9-high or lower)'],
]

/** Spot drills: a fixed preflop line, both hands dealt from their chart ranges, a flop of the
 *  chosen texture; you play from the flop on with instant grading. */
export default function DrillsLab() {
  const nav = useNavigate()
  const [profiles, setProfiles] = useState<BotProfile[]>([])
  const [hero, setHero] = useState('BTN')
  const [villain, setVillain] = useState('BB')
  const [pot, setPot] = useState<'SRP' | '3BP'>('SRP')
  const [texture, setTexture] = useState('any')
  const [vprof, setVprof] = useState('gto')
  const [err, setErr] = useState<string | null>(null)
  useEffect(() => {
    getJson<BotProfile[]>('/api/profiles').then(setProfiles).catch(() => setErr('Start the server to run drills.'))
  }, [])
  const hi = POSITIONS.indexOf(hero)
  const vi = POSITIONS.indexOf(villain)
  const opener = hi < vi ? hero : villain
  const other = hi < vi ? villain : hero
  const valid = hero !== villain
  const start = async () => {
    try {
      await postJson('/api/drill/start', { hero_pos: hero, villain_pos: villain, pot_type: pot, texture, villain_profile: vprof })
      nav('/play')
    } catch (e) {
      setErr(String(e))
    }
  }
  return (
    <div className="col" style={{ maxWidth: 760 }}>
      <h2>Spot drills</h2>
      <p className="muted">
        Pick a matchup, pot type and board texture. Each hand deals both players a hand from their chart range for that line and a flop with that texture; the preflop
        action is played automatically and you play from the flop with every decision graded immediately. Press <span className="kbd">N</span> for the next spot.
      </p>
      {err && <p className="pill-bad">{err}</p>}
      <div className="panel col">
        <div className="row wrap">
          <label>
            You{' '}
            <select value={hero} onChange={(e) => setHero(e.target.value)}>
              {POSITIONS.map((p) => (
                <option key={p}>{p}</option>
              ))}
            </select>
          </label>
          <label>
            vs{' '}
            <select value={villain} onChange={(e) => setVillain(e.target.value)}>
              {POSITIONS.map((p) => (
                <option key={p}>{p}</option>
              ))}
            </select>
          </label>
          <label>
            Pot{' '}
            <select value={pot} onChange={(e) => setPot(e.target.value as 'SRP' | '3BP')}>
              <option value="SRP">Single-raised</option>
              <option value="3BP">3-bet pot</option>
            </select>
          </label>
          <label>
            Board{' '}
            <select value={texture} onChange={(e) => setTexture(e.target.value)}>
              {TEXTURES.map(([k, v]) => (
                <option key={k} value={k}>
                  {v}
                </option>
              ))}
            </select>
          </label>
          <label>
            Opponent{' '}
            <select value={vprof} onChange={(e) => setVprof(e.target.value)}>
              {profiles.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name}
                </option>
              ))}
            </select>
          </label>
        </div>
        {valid ? (
          <p className="small muted">
            Line: everyone folds to {opener}, who opens; {other} {pot === 'SRP' ? 'calls' : `3-bets and ${opener} calls`}. You are {hero}.
          </p>
        ) : (
          <p className="pill-warn small">Pick two different positions.</p>
        )}
        <div className="row">
          <button className="primary" disabled={!valid} onClick={start}>
            Start drill
          </button>
        </div>
      </div>
      <p className="faint small">Leaks in the leak report have a "Drill this" button that opens the matching drill.</p>
    </div>
  )
}
