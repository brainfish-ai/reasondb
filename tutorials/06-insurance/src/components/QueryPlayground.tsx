"use client"
import { useState, useEffect, useRef } from "react"
import dynamic from "next/dynamic"
import type { OnMount } from "@monaco-editor/react"
import { Play, Trash2, Loader2, Clock } from "lucide-react"
import { Button } from "@/components/ui/button"
import { ReasonDBClient, type QueryResult } from "@/lib/api"
import { registerRqlLanguage, ensureTheme, RQL_LANGUAGE_ID, RQL_THEME_NAME } from "@reasondb/rql-editor"

const MonacoEditor = dynamic(() => import("@monaco-editor/react"), { ssr: false })

export interface ExampleQuery {
  label: string
  query: string
  badge?: "BM25" | "REASON" | "AGG" | "SQL" | "COMBO"
}

interface Props {
  serverUrl: string
  apiKey: string
  examples: ExampleQuery[]
  onResult: (result: QueryResult | null) => void
  onError: (err: string | null) => void
  isDataReady: boolean
  accentColor?: string
  selectedIdx?: number
}

const BADGE_STYLES: Record<string, string> = {
  BM25: "bg-amber-100 text-amber-800 border-amber-200",
  REASON: "bg-purple-100 text-purple-800 border-purple-200",
  COMBO: "bg-rose-100 text-rose-800 border-rose-200",
  AGG: "bg-blue-100 text-blue-800 border-blue-200",
  SQL: "bg-slate-100 text-slate-700 border-slate-200",
}

export function QueryPlayground({
  serverUrl,
  apiKey,
  examples,
  onResult,
  onError,
  isDataReady,
  selectedIdx,
}: Props) {
  const [query, setQuery] = useState(examples[0]?.query ?? "")
  const [running, setRunning] = useState(false)
  const [progressMsg, setProgressMsg] = useState("")
  const [activeIdx, setActiveIdx] = useState(0)
  const [elapsed, setElapsed] = useState(0)
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null)

  // Keep a stable ref to `run` so the Monaco keybinding always calls the latest closure
  const runRef = useRef<() => void>(() => {})

  const isReason = query.toUpperCase().includes("REASON")

  const run = async () => {
    if (!query.trim() || !serverUrl) return
    setRunning(true)
    setProgressMsg("")
    setElapsed(0)
    onResult(null)
    onError(null)

    const start = Date.now()
    timerRef.current = setInterval(() => {
      setElapsed(Math.floor((Date.now() - start) / 1000))
    }, 1000)

    try {
      const client = new ReasonDBClient(serverUrl, apiKey || undefined)
      let result
      if (isReason) {
        result = await client.executeQueryStream(query, (msg) => setProgressMsg(msg))
      } else {
        result = await client.executeQuery(query)
      }
      onResult(result)
    } catch (e) {
      onError(e instanceof Error ? e.message : "Query failed")
    } finally {
      if (timerRef.current) clearInterval(timerRef.current)
      setRunning(false)
      setProgressMsg("")
      setElapsed(0)
    }
  }

  // Always keep runRef pointing at the latest run function
  runRef.current = run

  // Sync when parent drives selectedIdx (e.g. clicking a step in the sidebar)
  useEffect(() => {
    if (selectedIdx !== undefined && examples[selectedIdx]) {
      setActiveIdx(selectedIdx)
      setQuery(examples[selectedIdx].query)
      onResult(null)
      onError(null)
    }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedIdx])

  const selectExample = (idx: number) => {
    setActiveIdx(idx)
    setQuery(examples[idx].query)
    onResult(null)
    onError(null)
  }

  const handleEditorMount: OnMount = (editor, monaco) => {
    // Register shared RQL language + theme (idempotent)
    ensureTheme(monaco)
    registerRqlLanguage(monaco)

    // Register ⌘/Ctrl+Enter to run the query
    editor.addAction({
      id: "run-query",
      label: "Run Query",
      keybindings: [monaco.KeyMod.CtrlCmd | monaco.KeyCode.Enter],
      run: () => runRef.current(),
    })
    editor.focus()
  }

  return (
    <div className="space-y-3">
      {/* Preset buttons */}
      <div className="flex flex-wrap gap-1.5">
        {examples.map((ex, i) => (
          <button
            key={i}
            onClick={() => selectExample(i)}
            className={`inline-flex items-center gap-1.5 px-2.5 py-1 rounded-md text-xs font-medium border transition-colors ${
              i === activeIdx
                ? "bg-primary text-primary-foreground border-primary"
                : "bg-background text-foreground border-border hover:bg-accent"
            }`}
          >
            {ex.label}
            {ex.badge && (
              <span className={`text-[10px] px-1 rounded border ${BADGE_STYLES[ex.badge] ?? ""}`}>
                {ex.badge}
              </span>
            )}
          </button>
        ))}
      </div>

      {/* Monaco Editor — RQL language with ReasonDB dark theme */}
      <div className="rounded-md overflow-hidden border border-slate-700">
        <MonacoEditor
          height={160}
          language={RQL_LANGUAGE_ID}
          theme={RQL_THEME_NAME}
          value={query}
          onChange={(val) => setQuery(val ?? "")}
          onMount={handleEditorMount}
          options={{
            minimap: { enabled: false },
            fontSize: 13,
            fontFamily: "'JetBrains Mono', 'Fira Code', 'SF Mono', Consolas, monospace",
            lineNumbers: "on",
            wordWrap: "on",
            scrollBeyondLastLine: false,
            renderLineHighlight: "line",
            padding: { top: 10, bottom: 10 },
            overviewRulerLanes: 0,
            hideCursorInOverviewRuler: true,
            scrollbar: {
              vertical: "auto",
              horizontal: "hidden",
              verticalScrollbarSize: 6,
            },
            bracketPairColorization: { enabled: true },
            quickSuggestions: false,
            contextmenu: false,
            folding: false,
            glyphMargin: false,
            lineDecorationsWidth: 4,
            lineNumbersMinChars: 3,
          }}
        />
      </div>

      {/* Controls */}
      <div className="flex items-center gap-2">
        <Button onClick={run} disabled={running || !isDataReady} className="gap-2">
          {running ? <Loader2 className="h-4 w-4 animate-spin" /> : <Play className="h-4 w-4" />}
          {running ? "Running…" : isReason ? "Run with Reason" : "Run Query"}
        </Button>
        <Button
          variant="outline"
          size="icon"
          onClick={() => { setQuery(""); onResult(null); onError(null) }}
        >
          <Trash2 className="h-4 w-4" />
        </Button>
        {running && elapsed > 0 && (
          <span className="flex items-center gap-1 text-xs text-muted-foreground tabular-nums">
            <Clock className="h-3 w-3" />
            {elapsed}s
          </span>
        )}
        {!isDataReady && (
          <span className="text-xs text-muted-foreground">Load dataset first</span>
        )}
      </div>

      {running && progressMsg && (
        <div className="flex items-center gap-2 text-xs text-muted-foreground">
          <Loader2 className="h-3 w-3 animate-spin" />
          {progressMsg}
        </div>
      )}

      <p className="text-[11px] text-muted-foreground">
        Tip: Press{" "}
        <kbd className="px-1 py-0.5 rounded border text-[10px] bg-muted">⌘ Enter</kbd> to run
      </p>
    </div>
  )
}
