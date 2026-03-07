"use client"
import { useState, useRef, useEffect, useCallback } from "react"
import ReactMarkdown from "react-markdown"
import { Sparkles, Loader2, AlertCircle, ChevronsUpDown, X, FileText } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Badge } from "@/components/ui/badge"
import type { QueryResult, MatchedNode } from "@/lib/api"

interface OpenRouterModel {
  id: string
  name: string
  context_length: number
  pricing: { prompt: number; completion: number }
}

interface Props {
  result: QueryResult | null
  /** When false, skip auto-generating the AI answer (source cards are still shown).
   *  Defaults to true. Set to false when the chat panel already handles answer streaming. */
  autoGenerate?: boolean
}

function ConfidenceBar({ value }: { value: number }) {
  const pct = Math.round(value * 100)
  const color = pct >= 80 ? "bg-emerald-500" : pct >= 60 ? "bg-amber-500" : "bg-rose-500"
  return (
    <div className="flex items-center gap-1.5">
      <div className="h-1 w-14 rounded-full bg-muted overflow-hidden">
        <div className={`h-full rounded-full ${color}`} style={{ width: `${pct}%` }} />
      </div>
      <span className="text-[10px] tabular-nums text-muted-foreground">{pct}%</span>
    </div>
  )
}

/** Derives a short document label from the node path, e.g. "Priority Protection PDS" */
function docLabel(node: MatchedNode): string {
  if (node.path?.length) {
    const root = node.path[0]
    if (root.toLowerCase().includes("income care")) return "Income Care Plus"
    if (root.toLowerCase().includes("priority protection")) {
      if (root.toLowerCase().includes("enhancement")) return "Priority Protection Enhancement"
      if (root.toLowerCase().includes("incorporated") || root.toLowerCase().includes("ibr")) return "Priority Protection IBR"
      return "Priority Protection PDS"
    }
    return root.split(" ").slice(0, 4).join(" ")
  }
  return node.title.split(" ").slice(0, 4).join(" ")
}

function SourceCard({
  node,
  index,
  highlighted,
  selected,
  onClick,
  cardRef,
}: {
  node: MatchedNode
  index: number
  highlighted: boolean
  selected: boolean
  onClick: () => void
  cardRef: (el: HTMLDivElement | null) => void
}) {
  const label = node.path?.length ? node.path[node.path.length - 1] : node.title
  const doc = docLabel(node)

  return (
    <div
      ref={cardRef}
      onClick={onClick}
      className={[
        "shrink-0 w-48 rounded-lg border p-2.5 cursor-pointer transition-all duration-200 select-none",
        highlighted
          ? "ring-2 ring-blue-500 ring-offset-1 bg-blue-50 border-blue-300"
          : selected
            ? "bg-muted/60 border-border"
            : "bg-background hover:bg-muted/40 border-border",
      ].join(" ")}
    >
      <div className="flex items-start gap-2">
        <span className="shrink-0 w-5 h-5 rounded-full bg-blue-100 text-blue-700 flex items-center justify-center text-[10px] font-bold mt-0.5">
          {index + 1}
        </span>
        <div className="min-w-0">
          <div className="flex items-center gap-1 mb-0.5">
            <FileText className="h-2.5 w-2.5 text-muted-foreground shrink-0" />
            <span className="text-[9px] text-muted-foreground truncate">{doc}</span>
          </div>
          <p className="text-[11px] font-medium leading-tight line-clamp-2 text-foreground">{label}</p>
          <div className="mt-1.5">
            <ConfidenceBar value={node.confidence} />
          </div>
        </div>
      </div>
    </div>
  )
}

