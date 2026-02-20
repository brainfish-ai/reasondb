import { create } from 'zustand'
import {
  createClient,
  type IngestTextRequest,
  type IngestUrlRequest,
  type IngestResponse,
  type JobStatusResponse,
} from '@/lib/api'
import { useConnectionStore } from '@/stores/connectionStore'
import { useTableStore } from '@/stores/tableStore'

const POLL_INTERVAL_INITIAL_MS = 1000
const POLL_INTERVAL_PROCESSING_MS = 4000
const AUTO_DISMISS_MS = 60_000
const MAX_POLL_FAILURES = 5

export interface IngestionJob {
  id: string
  serverJobId?: string
  mode: 'text' | 'url'
  title: string
  tableId: string
  tableName: string
  connectionId: string
  payload: IngestTextRequest | IngestUrlRequest
  status: 'queued' | 'ingesting' | 'success' | 'error'
  progress?: string
  response?: IngestResponse
  error?: string
  queuedAt: number
  completedAt?: number
}

interface IngestionState {
  jobs: IngestionJob[]

  queueJob: (job: Omit<IngestionJob, 'id' | 'status' | 'queuedAt'>) => void
  retryJob: (id: string) => void
  dismissJob: (id: string) => void
  clearCompleted: () => void
}

export const useIngestionStore = create<IngestionState>((set, get) => {
  const pollingTimers = new Map<string, ReturnType<typeof setInterval>>()
  const pollFailureCounts = new Map<string, number>()

  function updateJob(jobId: string, patch: Partial<IngestionJob>) {
    set({
      jobs: get().jobs.map(j =>
        j.id === jobId ? { ...j, ...patch } : j
      ),
    })
  }

  async function submitJob(job: IngestionJob) {
    const conn = useConnectionStore.getState().connections.find(c => c.id === job.connectionId)
    if (!conn) {
      updateJob(job.id, { status: 'error', error: 'Connection not found', completedAt: Date.now() })
      return
    }

    try {
      const client = createClient({
        host: conn.host,
        port: conn.port,
        apiKey: conn.apiKey,
        useSsl: conn.ssl,
      })

      let result: JobStatusResponse
      if (job.mode === 'text') {
        result = await client.ingestText(job.payload as IngestTextRequest)
      } else {
        result = await client.ingestUrl(job.payload as IngestUrlRequest)
      }

      updateJob(job.id, {
        serverJobId: result.job_id,
        status: 'ingesting',
      })

      startPolling(job.id, result.job_id, conn)
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to submit job'
      updateJob(job.id, { status: 'error', error: message, completedAt: Date.now() })
    }
  }

  function startPolling(
    localId: string,
    serverJobId: string,
    conn: { host: string; port: number; apiKey?: string; ssl?: boolean },
  ) {
    const client = createClient({
      host: conn.host,
      port: conn.port,
      apiKey: conn.apiKey,
      useSsl: conn.ssl,
    })

    let isProcessing = false

    function schedulePoll() {
      const delay = isProcessing ? POLL_INTERVAL_PROCESSING_MS : POLL_INTERVAL_INITIAL_MS
      const timer = setTimeout(poll, delay)
      pollingTimers.set(localId, timer)
    }

    async function poll() {
      try {
        const status = await client.getJobStatus(serverJobId)
        pollFailureCounts.set(localId, 0)

        if (status.status === 'processing') {
          isProcessing = true
          updateJob(localId, { progress: status.progress })
          schedulePoll()
        } else if (status.status === 'completed') {
          stopPolling(localId)
          updateJob(localId, {
            status: 'success',
            response: status.result,
            completedAt: Date.now(),
          })
          const job = get().jobs.find(j => j.id === localId)
          if (job) refreshAfterIngestion(job.tableId)
          scheduleAutoDismiss(localId)
        } else if (status.status === 'failed') {
          stopPolling(localId)
          updateJob(localId, {
            status: 'error',
            error: status.error || 'Ingestion failed on server',
            completedAt: Date.now(),
          })
        } else {
          schedulePoll()
        }
      } catch (err) {
        const msg = err instanceof Error ? err.message : ''
        const isNotFound = /not found/i.test(msg)

        if (isNotFound) {
          stopPolling(localId)
          updateJob(localId, {
            status: 'error',
            error: 'Job lost — server may have restarted',
            completedAt: Date.now(),
          })
          return
        }

        const failures = (pollFailureCounts.get(localId) ?? 0) + 1
        pollFailureCounts.set(localId, failures)

        if (failures >= MAX_POLL_FAILURES) {
          stopPolling(localId)
          updateJob(localId, {
            status: 'error',
            error: 'Server unreachable — connection lost',
            completedAt: Date.now(),
          })
        } else {
          schedulePoll()
        }
      }
    }

    schedulePoll()
  }

  function stopPolling(localId: string) {
    const timer = pollingTimers.get(localId)
    if (timer) {
      clearTimeout(timer)
      pollingTimers.delete(localId)
    }
    pollFailureCounts.delete(localId)
  }

  function refreshAfterIngestion(tableId: string) {
    const { selectedTableId } = useTableStore.getState()
    if (selectedTableId === tableId) {
      window.dispatchEvent(new CustomEvent('reasondb:documents-changed', { detail: { tableId } }))
    }
    window.dispatchEvent(new CustomEvent('reasondb:tables-changed'))
  }

  function scheduleAutoDismiss(jobId: string) {
    setTimeout(() => {
      const { jobs } = get()
      const job = jobs.find(j => j.id === jobId)
      if (job && job.status === 'success') {
        set({ jobs: get().jobs.filter(j => j.id !== jobId) })
      }
    }, AUTO_DISMISS_MS)
  }

  return {
    jobs: [],

    queueJob: (jobInput) => {
      const id = crypto.randomUUID()
      const job: IngestionJob = {
        ...jobInput,
        id,
        status: 'queued',
        queuedAt: Date.now(),
      }

      set({ jobs: [...get().jobs, job] })
      submitJob(job)
    },

    retryJob: (id) => {
      stopPolling(id)
      const job = get().jobs.find(j => j.id === id)
      if (!job) return

      updateJob(id, {
        status: 'queued',
        error: undefined,
        response: undefined,
        completedAt: undefined,
        serverJobId: undefined,
        progress: undefined,
      })

      submitJob({ ...job, status: 'queued', error: undefined, response: undefined, completedAt: undefined, serverJobId: undefined, progress: undefined })
    },

    dismissJob: (id) => {
      stopPolling(id)
      set({ jobs: get().jobs.filter(j => j.id !== id) })
    },

    clearCompleted: () => {
      for (const job of get().jobs) {
        if (job.status === 'success' || job.status === 'error') {
          stopPolling(job.id)
        }
      }
      set({ jobs: get().jobs.filter(j => j.status === 'queued' || j.status === 'ingesting') })
    },
  }
})
