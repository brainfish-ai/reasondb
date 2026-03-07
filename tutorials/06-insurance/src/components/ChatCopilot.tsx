"use client"
import { useState, useRef, useEffect, useCallback } from "react"
import ReactMarkdown from "react-markdown"
import { ArrowUp, Plus, Clock, X, Sparkles, Loader2, Code2, FileText, RefreshCw, ChevronDown } from "lucide-react"
import type { QueryResult, MatchedNode } from "@/lib/api"

const POLICIES = [
  { slug: "",                                label: "All Policies",                     desc: "Search across all 4 AIA documents" },
  { slug: "income-care-plus",                label: "Income Care Plus",                 desc: "2011 personal income protection policy" },
  { slug: "priority-protection-pds",         label: "Priority Protection PDS",          desc: "Nov 2025 Product Disclosure Statement" },
  { slug: "priority-protection-ibr",         label: "Priority Protection IBR",          desc: "Nov 2025 Incorporated by Reference" },
  { slug: "priority-protection-enhancement", label: "Priority Protection Enhancement",  desc: "Nov 2025 enhancement summary" },
]

function policyLabel(slug: string): string {
  return POLICIES.find((p) => p.slug === slug)?.label ?? "All Policies"
}

interface Message {
  id: string
  role: "user" | "assistant"
  content: string
  rqlQuery?: string
  /** LLM-rewritten version of content, contextualised against conversation history */
  contextualQuestion?: string
  status?: "contextualising" | "building" | "running" | "answering" | "done" | "error"
  progressMsg?: string
  nodes?: MatchedNode[]
  question?: string
  answer?: string
  answerLoading?: boolean
  /** Set when REASON query returned 0 results — carries the active policy label for the hint */
  noResultsPolicy?: string
}

interface Props {
  tableName: string
  isDataReady: boolean
  suggestedQuestions: string[]
  onQuery: (rqlQuery: string, question: string) => void
  isRunning: boolean
  progressMsg: string
  result: QueryResult | null
  onClose?: () => void
}

/** Inline citation badge — clickable superscript number */
function CitationBadge({ num, node }: { num: number; node: MatchedNode | undefined }) {
  const [open, setOpen] = useState(false)
  const label = node?.path?.length ? node.path[node.path.length - 1] : node?.title
  const excerpt = node?.content ? node.content.slice(0, 160) + (node.content.length > 160 ? "…" : "") : ""

  return (
    <span className="relative inline-block leading-none">
      <button
        onMouseEnter={() => setOpen(true)}
        onMouseLeave={() => setOpen(false)}
        className="inline-flex items-center justify-center w-4 h-4 text-[9px] font-bold rounded-full bg-blue-100 text-blue-700 border border-blue-300 cursor-pointer hover:bg-blue-200 hover:scale-110 transition-all align-super mx-0.5"
        title={label}
      >
        {num}
      </button>
      {open && node && (
        <div className="absolute bottom-full left-1/2 -translate-x-1/2 mb-2 z-50 w-64 rounded-lg border bg-white text-foreground shadow-lg p-3 pointer-events-none text-left">
          <p className="text-[11px] font-semibold leading-tight mb-1">{label}</p>
          <p className="text-[10px] text-muted-foreground leading-relaxed">{excerpt}</p>
          <div className="absolute top-full left-1/2 -translate-x-1/2 w-0 h-0 border-l-4 border-r-4 border-t-4 border-l-transparent border-r-transparent border-t-border" />
        </div>
      )}
    </span>
  )
}

/** Short document name derived from node path */
function docLabel(node: MatchedNode): string {
  const root = node.path?.[0] ?? node.title
  if (root.toLowerCase().includes("income care")) return "Income Care Plus"
  if (root.toLowerCase().includes("enhancement")) return "Priority Protection Enhancement"
  if (root.toLowerCase().includes("incorporated") || root.toLowerCase().includes("ibr")) return "Priority Protection IBR"
  if (root.toLowerCase().includes("priority")) return "Priority Protection PDS"
  return root.split(" ").slice(0, 3).join(" ")
}

