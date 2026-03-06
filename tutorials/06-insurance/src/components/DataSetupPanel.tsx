"use client"
import { useState, useEffect, useCallback, useRef } from "react"
import { Database, CheckCircle2, Loader2, AlertCircle, RefreshCw } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Progress } from "@/components/ui/progress"
import { ReasonDBClient } from "@/lib/api"

interface Props {
  tableName: string
  docCount: number
  serverUrl: string
  apiKey: string
  label: string
  description: string
  onInitialize: (serverUrl: string, apiKey: string) => Promise<{ jobIds: string[]; count: number }>
  onReady?: (docCount: number) => void
}

type Phase = "idle" | "loading" | "ingesting" | "done" | "error"

export function DataSetupPanel({
  tableName,
  docCount,
  serverUrl,
  apiKey,
  label,
  description,
  onInitialize,
  onReady,
}: Props) {
  const [phase, setPhase] = useState<Phase>("idle")
  const [progress, setProgress] = useState(0)
  const [statusText, setStatusText] = useState("")
  const [error, setError] = useState<string>()
  const [loadedCount, setLoadedCount] = useState(0)

  // Keep onReady in a ref so it never triggers effect re-runs
  const onReadyRef = useRef(onReady)
  useEffect(() => { onReadyRef.current = onReady })

  // Check whether the table already has data — runs once on mount and when
  // connection settings change. Does NOT depend on onReady to avoid loops.
  useEffect(() => {
    if (!serverUrl) return
    const client = new ReasonDBClient(serverUrl, apiKey || undefined)
    client.getTableDocCount(tableName).then((count) => {
      if (count > 0) {
        setLoadedCount(count)
        setPhase("done")
        onReadyRef.current?.(count)
      }
    })
  }, [serverUrl, apiKey, tableName])

  const pollJobs = useCallback(
    async (jobIds: string[], total: number) => {
      const client = new ReasonDBClient(serverUrl, apiKey || undefined)
      let completed = 0
      const pending = new Set(jobIds)

      while (pending.size > 0) {
        await new Promise((r) => setTimeout(r, 2000))
        for (const id of [...pending]) {
          try {
            const job = await client.getJobStatus(id)
            if (job.status === "completed") {
              pending.delete(id)
              completed++
              setProgress(Math.round((completed / total) * 100))
              setStatusText(`Ingested ${completed} of ${total} documents…`)
            } else if (job.status === "failed") {
              pending.delete(id)
              completed++
            }
          } catch {
            // retry next cycle
          }
        }
      }
    },
    [serverUrl, apiKey]
  )

  const handleInit = async () => {
    if (!serverUrl) {
      setError("Please configure the server URL first.")
      return
    }
    setPhase("loading")
    setError(undefined)
    setProgress(0)
    setStatusText("Creating table and submitting PDF URLs for ingestion…")
    try {
      const { jobIds, count } = await onInitialize(serverUrl, apiKey)
      setPhase("ingesting")
      setStatusText(`Fetching and ingesting ${count} PDF documents from aia.com.au…`)
      await pollJobs(jobIds, count)
      const client = new ReasonDBClient(serverUrl, apiKey || undefined)
      const finalCount = await client.getTableDocCount(tableName)
      setLoadedCount(finalCount)
      setPhase("done")
      setProgress(100)
      setStatusText("")
      onReadyRef.current?.(finalCount)
    } catch (e) {
      setPhase("error")
      setError(e instanceof Error ? e.message : "Setup failed")
    }
  }

  const handleReinit = () => {
    setPhase("idle")
    setLoadedCount(0)
    setProgress(0)
    setStatusText("")
    setError(undefined)
  }

  return (
    <div className="rounded-lg border bg-card p-4 space-y-3">
      <div className="flex items-center gap-2">
        <Database className="h-4 w-4 text-muted-foreground" />
        <span className="text-sm font-medium">{label}</span>
        {phase === "done" && (
          <CheckCircle2 className="h-4 w-4 text-emerald-500 ml-auto" />
        )}
      </div>

      <p className="text-xs text-muted-foreground">{description}</p>

      {phase === "done" ? (
        <div className="flex items-center justify-between">
          <span className="text-xs text-emerald-600 font-medium">
            {loadedCount} document{loadedCount !== 1 ? "s" : ""} ready in <code className="bg-muted px-1 rounded">{tableName}</code>
          </span>
          <Button variant="ghost" size="sm" className="h-7 text-xs gap-1" onClick={handleReinit}>
            <RefreshCw className="h-3 w-3" /> Reinitialize
          </Button>
        </div>
      ) : phase === "loading" || phase === "ingesting" ? (
        <div className="space-y-2">
          <div className="flex items-center gap-2 text-xs text-muted-foreground">
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
            {statusText}
          </div>
          {phase === "ingesting" && <Progress value={progress} className="h-1.5" />}
        </div>
      ) : phase === "error" ? (
        <div className="space-y-2">
          <div className="flex items-center gap-1.5 text-xs text-destructive">
            <AlertCircle className="h-3.5 w-3.5" />
            {error}
          </div>
          <Button size="sm" className="h-7 text-xs" onClick={handleInit}>Retry</Button>
        </div>
      ) : (
        <Button size="sm" className="h-8 text-xs w-full" onClick={handleInit}>
          Load Dataset ({docCount} docs)
        </Button>
      )}
    </div>
  )
}
