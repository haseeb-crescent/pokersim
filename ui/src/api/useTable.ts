import { useCallback, useEffect, useRef, useState } from 'react'
import type { ClientMsg, DecisionGrade, HandResultSummary, ServerMsg, TableState } from './types'

/** Live table connection: state snapshots, grades and explanations over /ws. */
export function useTable() {
  const [state, setState] = useState<TableState | null>(null)
  const [grades, setGrades] = useState<DecisionGrade[]>([])
  const [lastHand, setLastHand] = useState<HandResultSummary | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [connected, setConnected] = useState(false)
  const ws = useRef<WebSocket | null>(null)
  // explanations that arrive before their grade (hand mode) are merged in when it does
  const early = useRef(new Map<string, { text: string | null; error?: string }>())

  const merge = (g: DecisionGrade): DecisionGrade => {
    const e = early.current.get(`${g.hand_id}-${g.index}`)
    if (!e || g.explanation) return g
    return { ...g, explanation: e.text, explanation_error: e.error ?? null, explanation_status: 'none' }
  }

  useEffect(() => {
    let closed = false
    let retry: ReturnType<typeof setTimeout> | undefined
    const connect = () => {
      const proto = location.protocol === 'https:' ? 'wss' : 'ws'
      const sock = new WebSocket(`${proto}://${location.host}/ws`)
      ws.current = sock
      sock.onopen = () => setConnected(true)
      sock.onclose = () => {
        setConnected(false)
        if (!closed) retry = setTimeout(connect, 1000)
      }
      sock.onmessage = (ev) => {
        const msg = JSON.parse(ev.data) as ServerMsg
        switch (msg.type) {
          case 'state':
            setState(msg.state)
            break
          case 'grade':
            setGrades((g) => [...g.filter((x) => !(x.hand_id === msg.grade.hand_id && x.index === msg.grade.index)), merge(msg.grade)].slice(-60))
            break
          case 'explanation':
            early.current.set(`${msg.hand_id}-${msg.index}`, { text: msg.text, error: msg.error })
            setGrades((g) =>
              g.map((x) =>
                x.hand_id === msg.hand_id && x.index === msg.index
                  ? { ...x, explanation: msg.text, explanation_error: msg.error ?? null, explanation_status: 'none' }
                  : x,
              ),
            )
            break
          case 'hand_complete':
            setLastHand(msg.summary)
            if (msg.summary.grades?.length) {
              setGrades((g) => {
                const keep = g.filter((x) => x.hand_id !== msg.summary.hand_id)
                return [...keep, ...msg.summary.grades.map(merge)].slice(-60)
              })
            }
            break
          case 'error':
            setError(msg.message)
            setTimeout(() => setError(null), 4000)
            break
        }
      }
    }
    connect()
    return () => {
      closed = true
      if (retry) clearTimeout(retry)
      ws.current?.close()
    }
  }, [])

  const send = useCallback((m: ClientMsg) => {
    if (ws.current?.readyState === WebSocket.OPEN) ws.current.send(JSON.stringify(m))
  }, [])

  return { state, grades, lastHand, error, connected, send, setGrades }
}
