import { useMemo, useRef, useState, type MouseEvent } from 'react'
import { Term } from '../../glossary/Term'
import { compact, gaussian, niceTicks, normCdf, normPdf, quantile, rng } from './lib/stats'
import './lab.css'

const Z95 = 1.959964
const Z90 = 1.644854

interface Sim {
  steps: number
  stepHands: number
  paths: Float64Array[] // cumulative bb at step 0..steps (first entry 0)
  finals: Float64Array // sorted
  simLose: number
}

function simulate(wr: number, sd: number, hands: number, samples: number, shown: number, seed: number): Sim {
  const steps = Math.max(1, Math.min(200, Math.ceil(hands / 100)))
  const stepHands = hands / steps
  // Sum of stepHands/100 independent 100-hand blocks, each N(wr, sd^2).
  const mu = (wr * stepHands) / 100
  const sigma = sd * Math.sqrt(stepHands / 100)
  const g = gaussian(rng(seed))
  const finals = new Float64Array(samples)
  const paths: Float64Array[] = []
  let lose = 0
  for (let s = 0; s < samples; s++) {
    const keep = s < shown
    const path = keep ? new Float64Array(steps + 1) : null
    let cum = 0
    for (let k = 1; k <= steps; k++) {
      cum += mu + sigma * g()
      if (path) path[k] = cum
    }
    if (path) paths.push(path)
    finals[s] = cum
    if (cum < 0) lose++
  }
  finals.sort()
  return { steps, stepHands, paths, finals, simLose: lose / samples }
}

const fmt = (v: number, d = 0) => (Number.isFinite(v) ? v.toLocaleString(undefined, { maximumFractionDigits: d, minimumFractionDigits: d }) : '–')
const signed = (v: number, d = 0) => (v > 0 ? '+' : '') + fmt(v, d)