function CitationBadge({
  num,
  node,
  onClick,
}: {
  num: number
  node: MatchedNode | undefined
  onClick: () => void
}) {
  const [open, setOpen] = useState(false)
  const label = node?.path?.length ? node.path[node.path.length - 1] : node?.title
  const excerpt = node?.content
    ? node.content.slice(0, 180) + (node.content.length > 180 ? "…" : "")
    : ""

  return (
    <span className="relative inline-block leading-none">
      <button
        onClick={onClick}
        onMouseEnter={() => setOpen(true)}
        onMouseLeave={() => setOpen(false)}
        className="inline-flex items-center justify-center w-4 h-4 text-[9px] font-bold rounded-full bg-blue-100 text-blue-700 border border-blue-300 cursor-pointer hover:bg-blue-200 hover:scale-110 transition-all align-super"
        title={label}
      >
        {num}
      </button>
      {open && node && (
        <div className="absolute bottom-full left-1/2 -translate-x-1/2 mb-2 z-50 w-72 rounded-lg border bg-popover text-popover-foreground shadow-lg p-3 pointer-events-none">
          <div className="flex items-start gap-2 mb-1.5">
            <span className="shrink-0 w-4 h-4 rounded-full bg-blue-100 text-blue-700 flex items-center justify-center text-[9px] font-bold mt-0.5">
              {num}
            </span>
            <p className="text-[11px] font-semibold text-foreground leading-tight">{label}</p>
          </div>
          <p className="text-[10px] text-muted-foreground leading-relaxed">{excerpt}</p>
          <div className="absolute top-full left-1/2 -translate-x-1/2 w-0 h-0 border-l-4 border-r-4 border-t-4 border-l-transparent border-r-transparent border-t-border" />
        </div>
      )}
    </span>
  )
}

