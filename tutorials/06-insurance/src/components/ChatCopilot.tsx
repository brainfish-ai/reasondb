"use client"
import { useState, useRef, useEffect, useCallback } from "react"
import ReactMarkdown from "react-markdown"
import { ArrowUp, Plus, Clock, X, Sparkles, Loader2, Code2, FileText } from "lucide-react"
import type { QueryResult, MatchedNode } from "@/lib/api"

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

function MessageBubble({ msg }: { msg: Message }) {
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

      {/* Answering spinner */}
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
          {msg.answerLoading && (
            <span className="inline-block w-1 h-4 bg-blue-500 animate-pulse rounded-sm align-text-bottom ml-0.5" />
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

      {/* Error */}
      {msg.status === "error" && !msg.answer && (
        <div className="ml-8 text-[13px] text-destructive">
          Something went wrong. Please try again.
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
  const messagesEndRef = useRef<HTMLDivElement>(null)
  const textareaRef = useRef<HTMLTextAreaElement>(null)
  const prevResultRef = useRef<QueryResult | null>(null)
  const prevRunningRef = useRef(false)
  const answerAbortRef = useRef<AbortController | null>(null)
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
  }, [])

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
      const question = result?.question ?? ""

      if (nodes.length > 0 && question) {
        // Store matched nodes in the message, then stream the answer
        setMessages((prev) =>
          prev.map((m) =>
            m.id === pendingMsgId ? { ...m, nodes, question } : m
          )
        )
        const msgId = pendingMsgId
        setPendingMsgId(null)
        streamAnswer(msgId, question, nodes)
      } else {
        // No results
        setMessages((prev) =>
          prev.map((m) =>
            m.id === pendingMsgId
              ? { ...m, status: "error" }
              : m
          )
        )
        setPendingMsgId(null)
      }
    }

    prevRunningRef.current = isRunning
    prevResultRef.current = result
  }, [isRunning, progressMsg, result, pendingMsgId, streamAnswer])

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

      // Call /api/contextualize — falls back to raw question on any error
      let contextualQuestion = raw
      try {
        const res = await fetch("/api/contextualize", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ history, question: raw }),
        })
        if (res.ok) {
          const data = await res.json()
          contextualQuestion = (data.contextualQuestion ?? raw).trim()
        }
      } catch { /* fall back gracefully */ }

      const rqlQuery = `SELECT * FROM ${tableName} REASON '${contextualQuestion}' LIMIT 5`

      // Update assistant message: transition to "building" with RQL + contextual question
      setMessages((msgs) =>
        msgs.map((m) =>
          m.id === assistantMsgId
            ? { ...m, status: "building", rqlQuery, contextualQuestion }
            : m
        )
      )
      setPendingMsgId(assistantMsgId)
      onQuery(rqlQuery, contextualQuestion)
    },
    [tableName, isRunning, onQuery]
  )

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault()
      sendQuestion(inputValue)
    }
  }

  const canSend = inputValue.trim().length > 0 && !isRunning && isDataReady
  const isEmpty = messages.length === 0

  return (
    <div className="flex flex-col h-full border-l bg-white">
      {/* Header — Figma 33:1908 */}
      <div className="flex items-center justify-between px-3 py-2.5 border-b bg-white shrink-0">
        <div className="flex items-center gap-2">
          {/* Gradient sparkle logo */}
          <div
            className="w-6 h-6 rounded-md flex items-center justify-center shrink-0 shadow-sm"
            style={{ background: "linear-gradient(168deg, #1868DB 7%, #357DE8 24%, #7EE2B8 61%, #B3EE2B 83%)" }}
          >
            <Sparkles className="w-3.5 h-3.5 text-white drop-shadow-sm" />
          </div>
          <span className="text-[12px] font-medium text-foreground">Brainfish Assist</span>
        </div>

        {/* Control buttons — Figma 33:1913 */}
        <div className="flex items-center gap-0.5">
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

      {/* Messages area */}
      <div className="flex-1 overflow-y-auto px-3 py-4 space-y-4 min-h-0">
        {isEmpty ? (
          /* Empty state — show suggested questions (Figma 33:1899 style) */
          <div className="h-full flex flex-col justify-end gap-2 pb-2">
            {!isDataReady && (
              <p className="text-center text-xs text-muted-foreground mb-4 px-4">
                Load the insurance dataset first, then ask any policy question.
              </p>
            )}
            <div className="flex flex-col items-end gap-2">
              {suggestedQuestions.map((q, i) => (
                <button
                  key={i}
                  onClick={() => sendQuestion(q)}
                  disabled={!isDataReady || isRunning}
                  className="max-w-[90%] text-right px-3 py-2 rounded-full border border-[#e5e5e5] bg-white text-[12px] text-black leading-snug hover:bg-muted/40 transition-colors disabled:opacity-40 disabled:cursor-not-allowed text-left"
                >
                  {q}
                </button>
              ))}
            </div>
          </div>
        ) : (
          messages.map((msg) => <MessageBubble key={msg.id} msg={msg} />)
        )}
        <div ref={messagesEndRef} />
      </div>

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
                placeholder={isDataReady ? "What can we help with today?" : "Load dataset to start…"}
                disabled={!isDataReady || isRunning}
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
