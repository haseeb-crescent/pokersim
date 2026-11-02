import { useEffect, useState } from 'react'
import { Link } from 'react-router-dom'
import type { ActionMix, DecisionGrade } from '../api/types'
import { pct } from '../api/client'
import { CardText } from './PlayingCard'
import { actionColor } from './RangeGrid'
import { LinkTerms, Term } from './Term'
import './grade.css'

let explanationsEnabled: Promise<boolean> | null = null
function useExplanationsEnabled() {
  const [on, setOn] = useState<boolean | null>(null)
  useEffect(() => {
    explanationsEnabled ??= fetch('/api/health')
      .then((r) => r.json())
      .then((h) => Boolean(h.explanations))
      .catch(() => false)
    explanationsEnabled.then(setOn)
  }, [])
  return on
}

export const GRADE_COLORS: Record<string, string> = {
  Best: 'var(--good)',
  Correct: 'var(--ok)',
  Inaccuracy: 'var(--warn)',
  Mistake: '#fb923c',
  Blunder: 'var(--bad)',
  Unsolved: 'var(--text-3)',
  Pending: 'var(--text-3)',
}

function MixTable({ actions, chosen, showEv }: { actions: ActionMix[]; chosen?: string; showEv: boolean }) {
  const bestEv = Math.max(...actions.map((a) => a.ev_bb ?? -Infinity))
  return (
    <table className="mix">
      <thead>
        <tr>
          <th>Action</th>
          <th>Your hand</th>
          {showEv && <th title="EV relative to folding now, in big blinds">EV (bb)</th>}
          <th>Range</th>
        </tr>
      </thead>
      <tbody>
        {actions.map((a, i) => (
          <tr key={a.label} className={a.label === chosen ? 'chosen' : ''}>
            <td>
              <i className="dot" style={{ background: actionColor(a.label, i, actions.length) }} />
              {a.label}
            </td>
            <td>
              <span className="freqbar">
                <span style={{ width: `${100 * a.freq}%`, background: actionColor(a.label, i, actions.length) }} />
              </span>
              {pct(a.freq)}
            </td>
            {showEv && <td className={a.ev_bb === bestEv ? 'best-ev' : ''}>{a.ev_bb === null ? '–' : a.ev_bb.toFixed(2)}</td>}
            <td className="muted">{a.range_freq === null ? '–' : pct(a.range_freq)}</td>
          </tr>
        ))}
      </tbody>
    </table>
  )
}