function MessageBubble({ msg, onResetPolicy }: { msg: Message; onResetPolicy?: () => void }) {
  if (msg.role === "user") {
    return (
      <div className="flex justify-end">
        <div className="max-w-[85%] rounded-[16px] rounded-tr-[4px] bg-[#1868DB] text-white px-3.5 py-2.5 text-[13px] leading-relaxed">
          {msg.content}
        </div>
      </div>
    )
  }

  const nodes = msg.nodes ?? []

  return (
    <div className="flex flex-col gap-2">
      {/* Brainfish Assist header */}
      <div className="flex items-center gap-2">
        <div className="w-6 h-6 rounded-full flex items-center justify-center shrink-0"
          style={{ background: "linear-gradient(168deg, #1868DB 0%, #7EE2B8 60%, #B3EE2B 100%)" }}>
          <Sparkles className="w-3 h-3 text-white" />
        </div>
        <span className="text-[11px] font-medium text-muted-foreground">Brainfish Assist</span>
      </div>

      {/* Status: contextualising */}
      {msg.status === "contextualising" && (
        <div className="ml-8 flex items-center gap-2 text-xs text-muted-foreground">
          <Loader2 className="h-3 w-3 animate-spin shrink-0" />
          <span>Understanding context…</span>
        </div>
      )}

      {/* Status: building / searching */}
      {(msg.status === "building" || msg.status === "running") && (
        <div className="ml-8 flex items-center gap-2 text-xs text-muted-foreground">
          <Loader2 className="h-3 w-3 animate-spin shrink-0" />
          <span>{msg.status === "building" ? "Building query…" : (msg.progressMsg ?? "Searching policy documents…")}</span>
        </div>
      )}

      {/* Generated RQL — with optional "Interpreted as:" annotation */}
      {msg.rqlQuery && (
        <div className="ml-8 rounded-lg border border-slate-200 bg-slate-50 overflow-hidden">
          <div className="flex items-center gap-1.5 px-3 py-1.5 border-b border-slate-200 bg-slate-100">
            <Code2 className="h-3 w-3 text-slate-500" />
            <span className="text-[10px] font-medium text-slate-600 uppercase tracking-wide">Generated RQL</span>
          </div>
          {/* Show contextual question if it differs from original */}
          {msg.contextualQuestion && msg.contextualQuestion !== msg.content && (
            <div className="px-3 pt-2 pb-1 flex items-start gap-1.5 border-b border-slate-200">
              <span className="text-[10px] text-slate-500 font-medium mt-0.5 shrink-0">Interpreted as:</span>
              <span className="text-[11px] text-slate-700 italic leading-relaxed">{msg.contextualQuestion}</span>
            </div>
          )}
          <pre className="px-3 py-2 text-[11px] font-mono text-slate-700 leading-relaxed whitespace-pre-wrap overflow-x-auto">
            {msg.rqlQuery}
          </pre>
        </div>
      )}

      {/* Answering spinner — shown before first token arrives */}
      {msg.status === "answering" && !msg.answer && (
        <div className="ml-8 flex items-center gap-2 text-xs text-muted-foreground">
          <Loader2 className="h-3 w-3 animate-spin shrink-0" />
          <span>Generating answer…</span>
        </div>
      )}

      {/* Streaming answer with inline citation badges */}
      {(msg.answer || msg.answerLoading) && (
        <div className="ml-8 text-[13px] leading-relaxed text-foreground">
          <div className="prose prose-sm max-w-none
            [&_p]:text-[13px] [&_p]:leading-relaxed [&_p]:mb-2 [&_p:last-child]:mb-0
            [&_ul]:text-[13px] [&_ul]:my-1.5 [&_ul]:pl-4 [&_ul>li]:mb-0.5
            [&_ol]:text-[13px] [&_ol]:my-1.5 [&_ol]:pl-4 [&_ol>li]:mb-0.5
            [&_strong]:font-semibold [&_strong]:text-foreground
            [&_h2]:text-[13px] [&_h2]:font-bold [&_h2]:mb-1 [&_h2]:mt-2
            [&_h3]:text-[13px] [&_h3]:font-semibold [&_h3]:mb-1 [&_h3]:mt-2
            [&_code]:text-[11px] [&_code]:bg-muted [&_code]:px-1 [&_code]:rounded
            [&_blockquote]:border-l-2 [&_blockquote]:border-blue-200 [&_blockquote]:pl-2 [&_blockquote]:italic [&_blockquote]:text-muted-foreground">
            <ReactMarkdown
              components={{
                text({ children }) {
                  const str = String(children)
                  if (!/\[\d+\]/.test(str)) return <>{str}</>
                  const parts = str.split(/(\[\d+\])/)
                  return (
                    <>
                      {parts.map((part, i) => {
                        const m = part.match(/^\[(\d+)\]$/)
                        if (m) {
                          const num = parseInt(m[1], 10)
                          return <CitationBadge key={i} num={num} node={nodes[num - 1]} />
                        }
                        return <span key={i}>{part}</span>
                      })}
                    </>
                  )
                },
              }}
            >
              {msg.answer ?? ""}
            </ReactMarkdown>
          </div>
          {msg.answerLoading && msg.answer && (
            <div className="flex items-center gap-1.5 mt-2 text-xs text-muted-foreground">
              <Loader2 className="h-3 w-3 animate-spin shrink-0" />
              <span>Generating…</span>
            </div>
          )}
        </div>
      )}

      {/* Compact source pills */}
      {(msg.status === "done" || (msg.answer && !msg.answerLoading)) && nodes.length > 0 && (
        <div className="ml-8 flex flex-wrap gap-1.5 pt-1">
          {nodes.map((node, i) => (
            <span
              key={node.node_id}
              className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full bg-blue-50 border border-blue-100 text-[10px] text-blue-700"
            >
              <FileText className="h-2.5 w-2.5 shrink-0" />
              <span className="font-medium">{i + 1}</span>
              <span className="text-blue-500">·</span>
              <span className="max-w-[120px] truncate">{docLabel(node)}</span>
              <span className="text-blue-400 tabular-nums">{Math.round(node.confidence * 100)}%</span>
            </span>
          ))}
        </div>
      )}

      {/* Error / no-results */}
      {msg.status === "error" && !msg.answer && (
        <div className="ml-8 space-y-1">
          {msg.noResultsPolicy !== undefined ? (
            <>
              <p className="text-[13px] text-muted-foreground">
                No matching sections found
                {msg.noResultsPolicy ? (
                  <> in <span className="font-semibold text-foreground">{msg.noResultsPolicy}</span></>
                ) : null}
                .
              </p>
              <p className="text-[11px] text-muted-foreground">
                Try rephrasing, or switch to{" "}
                <button
                  className="underline hover:text-foreground transition-colors"
                  onClick={onResetPolicy}
                >
                  All Policies
                </button>{" "}
                for cross-document comparisons.
              </p>
            </>
          ) : (
            <p className="text-[13px] text-destructive">Something went wrong. Please try again.</p>
          )}
        </div>
      )}
    </div>
  )
}

