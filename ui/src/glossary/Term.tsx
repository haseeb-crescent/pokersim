import { useCallback, useEffect, useRef, useState, type ReactNode } from 'react'
import { createPortal } from 'react-dom'
import { Link } from 'react-router-dom'
import { CardText } from '../components/PlayingCard'
import { findTerm, splitTerms } from './match'
import './glossary.css'

/** Renders text where bracketed card runs like "[Ks7d2c]" become four-colour card text.
 *  With `link`, known terms are wrapped in <Term> (first occurrence of each, sharing `seen`). */
export function RichText({ text, link = false, exclude = [] }: { text: string; link?: boolean; exclude?: string[] }) {
  const seen = new Set<string>()
  const parts = text.split(/\[((?:[2-9TJQKA][shdc])+)\]/g)
  const out: ReactNode[] = []
  parts.forEach((p, i) => {
    if (i % 2 === 1) {
      out.push(<CardText key={i} text={p} />)
    } else if (link) {
      splitTerms(p, seen, exclude).forEach((piece, j) =>
        out.push(
          piece.id ? (
            <Term key={`${i}-${j}`} id={piece.id}>
              {piece.text}
            </Term>
          ) : (
            <span key={`${i}-${j}`}>{piece.text}</span>
          ),
        ),
      )
    } else if (p) {
      out.push(<span key={i}>{p}</span>)
    }
  })
  return <>{out}</>
}

/** Wraps known terms in plain text (e.g. a coach explanation) in <Term> links: longest match
 *  first, word boundaries, first occurrence of each term only. */
export function LinkTerms({ text, exclude }: { text: string; exclude?: string[] }) {
  const pieces = splitTerms(text ?? '', new Set(), exclude ?? [])
  return (
    <>
      {pieces.map((p, i) =>
        p.id ? (
          <Term key={i} id={p.id}>
            {p.text}
          </Term>
        ) : (
          <span key={i}>{p.text}</span>
        ),
      )}
    </>
  )
}

/** Inline link to /glossary/<id> with a hover tooltip showing the definition. `id` may also be
 *  an alias ("SPR"); without children the term's name is shown. */
export function Term({ id, children }: { id?: string; children?: ReactNode }) {
  const key = id ?? (typeof children === 'string' ? children : '')
  const t = findTerm(key)
  const ref = useRef<HTMLAnchorElement>(null)
  const [rect, setRect] = useState<DOMRect | null>(null)
  const timer = useRef<number | undefined>(undefined)

  const show = useCallback(() => {
    window.clearTimeout(timer.current)
    timer.current = window.setTimeout(() => {
      if (ref.current) setRect(ref.current.getBoundingClientRect())
    }, 120)
  }, [])
  const hide = useCallback(() => {
    window.clearTimeout(timer.current)
    setRect(null)
  }, [])
  useEffect(() => () => window.clearTimeout(timer.current), [])

  if (!t) {
    if (import.meta.env.DEV) console.warn(`Glossary: no term "${key}"`)
    return <span>{children}</span>
  }
  return (
    <>
      <Link
        ref={ref}
        to={`/glossary/${t.id}`}
        className="term"
        onMouseEnter={show}
        onMouseLeave={hide}
        onFocus={show}
        onBlur={hide}
        onClick={hide}
        aria-describedby={rect ? `tip-${t.id}` : undefined}
      >
        {children ?? t.term}
      </Link>
      {rect && createPortal(<Tooltip id={`tip-${t.id}`} rect={rect} title={t.term} def={t.def} />, document.body)}
    </>
  )
}

function Tooltip({ id, rect, title, def }: { id: string; rect: DOMRect; title: string; def: string }) {
  const width = 300
  const left = Math.max(8, Math.min(window.innerWidth - width - 8, rect.left + rect.width / 2 - width / 2))
  const below = rect.bottom + 140 < window.innerHeight
  const style = below ? { left, top: rect.bottom + 6, width } : { left, bottom: window.innerHeight - rect.top + 6, width }
  return (
    <div id={id} role="tooltip" className="term-tip" style={style}>
      <div className="term-tip-title">{title}</div>
      <div className="term-tip-def">
        <RichText text={def} />
      </div>
      <div className="term-tip-more">Click for a worked example</div>
    </div>
  )
}