export function AnswerPanel({ result, autoGenerate = true }: Props) {
  const [answer, setAnswer] = useState("")
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [highlightedIdx, setHighlightedIdx] = useState<number | null>(null)
  const [selectedSourceIdx, setSelectedSourceIdx] = useState<number | null>(null)
  const abortRef = useRef<AbortController | null>(null)
  const sourceRefs = useRef<Map<number, HTMLDivElement>>(new Map())

  const DEFAULT_MODEL = "google/gemini-2.0-flash-001"
  const [models, setModels] = useState<OpenRouterModel[]>([])
  const [selectedModel, setSelectedModel] = useState(DEFAULT_MODEL)
  const [modelsLoading, setModelsLoading] = useState(false)
  const [modelsFetched, setModelsFetched] = useState(false)

  const nodes = result?.matchedNodes
  const question = result?.question

  const fetchModels = async () => {
    if (modelsFetched || modelsLoading) return
    setModelsLoading(true)
    try {
      const res = await fetch("/api/models")
      if (res.ok) {
        const data = await res.json()
        setModels(data.models ?? [])
        setModelsFetched(true)
      }
    } catch { /* ignore */ } finally {
      setModelsLoading(false)
    }
  }

  useEffect(() => { fetchModels() }, []) // eslint-disable-line react-hooks/exhaustive-deps

  const scrollToSource = useCallback((idx: number) => {
    const el = sourceRefs.current.get(idx)
    el?.scrollIntoView({ behavior: "smooth", block: "nearest", inline: "center" })
    setHighlightedIdx(idx)
    setSelectedSourceIdx(idx)
    setTimeout(() => setHighlightedIdx(null), 1500)
  }, [])

  const generate = useCallback(async () => {
    if (!nodes?.length || !question) return
    if (abortRef.current) abortRef.current.abort()
    const ctrl = new AbortController()
    abortRef.current = ctrl
    setLoading(true)
    setAnswer("")
    setError(null)

    try {
      const res = await fetch("/api/answer", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          question,
          model: selectedModel,
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
        setAnswer((prev) => prev + decoder.decode(value, { stream: true }))
      }
    } catch (e) {
      if ((e as Error).name !== "AbortError") {
        setError(e instanceof Error ? e.message : "Generation failed")
      }
    } finally {
      setLoading(false)
    }
  }, [nodes, question, selectedModel]) // eslint-disable-line react-hooks/exhaustive-deps

  const generateFnRef = useRef(generate)
  generateFnRef.current = generate

  const prevResultKeyRef = useRef("")
  useEffect(() => {
    if (!autoGenerate) return
    const key = `${question ?? ""}|${nodes?.length ?? 0}`
    if (!nodes?.length || !question) { prevResultKeyRef.current = ""; return }
    if (key === prevResultKeyRef.current) return
    prevResultKeyRef.current = key
    setAnswer("")
    setError(null)
    setHighlightedIdx(null)
    setSelectedSourceIdx(null)
    generateFnRef.current()
  }, [result, autoGenerate]) // eslint-disable-line react-hooks/exhaustive-deps

  if (!nodes || nodes.length === 0 || !question) return null

  const avgConfidence = nodes.reduce((s, n) => s + n.confidence, 0) / nodes.length
  const selectedNode = selectedSourceIdx !== null ? nodes[selectedSourceIdx] : null

  return (
    <div className="rounded-lg border bg-gradient-to-br from-blue-50/60 to-slate-50 overflow-hidden">
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-3 border-b bg-background/60 backdrop-blur flex-wrap gap-2">
        <div className="flex items-center gap-2">
          <Sparkles className="h-4 w-4 text-blue-600" />
          <span className="text-sm font-semibold">AI Answer</span>
          <Badge variant="outline" className="text-[10px] px-1.5 py-0 h-4 border-blue-200 text-blue-700">
            via OpenRouter
          </Badge>
        </div>
        <div className="flex items-center gap-2 flex-wrap">
          <div className="relative">
            <select
              value={selectedModel}
              onChange={(e) => setSelectedModel(e.target.value)}
              disabled={loading}
              className="h-7 pl-2 pr-7 text-xs rounded-md border bg-background appearance-none cursor-pointer focus:outline-none focus:ring-1 focus:ring-blue-400 disabled:opacity-50 min-w-[180px] max-w-[260px] truncate"
            >
              {!models.find((m) => m.id === selectedModel) && (
                <option value={selectedModel}>{selectedModel}</option>
              )}
              {models.map((m) => (
                <option key={m.id} value={m.id} title={m.name}>{m.name}</option>
              ))}
            </select>
            <span className="pointer-events-none absolute right-2 top-1/2 -translate-y-1/2">
              {modelsLoading
                ? <Loader2 className="h-3 w-3 animate-spin text-muted-foreground" />
                : <ChevronsUpDown className="h-3 w-3 text-muted-foreground" />}
            </span>
          </div>
          <div className="flex items-center gap-1.5 text-[11px] text-muted-foreground">
            <span>{nodes.length} source{nodes.length !== 1 ? "s" : ""}</span>
            <span>·</span>
            <span className="flex items-center gap-1">avg <ConfidenceBar value={avgConfidence} /></span>
          </div>
          <Button
            size="sm"
            onClick={generate}
            disabled={loading}
            className="h-7 text-xs gap-1.5 bg-blue-600 hover:bg-blue-700 text-white"
          >
            {loading ? (
              <><Loader2 className="h-3 w-3 animate-spin" /> Generating…</>
            ) : answer ? (
              <><Sparkles className="h-3 w-3" /> Regenerate</>
            ) : (
              <><Sparkles className="h-3 w-3" /> Generate Answer</>
            )}
          </Button>
        </div>
      </div>

      <div className="p-4 space-y-3">
        {/* Question */}
        <div className="text-xs text-muted-foreground italic border-l-2 border-blue-300 pl-3">
          {question}
        </div>

        {/* Sources strip */}
        <div>
          <p className="text-[10px] font-semibold uppercase tracking-wide text-muted-foreground mb-2">
            Sources
          </p>
          <div
            className="flex gap-2 overflow-x-auto pb-1"
            style={{ scrollbarWidth: "thin", scrollbarColor: "rgba(37,99,235,0.3) transparent" }}
          >
            {nodes.map((node, i) => (
              <SourceCard
                key={node.node_id}
                node={node}
                index={i}
                highlighted={highlightedIdx === i}
                selected={selectedSourceIdx === i}
                onClick={() => setSelectedSourceIdx(selectedSourceIdx === i ? null : i)}
                cardRef={(el) => {
                  if (el) sourceRefs.current.set(i, el)
                  else sourceRefs.current.delete(i)
                }}
              />
            ))}
          </div>

          {/* Expanded source detail */}
          {selectedNode && (
            <div className="mt-2 rounded-lg border bg-background p-3 text-xs space-y-2 relative animate-in fade-in slide-in-from-top-1 duration-150">
              <button
                onClick={() => setSelectedSourceIdx(null)}
                className="absolute top-2.5 right-2.5 text-muted-foreground hover:text-foreground"
              >
                <X className="h-3.5 w-3.5" />
              </button>
              <div className="flex items-center gap-2 pr-6">
                <span className="shrink-0 w-5 h-5 rounded-full bg-blue-100 text-blue-700 flex items-center justify-center text-[10px] font-bold">
                  {selectedSourceIdx! + 1}
                </span>
                <p className="font-semibold text-foreground">{selectedNode.title}</p>
              </div>
              {selectedNode.path?.length > 0 && (
                <p className="text-[10px] text-muted-foreground pl-7">{selectedNode.path.join(" › ")}</p>
              )}
              <p className="text-muted-foreground leading-relaxed pl-7">
                {selectedNode.content.length > 500
                  ? selectedNode.content.slice(0, 500) + "…"
                  : selectedNode.content}
              </p>
              {selectedNode.reasoning_trace && selectedNode.reasoning_trace.length > 0 && (
                <div className="border-t pt-2 space-y-1 pl-7">
                  <p className="text-[10px] font-semibold uppercase tracking-wide text-muted-foreground">
                    Reasoning trace
                  </p>
                  {selectedNode.reasoning_trace.map((step, i) => (
                    <div key={i} className="flex items-start gap-2 text-[11px]">
                      <span className="shrink-0 text-muted-foreground">{i + 1}.</span>
                      <span className="flex-1 text-muted-foreground">
                        {step.node_title} — {step.decision}
                      </span>
                      <span className="shrink-0 tabular-nums text-muted-foreground">
                        {(step.confidence * 100).toFixed(0)}%
                      </span>
                    </div>
                  ))}
                </div>
              )}
            </div>
          )}
        </div>

        {/* Error */}
        {error && (
          <div className="flex items-start gap-2 rounded-md border border-destructive/40 bg-destructive/5 p-3 text-xs text-destructive">
            <AlertCircle className="h-3.5 w-3.5 mt-0.5 shrink-0" />
            <div>
              <p className="font-medium">Generation failed</p>
              <p className="mt-0.5 text-destructive/80">{error}</p>
              {error.includes("OPENROUTER_API_KEY") && (
                <p className="mt-1 text-destructive/60">
                  Add <code className="bg-destructive/10 px-1 rounded">OPENROUTER_API_KEY=sk-or-...</code> to{" "}
                  <code className="bg-destructive/10 px-1 rounded">.env.local</code> and restart.
                </p>
              )}
            </div>
          </div>
        )}

        {/* Streaming answer */}
        {(answer || loading) && (
          <div className="rounded-md border bg-background p-4">
            <div className="prose prose-sm prose-slate max-w-none
              [&_p]:text-sm [&_p]:leading-relaxed [&_p]:mb-3 [&_p:last-child]:mb-0
              [&_ul]:text-sm [&_ul]:my-2 [&_ul]:pl-4 [&_ul>li]:mb-1
              [&_ol]:text-sm [&_ol]:my-2 [&_ol]:pl-4 [&_ol>li]:mb-1
              [&_strong]:font-semibold [&_strong]:text-foreground
              [&_h1]:text-base [&_h1]:font-bold [&_h1]:mb-2 [&_h1]:mt-4
              [&_h2]:text-sm [&_h2]:font-bold [&_h2]:mb-1.5 [&_h2]:mt-3
              [&_h3]:text-sm [&_h3]:font-semibold [&_h3]:mb-1 [&_h3]:mt-2
              [&_code]:text-xs [&_code]:bg-muted [&_code]:px-1 [&_code]:rounded
              [&_blockquote]:border-l-2 [&_blockquote]:border-blue-300 [&_blockquote]:pl-3 [&_blockquote]:italic [&_blockquote]:text-muted-foreground">
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
                            return (
                              <CitationBadge
                                key={i}
                                num={num}
                                node={nodes[num - 1]}
                                onClick={() => scrollToSource(num - 1)}
                              />
                            )
                          }
                          return <span key={i}>{part}</span>
                        })}
                      </>
                    )
                  },
                }}
              >{answer}</ReactMarkdown>
            </div>
            {loading && (
              <span className="inline-block w-1.5 h-4 bg-blue-500 animate-pulse rounded-sm align-text-bottom mt-1" />
            )}
          </div>
        )}
      </div>
    </div>
  )
}
