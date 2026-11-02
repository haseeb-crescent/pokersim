// Lab endpoints used by the Lab and Learn pages, plus error wording for when the local
// server is not running.

import { getJson, postJson } from '../../api/client'
import type { EquityRequest, EquityResponse, PreflopNode, RangeParseResponse } from '../../api/types'

export const fetchEquity = (req: EquityRequest) => postJson<EquityResponse>('/api/lab/equity', req)

export const parseRangeOnServer = (text: string) => postJson<RangeParseResponse>('/api/lab/range/parse', { text })

const nodeCache = new Map<string, Promise<PreflopNode>>()

/** Preflop chart node by action key ('' = UTG to act). Successful results are cached. */
export function fetchPreflopNode(key: string): Promise<PreflopNode> {
  let p = nodeCache.get(key)
  if (!p) {
    p = getJson<PreflopNode>(`/api/lab/preflop/node?key=${encodeURIComponent(key)}`)
    p.catch(() => nodeCache.delete(key))
    nodeCache.set(key, p)
  }
  return p
}

export interface ApiProblem {
  /** true when the server looks unreachable rather than rejecting the input */
  offline: boolean
  message: string
}

/** Turns a fetch error from getJson/postJson ("<status> <body>" or a network error) into UI text. */
export function describeApiError(e: unknown): ApiProblem {
  const raw = e instanceof Error ? e.message : String(e)
  const m = /^(\d{3}) ?([^]*)$/.exec(raw)
  if (!m) return { offline: true, message: 'The local server is not reachable. Start the server to load this.' }
  const status = Number(m[1])
  const body = m[2].trim()
  if (status >= 502 || (status === 500 && (!body || /ECONNREFUSED|proxy/i.test(body)))) {
    return { offline: true, message: 'The local server is not reachable. Start the server to load this.' }
  }
  if (status === 404 && (!body || body.startsWith('<'))) {
    return { offline: true, message: 'The server did not recognise this request (404). Start the latest server build to load this.' }
  }
  return { offline: false, message: body || `Server error ${status}` }
}
