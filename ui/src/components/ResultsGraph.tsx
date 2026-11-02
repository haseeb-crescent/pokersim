/** Cumulative results over hands: actual winnings and the all-in-adjusted (EV) line, in bb. */
export function ResultsGraph({
  pts,
  height = 220,
  bbDollars,
  empty = 'Play some hands to see the graph.',
}: {
  pts: { hand: number; net: number; adj: number }[]
  height?: number
  /** when set, the y-axis labels also show dollars */
  bbDollars?: number
  empty?: string
}) {
  if (pts.length < 2) return <p className="faint small">{empty}</p>
  const W = 900
  const H = height
  const xs = pts.map((p) => p.hand)
  const ys = pts.flatMap((p) => [p.net, p.adj])
  const x0 = Math.min(...xs)
  const x1 = Math.max(...xs)
  const y0 = Math.min(0, ...ys)
  const y1 = Math.max(0, ...ys)
  const X = (x: number) => ((x - x0) / Math.max(1, x1 - x0)) * (W - 40) + 30
  const Y = (y: number) => H - 20 - ((y - y0) / Math.max(1e-9, y1 - y0)) * (H - 40)
  const path = (k: 'net' | 'adj') => pts.map((p, i) => `${i ? 'L' : 'M'}${X(p.hand).toFixed(1)},${Y(p[k]).toFixed(1)}`).join(' ')
  const label = (v: number) => {
    const b = `${v.toFixed(0)}bb`
    return bbDollars ? `${b} ($${(v * bbDollars).toFixed(0)})` : b
  }
  const last = pts[pts.length - 1]
  return (
    <svg viewBox={`0 0 ${W} ${H}`} style={{ width: '100%', maxWidth: W }} role="img" aria-label={`Results over ${pts.length} hands: ${last.net.toFixed(1)}bb actual, ${last.adj.toFixed(1)}bb all-in adjusted`}>
      <line x1={30} x2={W - 10} y1={Y(0)} y2={Y(0)} stroke="var(--border)" />
      <path d={path('adj')} fill="none" stroke="var(--warn)" strokeWidth={1.5} strokeDasharray="4 3" />
      <path d={path('net')} fill="none" stroke="var(--accent)" strokeWidth={2} />
      <text x={34} y={14} fill="var(--text-2)" fontSize={11}>
        {label(y1)}
      </text>
      <text x={34} y={H - 6} fill="var(--text-2)" fontSize={11}>
        {label(y0)} · {pts.length} hands
      </text>
      <text x={W - 220} y={14} fill="var(--accent)" fontSize={11}>
        — net won
      </text>
      <text x={W - 130} y={14} fill="var(--warn)" fontSize={11}>
        - - all-in adjusted
      </text>
    </svg>
  )
}