export function ChatCopilot({
  tableName,
  isDataReady,
  suggestedQuestions,
  onQuery,
  isRunning,
  progressMsg,
  result,
  onClose,
}: Props) {
  const [messages, setMessages] = useState<Message[]>([])
  const [inputValue, setInputValue] = useState("")
  const [pendingMsgId, setPendingMsgId] = useState<string | null>(null)
  const [selectedPolicy, setSelectedPolicy] = useState<string | null>(null)

  // Model selector
  const [selectedModel, setSelectedModel] = useState<string>("")
  const [availableModels, setAvailableModels] = useState<Array<{ id: string; name: string }>>([])
  const [modelMenuOpen, setModelMenuOpen] = useState(false)
  const modelMenuRef = useRef<HTMLDivElement>(null)

  const handleSelectModel = (modelId: string) => {
    setSelectedModel(modelId)
    if (modelId) {
      localStorage.setItem("reasondb_answer_model", modelId)
    } else {
      localStorage.removeItem("reasondb_answer_model")
    }
    setModelMenuOpen(false)
  }

  // Hydrate selectedModel from localStorage after mount (avoids SSR/CSR mismatch)
  useEffect(() => {
    const stored = localStorage.getItem("reasondb_answer_model")
    if (stored) setSelectedModel(stored)
  }, [])

  // Fetch available models once on mount
  useEffect(() => {
    fetch("/api/models")
      .then((r) => r.ok ? r.json() : null)
      .then((data) => {
        if (data?.models) setAvailableModels(data.models)
      })
      .catch(() => {})
  }, [])

  // Close model menu when clicking outside
  useEffect(() => {
    const handler = (e: MouseEvent) => {
      if (modelMenuRef.current && !modelMenuRef.current.contains(e.target as Node)) {
        setModelMenuOpen(false)
      }
    }
    document.addEventListener("mousedown", handler)
    return () => document.removeEventListener("mousedown", handler)
  }, [])
  const messagesEndRef = useRef<HTMLDivElement>(null)
  const textareaRef = useRef<HTMLTextAreaElement>(null)
  const prevResultRef = useRef<QueryResult | null>(null)
  const prevRunningRef = useRef(false)
  const answerAbortRef = useRef<AbortController | null>(null)
  // Stores the fallback (all-policies) RQL to fire when a scoped query returns 0 nodes
  const pendingRetryRef = useRef<string | null>(null)
  // Keep a live ref to messages so async callbacks can read current state
  const messagesRef = useRef<Message[]>([])
  useEffect(() => { messagesRef.current = messages }, [messages])

  const scrollToBottom = useCallback(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" })
  }, [])

  useEffect(() => { scrollToBottom() }, [messages, scrollToBottom])

  // Auto-resize textarea
  useEffect(() => {
    const el = textareaRef.current
    if (!el) return
    el.style.height = "auto"
    el.style.height = `${Math.min(el.scrollHeight, 120)}px`
  }, [inputValue])

  /** Stream the AI answer from /api/answer directly into the message bubble */
  const streamAnswer = useCallback(async (msgId: string, question: string, nodes: MatchedNode[]) => {
    if (answerAbortRef.current) answerAbortRef.current.abort()
    const ctrl = new AbortController()
    answerAbortRef.current = ctrl

    setMessages((prev) =>
      prev.map((m) =>
        m.id === msgId ? { ...m, status: "answering", answerLoading: true, answer: "" } : m
      )
    )

    try {
      const res = await fetch("/api/answer", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          question,
          context: nodes.map((n) => ({
            title: n.title,
            content: n.content,
            confidence: n.confidence,
            path: n.path,
          })),
          ...(selectedModel ? { model: selectedModel } : {}),
        }),
        signal: ctrl.signal,
      })

      if (!res.ok) {
        const err = await res.json().catch(() => ({}))
        throw new Error((err as { error?: string }).error ?? `HTTP ${res.status}`)
      }

      const reader = res.body?.getReader()
      if (!reader) throw new Error("No response body")
      const decoder = new TextDecoder()

      while (true) {
        const { done, value } = await reader.read()
        if (done) break
        const chunk = decoder.decode(value, { stream: true })
        setMessages((prev) =>
          prev.map((m) =>
            m.id === msgId ? { ...m, answer: (m.answer ?? "") + chunk } : m
          )
        )
      }

      setMessages((prev) =>
        prev.map((m) =>
          m.id === msgId ? { ...m, status: "done", answerLoading: false } : m
        )
      )
    } catch (e) {
      if ((e as Error).name !== "AbortError") {
        setMessages((prev) =>
          prev.map((m) =>
            m.id === msgId ? { ...m, status: "error", answerLoading: false } : m
          )
        )
      }
    }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedModel])

  // Update assistant message as query runs, then trigger answer streaming
  useEffect(() => {
    if (!pendingMsgId) return

    // Running just started
    if (isRunning && !prevRunningRef.current) {
      setMessages((prev) =>
        prev.map((m) =>
          m.id === pendingMsgId
            ? { ...m, status: "running", progressMsg: progressMsg || "Searching policy documents…" }
            : m
        )
      )
    }

    // Progress message updated while running
    if (isRunning && progressMsg) {
      setMessages((prev) =>
        prev.map((m) =>
          m.id === pendingMsgId ? { ...m, progressMsg } : m
        )
      )
    }

    // Query finished — result arrived
    if (!isRunning && prevRunningRef.current && result !== prevResultRef.current) {
      const nodes = result?.matchedNodes ?? []
      // Prefer result.question, fall back to the contextualQuestion stored on the pending message
      const pendingMsg = messagesRef.current.find((m) => m.id === pendingMsgId)
      const question = result?.question || pendingMsg?.contextualQuestion || pendingMsg?.content || ""

      if (nodes.length > 0) {
        // Store matched nodes in the message, then stream the answer
        setMessages((prev) =>
          prev.map((m) =>
            m.id === pendingMsgId ? { ...m, nodes, question } : m
          )
        )
        const msgId = pendingMsgId
        pendingRetryRef.current = null
        setPendingMsgId(null)
        streamAnswer(msgId, question, nodes)
      } else if (pendingRetryRef.current) {
        // Already retried with all-policies — still no results
        pendingRetryRef.current = null
        setMessages((prev) =>
          prev.map((m) =>
            m.id === pendingMsgId ? { ...m, status: "error", noResultsPolicy: undefined } : m
          )
        )
        setPendingMsgId(null)
      } else if (selectedPolicy !== null) {
        // Scoped query returned 0 nodes — auto-retry without policy filter
        // Read contextualQuestion from the pending message (result?.question can be empty)
        const pendingMsg = messagesRef.current.find((m) => m.id === pendingMsgId)
        const fallbackQ = pendingMsg?.contextualQuestion || pendingMsg?.content || ""
        const fallbackRql = `SELECT * FROM ${tableName} REASON '${fallbackQ}' LIMIT 10`
        pendingRetryRef.current = fallbackRql
        setMessages((prev) =>
          prev.map((m) =>
            m.id === pendingMsgId
              ? {
                  ...m,
                  status: "building",
                  rqlQuery: fallbackRql,
                  contextualQuestion: m.contextualQuestion,
                  progressMsg: "No results in selected policy — broadening to all policies…",
                }
              : m
          )
        )
        onQuery(fallbackRql, fallbackQ)
      } else {
        // No results at all — show hint
        setMessages((prev) =>
          prev.map((m) =>
            m.id === pendingMsgId ? { ...m, status: "error", noResultsPolicy: undefined } : m
          )
        )
        setPendingMsgId(null)
      }
    }

    prevRunningRef.current = isRunning
    prevResultRef.current = result
  }, [isRunning, progressMsg, result, pendingMsgId, streamAnswer, selectedPolicy, tableName, onQuery])

  const sendQuestion = useCallback(
    async (question: string) => {
      if (!question.trim() || isRunning) return

      const raw = question.trim()
      const userMsgId = crypto.randomUUID()
      const assistantMsgId = crypto.randomUUID()

      // Build conversation history from completed Q&A pairs BEFORE adding new messages
      const history = messagesRef.current
        .filter((m) => m.role === "user" || (m.role === "assistant" && m.answer))
        .slice(-6)
        .map((m) => ({
          role: m.role as "user" | "assistant",
          content: m.role === "user" ? m.content : (m.answer ?? ""),
        }))

      // Add user bubble + "Understanding context…" spinner immediately
      setMessages((prev) => [
        ...prev,
        { id: userMsgId, role: "user", content: raw },
        { id: assistantMsgId, role: "assistant", content: "", status: "contextualising" },
      ])
      setInputValue("")

      // Call /api/contextualize — the LLM decides:
      //   { intent: "query", contextualQuestion }  → search ReasonDB
      //   { intent: "direct_answer", answer }       → respond from conversation context
      let intent: "query" | "direct_answer" = "query"
      let contextualQuestion = raw
      let directAnswer = ""

      const activePolicyName = selectedPolicy !== null ? policyLabel(selectedPolicy) : ""

      try {
        const res = await fetch("/api/contextualize", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ history, question: raw, policyName: activePolicyName || undefined }),
        })
        if (res.ok) {
          const data = await res.json()
          intent = data.intent ?? "query"
          if (intent === "direct_answer") {
            directAnswer = (data.answer ?? "").trim()
          } else {
            contextualQuestion = (data.contextualQuestion ?? raw).trim()
          }
        }
      } catch { /* fall back gracefully to query */ }

      // --- Branch: direct answer (no ReasonDB needed) ---
      if (intent === "direct_answer") {
        setMessages((msgs) =>
          msgs.map((m) =>
            m.id === assistantMsgId
              ? { ...m, status: "done", answer: directAnswer || "I'm not sure how to help with that. Try asking a question about AIA insurance policies." }
              : m
          )
        )
        return
      }

      // --- Branch: query ReasonDB ---
      const whereClause = selectedPolicy ? `WHERE metadata.policy = '${selectedPolicy}' ` : ""
      const limit = selectedPolicy ? 10 : 5
      const rqlQuery = `SELECT * FROM ${tableName} ${whereClause}REASON '${contextualQuestion}' LIMIT ${limit}`

      // Update assistant message: transition to "building" with RQL + contextual question
      setMessages((msgs) =>
        msgs.map((m) =>
          m.id === assistantMsgId
            ? { ...m, status: "building", rqlQuery, contextualQuestion }
            : m
        )
      )
      setPendingMsgId(assistantMsgId)
      pendingRetryRef.current = null
      onQuery(rqlQuery, contextualQuestion)
    },
    [tableName, isRunning, onQuery, selectedPolicy]
  )

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault()
      sendQuestion(inputValue)
    }
  }

  const selectPolicy = useCallback((policy: typeof POLICIES[number]) => {
    setSelectedPolicy(policy.slug)
    const greeting = policy.slug
      ? `I'll answer your questions about **${policy.label}**. What would you like to know?`
      : `I'll search across **all 4 AIA insurance documents**. What would you like to know?`
    setMessages([{
      id: crypto.randomUUID(),
      role: "assistant",
      content: "",
      status: "done",
      answer: greeting,
    }])
  }, [])

  const resetPolicy = useCallback(() => {
    setSelectedPolicy(null)
    setMessages([])
  }, [])

  const canSend = inputValue.trim().length > 0 && !isRunning && isDataReady && selectedPolicy !== null
  const isEmpty = messages.length === 0

  return (
    <div className="flex flex-col h-full border-l bg-white">
      {/* Header — Figma 33:1908 */}
      <div className="flex items-center justify-between px-3 py-2.5 border-b bg-white shrink-0">
        <div className="flex items-center gap-2 min-w-0">
          {/* Gradient sparkle logo */}
          <div
            className="w-6 h-6 rounded-md flex items-center justify-center shrink-0 shadow-sm"
            style={{ background: "linear-gradient(168deg, #1868DB 7%, #357DE8 24%, #7EE2B8 61%, #B3EE2B 83%)" }}
          >
            <Sparkles className="w-3.5 h-3.5 text-white drop-shadow-sm" />
          </div>
          <span className="text-[12px] font-medium text-foreground shrink-0">Brainfish Assist</span>
          {/* Active policy badge */}
          {selectedPolicy !== null && (
            <span className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded-full bg-blue-50 border border-blue-200 text-[10px] font-medium text-blue-700 truncate max-w-[120px]">
              <FileText className="h-2.5 w-2.5 shrink-0" />
              <span className="truncate">{policyLabel(selectedPolicy)}</span>
            </span>
          )}
        </div>

        {/* Control buttons — Figma 33:1913 */}
        <div className="flex items-center gap-0.5 shrink-0">
          {/* Change policy button — only shown when policy is selected */}
          {selectedPolicy !== null && (
            <button
              onClick={resetPolicy}
              className="p-1.5 rounded hover:bg-muted transition-colors text-muted-foreground hover:text-foreground"
              title="Change policy"
            >
              <RefreshCw className="h-3.5 w-3.5" />
            </button>
          )}
          <button
            onClick={() => setMessages([])}
            className="p-1.5 rounded hover:bg-muted transition-colors text-muted-foreground hover:text-foreground"
            title="New conversation"
          >
            <Plus className="h-4 w-4" />
          </button>
          <button
            className="p-1.5 rounded hover:bg-muted transition-colors text-muted-foreground hover:text-foreground"
            title="History"
          >
            <Clock className="h-4 w-4" />
          </button>
          {onClose && (
            <>
              <div className="w-px h-3.5 bg-border mx-0.5" />
              <button
                onClick={onClose}
                className="p-1.5 rounded hover:bg-muted transition-colors text-muted-foreground hover:text-foreground"
                title="Close"
              >
                <X className="h-4 w-4" />
              </button>
            </>
          )}
        </div>
      </div>

      {/* Answer model selector */}
      <div className="px-3 py-1.5 border-b bg-muted/30 shrink-0 flex items-center justify-between gap-2">
        <span className="text-[11px] text-muted-foreground shrink-0">Answer model</span>
        <div className="relative" ref={modelMenuRef}>
          <button
            onClick={() => setModelMenuOpen((v) => !v)}
            className="flex items-center gap-1 text-[11px] font-medium text-foreground hover:bg-muted rounded px-2 py-1 transition-colors max-w-[200px]"
          >
            <span className="truncate">
              {selectedModel
                ? (availableModels.find((m) => m.id === selectedModel)?.name ?? selectedModel.split("/").pop())
                : (process.env.NEXT_PUBLIC_OPENROUTER_MODEL ?? "gemini-2.0-flash (default)")}
            </span>
            <ChevronDown className="h-3 w-3 shrink-0 text-muted-foreground" />
          </button>
          {modelMenuOpen && (
            <div className="absolute right-0 top-full mt-1 z-50 w-72 max-h-72 overflow-y-auto rounded-lg border bg-white shadow-lg">
              {/* Default option */}
              <button
                className={`w-full text-left px-3 py-2 text-[12px] hover:bg-muted transition-colors ${selectedModel === "" ? "bg-blue-50 text-blue-700 font-medium" : ""}`}
                onClick={() => handleSelectModel("")}
              >
                <span className="font-medium">Default</span>
                <span className="text-muted-foreground ml-1">(gemini-2.0-flash)</span>
              </button>
              <div className="border-t" />
              {availableModels.length === 0 && (
                <div className="px-3 py-3 text-[11px] text-muted-foreground flex items-center gap-1.5">
                  <Loader2 className="h-3 w-3 animate-spin" /> Loading models…
                </div>
              )}
              {availableModels.map((m) => (
                <button
                  key={m.id}
                  className={`w-full text-left px-3 py-2 text-[11px] hover:bg-muted transition-colors ${selectedModel === m.id ? "bg-blue-50 text-blue-700 font-medium" : ""}`}
                  onClick={() => handleSelectModel(m.id)}
                >
                  <span className="block font-medium truncate">{m.name}</span>
                  <span className="block text-[10px] text-muted-foreground truncate">{m.id}</span>
                </button>
              ))}
            </div>
          )}
        </div>
      </div>

      {/* Messages area */}
      <div className="flex-1 overflow-y-auto px-3 py-4 space-y-4 min-h-0">
        {/* Policy picker — shown until user selects a policy */}
        {isEmpty && selectedPolicy === null ? (
          <div className="h-full flex flex-col justify-center gap-4 px-1">
            {!isDataReady ? (
              <p className="text-center text-xs text-muted-foreground px-4">
                Load the insurance dataset first, then choose a policy to ask about.
              </p>
            ) : (
              <>
                <p className="text-[12px] font-semibold text-foreground text-center">
                  Which policy are you asking about?
                </p>
                <div className="flex flex-col gap-2">
                  {POLICIES.map((policy) => (
                    <button
                      key={policy.slug || "all"}
                      onClick={() => selectPolicy(policy)}
                      className="w-full text-left px-3 py-2.5 rounded-xl border border-[#e5e5e5] bg-white hover:bg-blue-50 hover:border-blue-200 transition-colors group"
                    >
                      <p className="text-[12px] font-medium text-foreground group-hover:text-blue-700 leading-tight">
                        {policy.label}
                      </p>
                      <p className="text-[11px] text-muted-foreground mt-0.5 leading-tight">
                        {policy.desc}
                      </p>
                    </button>
                  ))}
                </div>
              </>
            )}
          </div>
        ) : (
          <>
            {messages.map((msg) => <MessageBubble key={msg.id} msg={msg} onResetPolicy={resetPolicy} />)}

          </>
        )}
        <div ref={messagesEndRef} />
      </div>

      {/* Suggested questions — above the input, until first user message */}
      {selectedPolicy !== null && !isRunning &&
        messages.filter((m) => m.role === "user").length === 0 && (
        <div className="shrink-0 px-2 pb-1 flex flex-col items-end gap-1.5">
          {suggestedQuestions.map((q, i) => (
            <button
              key={i}
              onClick={() => sendQuestion(q)}
              disabled={!isDataReady || isRunning}
              className="max-w-[90%] px-3 py-1.5 rounded-full border border-[#e5e5e5] bg-white text-[12px] text-black leading-snug hover:bg-muted/40 transition-colors disabled:opacity-40 disabled:cursor-not-allowed text-left"
            >
              {q}
            </button>
          ))}
        </div>
      )}

      {/* Input — Figma 33:1919 gradient border prompt */}
      <div className="shrink-0 px-2 pb-2">
        <div
          className="rounded-[16px] rounded-b-[8px] p-[2px]"
          style={{
            background: "linear-gradient(168.324deg, #1868DB 7.35%, #357DE8 23.69%, #7EE2B8 61.42%, #B3EE2B 82.59%)",
          }}
        >
          <div className="bg-white rounded-[14px] rounded-b-[6px] shadow-[2px_4px_12px_0px_rgba(0,0,0,0.14)] overflow-hidden">
            <div className="px-4 pt-3 pb-1">
              <textarea
                ref={textareaRef}
                value={inputValue}
                onChange={(e) => setInputValue(e.target.value)}
                onKeyDown={handleKeyDown}
                placeholder={
                  !isDataReady ? "Load dataset to start…"
                  : selectedPolicy === null ? "Choose a policy above to start…"
                  : "What can we help with today?"
                }
                disabled={!isDataReady || isRunning || selectedPolicy === null}
                rows={1}
                className="w-full resize-none bg-transparent text-[16px] leading-6 text-foreground placeholder:text-[#737373] outline-none disabled:opacity-50 min-h-[32px] max-h-[120px]"
                style={{ fontFamily: "inherit" }}
              />
            </div>
            <div className="flex items-center justify-between px-2 pb-2">
              <button
                className="w-6 h-6 rounded flex items-center justify-center text-muted-foreground hover:text-foreground hover:bg-muted transition-colors"
                title="Attach"
              >
                <Plus className="h-4 w-4" />
              </button>
              <button
                onClick={() => sendQuestion(inputValue)}
                disabled={!canSend}
                className="w-6 h-6 rounded flex items-center justify-center transition-colors disabled:opacity-40"
                style={{
                  background: canSend ? "#262626" : "#e5e5e5",
                }}
                title="Send"
              >
                {isRunning ? (
                  <Loader2 className="h-3.5 w-3.5 text-white animate-spin" />
                ) : (
                  <ArrowUp className={`h-3.5 w-3.5 ${canSend ? "text-white" : "text-muted-foreground"}`} />
                )}
              </button>
            </div>
          </div>
        </div>
      </div>
    </div>
  )
}
