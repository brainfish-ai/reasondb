"use client"
import { useState, useEffect, useCallback, useRef } from "react"
import { Shield, ChevronRight, Search, Brain, Layers, Loader2, MessageSquare, Clock, Rows, Database, FileText, ChevronDown } from "lucide-react"
import { ConnectionBar } from "@/components/ConnectionBar"
import { DataSetupPanel } from "@/components/DataSetupPanel"
import { ChatCopilot } from "@/components/ChatCopilot"
import { TraceViewer } from "@/components/TraceViewer"
import { MatchedNodesTree } from "@/components/MatchedNodesTree"
import { Badge } from "@/components/ui/badge"
import { ScrollArea } from "@/components/ui/scroll-area"
import { Separator } from "@/components/ui/separator"
import { initializeDataset } from "./actions"
import { ReasonDBClient } from "@/lib/api"
import type { QueryResult, QueryTrace } from "@/lib/api"

const TABLE_NAME = "aia_insurance"

interface TableInfo {
  id: string
  name: string
  description?: string
  metadata: Record<string, unknown>
  document_count: number
  total_nodes: number
}

type StepGroup = "search" | "reason" | "combo"

interface Step {
  num: number
  title: string
  badge: string
  desc: string
  query: string
  group: StepGroup
}

const STEPS: Step[] = [
  // Search
  {
    num: 1, title: "Browse Documents", badge: "SQL", group: "search",
    desc: "List all 4 AIA insurance documents ordered by year.",
    query: `SELECT title, metadata.year, metadata.type FROM ${TABLE_NAME} ORDER BY metadata.year ASC`,
  },
  {
    num: 2, title: "Filter by Type", badge: "SQL", group: "search",
    desc: "Retrieve only Product Disclosure Statement documents.",
    query: `SELECT * FROM ${TABLE_NAME} WHERE metadata.type = 'product-disclosure-statement'`,
  },
  {
    num: 3, title: "SEARCH Terms", badge: "BM25", group: "search",
    desc: "BM25 full-text search for waiting period and disability clauses.",
    query: `SELECT * FROM ${TABLE_NAME} SEARCH 'waiting period income protection disability'`,
  },
  {
    num: 4, title: "COUNT Documents", badge: "AGG", group: "search",
    desc: "Verify all 4 documents were ingested.",
    query: `SELECT COUNT(*) FROM ${TABLE_NAME}`,
  },
  // Reason
  {
    num: 5, title: "REASON — Waiting Period", badge: "REASON", group: "reason",
    desc: "Ask about income protection waiting periods across all policy documents.",
    query: `SELECT * FROM ${TABLE_NAME} REASON 'What is the waiting period for income protection?' LIMIT 5`,
  },
  {
    num: 6, title: "REASON — TPD Benefit", badge: "REASON", group: "reason",
    desc: "Find the maximum benefit for total and permanent disability.",
    query: `SELECT * FROM ${TABLE_NAME} REASON 'What is the maximum benefit amount for total and permanent disability?' LIMIT 5`,
  },
  {
    num: 7, title: "REASON — Exclusions", badge: "REASON", group: "reason",
    desc: "Identify all income protection claim exclusions.",
    query: `SELECT * FROM ${TABLE_NAME} REASON 'What exclusions apply to income protection claims?' LIMIT 5`,
  },
  {
    num: 8, title: "REASON — Disability Defs", badge: "REASON", group: "reason",
    desc: "Compare how Income Care Plus and Priority Protection define disability.",
    query: `SELECT * FROM ${TABLE_NAME} REASON 'How do the Income Care Plus policy and Priority Protection policy differ in their definition of disability?' LIMIT 5`,
  },
  {
    num: 9, title: "REASON — 2025 Changes", badge: "REASON", group: "reason",
    desc: "Summarise what changed in the November 2025 policy enhancement.",
    query: `SELECT * FROM ${TABLE_NAME} REASON 'What changes were made to the Priority Protection policy in the 2025 enhancement update?' LIMIT 5`,
  },
  {
    num: 10, title: "REASON — Mental Health", badge: "REASON", group: "reason",
    desc: "Multi-hop: pre-existing mental health condition + disability claim — what exclusions and waits apply?",
    query: `SELECT * FROM ${TABLE_NAME} REASON 'If a person has a pre-existing mental health condition and later files a disability claim, what exclusions and waiting periods apply?' LIMIT 5`,
  },
  {
    num: 11, title: "REASON — All Benefits", badge: "REASON", group: "reason",
    desc: "Synthesise all benefit types across Priority Protection policies.",
    query: `SELECT * FROM ${TABLE_NAME} REASON 'List all the different types of insurance benefits available under the Priority Protection policies' LIMIT 5`,
  },
  // Combo
  {
    num: 12, title: "COMBO — Cancel Policy", badge: "COMBO", group: "combo",
    desc: "BM25-search cancel/terminate passages, then reason about insurer cancellation rights.",
    query: `SELECT * FROM ${TABLE_NAME} SEARCH 'cancel terminate lapse policy' REASON 'Under what circumstances can the insurer cancel or alter a policy?' LIMIT 5`,
  },
  {
    num: 13, title: "COMBO — Hazardous Work", badge: "COMBO", group: "combo",
    desc: "Search dangerous occupation passages, then reason about claim impact.",
    query: `SELECT * FROM ${TABLE_NAME} SEARCH 'dangerous occupation aviation war exclusion' REASON 'What happens to a claim if the insured person engages in a hazardous occupation or extreme sport?' LIMIT 5`,
  },
]

