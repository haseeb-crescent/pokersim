import { useState } from 'react'
import { PlayingCard } from '../../components/PlayingCard'
import { cardStr, parseCards } from './lib/poker'
import './lab.css'

const RANK_ORDER = [12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0]

/** 4 x 13 deck for picking cards. `used` cards (taken elsewhere) are disabled. */
export function CardPicker({ selected, used, onToggle, full }: { selected: number[]; used?: Set<number>; onToggle: (card: number) => void; full?: boolean }) {
  return (
    <div className="cpick" role="group" aria-label="Card picker">
      {[0, 1, 2, 3].map((suit) => (
        <div key={suit} className="cpick-row">
          {RANK_ORDER.map((rank) => {
            const c = rank * 4 + suit
            const on = selected.includes(c)
            const taken = !on && used?.has(c)
            return (
              <button
                key={c}
                type="button"
                className={'cpick-card' + (on ? ' on' : '')}
                disabled={taken || (!on && full)}
                onClick={() => onToggle(c)}
                title={cardStr(c)}
                aria-pressed={on}
              >
                <PlayingCard card={cardStr(c)} size="sm" />
              </button>
            )
          })}
        </div>
      ))}
    </div>
  )
}

/** Text input for cards ("Td9d6h") with an optional picker and inline validation. */
export function CardInput({
  label,
  value,
  onChange,
  max,
  used,
  placeholder,
  allowed,
}: {
  label: string
  value: string
  onChange: (v: string) => void
  max: number
  used?: Set<number>
  placeholder?: string
  /** allowed card counts, e.g. [0, 3, 4, 5] for a board */
  allowed?: number[]
}) {
  const [open, setOpen] = useState(false)
  const { cards, error } = parseCards(value)
  const clash = cards.find((c) => used?.has(c))
  const countErr = !error && allowed && !allowed.includes(cards.length) ? `needs ${allowed.filter((n) => n > 0).join(', ')} cards` : null
  const tooMany = !error && cards.length > max ? `at most ${max} cards` : null
  const problem = error ?? tooMany ?? countErr ?? (clash !== undefined ? `${cardStr(clash)} is already used` : null)
  const toggle = (c: number) => {
    const next = cards.includes(c) ? cards.filter((x) => x !== c) : [...cards, c]
    onChange(next.map(cardStr).join(''))
  }
  return (
    <div className="cinput">
      <label>{label}</label>
      <div className="row">
        <input className={'grow mono' + (problem && value ? ' invalid' : '')} value={value} placeholder={placeholder} onChange={(e) => onChange(e.target.value)} spellCheck={false} />
        <button type="button" onClick={() => setOpen((o) => !o)} aria-expanded={open}>
          {open ? 'Close' : 'Pick'}
        </button>
        {value && (
          <button type="button" className="ghost" onClick={() => onChange('')} title="Clear">
            ✕
          </button>
        )}
      </div>
      {problem && value && <div className="cinput-err">{problem}</div>}
      {open && <CardPicker selected={error ? [] : cards} used={used} onToggle={toggle} full={!error && cards.length >= max} />}
    </div>
  )
}

/** Validation result for a card field, for callers that need to block a request. */
export function cardFieldProblem(value: string, max: number, allowed?: number[], used?: Set<number>): string | null {
  const { cards, error } = parseCards(value)
  if (error) return error
  if (cards.length > max) return `at most ${max} cards`
  if (allowed && !allowed.includes(cards.length)) return `needs ${allowed.filter((n) => n > 0).join(', ')} cards`
  const clash = cards.find((c) => used?.has(c))
  return clash !== undefined ? `${cardStr(clash)} is used twice` : null
}
