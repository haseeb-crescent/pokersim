// Finding glossary terms in running text.

import { TERMS, type GlossaryTerm } from './terms'

export const TERM_BY_ID: Record<string, GlossaryTerm> = Object.fromEntries(TERMS.map((t) => [t.id, t]))

/** Strings that auto-link to each term. */
export function linkStrings(t: GlossaryTerm): string[] {
  return t.links ?? [t.term]
}

/** Abbreviations such as "SPR", "BB", "TAGs" must match case so "co", "lag" or "bb" in prose
 *  are not mistaken for them. Everything else matches case-insensitively. */
function isCaseSensitive(alias: string): boolean {
  const core = alias.endsWith('s') ? alias.slice(0, -1) : alias
  return /^[A-Z$+\-/]+$/.test(core) && (core.match(/[A-Z]/g) ?? []).length >= 2
}

interface AliasInfo {
  id: string
  alias: string
  caseSensitive: boolean
}

let compiled: { re: RegExp; byLower: Map<string, AliasInfo[]> } | null = null

function escapeRe(s: string) {
  return s.replace(/[.*+?^${}()|[\]\\/]/g, '\\$&')
}

function compile() {
  if (compiled) return compiled
  const byLower = new Map<string, AliasInfo[]>()
  for (const t of TERMS) {
    for (const alias of linkStrings(t)) {
      const key = alias.toLowerCase()
      const list = byLower.get(key) ?? []
      if (!list.some((x) => x.id === t.id && x.alias === alias)) list.push({ id: t.id, alias, caseSensitive: isCaseSensitive(alias) })
      byLower.set(key, list)
    }
  }
  const keys = [...byLower.keys()].sort((a, b) => b.length - a.length)
  // Word boundaries that understand hyphens ("3-bet" is one word, "open" is not inside
  // "open-ended") and allow a leading digit ("5bb/100"). Optional plural/verb endings.
  const re = new RegExp(
    `(?<![A-Za-z_\\-])(${keys.map(escapeRe).join('|')})(s|es|d|ed|ing|ting|ged|er|ers)?(?![A-Za-z0-9_\\-])`,
    'gi',
  )
  compiled = { re, byLower }
  return compiled
}

export interface TextPiece {
  text: string
  /** term id when this piece should link */
  id?: string
}

/**
 * Splits text into plain pieces and term pieces. Longest alias wins at each position; only
 * the first occurrence of each term is linked (pass a shared `seen` set across calls to keep
 * that rule over several strings); `exclude` ids are never linked.
 */
export function splitTerms(text: string, seen: Set<string> = new Set(), exclude: string[] = []): TextPiece[] {
  const { re, byLower } = compile()
  const out: TextPiece[] = []
  let last = 0
  re.lastIndex = 0
  let m: RegExpExecArray | null
  while ((m = re.exec(text))) {
    const base = m[1]
    const candidates = byLower.get(base.toLowerCase()) ?? []
    const hit = candidates.find((c) => !c.caseSensitive || c.alias === base)
    if (!hit || seen.has(hit.id) || exclude.includes(hit.id)) continue
    seen.add(hit.id)
    if (m.index > last) out.push({ text: text.slice(last, m.index) })
    out.push({ text: m[0], id: hit.id })
    last = m.index + m[0].length
  }
  if (last < text.length) out.push({ text: text.slice(last) })
  return out
}

/** Looks a term up by id or by any of its link strings (case-insensitive). */
export function findTerm(key: string): GlossaryTerm | undefined {
  if (TERM_BY_ID[key]) return TERM_BY_ID[key]
  const k = key.trim().toLowerCase()
  return TERMS.find((t) => t.id === k || t.term.toLowerCase() === k || linkStrings(t).some((a) => a.toLowerCase() === k))
}