const BADGE_COLORS: Record<string, string> = {
  SQL:    "bg-slate-100 text-slate-700",
  BM25:   "bg-amber-100 text-amber-800",
  REASON: "bg-blue-100 text-blue-800",
  AGG:    "bg-emerald-100 text-emerald-800",
  COMBO:  "bg-rose-100 text-rose-800",
}

const GROUP_META: Record<StepGroup, { label: string; icon: React.ReactNode; color: string }> = {
  search: { label: "Search",      icon: <Search className="h-3 w-3" />, color: "text-slate-500" },
  reason: { label: "Reason",      icon: <Brain className="h-3 w-3" />,  color: "text-blue-600" },
  combo:  { label: "Combination", icon: <Layers className="h-3 w-3" />, color: "text-rose-600" },
}

const CHAT_SUGGESTIONS = [
  "What is the waiting period for income protection?",
  "What exclusions apply to mental health claims?",
  "How do Income Care Plus and Priority Protection differ on disability definitions?",
  "What benefit changes were introduced in the 2025 enhancement update?",
  "What happens if I engage in a hazardous occupation?",
]

export default function Page() {
  const [serverUrl, setServerUrl] = useState("http://localhost:4444")
  const [apiKey, setApiKey] = useState("")
  const [isDataReady, setIsDataReady] = useState(false)

  const [tableInfo, setTableInfo] = useState<TableInfo | null>(null)
  const [tableInfoExpanded, setTableInfoExpanded] = useState(false)

  const [result, setResult] = useState<QueryResult | null>(null)
  const [traceData, setTraceData] = useState<QueryTrace | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [isRunning, setIsRunning] = useState(false)
  const [progressMsg, setProgressMsg] = useState("")
  const [activeStep, setActiveStep] = useState<number | null>(null)
  const [lastQuery, setLastQuery] = useState<string>("")

  // Keep a stable ref to the current server URL and API key for the executeQuery callback
  const serverUrlRef = useRef(serverUrl)
  const apiKeyRef = useRef(apiKey)
  useEffect(() => { serverUrlRef.current = serverUrl }, [serverUrl])
  useEffect(() => { apiKeyRef.current = apiKey }, [apiKey])

  useEffect(() => {
    const url = localStorage.getItem("reasondb_server_url")
    const key = localStorage.getItem("reasondb_api_key")
    if (url) setServerUrl(url)
    if (key) setApiKey(key)
  }, [])

  const handleUrlChange = (url: string) => {
    setServerUrl(url)
    localStorage.setItem("reasondb_server_url", url)
  }
  const handleKeyChange = (key: string) => {
    setApiKey(key)
    localStorage.setItem("reasondb_api_key", key)
  }

  const executeQuery = useCallback(async (rql: string) => {
    const client = new ReasonDBClient(serverUrlRef.current, apiKeyRef.current)
    setIsRunning(true)
    setTraceData(null)
    setResult(null)
    setError(null)
    setProgressMsg("")
    setLastQuery(rql)
    try {
      const queryResult = await client.executeQueryStream(rql, setProgressMsg)
      // Release the running state BEFORE fetching the trace so that the
      // ChatCopilot effect sees isRunning=false and result at the same time
      // and can trigger answer streaming immediately.
      setResult(queryResult)
      setIsRunning(false)
      setProgressMsg("")
      if (queryResult.trace_id) {
        try {
          const trace = await client.fetchTrace(TABLE_NAME, queryResult.trace_id)
          setTraceData(trace)
        } catch {
          // Trace fetch failing is non-fatal
        }
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : "Query failed")
      setIsRunning(false)
      setProgressMsg("")
    }
  }, [])

  // Fetch live table info whenever data becomes ready or connection changes
  useEffect(() => {
    if (!isDataReady || !serverUrl) return
    const client = new ReasonDBClient(serverUrl, apiKey || undefined)
    fetch(`${serverUrl.replace(/\/$/, "")}/v1/tables/${TABLE_NAME}`, {
      headers: apiKey ? { "X-API-Key": apiKey } : {},
    })
      .then((r) => r.ok ? r.json() : null)
      .then((data) => { if (data) setTableInfo(data as TableInfo) })
      .catch(() => {})
  }, [isDataReady, serverUrl, apiKey])

  const handleChatQuery = useCallback((rqlQuery: string) => {
    setActiveStep(null)
    executeQuery(rqlQuery)
  }, [executeQuery])

  const groups: StepGroup[] = ["search", "reason", "combo"]
  const isReason = !!(result?.matchedNodes && result.matchedNodes.length > 0)

  return (
    <div className="flex flex-col h-screen overflow-hidden">
      <ConnectionBar
        serverUrl={serverUrl}
        apiKey={apiKey}
        onServerUrlChange={handleUrlChange}
        onApiKeyChange={handleKeyChange}
      />

      <div className="flex flex-1 overflow-hidden min-h-0">

        {/* ── Left panel: setup + step list ── */}
        <div className="w-72 shrink-0 border-r flex flex-col overflow-hidden">
          {/* Header */}
          <div className="p-4 border-b bg-gradient-to-br from-blue-50 to-sky-50 shrink-0">
            <div className="flex items-center gap-2 mb-2">
              <div className="p-1.5 rounded-md bg-blue-600">
                <Shield className="h-4 w-4 text-white" />
              </div>
              <div>
                <h1 className="text-sm font-bold">Insurance Policy Analyser</h1>
                <p className="text-[11px] text-muted-foreground">AIA Australia · POV Demo</p>
              </div>
            </div>
            <p className="text-xs text-muted-foreground">
              Ask natural language questions about 4 AIA insurance policy documents and get cited, traceable answers.
            </p>
          </div>

          {/* Dataset setup */}
          <div className="p-3 border-b shrink-0">
            <DataSetupPanel
              tableName={TABLE_NAME}
              docCount={4}
              serverUrl={serverUrl}
              apiKey={apiKey}
              label="AIA Insurance Documents"
              description="4 AIA Australia documents: Income Care Plus (2011), Priority Protection PDS, IBR, and Enhancement Summary (Nov 2025)."
              onInitialize={initializeDataset}
              onReady={() => setIsDataReady(true)}
            />
          </div>

          {/* Table info */}
          {tableInfo && (
            <div className="border-b shrink-0">
              <button
                className="w-full flex items-center justify-between px-3 py-2 hover:bg-muted/40 transition-colors"
                onClick={() => setTableInfoExpanded((v) => !v)}
              >
                <div className="flex items-center gap-1.5 text-xs font-semibold text-muted-foreground">
                  <Database className="h-3.5 w-3.5" />
                  Table Info
                </div>
                <ChevronDown className={`h-3.5 w-3.5 text-muted-foreground transition-transform ${tableInfoExpanded ? "rotate-180" : ""}`} />
              </button>
              {tableInfoExpanded && (
                <div className="px-3 pb-3 space-y-2">
                  {/* Name + doc count */}
                  <div className="flex items-start justify-between gap-2">
                    <div>
                      <p className="text-xs font-semibold">{tableInfo.name}</p>
                      {tableInfo.description && (
                        <p className="text-[11px] text-muted-foreground mt-0.5 leading-relaxed">{tableInfo.description}</p>
                      )}
                    </div>
                  </div>
                  <div className="flex items-center gap-3 text-[11px] text-muted-foreground">
                    <span className="flex items-center gap-1">
                      <FileText className="h-3 w-3" />
                      {tableInfo.document_count} docs
                    </span>
                    <span className="flex items-center gap-1">
                      <Rows className="h-3 w-3" />
                      {tableInfo.total_nodes.toLocaleString()} nodes
                    </span>
                  </div>
                  {/* Metadata key-value */}
                  {Object.keys(tableInfo.metadata).length > 0 && (
                    <div className="space-y-1">
                      <p className="text-[10px] font-semibold uppercase tracking-wide text-muted-foreground">Metadata</p>
                      <div className="rounded-md border bg-muted/30 divide-y">
                        {Object.entries(tableInfo.metadata)
                          .filter(([k]) => k !== "domain_vocab")
                          .map(([k, v]) => (
                            <div key={k} className="flex items-start gap-2 px-2 py-1.5 text-[11px]">
                              <span className="font-medium text-muted-foreground shrink-0 w-24 truncate">{k}</span>
                              <span className="text-foreground break-all">
                                {typeof v === "object" ? JSON.stringify(v).slice(0, 60) : String(v)}
                              </span>
                            </div>
                          ))}
                        {/* Domain vocab as pill list */}
                        {Array.isArray(tableInfo.metadata.domain_vocab) && (
                          <div className="px-2 py-1.5">
                            <p className="text-[10px] font-medium text-muted-foreground mb-1">domain_vocab</p>
                            <div className="flex flex-wrap gap-1">
                              {(tableInfo.metadata.domain_vocab as string[]).slice(0, 24).map((t) => (
                                <span key={t} className="px-1.5 py-0.5 rounded-full bg-blue-50 border border-blue-100 text-[10px] text-blue-700">
                                  {t}
                                </span>
                              ))}
                              {(tableInfo.metadata.domain_vocab as string[]).length > 24 && (
                                <span className="text-[10px] text-muted-foreground">
                                  +{(tableInfo.metadata.domain_vocab as string[]).length - 24} more
                                </span>
                              )}
                            </div>
                          </div>
                        )}
                      </div>
                    </div>
                  )}
                </div>
              )}
            </div>
          )}

          {/* Step list */}
          <div className="flex-1 overflow-y-auto p-3 space-y-3">
            {groups.map((group) => {
              const meta = GROUP_META[group]
              const groupSteps = STEPS.filter((s) => s.group === group)
              return (
                <div key={group}>
                  <div className={`flex items-center gap-1.5 px-1 mb-1.5 ${meta.color}`}>
                    {meta.icon}
                    <p className="text-[11px] font-semibold uppercase tracking-wide">{meta.label}</p>
                  </div>
                  <div className="space-y-1.5">
                    {groupSteps.map((step) => (
                      <div
                        key={step.num}
                        className={`rounded-md border p-3 space-y-1.5 cursor-pointer transition-colors ${
                          activeStep === step.num ? "border-blue-200 bg-blue-50" : "hover:bg-muted/40"
                        }`}
                        onClick={() => {
                          setActiveStep(step.num)
                          executeQuery(step.query)
                        }}
                      >
                        <div className="flex items-center gap-2">
                          <span className="w-5 h-5 rounded-full bg-muted flex items-center justify-center text-[10px] font-bold text-muted-foreground shrink-0">
                            {step.num}
                          </span>
                          <span className="text-xs font-medium flex-1">{step.title}</span>
                          <span className={`text-[10px] px-1.5 py-0.5 rounded font-medium ${BADGE_COLORS[step.badge]}`}>
                            {step.badge}
                          </span>
                        </div>
                        <p className="text-[11px] text-muted-foreground pl-7">{step.desc}</p>
                        <div className="pl-7">
                          <button className="flex items-center gap-1 text-[11px] text-blue-700 hover:text-blue-900 font-medium">
                            Run <ChevronRight className="h-3 w-3" />
                          </button>
                        </div>
                      </div>
                    ))}
                  </div>
                </div>
              )
            })}
          </div>
        </div>

        {/* ── Centre panel: analysis results + trace ── */}
        <div className="flex-1 flex flex-col overflow-hidden min-w-0">
          {/* Header */}
          <div className="p-4 border-b shrink-0">
            <div className="flex items-center gap-2 mb-1">
              <h2 className="text-sm font-semibold">Analysis</h2>
              {result && (
                <>
                  <Badge variant="outline" className="text-xs flex items-center gap-1">
                    <Clock className="h-3 w-3" />
                    {result.executionTimeMs}ms
                  </Badge>
                  <Badge variant="outline" className="text-xs flex items-center gap-1">
                    <Rows className="h-3 w-3" />
                    {result.rowCount} row{result.rowCount !== 1 ? "s" : ""}
                  </Badge>
                </>
              )}
              {isRunning && (
                <Badge variant="outline" className="text-xs flex items-center gap-1 text-blue-600 border-blue-200">
                  <Loader2 className="h-3 w-3 animate-spin" />
                  Running
                </Badge>
              )}
            </div>
            <p className="text-xs text-muted-foreground">
              {lastQuery
                ? <span className="font-mono text-[11px] truncate block max-w-full">{lastQuery}</span>
                : "Ask a question in the chat or click a step to run a query."}
            </p>
          </div>

          {/* Content */}
          <div className="flex-1 overflow-hidden min-h-0">

            {/* Idle state */}
            {!isRunning && !result && !error && (
              <div className="h-full flex flex-col items-center justify-center gap-3 text-center px-8">
                <div className="p-4 rounded-full bg-blue-50">
                  <MessageSquare className="h-8 w-8 text-blue-400" />
                </div>
                <div>
                  <p className="text-sm font-medium text-gray-700">Ask the copilot a question</p>
                  <p className="text-xs text-muted-foreground mt-1">
                    The pipeline trace and raw results will appear here. AI answers appear in the Brainfish Assist panel.
                  </p>
                </div>
              </div>
            )}

            {/* Running state */}
            {isRunning && (
              <div className="h-full flex flex-col items-center justify-center gap-3">
                <Loader2 className="h-8 w-8 animate-spin text-blue-500" />
                <p className="text-sm text-muted-foreground">
                  {progressMsg || "Analyzing policy documents…"}
                </p>
              </div>
            )}

            {/* Error state */}
            {!isRunning && error && (
              <div className="p-4">
                <div className="rounded-lg border border-destructive/40 bg-destructive/5 p-4">
                  <p className="text-sm font-medium text-destructive mb-1">Query Error</p>
                  <pre className="text-xs text-destructive/80 whitespace-pre-wrap font-mono">{error}</pre>
                </div>
              </div>
            )}

            {/* Results */}
            {!isRunning && result && (
              <div className="h-full flex flex-col overflow-hidden">

                {/* REASON result: matched nodes tree stacked above pipeline trace */}
                {isReason && result.matchedNodes && (
                  <ScrollArea className="h-full">
                    <div className="flex flex-col">
                      {/* Matched Nodes */}
                      <div className="border-b">
                        <div className="px-3 py-2 border-b bg-muted/20">
                          <span className="text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">
                            Matched Nodes
                          </span>
                        </div>
                        <MatchedNodesTree matchedNodes={result.matchedNodes} />
                      </div>

                      {/* Pipeline Trace */}
                      <div>
                        <div className="px-3 py-2 border-b bg-muted/20">
                          <span className="text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">
                            Pipeline Trace
                            {!traceData && <span className="ml-1.5 normal-case font-normal text-muted-foreground/60">(loading…)</span>}
                          </span>
                        </div>
                        {traceData ? (
                          <div className="p-4">
                            <TraceViewer trace={traceData} />
                          </div>
                        ) : (
                          <div className="flex items-center justify-center py-12 text-sm text-muted-foreground">
                            <Loader2 className="h-4 w-4 animate-spin mr-2" /> Loading trace…
                          </div>
                        )}
                      </div>
                    </div>
                  </ScrollArea>
                )}

                {/* Non-REASON result: simple data table */}
                {!isReason && (
                  <ScrollArea className="h-full">
                    <div className="p-4 space-y-2">
                      <div className="flex items-center gap-2 text-xs text-muted-foreground">
                        <Rows className="h-3.5 w-3.5" />
                        <span>{result.rowCount} row{result.rowCount !== 1 ? "s" : ""}</span>
                        <Clock className="h-3.5 w-3.5 ml-2" />
                        <span>{result.executionTimeMs}ms</span>
                      </div>
                      {result.rows.length === 0 ? (
                        <div className="rounded-md border p-6 text-center text-sm text-muted-foreground">
                          No rows returned
                        </div>
                      ) : (
                        <div className="rounded-md border overflow-auto">
                          <table className="w-full text-xs">
                            <thead className="bg-muted/80">
                              <tr>
                                {result.columns.map((col, ci) => (
                                  <th
                                    key={`${col}-${ci}`}
                                    className="text-left px-3 py-2 font-medium text-muted-foreground whitespace-nowrap border-b"
                                  >
                                    {col}
                                  </th>
                                ))}
                              </tr>
                            </thead>
                            <tbody>
                              {result.rows.map((row, i) => (
                                <tr key={i} className="border-b hover:bg-muted/30 transition-colors">
                                  {result.columns.map((col, ci) => {
                                    const val = row[col]
                                    const display =
                                      val === null || val === undefined ? (
                                        <span className="text-muted-foreground/50">null</span>
                                      ) : typeof val === "object" ? (
                                        <span className="text-blue-600">{JSON.stringify(val).slice(0, 80)}</span>
                                      ) : String(val).length > 120 ? (
                                        String(val).slice(0, 120) + "…"
                                      ) : (
                                        String(val)
                                      )
                                    return (
                                      <td key={`${col}-${ci}`} className="px-3 py-2 max-w-xs truncate align-top">
                                        {display}
                                      </td>
                                    )
                                  })}
                                </tr>
                              ))}
                            </tbody>
                          </table>
                        </div>
                      )}
                    </div>
                  </ScrollArea>
                )}

              </div>
            )}
          </div>
        </div>

        {/* ── Right panel: Brainfish Assist chat copilot ── */}
        <div className="w-[380px] shrink-0 flex flex-col overflow-hidden">
          <ChatCopilot
            tableName={TABLE_NAME}
            isDataReady={isDataReady}
            suggestedQuestions={CHAT_SUGGESTIONS}
            onQuery={handleChatQuery}
            isRunning={isRunning}
            progressMsg={progressMsg}
            result={result}
          />
        </div>

      </div>
    </div>
  )
}
