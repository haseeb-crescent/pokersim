import type { ApiProblem } from './api'

/** Notice shown where a server-backed widget could not load. */
export function Offline({ problem, onRetry }: { problem: ApiProblem; onRetry?: () => void }) {
  return (
    <div className="offline" role="status">
      <div className="grow">
        {problem.offline ? <b>Server not running. </b> : <b>Server error. </b>}
        {problem.offline ? (
          <>
            {problem.message.replace(/^The local server is not reachable\. /, '')} The rest of this page works without it.
          </>
        ) : (
          problem.message
        )}
      </div>
      {onRetry && (
        <button type="button" onClick={onRetry}>
          Retry
        </button>
      )}
    </div>
  )
}
