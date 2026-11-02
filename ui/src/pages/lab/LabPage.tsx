import { NavLink, Navigate, Route, Routes } from 'react-router-dom'
import EquityPage from './EquityPage'
import RangesPage from './RangesPage'
import DrillsPage from './DrillsPage'
import SimPage from './SimPage'
import VariancePage from './VariancePage'
import PostflopLab from '../labx/PostflopLab'
import './lab.css'

const TABS = [
  { to: 'equity', label: 'Equity' },
  { to: 'ranges', label: 'Ranges & charts' },
  { to: 'postflop', label: 'Postflop explorer' },
  { to: 'drills', label: 'Drills' },
  { to: 'sim', label: 'Bot sim' },
  { to: 'variance', label: 'Variance' },
]

export default function LabPage() {
  return (
    <div className="page">
      <div className="lab-head">
        <h1>Lab</h1>
        <nav className="lab-tabs" aria-label="Lab tools">
          {TABS.map((t) => (
            <NavLink key={t.to} to={`/lab/${t.to}`} className={({ isActive }) => (isActive ? 'active' : '')}>
              {t.label}
            </NavLink>
          ))}
        </nav>
      </div>
      <Routes>
        <Route index element={<Navigate to="equity" replace />} />
        <Route path="equity" element={<EquityPage />} />
        <Route path="ranges" element={<RangesPage />} />
        <Route path="postflop" element={<PostflopLab />} />
        <Route path="drills/*" element={<DrillsPage />} />
        <Route path="sim/*" element={<SimPage />} />
        <Route path="variance" element={<VariancePage />} />
        <Route path="*" element={<Navigate to="/lab/equity" replace />} />
      </Routes>
    </div>
  )
}
