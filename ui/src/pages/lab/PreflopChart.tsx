import { useCallback, useEffect, useState } from 'react'
import { classAt, comboCount, pct } from '../../api/client'
import type { PreflopNode } from '../../api/types'
import { RangeGrid, StrategyBar, actionColor } from '../../components/RangeGrid'
import { describeApiError, fetchPreflopNode, type ApiProblem } from './api'
import './lab.css'

export function usePreflopNode(key: string | null) {
  const [state, setState] = useState<{ key: string | null; node: PreflopNode | null; error: ApiProblem | null }>({ key: null, node: null, error: null })
  const [nonce, setNonce] = useState(0)
  useEffect(() => {
    if (key === null) return
    let live = true
    fetchPreflopNode(key).then(
      (node) => live && setState({ key, node, error: null }),
      (e) => live && setState({ key, node: null, error: describeApiError(e) }),
    )
    return () => {
      live = false
    }
  }, [key, nonce])
  const reload = useCallback(() => setNonce((n) => n + 1), [])
  const loading = key !== null && state.key !== key
  return { node: state.key === key ? state.node : null, error: state.key === key ? state.error : null, loading, reload }
}

/** Reach weight (0..1) of the acting player's range for each class; 1s when not provided. */
export function actingRange(node: PreflopNode): number[] {
  const r = node.ranges?.[node.player]
  return r && r.length === 169 ? r : new Array(169).fill(1)
}

/** Overall frequency of each action over the acting player's range, weighted by combos. */
export function actionTotals(node: PreflopNode): number[] {
  const w = actingRange(node)
  let total = 0
  const sums = node.actions.map(() => 0)
  for (let i = 0; i < 169; i++) {
    const n = comboCount(i) * w[i]
    total += n
    node.freq.forEach((f, a) => (sums[a] += n * (f[i] ?? 0)))
  }
  return sums.map((s) => (total > 0 ? s / total : 0))
}

export function rangeCombos(node: PreflopNode): number {
  const w = actingRange(node)
  let n = 0
  for (let i = 0; i < 169; i++) n += comboCount(i) * w[i]
  return n
}

export function actionLabel(a: PreflopNode['actions'][number]): string {
  return a.label || a.code
}

/** Chart grid with per-action totals and the mix of the hovered or selected class. */
export function PreflopChartView({ node, size = 30, selected, onSelect }: { node: PreflopNode; size?: number; selected?: number | null; onSelect?: (index: number | null) => void }) {
  const [hover, setHover] = useState<number | null>(null)
  const [localSel, setLocalSel] = useState<number | null>(null)
  const sel = selected !== undefined ? selected : localSel
  const setSel = onSelect ?? setLocalSel
  const labels = node.actions.map(actionLabel)
  const totals = actionTotals(node)
  const reach = actingRange(node)
  const combos = rangeCombos(node)
  const focus = hover ?? sel
  const focusName = focus !== null ? cellName(focus) : null
  return (
    <div className="pfc">
      <div className="rg-click">
        <RangeGrid
          strategy={node.freq}
          actionLabels={labels}
          weights={reach}
          size={size}
          highlight={sel !== null ? cellName(sel) : null}
          onCellClick={(_, i) => setSel(sel === i ? null : i)}
          onCellHover={(_, i) => setHover(i >= 0 ? i : null)}
        />
      </div>
      <div className="pfc-side">
        <div className="pfc-block">
          <div className="pfc-cap">
            {node.player} range here: {combos.toFixed(0)} combos ({pct(combos / 1326, 1)} of hands)
          </div>
          <StrategyBar labels={labels} freqs={totals} height={12} />
          <table className="pfc-table">
            <thead>
              <tr>
                <th>Action</th>
                <th>Of range</th>
                <th>Combos</th>
              </tr>
            </thead>
            <tbody>
              {node.actions.map((a, i) => (
                <tr key={a.code}>
                  <td>
                    <i className="swatch" style={{ background: actionColor(labels[i], i, labels.length) }} />
                    {labels[i]}
                  </td>
                  <td className="mono">{pct(totals[i], 1)}</td>
                  <td className="mono">{(totals[i] * combos).toFixed(1)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        <div className="pfc-block pfc-cell">
          {focus === null || focusName === null ? (
            <span className="faint">Hover or click a cell to see that hand's mix.</span>
          ) : (
            <>
              <div className="pfc-cellhead">
                <b>{focusName}</b>
                <span className="faint">
                  {comboCount(focus)} combos{reach[focus] < 0.999 ? ` · ${pct(reach[focus], 0)} of them reach this spot` : ''}
                </span>
              </div>
              {reach[focus] <= 0.001 ? (
                <span className="faint">{node.player} never gets here with {focusName} (it took a different action earlier).</span>
              ) : (
                <StrategyBar labels={labels} freqs={node.freq.map((f) => f[focus] ?? 0)} height={12} />
              )}
            </>
          )}
        </div>
      </div>
    </div>
  )
}

const cellName = (i: number) => classAt(Math.floor(i / 13), i % 13)
