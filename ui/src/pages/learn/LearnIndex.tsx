import { useState } from 'react'
import { Link } from 'react-router-dom'
import { CONCEPTS } from './concepts'
import { loadProgress, resetProgress } from './progress'
import './learn.css'

export default function LearnIndex() {
  const [progress, setProgress] = useState(() => loadProgress())
  const visited = CONCEPTS.filter((c) => progress[c.slug]?.visited).length
  const nextUp = CONCEPTS.find((c) => !progress[c.slug]?.visited)
  return (
    <div className="page learn">
      <h1>Learn</h1>
      <p className="learn-lede">
        Ten core ideas behind solver-based poker, from the price of a call to exploiting real opponents. Each page explains the idea from first principles, lets you change the numbers, and ends with a drill.
      </p>
      <div className="lprog">
        <div className="lprog-bar">
          <span style={{ width: `${(100 * visited) / CONCEPTS.length}%` }} />
        </div>
        <span>
          {visited} of {CONCEPTS.length} opened
        </span>
        {nextUp && (
          <Link to={`/learn/${nextUp.slug}`}>
            {visited ? 'Continue' : 'Start'} with {nextUp.title} →
          </Link>
        )}
        {visited > 0 && (
          <button
            className="ghost"
            style={{ marginLeft: 'auto', fontSize: 12 }}
            onClick={() => {
              resetProgress()
              setProgress({})
            }}
          >
            Reset progress
          </button>
        )}
      </div>
      <div className="lindex">
        {CONCEPTS.map((c, i) => {
          const p = progress[c.slug]
          return (
            <Link key={c.slug} to={`/learn/${c.slug}`} className={'lcard' + (p?.visited ? ' done' : '')}>
              <span className="num">{p?.visited ? '✓' : i + 1}</span>
              <div>
                <h3>{c.title}</h3>
                <p>{c.blurb}</p>
              </div>
              <div className="meta">
                {p && p.total > 0 ? (
                  <>
                    Drill <b>{p.right}</b>/{p.total} ({Math.round((100 * p.right) / p.total)}%)
                    <br />
                    best streak {p.best}
                  </>
                ) : (
                  'Drill not started'
                )}
                {c.server && (
                  <>
                    <br />
                    <span title="Part of this page loads data from the local server">uses server</span>
                  </>
                )}
              </div>
            </Link>
          )
        })}
      </div>
    </div>
  )
}
