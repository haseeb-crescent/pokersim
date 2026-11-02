import { useEffect, type ComponentType } from 'react'
import { Link, useParams } from 'react-router-dom'
import { CONCEPTS } from './concepts'
import { markVisited } from './progress'
import PotOdds from './modules/PotOdds'
import Equity from './modules/Equity'
import ExpectedValue from './modules/ExpectedValue'
import RangesPosition from './modules/RangesPosition'
import PreflopCharts from './modules/PreflopCharts'
import RangeNutAdvantage from './modules/RangeNutAdvantage'
import MdfBluffRatios from './modules/MdfBluffRatios'
import BetSizing from './modules/BetSizing'
import Blockers from './modules/Blockers'
import Exploitation from './modules/Exploitation'
import './learn.css'

const MODULES: Record<string, ComponentType> = {
  'pot-odds': PotOdds,
  equity: Equity,
  ev: ExpectedValue,
  'ranges-position': RangesPosition,
  'preflop-charts': PreflopCharts,
  'range-nut-advantage': RangeNutAdvantage,
  'mdf-bluff-ratios': MdfBluffRatios,
  'bet-sizing-polarization': BetSizing,
  blockers: Blockers,
  exploitation: Exploitation,
}

export default function LearnPage() {
  const { slug = '' } = useParams()
  const index = CONCEPTS.findIndex((c) => c.slug === slug)
  const concept = CONCEPTS[index]
  const Module = MODULES[slug]

  useEffect(() => {
    if (concept) markVisited(concept.slug)
    document.querySelector('.main')?.scrollTo({ top: 0 })
  }, [concept])

  if (!concept || !Module) {
    return (
      <div className="page learn">
        <h1>Not found</h1>
        <p className="muted">
          There is no Learn page called “{slug}”. <Link to="/learn">See all concepts</Link>.
        </p>
      </div>
    )
  }
  const prev = CONCEPTS[index - 1]
  const next = CONCEPTS[index + 1]
  return (
    <div className="page learn">
      <div className="learn-eyebrow">
        <Link to="/learn">Learn</Link> · Concept {index + 1} of {CONCEPTS.length}
      </div>
      <h1>{concept.title}</h1>
      <Module key={slug} />
      <nav className="lnav" aria-label="Concept navigation">
        {prev ? (
          <Link to={`/learn/${prev.slug}`}>
            <span>← Previous</span>
            <b>{prev.title}</b>
          </Link>
        ) : (
          <Link to="/learn">
            <span>← Back</span>
            <b>All concepts</b>
          </Link>
        )}
        {next ? (
          <Link className="next" to={`/learn/${next.slug}`}>
            <span>Next →</span>
            <b>{next.title}</b>
          </Link>
        ) : (
          <Link className="next" to="/learn">
            <span>Done →</span>
            <b>All concepts</b>
          </Link>
        )}
      </nav>
    </div>
  )
}