export default function VariancePage() {
  const [wr, setWr] = useState(5)
  const [sd, setSd] = useState(100)
  const [hands, setHands] = useState(50000)
  const [samples, setSamples] = useState(2000)
  const [seed, setSeed] = useState(1)

  const valid = Number.isFinite(wr) && sd > 0 && hands >= 100 && samples >= 10
  const H = Math.min(Math.max(100, Math.round(hands)), 10_000_000)
  const S = Math.min(Math.max(10, Math.round(samples)), 20000)
  const sim = useMemo(() => (valid ? simulate(wr, sd, H, S, 40, seed) : null), [valid, wr, sd, H, S, seed])

  const mean = (wr * H) / 100
  const sdN = sd * Math.sqrt(H / 100)
  const pLose = normCdf(-mean / sdN)
  const ciHalf = (Z95 * sd) / Math.sqrt(H / 100)
  const handsToExclude = wr !== 0 ? 100 * ((Z95 * sd) / wr) ** 2 : Infinity

  return (
    <div className="col" style={{ gap: 16 }}>
      <p className="lab-sub">
        How much can luck move your results? This uses the standard normal model: every 100 hands your result is drawn from a normal distribution with mean = your <Term id="bb-100">win rate</Term> and spread = your <Term id="standard-deviation">standard deviation</Term>. Totals over N hands then have mean wr × N/100 and SD sd × √(N/100).
      </p>
      <div className="panel">
        <div className="var-inputs">
          <NumInput label="Win rate (bb/100)" value={wr} step={0.5} onChange={setWr} />
          <NumInput label="Std. deviation (bb/100)" value={sd} step={5} min={1} onChange={setSd} hint="6-max NL: ~80–120" />
          <NumInput label="Hands" value={hands} step={10000} min={100} onChange={setHands} />
          <NumInput label="Simulated samples" value={samples} step={500} min={10} max={20000} onChange={setSamples} hint="max 20,000" />
          <button type="button" onClick={() => setSeed((s) => s + 1)}>
            Re-run simulation
          </button>
        </div>
      </div>

      {!valid || !sim ? (
        <div className="panel err">Enter a standard deviation above 0, at least 100 hands and at least 10 samples.</div>
      ) : (
        <>
          <div className="stat-row">
            <div className="stat">
              <div className="k">Expected result</div>
              <div className="v">{signed(mean)}bb</div>
              <div className="s">SD ±{fmt(sdN)}bb over {fmt(H)} hands</div>
            </div>
            <div className="stat">
              <div className="k">P(losing after {compact(H)} hands)</div>
              <div className="v">{(100 * pLose).toFixed(1)}%</div>
              <div className="s">simulated: {(100 * sim.simLose).toFixed(1)}%</div>
            </div>
            <div className="stat">
              <div className="k">Your measured win rate, 95% CI</div>
              <div className="v" style={{ fontSize: 17 }}>
                {fmt(wr - ciHalf, 1)} to {fmt(wr + ciHalf, 1)}
              </div>
              <div className="s">bb/100 (±{fmt(ciHalf, 1)}) after {fmt(H)} hands</div>
            </div>
            <div className="stat">
              <div className="k">Hands for the 95% CI to exclude 0</div>
              <div className="v">{Number.isFinite(handsToExclude) ? compact(Math.ceil(handsToExclude)) : 'never'}</div>
              <div className="s">{Number.isFinite(handsToExclude) ? `100 × (1.96 × ${fmt(sd)} / ${fmt(Math.abs(wr), 1)})² = ${fmt(Math.ceil(handsToExclude))}` : 'a 0 bb/100 win rate is never distinguishable from 0'}</div>
            </div>
          </div>

          <div className="panel">
            <h2>Sample paths</h2>
            <FanChart sim={sim} wr={wr} sd={sd} hands={H} />
            <div className="legend">
              <span>
                <i style={{ background: '#7d8da1' }} />
                {sim.paths.length} simulated players
              </span>
              <span>
                <i style={{ background: 'var(--accent)' }} />
                Expected ({signed(wr, 1)} bb/100)
              </span>
              <span>
                <i className="box" style={{ background: 'rgba(45,212,191,0.16)' }} />
                95% band (expected ± 1.96 SD)
              </span>
            </div>
          </div>

          <div className="lab-grid">
            <div className="panel">
              <h2>Final results of {fmt(S)} simulated players</h2>
              <Histogram finals={sim.finals} mean={mean} sdN={sdN} />
              <div className="legend">
                <span>
                  <i className="box" style={{ background: 'var(--accent-2)' }} />
                  Won
                </span>
                <span>
                  <i className="box" style={{ background: 'var(--bad)' }} />
                  Lost
                </span>
                <span>
                  <i style={{ background: 'var(--text-2)' }} />
                  Normal model
                </span>
              </div>
            </div>
            <div className="panel">
              <h2>Percentiles after {fmt(H)} hands</h2>
              <table className="compact">
                <thead>
                  <tr>
                    <th>Percentile</th>
                    <th>Simulated</th>
                    <th>Normal model</th>
                    <th>As bb/100</th>
                  </tr>
                </thead>
                <tbody>
                  {[
                    { p: 0.05, z: -Z90, label: '5th (bad luck)' },
                    { p: 0.5, z: 0, label: '50th (median)' },
                    { p: 0.95, z: Z90, label: '95th (good luck)' },
                  ].map((r) => {
                    const model = mean + r.z * sdN
                    return (
                      <tr key={r.p}>
                        <td>{r.label}</td>
                        <td className="mono">{signed(quantile(sim.finals, r.p))}bb</td>
                        <td className="mono">{signed(model)}bb</td>
                        <td className="mono">{signed((model / H) * 100, 1)}</td>
                      </tr>
                    )
                  })}
                </tbody>
              </table>
              <p className="note" style={{ marginTop: 10 }}>
                One in twenty players with exactly your skill finishes at or below the 5th percentile. The model ignores rake changes, opponent pools and tilt, and treats blocks of hands as independent.
              </p>
              <h3 style={{ marginTop: 14 }}>The formulas</h3>
              <div className="note mono" style={{ lineHeight: 1.8 }}>
                mean = wr × N/100 = {fmt(mean)}bb
                <br />
                SD = sd × √(N/100) = {fmt(sdN)}bb
                <br />
                P(lose) = Φ(−mean / SD) = Φ({(-mean / sdN).toFixed(2)}) = {(100 * pLose).toFixed(1)}%
                <br />
                N for CI to exclude 0 = 100 × (1.96 sd / wr)²
              </div>
            </div>
          </div>
        </>
      )}
    </div>
  )
}

