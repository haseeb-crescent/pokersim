import { lazy, Suspense } from 'react'
import { NavLink, Route, Routes, Navigate } from 'react-router-dom'
import './app.css'
import PlayPage from './pages/PlayPage'
import LogbookPage from './pages/LogbookPage'
import HandPage from './pages/HandPage'
import StatsPage from './pages/StatsPage'
import LeaksPage from './pages/LeaksPage'
const LabPage = lazy(() => import('./pages/lab/LabPage'))
const LearnIndex = lazy(() => import('./pages/learn/LearnIndex'))
const LearnPage = lazy(() => import('./pages/learn/LearnPage'))
const GlossaryPage = lazy(() => import('./pages/GlossaryPage'))

const NAV = [
  { to: '/play', label: 'Play' },
  { to: '/logbook', label: 'Logbook' },
  { to: '/stats', label: 'Stats' },
  { to: '/leaks', label: 'Leaks' },
  { to: '/lab', label: 'Lab' },
  { to: '/learn', label: 'Learn' },
  { to: '/glossary', label: 'Glossary' },
]

export default function App() {
  return (
    <div className="shell">
      <nav className="sidebar">
        <div className="brand">
          <span className="brand-mark">♠</span>
          <span>Solver Trainer</span>
        </div>
        {NAV.map((n) => (
          <NavLink key={n.to} to={n.to} className={({ isActive }) => 'nav-link' + (isActive ? ' active' : '')}>
            {n.label}
          </NavLink>
        ))}
      </nav>
      <main className="main">
        <Suspense fallback={<div className="page muted">Loading…</div>}>
        <Routes>
          <Route path="/" element={<Navigate to="/play" replace />} />
          <Route path="/play" element={<PlayPage />} />
          <Route path="/logbook" element={<LogbookPage />} />
          <Route path="/hand/:id" element={<HandPage />} />
          <Route path="/stats" element={<StatsPage />} />
          <Route path="/leaks" element={<LeaksPage />} />
          <Route path="/lab/*" element={<LabPage />} />
          <Route path="/learn" element={<LearnIndex />} />
          <Route path="/learn/:slug" element={<LearnPage />} />
          <Route path="/glossary" element={<GlossaryPage />} />
          <Route path="/glossary/:term" element={<GlossaryPage />} />
        </Routes>
        </Suspense>
      </main>
    </div>
  )
}
