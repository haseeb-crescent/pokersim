import { classAt } from '../api/client'
import './components.css'

export const ACTION_COLORS = ['var(--act-fold)', 'var(--act-call)', 'var(--act-bet-s)', 'var(--act-bet-m)', 'var(--act-bet-l)', 'var(--act-allin)']

/** Colour for an action label: fold blue, check/call green, bets warm by size, all-in dark red. */
export function actionColor(label: string, index: number, count: number): string {
  const l = label.toLowerCase()
  if (l.startsWith('fold') || l === 'f') return 'var(--act-fold)'
  if (l.startsWith('check') || l.startsWith('call') || l === 'c' || l === 'k') return 'var(--act-call)'
  if (l.startsWith('all') || l === 'a') return 'var(--act-allin)'
  const warm = ['var(--act-bet-s)', 'var(--act-bet-m)', 'var(--act-bet-l)']
  return warm[Math.min(warm.length - 1, Math.max(0, index - (count - 1 - warm.length)))] ?? 'var(--act-bet-m)'
}

interface Props {
  /** per action, 169 frequencies (0..1); stacked horizontally in each cell */
  strategy?: number[][]
  actionLabels?: string[]
  /** 169 weights (0..1); scales cell fill height, or sets opacity when no strategy */
  weights?: number[]
  /** single-colour heat map when no strategy is given */
  color?: string
  highlight?: string | null
  onCellClick?: (cls: string, index: number) => void
  /** called with the class under the pointer, or null when the pointer leaves the grid */
  onCellHover?: (cls: string | null, index: number) => void
  size?: number
  showLabels?: boolean
}

/** 13x13 hand-class grid. With `strategy`, each cell shows the action mix as coloured bands
 *  (fill height = weight). Without, it is a heat map of `weights`. */
export function RangeGrid({ strategy, actionLabels = [], weights, color = 'var(--accent)', highlight, onCellClick, onCellHover, size = 34, showLabels = true }: Props) {
  const cells = []
  for (let row = 0; row < 13; row++) {
    for (let col = 0; col < 13; col++) {
      const i = row * 13 + col
      const cls = classAt(row, col)
      const w = weights ? Math.max(0, Math.min(1, weights[i])) : 1
      let fill
      if (strategy && strategy.length) {
        let x = 0
        const bands = strategy.map((s, a) => {
          const f = Math.max(0, s[i] ?? 0)
          const el = (
            <span
              key={a}
              className="rg-band"
              style={{ left: `${x * 100}%`, width: `${f * 100}%`, background: actionColor(actionLabels[a] ?? '', a, strategy.length) }}
            />
          )
          x += f
          return el
        })
        fill = (
          <span className="rg-fill" style={{ height: `${w * 100}%` }}>
            {bands}
          </span>
        )
      } else {
        fill = <span className="rg-fill" style={{ height: '100%', background: color, opacity: w }} />
      }
      cells.push(
        <div
          key={i}
          className={'rg-cell' + (highlight === cls ? ' rg-hl' : '') + (row === col ? ' rg-pair' : '')}
          style={{ width: size, height: size }}
          onClick={() => onCellClick?.(cls, i)}
          onMouseEnter={onCellHover ? () => onCellHover(cls, i) : undefined}
          title={cls}
        >
          {fill}
          {showLabels && <span className="rg-label">{cls}</span>}
        </div>,
      )
    }
  }
  return (
    <div className="rg" style={{ gridTemplateColumns: `repeat(13, ${size}px)` }} onMouseLeave={onCellHover ? () => onCellHover(null, -1) : undefined}>
      {cells}
    </div>
  )
}

/** Horizontal stacked bar for an action mix with a legend. */
export function StrategyBar({ labels, freqs, height = 10 }: { labels: string[]; freqs: number[]; height?: number }) {
  let x = 0
  return (
    <div>
      <div className="sbar" style={{ height }}>
        {freqs.map((f, i) => {
          const el = <span key={i} style={{ left: `${x * 100}%`, width: `${f * 100}%`, background: actionColor(labels[i], i, labels.length) }} />
          x += f
          return el
        })}
      </div>
      <div className="sbar-legend">
        {labels.map((l, i) => (
          <span key={i}>
            <i style={{ background: actionColor(l, i, labels.length) }} />
            {l} {(100 * (freqs[i] ?? 0)).toFixed(0)}%
          </span>
        ))}
      </div>
    </div>
  )
}
