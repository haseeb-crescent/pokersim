import { useEffect, useMemo, useState } from 'react'
import { Link, useNavigate, useParams } from 'react-router-dom'
import { CATEGORIES, TERMS, TERM_BY_ID, findTerm, linkStrings, RichText, type GlossaryTerm } from '../glossary'
import { CONCEPT_BY_SLUG } from './learn/concepts'

function matches(t: GlossaryTerm, q: string) {
  if (!q) return true
  const hay = [t.term, t.id, ...linkStrings(t), t.def, t.example].join(' ').toLowerCase()
  return q
    .toLowerCase()
    .split(/\s+/)
    .filter(Boolean)
    .every((w) => hay.includes(w))
}

export default function GlossaryPage() {
  const { term } = useParams()
  const navigate = useNavigate()
  const [query, setQuery] = useState('')
  const target = term ? findTerm(term) : undefined
  const [flash, setFlash] = useState<string | null>(null)

  // Deep link: clear a search that would hide the entry, scroll to it and highlight it.
  useEffect(() => {
    if (!target) return
    setQuery((q) => (matches(target, q) ? q : ''))
    setFlash(target.id)
    const raf = requestAnimationFrame(() => {
      // instant: a smooth scroll across a long list can outlast the highlight
      document.getElementById(`g-${target.id}`)?.scrollIntoView({ block: 'center' })
    })
    const t = window.setTimeout(() => setFlash(null), 3200)
    return () => {
      cancelAnimationFrame(raf)
      window.clearTimeout(t)
    }
  }, [target])

  const groups = useMemo(() => {
    return CATEGORIES.map((c) => ({
      ...c,
      items: TERMS.filter((t) => t.category === c.id && matches(t, query)).sort((a, b) => a.term.localeCompare(b.term)),
    })).filter((g) => g.items.length)
  }, [query])
  const total = groups.reduce((n, g) => n + g.items.length, 0)

  return (
    <div className="page gl">
      <div className="gl-head">
        <div>
          <h1>Glossary</h1>
          <p className="muted">
            {TERMS.length} terms, each with a plain definition and a worked example. Dotted-underlined words anywhere in the app link here.
          </p>
        </div>
        <input
          className="gl-search"
          type="search"
          placeholder="Search terms, e.g. SPR, blocker, 3-bet…"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          aria-label="Search the glossary"
        />
      </div>

      {term && !target && (
        <div className="panel gl-missing">
          No entry for “{term}”. <Link to="/glossary">Show all terms</Link>
        </div>
      )}

      <div className="gl-layout">
        <nav className="gl-toc" aria-label="Categories">
          {groups.map((g) => (
            <a
              key={g.id}
              href={`#cat-${g.id}`}
              onClick={(e) => {
                e.preventDefault()
                document.getElementById(`cat-${g.id}`)?.scrollIntoView({ behavior: 'smooth', block: 'start' })
              }}
            >
              <span>{g.label}</span>
              <span className="n">{g.items.length}</span>
            </a>
          ))}
        </nav>

        <div>
          {total === 0 && (
            <div className="panel gl-empty muted">
              Nothing matches “{query}”.{' '}
              <button className="ghost" onClick={() => setQuery('')}>
                Clear search
              </button>
            </div>
          )}
          {groups.map((g) => (
            <section key={g.id} id={`cat-${g.id}`} className="gl-cat">
              <h2>{g.label}</h2>
              <div className="gl-entries">
                {g.items.map((t) => (
                  <Entry key={t.id} t={t} highlighted={flash === t.id} onOpen={(id) => navigate(`/glossary/${id}`)} />
                ))}
              </div>
            </section>
          ))}
        </div>
      </div>
    </div>
  )
}

function Entry({ t, highlighted, onOpen }: { t: GlossaryTerm; highlighted: boolean; onOpen: (id: string) => void }) {
  const aliases = linkStrings(t).filter((a) => a.toLowerCase() !== t.term.toLowerCase())
  const learn = t.learn ? CONCEPT_BY_SLUG[t.learn] : undefined
  return (
    <article id={`g-${t.id}`} className={'gl-entry' + (highlighted ? ' hl' : '')}>
      <h3>
        <Link to={`/glossary/${t.id}`}>{t.term}</Link>
      </h3>
      {aliases.length > 0 && <div className="gl-alias">Also: {aliases.slice(0, 5).join(', ')}</div>}
      <p className="gl-def">
        <RichText text={t.def} link exclude={[t.id]} />
      </p>
      <div className="gl-ex">
        <b>Example</b>
        <RichText text={t.example} />
      </div>
      <div className="gl-foot">
        {(t.related ?? [])
          .filter((r) => TERM_BY_ID[r])
          .map((r) => (
            <a
              key={r}
              className="tag"
              href={`/glossary/${r}`}
              onClick={(e) => {
                e.preventDefault()
                onOpen(r)
              }}
            >
              {TERM_BY_ID[r].term}
            </a>
          ))}
        {learn && (
          <Link className="gl-learn" to={`/learn/${learn.slug}`}>
            Learn: {learn.title} →
          </Link>
        )}
      </div>
    </article>
  )
}
