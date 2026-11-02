// Building blocks shared by the Learn modules: layout, inputs, visuals and the drill engine.

import { useCallback, useEffect, useRef, useState, type ReactNode } from 'react'
import { CardText } from '../../components/PlayingCard'
import { shuffle } from '../lab/lib/poker'
import { loadProgress, recordAnswer } from './progress'
import './learn.css'

export function Section({ title, children, kind }: { title: string; children: ReactNode; kind?: 'try' | 'drill' }) {
  return (
    <section className={'lsec' + (kind ? ` lsec-${kind}` : '')}>
      <h2>
        {kind === 'try' && <span className="lsec-tag">Interactive</span>}
        {kind === 'drill' && <span className="lsec-tag">Drill</span>}
        {title}
      </h2>
      {children}
    </section>
  )
}

export function Formula({ children, label }: { children: ReactNode; label?: string }) {
  return (
    <div className="formula">
      {label && <div className="formula-label">{label}</div>}
      <div className="formula-body">{children}</div>
    </div>
  )
}

export function Callout({ children, title }: { children: ReactNode; title?: string }) {
  return (
    <div className="callout">
      {title && <b>{title} </b>}
      {children}
    </div>
  )
}

/** Inline four-colour card text: <C>AsKd</C>. */
export function C({ children }: { children: string }) {
  return <CardText text={children} />
}

export function Slider({
  label,
  value,
  onChange,
  min,
  max,
  step = 1,
  unit,
  format,
}: {
  label: ReactNode
  value: number
  onChange: (v: number) => void
  min: number
  max: number
  step?: number
  unit?: string
  format?: (v: number) => string
}) {
  return (
    <div className="slider">
      <div className="slider-top">
        <label>{label}</label>
        <span className="slider-val">
          <input
            type="number"
            value={Number.isFinite(value) ? value : ''}
            min={min}
            step={step}
            onChange={(e) => {
              const v = Number(e.target.value)
              if (e.target.value !== '' && Number.isFinite(v)) onChange(v)
            }}
          />
          {unit && <span className="faint">{unit}</span>}
          {format && <span className="faint">{format(value)}</span>}
        </span>
      </div>
      <input type="range" min={min} max={max} step={step} value={Math.min(max, Math.max(min, value))} onChange={(e) => onChange(Number(e.target.value))} />
    </div>
  )
}

export function Stat({ label, value, sub, tone }: { label: ReactNode; value: ReactNode; sub?: ReactNode; tone?: 'good' | 'bad' | 'accent' }) {
  return (
    <div className={'lstat' + (tone ? ` tone-${tone}` : '')}>
      <div className="k">{label}</div>
      <div className="v">{value}</div>
      {sub && <div className="s">{sub}</div>}
    </div>
  )
}

export interface BarPart {
  label: string
  value: number
  color: string
}

/** Horizontal part-to-whole bar with labels underneath. */
export function SplitBar({ parts, height = 22 }: { parts: BarPart[]; height?: number }) {
  const total = parts.reduce((s, p) => s + Math.max(0, p.value), 0) || 1
  return (
    <div>
      <div className="splitbar" style={{ height }}>
        {parts.map((p, i) => (
          <span key={i} style={{ flexGrow: Math.max(0, p.value), background: p.color }} title={`${p.label}: ${((100 * p.value) / total).toFixed(1)}%`} />
        ))}
      </div>
      <div className="splitbar-legend">
        {parts.map((p, i) => (
          <span key={i}>
            <i style={{ background: p.color }} />
            {p.label} <b>{((100 * p.value) / total).toFixed(1)}%</b>
          </span>
        ))}
      </div>
    </div>
  )
}

// ---- formatting ----

export const pct1 = (x: number) => (100 * x).toFixed(1) + '%'
export const pct0 = (x: number) => (100 * x).toFixed(0) + '%'
/** "12.5bb", "10bb" */
export function bbs(x: number, d = 1) {
  const r = Math.round(x * 10 ** d) / 10 ** d
  return (Object.is(r, -0) ? 0 : r).toString() + 'bb'
}
export function signedBb(x: number, d = 1) {
  const s = bbs(x, d)
  return x > 0.0000001 && !s.startsWith('0bb') ? '+' + s : s
}

// ---- drills ----

export interface Question {
  /** identifies the question to avoid immediate repeats */
  key: string
  prompt: ReactNode
  /** multiple choice */
  choices?: ReactNode[]
  correct?: number[]
  /** free numeric answer */
  numeric?: { answer: number; tolerance: number; unit?: string }
  explanation: ReactNode
}

/** Builds shuffled multiple-choice options from the correct label and distractors (deduplicated). */
export function makeChoices(correct: string, wrong: string[], n = 4): { choices: string[]; correct: number[] } {
  const uniq = [...new Set(wrong.filter((w) => w !== correct))].slice(0, n - 1)
  const all = shuffle([correct, ...uniq])
  return { choices: all, correct: [all.indexOf(correct)] }
}

/** Multiple-choice options in a fixed order (e.g. Call / Fold). */
export function fixedChoices(labels: string[], correct: string[]): { choices: string[]; correct: number[] } {
  return { choices: labels, correct: labels.map((l, i) => (correct.includes(l) ? i : -1)).filter((i) => i >= 0) }
}

