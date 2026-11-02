// Learn progress in localStorage. Every access is wrapped: storage can be missing or blocked.

export interface ConceptProgress {
  visited?: number
  right: number
  total: number
  best: number
}

const KEY = 'poker-trainer.learn.v1'

export function loadProgress(): Record<string, ConceptProgress> {
  try {
    const raw = localStorage.getItem(KEY)
    const v = raw ? JSON.parse(raw) : {}
    return v && typeof v === 'object' ? v : {}
  } catch {
    return {}
  }
}

function save(p: Record<string, ConceptProgress>) {
  try {
    localStorage.setItem(KEY, JSON.stringify(p))
  } catch {
    // storage unavailable: progress lives only in component state
  }
}

function entry(p: Record<string, ConceptProgress>, slug: string): ConceptProgress {
  return (p[slug] ??= { right: 0, total: 0, best: 0 })
}

export function markVisited(slug: string) {
  const p = loadProgress()
  entry(p, slug).visited = Date.now()
  save(p)
}

export function recordAnswer(slug: string, ok: boolean, streak: number) {
  const p = loadProgress()
  const e = entry(p, slug)
  e.total += 1
  if (ok) e.right += 1
  e.best = Math.max(e.best, streak)
  save(p)
}

export function resetProgress() {
  try {
    localStorage.removeItem(KEY)
  } catch {
    // ignore
  }
}
