import { useState, useEffect, useCallback, useRef } from 'react'
import {
  Table,
  WarningCircle,
  Timer,
  Graph,
  CircleNotch,
} from '@phosphor-icons/react'
import { useQueryStore } from '@/stores/queryStore'
import { useConnectionStore } from '@/stores/connectionStore'
import { RecordTable } from '@/components/shared/data-table'
import { TraceViewer } from '@/components/shared/TraceViewer'
import { createClient } from '@/lib/api'
import type { QueryTrace } from '@/lib/api'
import { cn } from '@/lib/utils'

function formatElapsed(ms: number): string {
  if (ms < 1000) return `${ms}ms`
  const seconds = ms / 1000
  if (seconds < 60) return `${seconds.toFixed(1)}s`
  const minutes = Math.floor(seconds / 60)
  const remainingSec = Math.floor(seconds % 60)
  return `${minutes}m ${remainingSec}s`
}

function ElapsedTimer({ startedAt }: { startedAt: number }) {
  const [elapsed, setElapsed] = useState(() => Date.now() - startedAt)

  useEffect(() => {
    const id = setInterval(() => setElapsed(Date.now() - startedAt), 100)
    return () => clearInterval(id)
  }, [startedAt])

  return (
    <div className="flex items-center gap-1.5 font-mono text-xs text-overlay-0 mt-3">
      <Timer size={13} className="text-mauve" />
      <span>{formatElapsed(elapsed)}</span>
    </div>
  )
}

type ViewTab = 'results' | 'trace'

interface ResultPanelProps {
  resultIndex: number
  multiResult?: boolean
  totalResults?: number
  onSelectIndex?: (i: number) => void
  activeResultIndex?: number
}

function ResultPanel({
  resultIndex,
  multiResult,
  totalResults,
  onSelectIndex,
  activeResultIndex,
}: ResultPanelProps) {
  const { results } = useQueryStore()
  const { activeConnectionId, connections } = useConnectionStore()
  const activeConnection = connections.find((c) => c.id === activeConnectionId)

  const result = results[resultIndex] ?? results[0]
  const hasTrace = Boolean(result?.traceId)

  const [viewTab, setViewTab] = useState<ViewTab>('results')
  const [traceData, setTraceData] = useState<QueryTrace | null>(null)
  const [traceLoading, setTraceLoading] = useState(false)
  const [traceError, setTraceError] = useState<string | null>(null)
  const fetchedTraceIdRef = useRef<string | null>(null)

  // Reset trace when result changes
  useEffect(() => {
    setViewTab('results')
    setTraceData(null)
    setTraceError(null)
    fetchedTraceIdRef.current = null
  }, [result?.traceId])

  const fetchTrace = useCallback(async () => {
    if (!result?.traceId || !activeConnection) return
    if (fetchedTraceIdRef.current === result.traceId) return

    // Determine tableId from first result row
    const tableId = result.rows[0]?.table_id as string | undefined
    if (!tableId) {
      setTraceError('Cannot fetch trace: table ID not found in results')
      return
    }

    setTraceLoading(true)
    setTraceError(null)
    try {
      const client = createClient({
        host: activeConnection.host,
        port: activeConnection.port,
        apiKey: activeConnection.apiKey,
        useSsl: activeConnection.ssl,
      })
      const trace = await client.getTrace(tableId, result.traceId)
      setTraceData(trace)
      fetchedTraceIdRef.current = result.traceId
    } catch (err) {
      setTraceError(err instanceof Error ? err.message : 'Failed to load trace')
    } finally {
      setTraceLoading(false)
    }
  }, [result, activeConnection])

  const handleTabChange = (tab: ViewTab) => {
    setViewTab(tab)
    if (tab === 'trace') {
      fetchTrace()
    }
  }

  return (
    <div className="flex flex-col h-full">
      {/* Multi-result tab strip + trace toggle in one bar */}
      <div className="flex items-center border-b border-border bg-mantle overflow-x-auto scrollbar-none shrink-0">
        {multiResult && (
          <div className="flex items-center gap-0.5 px-2 py-1">
            {Array.from({ length: totalResults ?? 1 }, (_, i) => {
              const r = results[i]
              return (
                <button
                  key={i}
                  onClick={() => onSelectIndex?.(i)}
                  className={cn(
                    'px-3 py-1 text-xs rounded-md transition-colors whitespace-nowrap',
                    i === activeResultIndex
                      ? 'bg-surface-1 text-text font-medium'
                      : 'text-subtext-0 hover:text-text hover:bg-surface-0'
                  )}
                >
                  Result {i + 1}
                  {r && (
                    <span className="ml-1.5 text-overlay-0">
                      {r.rowCount} rows · {r.executionTime}ms
                    </span>
                  )}
                </button>
              )
            })}
          </div>
        )}

        {hasTrace && (
          <div className="flex items-center ml-auto px-1 py-1 gap-0.5 border-l border-border">
            <button
              onClick={() => handleTabChange('results')}
              className={cn(
                'relative flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium rounded-md transition-colors',
                viewTab === 'results'
                  ? 'bg-surface-1 text-text'
                  : 'text-overlay-0 hover:text-text hover:bg-surface-0'
              )}
            >
              <Table size={12} />
              Results
            </button>
            <button
              onClick={() => handleTabChange('trace')}
              className={cn(
                'flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium rounded-md transition-colors',
                viewTab === 'trace'
                  ? 'bg-surface-1 text-text'
                  : 'text-overlay-0 hover:text-text hover:bg-surface-0'
              )}
            >
              <Graph size={12} />
              Trace
            </button>
          </div>
        )}
      </div>

      {/* Content */}
      <div className="flex-1 min-h-0">
        {viewTab === 'results' && (
          <RecordTable
            records={result.rows}
            columns={result.columns}
            totalCount={result.rowCount}
            executionTime={result.executionTime}
            isQueryResult
          />
        )}

        {viewTab === 'trace' && (
          <>
            {traceLoading && (
              <div className="flex flex-col items-center justify-center h-full gap-3">
                <CircleNotch size={24} className="animate-spin text-mauve" />
                <span className="text-sm text-overlay-0">Loading trace...</span>
              </div>
            )}
            {traceError && (
              <div className="flex flex-col items-center justify-center h-full gap-3 p-4 text-center">
                <WarningCircle size={24} className="text-red" />
                <span className="text-sm text-red">{traceError}</span>
                <button onClick={fetchTrace} className="text-xs text-mauve hover:underline">
                  Retry
                </button>
              </div>
            )}
            {!traceLoading && !traceError && traceData && (
              <TraceViewer trace={traceData} />
            )}
            {!traceLoading && !traceError && !traceData && (
              <div className="flex items-center justify-center h-full text-overlay-0 text-sm">
                No trace data available
              </div>
            )}
          </>
        )}
      </div>
    </div>
  )
}

