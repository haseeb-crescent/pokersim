import type React from 'react'
import './components.css'

const SUIT_GLYPH: Record<string, string> = { s: '♠', h: '♥', d: '♦', c: '♣' }

/** A playing card in the four-colour deck. `card` like "As"; null renders a card back. */
export function PlayingCard({ card, size = 'md' }: { card: string | null; size?: 'sm' | 'md' | 'lg' }) {
  if (!card) return <span className={`pcard back ${size}`} />
  const rank = card[0] === 'T' ? '10' : card[0]
  const suit = card[1]
  return (
    <span className={`pcard ${size} suit-${suit}`}>
      <span className="pcard-rank">{rank}</span>
      <span className="pcard-suit">{SUIT_GLYPH[suit]}</span>
    </span>
  )
}

export function Cards({ cards, size = 'md' }: { cards: string[]; size?: 'sm' | 'md' | 'lg' }) {
  return (
    <span className="pcards">
      {cards.map((c, i) => (
        <PlayingCard key={i} card={c} size={size} />
      ))}
    </span>
  )
}

/** Inline text rendering of cards with suit colours, e.g. in logs. Only runs of cards that
 *  stand alone ("AsKd", "Td9d6h") are rendered as cards, so words like "The" are left alone. */
export function CardText({ text }: { text: string }) {
  const out: React.ReactNode[] = []
  const re = /(?<![A-Za-z0-9])((?:[2-9TJQKA][shdc])+)(?![A-Za-z0-9])/g
  let last = 0
  let m: RegExpExecArray | null
  let k = 0
  while ((m = re.exec(text))) {
    if (m.index > last) out.push(<span key={k++}>{text.slice(last, m.index)}</span>)
    for (const c of m[1].match(/../g) ?? []) {
      out.push(
        <span key={k++} className={`ctext suit-${c[1]}`}>
          {c[0]}
          {SUIT_GLYPH[c[1]]}
        </span>,
      )
    }
    last = m.index + m[0].length
  }
  if (last < text.length) out.push(<span key={k++}>{text.slice(last)}</span>)
  return <>{out}</>
}
