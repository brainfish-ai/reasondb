import { useState, useEffect } from 'react'
import {
  SpinnerGap,
  CheckCircle,
  WarningCircle,
  ArrowCounterClockwise,
  X,
  CaretUp,
  CaretDown,
  Queue,
} from '@phosphor-icons/react'
import { useIngestionStore, type IngestionJob } from '@/stores/ingestionStore'
import { formatDuration } from '@/lib/utils'
import { cn } from '@/lib/utils'

function ElapsedTime({ since }: { since: number }) {
  const [elapsed, setElapsed] = useState(Date.now() - since)

  useEffect(() => {
    const interval = setInterval(() => setElapsed(Date.now() - since), 1000)
    return () => clearInterval(interval)
  }, [since])

  return <span className="tabular-nums">{formatDuration(elapsed)}</span>
}

function JobRow({ job }: { job: IngestionJob }) {
  const { retryJob, dismissJob } = useIngestionStore()

  return (
    <div className="flex items-start gap-2.5 px-3 py-2 group">
      {/* Status icon */}
      <div className="mt-0.5 shrink-0">
        {job.status === 'queued' && (
          <Queue size={14} weight="bold" className="text-overlay-0" />
        )}
        {job.status === 'ingesting' && (
          <SpinnerGap size={14} weight="bold" className="text-mauve animate-spin" />
        )}
        {job.status === 'success' && (
          <CheckCircle size={14} weight="fill" className="text-green" />
        )}
        {job.status === 'error' && (
          <WarningCircle size={14} weight="fill" className="text-red" />
        )}
      </div>

      {/* Content */}
      <div className="flex-1 min-w-0">
        <p className="text-xs font-medium text-text truncate">{job.title}</p>
        <p className="text-[11px] text-overlay-0 truncate">
          {job.tableName}
          {job.status === 'ingesting' && (
            <>
              {' '}&middot; {job.progress || <ElapsedTime since={job.queuedAt} />}
            </>
          )}
          {job.status === 'success' && job.response && (
            <> &middot; {job.response.total_nodes} nodes in {formatDuration(job.response.stats.total_time_ms)}</>
          )}
          {job.status === 'error' && (
            <> &middot; <span className="text-red">{job.error}</span></>
          )}
          {job.status === 'queued' && (
            <> &middot; Queued</>
          )}
        </p>
      </div>

      {/* Actions */}
      <div className="flex items-center gap-0.5 shrink-0 opacity-0 group-hover:opacity-100 transition-opacity">
        {job.status === 'error' && (
          <button
            onClick={() => retryJob(job.id)}
            className="p-1 rounded hover:bg-surface-1 text-overlay-0 hover:text-text transition-colors"
            title="Retry"
          >
            <ArrowCounterClockwise size={12} />
          </button>
        )}
        {(job.status === 'success' || job.status === 'error') && (
          <button
            onClick={() => dismissJob(job.id)}
            className="p-1 rounded hover:bg-surface-1 text-overlay-0 hover:text-text transition-colors"
            title="Dismiss"
          >
            <X size={12} />
          </button>
        )}
      </div>
    </div>
  )
}

export function IngestionStatus() {
  const { jobs, clearCompleted } = useIngestionStore()
  const [expanded, setExpanded] = useState(false)

  if (jobs.length === 0) return null

  const activeJobs = jobs.filter(j => j.status === 'ingesting' || j.status === 'queued')
  const completedJobs = jobs.filter(j => j.status === 'success' || j.status === 'error')
  const hasActive = activeJobs.length > 0

  const pillLabel = hasActive
    ? `${activeJobs.length} ingesting...`
    : `${completedJobs.length} completed`

  return (
    <div className="fixed bottom-8 right-4 z-50 flex flex-col items-end">
      {/* Expanded panel */}
      {expanded && (
        <div className="mb-2 w-80 rounded-lg border border-border bg-mantle shadow-lg overflow-hidden animate-in slide-in-from-bottom-2 fade-in-0 duration-200">
          {/* Header */}
          <div className="flex items-center justify-between px-3 py-2 border-b border-border">
            <span className="text-xs font-medium text-text">Ingestion Queue</span>
            {completedJobs.length > 0 && (
              <button
                onClick={clearCompleted}
                className="text-[11px] text-overlay-0 hover:text-text transition-colors"
              >
                Clear completed
              </button>
            )}
          </div>

          {/* Job list */}
          <div className="max-h-60 overflow-auto divide-y divide-border/50">
            {jobs.map(job => (
              <JobRow key={job.id} job={job} />
            ))}
          </div>
        </div>
      )}

      {/* Pill button */}
      <button
        onClick={() => setExpanded(!expanded)}
        className={cn(
          'flex items-center gap-2 pl-3 pr-2 py-1.5 rounded-full border shadow-md transition-colors',
          'text-xs font-medium',
          hasActive
            ? 'bg-mauve/10 border-mauve/30 text-mauve hover:bg-mauve/15'
            : 'bg-mantle border-border text-subtext-0 hover:text-text'
        )}
      >
        {hasActive && (
          <SpinnerGap size={14} weight="bold" className="animate-spin" />
        )}
        {!hasActive && completedJobs.some(j => j.status === 'error') && (
          <WarningCircle size={14} weight="fill" className="text-red" />
        )}
        {!hasActive && completedJobs.every(j => j.status === 'success') && (
          <CheckCircle size={14} weight="fill" className="text-green" />
        )}
        <span>{pillLabel}</span>
        {expanded ? <CaretDown size={12} /> : <CaretUp size={12} />}
      </button>
    </div>
  )
}