export function Drill({ slug, generate, intro }: { slug: string; generate: () => Question; intro?: ReactNode }) {
  const [q, setQ] = useState<Question>(() => generate())
  const [picked, setPicked] = useState<number | null>(null)
  const [entry, setEntry] = useState('')
  const [result, setResult] = useState<boolean | null>(null)
  const [score, setScore] = useState({ right: 0, total: 0, streak: 0, best: 0 })
  const [allTime, setAllTime] = useState(() => loadProgress()[slug])
  const nextRef = useRef<HTMLButtonElement>(null)
  const boxRef = useRef<HTMLDivElement>(null)

  const finish = useCallback(
    (ok: boolean) => {
      const streak = ok ? score.streak + 1 : 0
      recordAnswer(slug, ok, streak)
      setScore({ right: score.right + (ok ? 1 : 0), total: score.total + 1, streak, best: Math.max(score.best, streak) })
      setAllTime(loadProgress()[slug])
      setResult(ok)
    },
    [slug, score],
  )

  const choose = useCallback(
    (i: number) => {
      if (result !== null || !q.choices) return
      setPicked(i)
      finish((q.correct ?? []).includes(i))
    },
    [q, result, finish],
  )

  const submitNumber = () => {
    if (result !== null || !q.numeric) return
    const v = Number(entry.replace('%', '').trim())
    if (entry.trim() === '' || !Number.isFinite(v)) return
    finish(Math.abs(v - q.numeric.answer) <= q.numeric.tolerance + 1e-9)
  }

  const next = useCallback(() => {
    let n = generate()
    for (let i = 0; i < 6 && n.key === q.key; i++) n = generate()
    setQ(n)
    setPicked(null)
    setEntry('')
    setResult(null)
  }, [generate, q.key])

  useEffect(() => {
    if (result !== null) nextRef.current?.focus({ preventScroll: true })
  }, [result])

  // Keys: 1-5 pick an option, Enter moves on. Ignored while typing in other inputs.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement | null
      const inDrill = !!t && !!boxRef.current?.contains(t)
      if (t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.tagName === 'SELECT') && !inDrill) return
      if (!boxRef.current) return
      const r = boxRef.current.getBoundingClientRect()
      if (r.bottom < 0 || r.top > window.innerHeight) return
      if (result === null && q.choices && /^[1-9]$/.test(e.key)) {
        const i = Number(e.key) - 1
        if (i < q.choices.length) {
          e.preventDefault()
          choose(i)
        }
      } else if (result !== null && e.key === 'Enter' && t?.tagName !== 'BUTTON') {
        e.preventDefault()
        next()
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [q, result, choose, next])

  const answerText = q.choices ? (q.correct ?? []).map((i) => q.choices![i]) : null

  return (
    <div className="drill" ref={boxRef}>
      <div className="drill-score">
        <span>
          This session: <b>{score.right}</b> / {score.total}
          {score.total > 0 && <span className="faint"> ({Math.round((100 * score.right) / score.total)}%)</span>}
        </span>
        <span>
          Streak <b>{score.streak}</b>
          <span className="faint"> · best {score.best}</span>
        </span>
        {allTime && allTime.total > 0 && (
          <span className="faint">
            All time {allTime.right}/{allTime.total}
          </span>
        )}
      </div>
      {intro && <div className="drill-intro">{intro}</div>}
      <div className="drill-q">{q.prompt}</div>
      {q.choices && (
        <div className="drill-choices">
          {q.choices.map((c, i) => {
            const isRight = (q.correct ?? []).includes(i)
            const cls = result === null ? '' : isRight ? ' right' : picked === i ? ' wrong' : ' dim'
            return (
              <button key={i} type="button" className={'drill-choice' + cls} onClick={() => choose(i)} disabled={result !== null && !isRight && picked !== i}>
                <span className="kbd">{i + 1}</span>
                <span>{c}</span>
              </button>
            )
          })}
        </div>
      )}
      {q.numeric && (
        <form
          className="row drill-num"
          onSubmit={(e) => {
            e.preventDefault()
            submitNumber()
          }}
        >
          <input
            type="text"
            inputMode="decimal"
            value={entry}
            onChange={(e) => setEntry(e.target.value)}
            disabled={result !== null}
            placeholder="your answer"
            autoComplete="off"
            className={result === null ? '' : result ? 'ok' : 'bad'}
          />
          {q.numeric.unit && <span className="muted">{q.numeric.unit}</span>}
          <button type="submit" className="primary" disabled={result !== null || entry.trim() === ''}>
            Check
          </button>
        </form>
      )}
      {result !== null && (
        <div className={'drill-feedback ' + (result ? 'ok' : 'bad')}>
          <div className="drill-verdict">
            {result ? 'Correct.' : 'Not quite.'}{' '}
            {!result && q.numeric && (
              <span>
                The answer is {+q.numeric.answer.toFixed(2)}
                {q.numeric.unit ?? ''}.
              </span>
            )}
            {!result && answerText && answerText.length === 1 && <span>The answer is {answerText[0]}.</span>}
            {!result && answerText && answerText.length > 1 && <span>Acceptable answers: {answerText.map((a, i) => <span key={i}>{i ? ' or ' : ''}{a}</span>)}.</span>}
          </div>
          <div className="drill-expl">{q.explanation}</div>
          <button ref={nextRef} type="button" className="primary" onClick={next}>
            Next question <span className="kbd">Enter</span>
          </button>
        </div>
      )}
    </div>
  )
}