export function GradeCard({ g, compact = false }: { g: DecisionGrade; compact?: boolean }) {
  const [open, setOpen] = useState(!compact)
  const explanations = useExplanationsEnabled()
  const f = g.facts
  const showEv = g.kind === 'solver'
  return (
    <div className="grade-card">
      <div className="grade-head" onClick={() => setOpen(!open)}>
        <span className="grade-badge" style={{ color: GRADE_COLORS[g.grade], borderColor: GRADE_COLORS[g.grade] }}>
          {g.grade}
        </span>
        <span className="grade-spot">
          <CardText text={g.hero_hand} /> · {g.street} · {g.spot}
        </span>
        {g.ev_loss_bb !== null && g.ev_loss_bb > 0 && <span className="grade-loss">−{g.ev_loss_bb.toFixed(2)}bb</span>}
      </div>
      {open && (
        <div className="grade-body">
          <div className="row wrap">
            <span>
              You: <b>{g.chosen}</b>
              {g.chosen_freq !== null && <span className="muted"> ({pct(g.chosen_freq)} in the {g.kind === 'preflop_chart' ? 'chart' : 'solver mix'})</span>}
            </span>
            {g.best && (
              <span>
                · {g.kind === 'preflop_chart' ? 'Chart' : 'Solver'} prefers <b>{g.best}</b>
              </span>
            )}
            {g.board.length > 0 && (
              <span className="muted">
                · board <CardText text={g.board.join('')} />
              </span>
            )}
          </div>
          {g.actions.length > 0 && <MixTable actions={g.actions} chosen={g.chosen} showEv={showEv} />}
          {g.explanation ? (
            <p className="explanation">
              <LinkTerms text={g.explanation} />
            </p>
          ) : (
            <p className="faint small">
              {explanations === false ? 'Add ANTHROPIC_API_KEY to .env for plain-English explanations.' : 'Explanation pending…'}
            </p>
          )}
          {g.note && <p className="note">{g.note}</p>}
          {g.exploit && (
            <div className="exploit">
              <div className="exploit-title">
                <Term id="exploit">Exploit</Term> vs {g.exploit.profile}: best response is <b>{g.exploit.best}</b>
              </div>
              <MixTable actions={g.exploit.actions} chosen={g.chosen} showEv />
              <p className="faint small">{g.exploit.note}</p>
            </div>
          )}
          {f && (
            <details className="facts">
              <summary>Fact sheet</summary>
              <div className="facts-grid">
                <span>Hand</span>
                <span>{f.hand_description}</span>
                <span>Board</span>
                <span>{f.board_texture}</span>
                <span>Position</span>
                <span>{f.position}</span>
                <span>Pot / to call</span>
                <span>
                  {f.pot_bb}bb / {f.to_call_bb}bb
                </span>
                <span>
                  <Term id="spr">SPR</Term>
                </span>
                <span>{f.spr}</span>
                {f.pot_odds_needed !== null && (
                  <>
                    <span>
                      <Term id="pot-odds">Pot odds</Term> need
                    </span>
                    <span>{pct(f.pot_odds_needed, 1)} equity</span>
                  </>
                )}
                {f.equity_vs_range !== null && (
                  <>
                    <span>
                      <Term id="equity">Equity</Term> vs range
                    </span>
                    <span>{pct(f.equity_vs_range, 1)}</span>
                  </>
                )}
                {f.range_advantage && (
                  <>
                    <span>
                      <Term id="range-advantage">Range advantage</Term>
                    </span>
                    <span>
                      you {pct(f.range_advantage.hero, 1)} · villain {pct(f.range_advantage.villain, 1)}
                    </span>
                  </>
                )}
                {f.nut_advantage && (
                  <>
                    <span>
                      <Term id="nut-advantage">Nut advantage</Term>
                    </span>
                    <span>
                      strong hands: you {pct(f.nut_advantage.hero, 1)} · villain {pct(f.nut_advantage.villain, 1)}
                    </span>
                  </>
                )}
                {f.mdf !== null && (
                  <>
                    <span>
                      <Term id="mdf">MDF</Term>
                    </span>
                    <span>{pct(f.mdf, 1)}</span>
                  </>
                )}
                {f.bluff_ratio !== null && (
                  <>
                    <span>
                      <Term id="bluff-to-value">Bluff share</Term>
                    </span>
                    <span>{pct(f.bluff_ratio, 1)}</span>
                  </>
                )}
                {f.blockers.length > 0 && (
                  <>
                    <span>
                      <Term id="blockers">Blockers</Term>
                    </span>
                    <span>{f.blockers.join('; ')}</span>
                  </>
                )}
                {g.precision_pct !== null && (
                  <>
                    <span>
                      Solver <Term id="exploitability">precision</Term>
                    </span>
                    <span>{g.precision_pct.toFixed(2)}% of pot</span>
                  </>
                )}
              </div>
              {f.notes.length > 0 && (
                <ul className="faint small">
                  {f.notes.map((n, i) => (
                    <li key={i}>{n}</li>
                  ))}
                </ul>
              )}
            </details>
          )}
          <div className="row">
            <Link to={`/hand/${g.hand_id}?d=${g.index}`} className="small">
              Open hand · ask why →
            </Link>
          </div>
        </div>
      )}
    </div>
  )
}