function NumInput({ label, value, onChange, step, min, max, hint }: { label: string; value: number; onChange: (v: number) => void; step?: number; min?: number; max?: number; hint?: string }) {
  const [text, setText] = useState(String(value))
  return (
    <div className="field">
      <label>{label}</label>
      <input
        type="number"
        value={text}
        step={step}
        min={min}
        max={max}
        onChange={(e) => {
          setText(e.target.value)
          const v = Number(e.target.value)
          if (e.target.value.trim() !== '' && Number.isFinite(v)) onChange(v)
        }}
      />
      {hint && <span className="note">{hint}</span>}
    </div>
  )
}

const W = 760
const PAD = { l: 56, r: 16, t: 14, b: 40 }

function FanChart({ sim, wr, sd, hands }: { sim: Sim; wr: number; sd: number; hands: number }) {
  const [hoverStep, setHoverStep] = useState<number | null>(null)
  const svgRef = useRef<SVGSVGElement>(null)
  const Hh = 320
  const steps = sim.steps
  const xs = Array.from({ length: steps + 1 }, (_, k) => k * sim.stepHands)
  const band = xs.map((h) => {
    const m = (wr * h) / 100
    const half = Z95 * sd * Math.sqrt(h / 100)
    return { m, lo: m - half, hi: m + half }
  })
  let yMin = Math.min(0, ...band.map((b) => b.lo))
  let yMax = Math.max(0, ...band.map((b) => b.hi))
  for (const p of sim.paths)
    for (const v of p) {
      if (v < yMin) yMin = v
      if (v > yMax) yMax = v
    }
  const padY = (yMax - yMin) * 0.05
  yMin -= padY
  yMax += padY
  const x = (h: number) => PAD.l + (h / hands) * (W - PAD.l - PAD.r)
  const y = (v: number) => PAD.t + (1 - (v - yMin) / (yMax - yMin)) * (Hh - PAD.t - PAD.b)
  const line = (vals: ArrayLike<number>) => {
    let d = ''
    for (let k = 0; k < vals.length; k++) d += `${k ? 'L' : 'M'}${x(xs[k]).toFixed(1)},${y(vals[k]).toFixed(1)}`
    return d
  }
  const area = `M${band.map((b, k) => `${x(xs[k]).toFixed(1)},${y(b.hi).toFixed(1)}`).join('L')}L${band
    .map((b, k) => `${x(xs[k]).toFixed(1)},${y(b.lo).toFixed(1)}`)
    .reverse()
    .join('L')}Z`
  const yTicks = niceTicks(yMin, yMax, 6)
  const xTicks = niceTicks(0, hands, 6)

  const onMove = (e: MouseEvent<SVGSVGElement>) => {
    const r = svgRef.current?.getBoundingClientRect()
    if (!r) return
    const vx = ((e.clientX - r.left) / r.width) * W
    const h = ((vx - PAD.l) / (W - PAD.l - PAD.r)) * hands
    if (h < 0 || h > hands) return setHoverStep(null)
    setHoverStep(Math.round(h / sim.stepHands))
  }
  const hs = hoverStep !== null ? Math.max(0, Math.min(steps, hoverStep)) : null
  const pathVals = hs !== null ? sim.paths.map((p) => p[hs]).sort((a, b) => a - b) : []

  return (
    <div className="chart-wrap">
      <svg ref={svgRef} className="chart" viewBox={`0 0 ${W} ${Hh}`} onMouseMove={onMove} onMouseLeave={() => setHoverStep(null)} role="img" aria-label="Simulated bankroll paths with expected line and 95% band">
        <g className="grid">
          {yTicks.map((v) => (
            <line key={v} x1={PAD.l} x2={W - PAD.r} y1={y(v)} y2={y(v)} />
          ))}
        </g>
        {yTicks.map((v) => (
          <text key={v} x={PAD.l - 8} y={y(v) + 4} textAnchor="end">
            {compact(v)}
          </text>
        ))}
        {xTicks.map((v) => (
          <text key={v} x={x(v)} y={Hh - PAD.b + 16} textAnchor="middle">
            {compact(v)}
          </text>
        ))}
        <text className="axis-label" x={W - PAD.r} y={Hh - 4} textAnchor="end">
          hands
        </text>
        <text className="axis-label" x={12} y={PAD.t + 4} transform={`rotate(-90 12 ${PAD.t + 4})`} textAnchor="end">
          bb won
        </text>
        <path d={area} fill="rgba(45,212,191,0.10)" />
        {sim.paths.map((p, i) => (
          <path key={i} d={line(p)} fill="none" stroke="#7d8da1" strokeOpacity={0.45} strokeWidth={1} />
        ))}
        <line x1={PAD.l} x2={W - PAD.r} y1={y(0)} y2={y(0)} stroke="var(--text-2)" strokeWidth={1} />
        <path d={line(band.map((b) => b.m))} fill="none" stroke="var(--accent)" strokeWidth={2} strokeLinecap="round" />
        {[band.map((b) => b.hi), band.map((b) => b.lo)].map((vals, i) => (
          <path key={i} d={line(vals)} fill="none" stroke="var(--accent)" strokeOpacity={0.5} strokeWidth={1} />
        ))}
        {hs !== null && (
          <g>
            <line x1={x(xs[hs])} x2={x(xs[hs])} y1={PAD.t} y2={Hh - PAD.b} stroke="var(--text-2)" strokeWidth={1} />
            <circle cx={x(xs[hs])} cy={y(band[hs].m)} r={4} fill="var(--accent)" stroke="var(--panel)" strokeWidth={2} />
          </g>
        )}
      </svg>
      {hs !== null && (
        <div className="chart-tip" style={{ left: `${(x(xs[hs]) / W) * 100}%`, top: 8, transform: x(xs[hs]) > W * 0.65 ? 'translateX(calc(-100% - 10px))' : 'translateX(10px)' }}>
          <div>
            <span>Hands</span>
            <span>{fmt(xs[hs])}</span>
          </div>
          <div>
            <span>Expected</span>
            <span>{signed(band[hs].m)}bb</span>
          </div>
          <div>
            <span>95% band</span>
            <span>
              {fmt(band[hs].lo)} to {fmt(band[hs].hi)}
            </span>
          </div>
          <div>
            <span>Shown paths</span>
            <span>
              {fmt(pathVals[0])} to {fmt(pathVals[pathVals.length - 1])}
            </span>
          </div>
        </div>
      )}
    </div>
  )
}