export function QueryResults() {
  const { results, activeResultIndex, setActiveResultIndex, error, isExecuting, executionStartedAt, reasonProgress } = useQueryStore()

  if (isExecuting) {
    const message = reasonProgress?.message ?? 'Executing query...'
    const phase = reasonProgress?.phase

    return (
      <div className="flex flex-col items-center justify-center h-full bg-base text-subtext-0">
        <div className="w-6 h-6 border-2 border-primary border-t-transparent rounded-full animate-spin mb-4" />
        <p
          key={message}
          className="text-sm animate-[fadeIn_0.3s_ease-in-out]"
        >
          {message}
        </p>
        {phase && (
          <span className="text-[11px] text-overlay-0 mt-1 font-mono">{phase}</span>
        )}
        {executionStartedAt && <ElapsedTimer startedAt={executionStartedAt} />}
      </div>
    )
  }

  if (error) {
    return (
      <div className="flex flex-col items-center justify-center h-full bg-base p-6">
        <WarningCircle size={48} weight="duotone" className="text-red mb-3" />
        <p className="text-sm font-medium text-red mb-2">Query Error</p>
        <pre className="text-xs text-subtext-0 bg-surface-0 p-3 rounded-md max-w-full overflow-auto">
          {error}
        </pre>
      </div>
    )
  }

  if (results.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center h-full bg-base text-subtext-0">
        <Table size={48} weight="duotone" className="mb-3 opacity-50" />
        <p className="text-sm">Run a query to see results</p>
      </div>
    )
  }

  return (
    <ResultPanel
      resultIndex={activeResultIndex}
      multiResult={results.length > 1}
      totalResults={results.length}
      onSelectIndex={setActiveResultIndex}
      activeResultIndex={activeResultIndex}
    />
  )
}
