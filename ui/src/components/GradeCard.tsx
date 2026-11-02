import { useEffect, useState } from 'react'
import { Link } from 'react-router-dom'
import type { ActionMix, DecisionGrade } from '../api/types'
import { pct, postJson } from '../api/client'
import { CardText } from './PlayingCard'
import { actionColor } from './RangeGrid'
import { LinkTerms, Term } from './Term'
import './grade.css'

type Via = 'claude-cli' | 'api' | 'none'
let explanationsVia: Promise<Via> | null = null
/** Which backend writes explanations: the claude CLI (subscription), the API, or none. */
export function useExplanationsVia() {
  const [via, setVia] = useState<Via | null>(null)
  useEffect(() => {
    explanationsVia ??= fetch('/api/health')
      .then((r) => r.json())
      .then((h) => (h.explanations_via ?? (h.explanations ? 'api' : 'none')) as Via)
      .catch(() => 'none' as Via)
    explanationsVia.then(setVia)
  }, [])
  return via
}

export function Thinking({ label = 'Coach is thinking' }: { label?: string }) {
  return (
    <span className="thinking-dots">
      {label}
      <i />
      <i />
      <i />
    </span>
  )
}

/** The prose explanation, with its loading, error and on-demand states. */
function Explanation({ g }: { g: DecisionGrade }) {
  const via = useExplanationsVia()
  const [text, setText] = useState<string | null>(g.explanation)
  const [busy, setBusy] = useState(false)
  const [err, setErr] = useState<string | null>(g.explanation_error ?? null)
  useEffect(() => {
    setText(g.explanation)
    setErr(g.explanation_error ?? null)
  }, [g.explanation, g.explanation_error])
  const request = async () => {
    setBusy(true)
    setErr(null)
    try {
      const r = await postJson<{ text: string }>(`/api/hands/${g.hand_id}/explain`, { index: g.index })
      setText(r.text)
    } catch (e) {
      setErr(String(e).replace(/^Error: \d+ /, ''))
    }
    setBusy(false)
  }
  if (text)
    return (
      <p className="explanation">
        <LinkTerms text={text} />
      </p>
    )
  if (busy || (g.explanation_status === 'pending' && !err))
    return (
      <p className="explanation thinking">
        <Thinking />
      </p>
    )
  if (via === 'none')
    return <p className="faint small">No explanation backend: log in to the claude CLI (uses your Claude subscription) or add ANTHROPIC_API_KEY to .env. The facts below still apply.</p>
  return (
    <p className="small row">
      {err && <span className="pill-bad">Explanation failed: {err}</span>}
      {via !== null && (
        <button className="ghost small" onClick={request}>
          {err ? 'Retry' : 'Explain this decision'}
        </button>
      )}
    </p>
  )
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

function MixTable({ actions, chosen, showEv, model = false }: { actions: ActionMix[]; chosen?: string; showEv: boolean; model?: boolean }) {
  const bestEv = Math.max(...actions.map((a) => a.ev_bb ?? -Infinity))
  if (model)
    return (
      <table className="mix">
        <thead>
          <tr>
            <th>Option</th>
            <th title="One-step model EV relative to folding now, in big blinds">Model EV (bb)</th>
            <th />
          </tr>
        </thead>
        <tbody>
          {actions.map((a, i) => (
            <tr key={a.label} className={a.label === chosen ? 'chosen' : ''}>
              <td>
                <i className="dot" style={{ background: actionColor(a.label, i, actions.length) }} />
                {a.label}
              </td>
              <td className={a.ev_bb === bestEv ? 'best-ev' : ''}>{a.ev_bb === null ? '–' : a.ev_bb.toFixed(2)}</td>
              <td className="faint small">{a.ev_bb === bestEv ? 'best' : ''}</td>
            </tr>
          ))}
        </tbody>
      </table>
    )
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
                · {g.kind === 'preflop_chart' ? 'Chart' : g.kind === 'heuristic' ? 'Model' : 'Solver'} prefers <b>{g.best}</b>
              </span>
            )}
            {g.board.length > 0 && (
              <span className="muted">
                · board <CardText text={g.board.join('')} />
              </span>
            )}
          </div>
          {g.heuristic && (
            <div className="offchart">
              <b>Off-chart: heuristic grade.</b> {g.heuristic.reason}.
              <div className="small">
                Options compared with a one-step model against the actual range{g.heuristic.ranges.length > 1 ? 's' : ''}:{' '}
                {g.heuristic.ranges.map((r) => `${r.profile} (${r.position}) ${r.percent}% of hands`).join(', ')}
                {g.heuristic.fold_to_raise !== null && <> · modelled fold to a raise {pct(g.heuristic.fold_to_raise)}</>}. Not a solver or chart result.
              </div>
              {g.heuristic.sizing && <div className="small">Sizing: {g.heuristic.sizing}.</div>}
            </div>
          )}
          {g.actions.length > 0 && <MixTable actions={g.actions} chosen={g.chosen} showEv={showEv} model={g.kind === 'heuristic'} />}
          {g.off_chart && (
            <div className="offchart">
              <b>Off-chart:</b> {g.off_chart.action} is 0% in the chart — the chart says <b>{g.off_chart.chart}</b>.
              {g.off_chart.leaves_chart && <div className="small">This takes the hand off the chart tree, so the rest of this hand gets pot odds and equity against estimated ranges instead of solver grades.</div>}
            </div>
          )}
          {g.kind === 'unsolved' && f && (
            <div className="unsolved">
              <div className="small">
                <b>Why no grade:</b> {g.unsolved_reason ?? g.note}
              </div>
              <div className="fact-strip">
                {f.pot_odds_needed !== null && (
                  <span>
                    <Term id="pot-odds">Pot odds</Term>: need <b>{pct(f.pot_odds_needed, 1)}</b>
                  </span>
                )}
                {f.equity_vs_range !== null && (
                  <span>
                    <Term id="equity">Equity</Term> vs estimated range: <b>{pct(f.equity_vs_range, 1)}</b>
                  </span>
                )}
                <span>
                  <Term id="spr">SPR</Term> {f.spr}
                </span>
                <span>{f.hand_description}</span>
              </div>
              {f.pot_odds_needed !== null && f.equity_vs_range !== null && (
                <div className="small muted">
                  {f.equity_vs_range >= f.pot_odds_needed
                    ? 'Your equity is above what the price requires (before considering how much of it you realize).'
                    : 'Your equity is below what the price requires, so calling needs implied odds or fold equity to make up the gap.'}
                </div>
              )}
            </div>
          )}
          <Explanation g={g} />
          {g.note && g.kind !== 'unsolved' && g.kind !== 'heuristic' && <p className="note">{g.note}</p>}
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
            <details className="facts" open={g.kind === 'unsolved'}>
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