function Histogram({ finals, mean, sdN }: { finals: Float64Array; mean: number; sdN: number }) {
  const [hover, setHover] = useState<number | null>(null)
  const Hh = 272
  const Wh = 520
  const pad = { l: 44, r: 12, t: 22, b: 40 }
  const lo = Math.min(finals[0], mean - 3.5 * sdN)
  const hi = Math.max(finals[finals.length - 1], mean + 3.5 * sdN)
  const nb = 36
  const bw = (hi - lo) / nb
  const counts = new Array(nb).fill(0)
  for (const v of finals) counts[Math.min(nb - 1, Math.floor((v - lo) / bw))]++
  const maxC = Math.max(...counts)
  // expected count per bin under the normal model
  const pdfScale = finals.length * bw
  const maxPdf = (normPdf(0) / sdN) * pdfScale
  const top = Math.max(maxC, maxPdf) * 1.08
  const x = (v: number) => pad.l + ((v - lo) / (hi - lo)) * (Wh - pad.l - pad.r)
  const y = (c: number) => pad.t + (1 - c / top) * (Hh - pad.t - pad.b)
  const curve = Array.from({ length: 121 }, (_, i) => {
    const v = lo + ((hi - lo) * i) / 120
    return `${i ? 'L' : 'M'}${x(v).toFixed(1)},${y((normPdf((v - mean) / sdN) / sdN) * pdfScale).toFixed(1)}`
  }).join('')
  const pcts = [0.05, 0.5, 0.95].map((p) => ({ p, v: quantile(finals, p) }))
  const xTicks = niceTicks(lo, hi, 6)
  const yTicks = niceTicks(0, top, 4)
  const barW = Math.max(1, x(lo + bw) - x(lo) - 2)
  return (
    <div className="chart-wrap">
      <svg className="chart" viewBox={`0 0 ${Wh} ${Hh}`} role="img" aria-label="Histogram of final results" onMouseLeave={() => setHover(null)}>
        <g className="grid">
          {yTicks.map((v) => (
            <line key={v} x1={pad.l} x2={Wh - pad.r} y1={y(v)} y2={y(v)} />
          ))}
        </g>
        {yTicks.map((v) => (
          <text key={v} x={pad.l - 6} y={y(v) + 4} textAnchor="end">
            {compact(v)}
          </text>
        ))}
        {xTicks.map((v) => (
          <text key={v} x={x(v)} y={Hh - pad.b + 16} textAnchor="middle">
            {compact(v)}
          </text>
        ))}
        <text className="axis-label" x={Wh - pad.r} y={Hh - 3} textAnchor="end">
          final result (bb)
        </text>
        {counts.map((c, i) => {
          const x0 = x(lo + i * bw) + 1
          const h = Math.max(0, y(0) - y(c))
          const r = Math.min(4, h, barW / 2)
          const yt = y(c)
          const yb = y(0)
          const mid = lo + (i + 0.5) * bw
          const d = h > 0 ? `M${x0},${yb}V${yt + r}Q${x0},${yt} ${x0 + r},${yt}H${x0 + barW - r}Q${x0 + barW},${yt} ${x0 + barW},${yt + r}V${yb}Z` : ''
          return (
            <g key={i} onMouseEnter={() => setHover(i)}>
              <rect x={x0 - 1} y={pad.t} width={barW + 2} height={Hh - pad.t - pad.b} fill="transparent" />
              {d && <path d={d} fill={mid < 0 ? 'var(--bad)' : 'var(--accent-2)'} opacity={hover === null || hover === i ? 0.85 : 0.45} />}
            </g>
          )
        })}
        <path d={curve} fill="none" stroke="var(--text-2)" strokeWidth={1.5} pointerEvents="none" />
        <line x1={x(0)} x2={x(0)} y1={pad.t} y2={Hh - pad.b} stroke="var(--text)" strokeWidth={1} pointerEvents="none" />
        {pcts.map(({ p, v }) => (
          <g key={p} pointerEvents="none">
            <line x1={x(v)} x2={x(v)} y1={pad.t} y2={Hh - pad.b} stroke="var(--warn)" strokeWidth={1} strokeDasharray="3 3" />
            <text x={x(v)} y={pad.t - 6} textAnchor="middle" style={{ fill: 'var(--warn)' }}>
              p{Math.round(p * 100)}
            </text>
          </g>
        ))}
      </svg>
      {hover !== null && (
        <div className="chart-tip" style={{ left: `${(x(lo + (hover + 0.5) * bw) / Wh) * 100}%`, top: 24, transform: hover > nb * 0.6 ? 'translateX(calc(-100% - 8px))' : 'translateX(8px)' }}>
          <div>
            <span>Result</span>
            <span>
              {fmt(lo + hover * bw)} to {fmt(lo + (hover + 1) * bw)}bb
            </span>
          </div>
          <div>
            <span>Players</span>
            <span>
              {counts[hover]} ({((100 * counts[hover]) / finals.length).toFixed(1)}%)
            </span>
          </div>
        </div>
      )}
    </div>
  )
}
