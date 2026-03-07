"use client"
/**
 * TraceViewer — Chrome DevTools Performance-style REASON pipeline trace.
 * Ported from the ReasonDB desktop client, adapted to use lucide-react
 * and standard Tailwind colour classes.
 */

import { useState } from "react"
import {
  ChevronDown,
  ChevronRight,
  CheckCircle2,
  XCircle,
  Clock,
  Zap,
  GitBranch,
  AlertTriangle,
  ArrowDown,
} from "lucide-react"
import { cn } from "@/lib/utils"
import type {
  QueryTrace,
  BeamDocumentTrace,
  LeafVerificationTrace,
  SubQueryTrace,
  Bm25HitTrace,
  TreeGrepScoreTrace,
  DocumentRankingTrace,
} from "@/lib/api"

// ==================== Constants ====================

const PHASE_COLORS: Record<number, { border: string; text: string; bar: string }> = {
  0: { border: "border-l-gray-400",   text: "text-gray-500",   bar: "bg-gray-400"   },
  1: { border: "border-l-purple-400", text: "text-purple-600", bar: "bg-purple-400" },
  2: { border: "border-l-blue-400",   text: "text-blue-600",   bar: "bg-blue-400"   },
  3: { border: "border-l-orange-400", text: "text-orange-500", bar: "bg-orange-400" },
  4: { border: "border-l-green-500",  text: "text-green-600",  bar: "bg-green-500"  },
}

// ==================== Utilities ====================

function formatMs(ms: number): string {
  if (ms < 1000) return `${ms}ms`
  return `${(ms / 1000).toFixed(2)}s`
}

function confPct(v: number): string {
  return `${Math.round(v * 100)}%`
}

// ==================== Shared primitives ====================

function CountBar({ count, max, colorClass }: { count: number; max: number; colorClass: string }) {
  const w = max > 0 ? Math.max((count / max) * 100, count > 0 ? 3 : 0) : 0
  return (
    <div className="flex-1 h-1.5 bg-gray-100 rounded-full overflow-hidden mx-3">
      <div
        className={cn("h-full rounded-full transition-all opacity-50", colorClass)}
        style={{ width: `${w}%` }}
      />
    </div>
  )
}

function Chip({ value, fmt = "pct" }: { value: number; fmt?: "pct" | "score" }) {
  const pct = value * 100
  const color = pct >= 70 ? "text-green-600" : pct >= 40 ? "text-yellow-600" : "text-gray-400"
  return (
    <span className={cn("font-mono text-[10px] tabular-nums", color)}>
      {fmt === "pct" ? confPct(value) : value.toFixed(2)}
    </span>
  )
}

function GuideLines({ depth }: { depth: number }) {
  return (
    <>
      {Array.from({ length: depth }, (_, i) => (
        <div
          key={i}
          className="absolute top-0 bottom-0 w-px bg-gray-200"
          style={{ left: `${16 + i * 20}px` }}
        />
      ))}
    </>
  )
}

// ==================== Phase row ====================

interface PhaseRowProps {
  phase: number
  label: string
  count: number
  maxCount: number
  stat: string
  skipped?: boolean
  children?: React.ReactNode
  defaultOpen?: boolean
}

function PhaseRow({ phase, label, count, maxCount, stat, skipped, children, defaultOpen = false }: PhaseRowProps) {
  const [open, setOpen] = useState(defaultOpen)
  const col = PHASE_COLORS[phase]
  const hasChildren = Boolean(children) && !skipped

  return (
    <div>
      <button
        onClick={() => hasChildren && setOpen(!open)}
        className={cn(
          "w-full flex items-center gap-0 border-l-[3px] pl-3 pr-4 h-9",
          "bg-gray-50 hover:bg-gray-100/60 transition-colors text-left",
          "border-b border-gray-100",
          col.border,
          !hasChildren && "cursor-default",
          skipped && "opacity-40"
        )}
      >
        <span className="w-4 shrink-0 text-gray-400">
          {hasChildren ? (
            open ? <ChevronDown size={10} strokeWidth={2.5} /> : <ChevronRight size={10} strokeWidth={2.5} />
          ) : (
            <span className="w-4" />
          )}
        </span>
        <span className={cn("text-[10px] font-mono w-4 shrink-0", col.text)}>{phase}</span>
        <span className="text-xs font-medium text-gray-800 ml-2 w-36 shrink-0 truncate">{label}</span>
        <CountBar count={count} max={maxCount} colorClass={col.bar} />
        <span className="text-[11px] font-mono tabular-nums text-gray-400 shrink-0 w-40 text-right">
          {skipped ? <span className="italic text-[10px]">skipped</span> : stat}
        </span>
      </button>

      {open && hasChildren && (
        <div className="relative bg-white/60">
          <div className={cn("absolute top-0 bottom-0 w-[3px] opacity-30", col.bar)} />
          {children}
        </div>
      )}
    </div>
  )
}

// ==================== Sub-item rows ====================

function SubRow({
  depth = 1,
  left,
  right,
  sub,
  extra,
  faint,
}: {
  depth?: number
  left: React.ReactNode
  right?: React.ReactNode
  sub?: React.ReactNode
  extra?: React.ReactNode
  faint?: boolean
}) {
  return (
    <div
      className={cn(
        "relative flex items-start gap-2 px-4 py-1.5 border-b border-gray-100",
        "hover:bg-gray-50/40 transition-colors",
        faint && "opacity-50"
      )}
      style={{ paddingLeft: `${16 + depth * 20}px` }}
    >
      <GuideLines depth={depth} />
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-2">
          <span className="text-xs text-gray-800 truncate flex-1">{left}</span>
          {right && (
            <span className="text-[11px] font-mono tabular-nums text-gray-400 shrink-0">{right}</span>
          )}
        </div>
        {sub && <div className="text-[11px] text-gray-500 mt-0.5 leading-relaxed">{sub}</div>}
        {extra}
      </div>
    </div>
  )
}

// ==================== Decomposition children ====================

function DecompChildren({ subQueries }: { subQueries: SubQueryTrace[] }) {
  return (
    <>
      {subQueries.map((sq, i) => (
        <SubRow
          key={i}
          depth={1}
          left={
            <span className="flex items-center gap-2">
              <span className="text-[10px] font-mono text-gray-400 shrink-0">
                {String(i + 1).padStart(2, "0")}
              </span>
              <span className="font-medium">{sq.text}</span>
            </span>
          }
          right={`${sq.bm25_hits} hits`}
          sub={sq.rationale}
        />
      ))}
    </>
  )
}

// ==================== BM25 children ====================

function Bm25Children({ hits }: { hits: Bm25HitTrace[] }) {
  return (
    <>
      {hits.map((h, i) => (
        <SubRow
          key={i}
          depth={1}
          left={h.document_title}
          right={
            <span className="flex gap-3">
              <span>{h.matched_node_count} nodes</span>
              <span className="text-gray-400">score {h.score.toFixed(3)}</span>
            </span>
          }
        />
      ))}
    </>
  )
}

// ==================== Structural filter children ====================

function StructuralChildren({ scores }: { scores: TreeGrepScoreTrace[] }) {
  return (
    <>
      {scores.map((s, i) => (
        <SubRow
          key={i}
          depth={1}
          left={s.document_title}
          right={`score ${s.combined_score.toFixed(3)}`}
          sub={
            s.matched_sections.length > 0 ? (
              <span className="text-gray-400/60">
                Matched: {s.matched_sections.slice(0, 4).join(" · ")}
                {s.matched_sections.length > 4 && ` +${s.matched_sections.length - 4} more`}
              </span>
            ) : undefined
          }
        />
      ))}
    </>
  )
}

// ==================== LLM ranking children ====================

function RankingChildren({ rankings }: { rankings: DocumentRankingTrace[] }) {
  return (
    <>
      {rankings.map((r, i) => (
        <SubRow
          key={i}
          depth={1}
          left={r.document_title}
          right={<Chip value={r.relevance} fmt="pct" />}
          sub={r.reasoning || undefined}
        />
      ))}
    </>
  )
}

// ==================== Beam reasoning children ====================

function LeafRow({ leaf, depth }: { leaf: LeafVerificationTrace; depth: number }) {
  const [open, setOpen] = useState(false)

  return (
    <div>
      <div
        className={cn(
          "relative flex items-center gap-2 border-b border-gray-100 h-8",
          "hover:bg-gray-50/40 transition-colors cursor-pointer",
          !leaf.is_relevant && "opacity-45"
        )}
        style={{ paddingLeft: `${16 + depth * 20}px` }}
        onClick={() => leaf.reasoning_steps.length > 0 && setOpen(!open)}
      >
        <GuideLines depth={depth} />
        {leaf.is_relevant ? (
          <CheckCircle2 size={11} className="text-green-600 shrink-0" />
        ) : (
          <XCircle size={11} className="text-gray-400 shrink-0" />
        )}
        <span className="text-xs text-gray-800 flex-1 truncate">{leaf.node_title}</span>

        {leaf.path.length > 0 && (
          <span className="hidden lg:flex items-center gap-1 text-[10px] font-mono text-gray-400 shrink-0 max-w-[220px] overflow-hidden">
            <GitBranch size={9} className="shrink-0" />
            {leaf.path.map((p, j) => (
              <span key={j} className="flex items-center gap-1 shrink-0 truncate">
                {j > 0 && <span className="opacity-40">›</span>}
                <span className={j === leaf.path.length - 1 ? "text-gray-500" : ""}>{p}</span>
              </span>
            ))}
          </span>
        )}

        <span className="text-[10px] font-mono tabular-nums text-gray-400 shrink-0 w-10 text-right pr-4">
          <Chip value={leaf.confidence} />
        </span>
        {leaf.reasoning_steps.length > 0 && (
          <span className="absolute right-2 text-gray-400">
            {open ? <ChevronDown size={9} /> : <ChevronRight size={9} />}
          </span>
        )}
      </div>

      {open && leaf.reasoning_steps.length > 0 && (
        <div className="bg-gray-50/80">
          {leaf.reasoning_steps.map((step, i) => {
            const isLast = i === leaf.reasoning_steps.length - 1
            return (
              <div
                key={i}
                className={cn(
                  "relative flex items-start gap-2 border-b border-gray-100 py-1.5 pr-4",
                  isLast && "bg-gray-50/20"
                )}
                style={{ paddingLeft: `${16 + (depth + 1) * 20}px` }}
              >
                <GuideLines depth={depth + 1} />
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-2">
                    <span className="text-[10px] font-mono text-gray-400 shrink-0">{i + 1}.</span>
                    <span className={cn("text-xs font-medium truncate", isLast ? "text-purple-600" : "text-gray-500")}>
                      {step.node_title}
                    </span>
                    <span className="text-[10px] font-mono tabular-nums text-gray-400 shrink-0 ml-auto">
                      {confPct(step.confidence)}
                    </span>
                  </div>
                  {step.decision && (
                    <p className="text-[11px] text-gray-500 leading-relaxed mt-0.5 ml-5">{step.decision}</p>
                  )}
                </div>
              </div>
            )
          })}
        </div>
      )}
    </div>
  )
}

function BeamDocRow({ doc, depth }: { doc: BeamDocumentTrace; depth: number }) {
  const [open, setOpen] = useState(false)
  const relevantCount = doc.relevant_leaves.filter((l) => l.is_relevant).length

  return (
    <div>
      <div
        className="relative flex items-center gap-2 border-b border-gray-100 h-9 cursor-pointer hover:bg-gray-50/40 transition-colors pr-4"
        style={{ paddingLeft: `${16 + depth * 20}px` }}
        onClick={() => setOpen(!open)}
      >
        <GuideLines depth={depth} />
        <span className="text-gray-400 shrink-0">
          {open ? <ChevronDown size={10} /> : <ChevronRight size={10} />}
        </span>
        <span className="text-xs font-medium text-gray-800 flex-1 truncate">{doc.document_title}</span>
        <span className="flex items-center gap-3 text-[11px] font-mono tabular-nums text-gray-400 shrink-0">
          {relevantCount > 0 && (
            <span className="text-green-600">
              {relevantCount} match{relevantCount !== 1 ? "es" : ""}
            </span>
          )}
          <span>{doc.nodes_visited} visited</span>
          <span className="text-gray-400">{doc.nodes_pruned} pruned</span>
          <span className="text-orange-500">{doc.llm_calls} agent calls</span>
        </span>
      </div>

      {open && (
        doc.relevant_leaves.length > 0 ? (
          doc.relevant_leaves.map((leaf, i) => (
            <LeafRow key={leaf.node_id || i} leaf={leaf} depth={depth + 1} />
          ))
        ) : (
          <div
            className="relative flex items-center gap-1.5 h-7 border-b border-gray-100 text-xs text-gray-400 pr-4"
            style={{ paddingLeft: `${16 + (depth + 1) * 20}px` }}
          >
            <GuideLines depth={depth + 1} />
            <AlertTriangle size={11} />
            No leaf nodes recorded
          </div>
        )
      )}
    </div>
  )
}

function BeamChildren({ trace }: { trace: QueryTrace }) {
  return (
    <>
      {trace.beam_reasoning.documents.map((doc, i) => (
        <BeamDocRow key={doc.document_id || i} doc={doc} depth={1} />
      ))}
    </>
  )
}

// ==================== Funnel summary strip ====================

function FunnelStrip({ trace }: { trace: QueryTrace }) {
  const stages = [
    { label: "Candidates", count: trace.bm25_selection.total_candidates, color: "bg-purple-400" },
    { label: "Structural", count: trace.structural_filter.filtered_count, color: "bg-blue-400" },
    { label: "Ranked",     count: trace.llm_ranking.selected_count,       color: "bg-yellow-400" },
    { label: "Beam",       count: trace.beam_reasoning.documents_processed, color: "bg-green-500" },
    { label: "Results",    count: trace.final_results.length,              color: "bg-teal-500" },
  ]
  const max = stages[0].count || 1

  return (
    <div className="flex items-center gap-px px-4 py-2 border-b border-gray-200 bg-gray-50">
      {stages.map((s, i) => {
        const w = Math.max((s.count / max) * 100, s.count > 0 ? 6 : 0)
        const next = stages[i + 1]
        const dropped = next ? s.count - next.count : 0
        return (
          <div key={i} className="flex items-center gap-px">
            <div className="flex flex-col items-start gap-0.5" title={`${s.label}: ${s.count}`}>
              <div
                className={cn("h-4 rounded-sm transition-all min-w-[4px] opacity-50", s.color)}
                style={{ width: `${Math.max(w * 0.8, 4)}px` }}
              />
              <span className="text-[9px] font-mono tabular-nums text-gray-400 whitespace-nowrap">
                {s.count}
              </span>
            </div>
            {i < stages.length - 1 && (
              <div className="flex flex-col items-center mx-1">
                <ArrowDown size={8} className="text-gray-300" />
                {dropped > 0 && (
                  <span className="text-[8px] font-mono text-gray-400/60 whitespace-nowrap">
                    -{dropped}
                  </span>
                )}
              </div>
            )}
          </div>
        )
      })}
      <div className="flex-1" />
      <span className="text-[10px] font-mono text-gray-400 ml-2">
        {stages[0].count} → {stages[stages.length - 1].count} docs
      </span>
    </div>
  )
}

// ==================== Main component ====================

interface TraceViewerProps {
  trace: QueryTrace
}

export function TraceViewer({ trace }: TraceViewerProps) {
  const subQueries = trace.decomposition?.sub_queries ?? []
  const totalLlmCalls = trace.beam_reasoning.total_llm_calls + (trace.llm_ranking.skipped_llm ? 0 : 1)
  const maxCount = trace.bm25_selection.total_candidates || 1

  const phases = [
    {
      phase: 0,
      label: "Decomposition",
      count: subQueries.length,
      stat:
        subQueries.length === 0
          ? "0 sub-queries"
          : `${subQueries.length} sub-quer${subQueries.length === 1 ? "y" : "ies"}`,
      skipped: subQueries.length === 0,
      children: subQueries.length > 0 ? <DecompChildren subQueries={subQueries} /> : null,
    },
    {
      phase: 1,
      label: "BM25 Selection",
      count: trace.bm25_selection.total_candidates,
      stat: `${trace.bm25_selection.total_candidates} candidates`,
      skipped: false,
      children:
        trace.bm25_selection.hits.length > 0 ? <Bm25Children hits={trace.bm25_selection.hits} /> : null,
    },
    {
      phase: 2,
      label: "Structural Filter",
      count: trace.structural_filter.filtered_count,
      stat: `${trace.structural_filter.filtered_count} passed`,
      skipped: false,
      children:
        trace.structural_filter.scores.length > 0
          ? <StructuralChildren scores={trace.structural_filter.scores} />
          : null,
    },
    {
      phase: 3,
      label: "Agent Ranking",
      count: trace.llm_ranking.selected_count,
      stat: trace.llm_ranking.skipped_llm
        ? "skipped"
        : `${trace.llm_ranking.selected_count} / ${trace.llm_ranking.input_count} selected`,
      skipped: trace.llm_ranking.skipped_llm,
      children:
        !trace.llm_ranking.skipped_llm && trace.llm_ranking.rankings.length > 0
          ? <RankingChildren rankings={trace.llm_ranking.rankings} />
          : null,
    },
    {
      phase: 4,
      label: "Beam Reasoning",
      count: trace.beam_reasoning.documents_processed,
      stat: `${trace.beam_reasoning.documents_processed} docs · ${trace.beam_reasoning.total_llm_calls} agent calls`,
      skipped: false,
      children:
        trace.beam_reasoning.documents.length > 0 ? <BeamChildren trace={trace} /> : null,
    },
  ]

  return (
    <div className="flex flex-col h-full font-sans border rounded-lg overflow-hidden">
      {/* Summary header */}
      <div className="flex items-center gap-5 px-4 py-2.5 border-b border-gray-200 bg-gray-50 shrink-0">
        <div className="flex items-center gap-1.5 text-xs text-gray-500">
          <Clock size={12} className="text-gray-400" />
          <span className="font-mono tabular-nums">{formatMs(trace.duration_ms)}</span>
          <span className="text-gray-400">total</span>
        </div>
        <div className="h-3 w-px bg-gray-200" />
        <div className="flex items-center gap-1.5 text-xs text-gray-500">
          <Zap size={12} className="text-gray-400" />
          <span className="font-mono tabular-nums">{totalLlmCalls}</span>
          <span className="text-gray-400">agent calls</span>
        </div>
        <div className="h-3 w-px bg-gray-200" />
        <div className="flex items-center gap-1.5 text-xs text-gray-500">
          <CheckCircle2 size={12} className="text-gray-400" />
          <span className="font-mono tabular-nums">{trace.final_results.length}</span>
          <span className="text-gray-400">results</span>
        </div>
        <div className="flex-1 min-w-0" />
        <span className="text-[10px] font-mono text-gray-400/60 truncate max-w-[260px]" title={trace.query}>
          {trace.query}
        </span>
      </div>

      {/* Funnel strip */}
      <FunnelStrip trace={trace} />

      {/* Column headers */}
      <div className="flex items-center gap-0 border-b border-gray-200 bg-gray-100 shrink-0 pl-[3px]">
        <div className="pl-3 flex items-center gap-0 flex-1">
          <span className="w-4 shrink-0" />
          <span className="text-[10px] font-mono uppercase tracking-wider text-gray-400 ml-2 w-4 shrink-0">#</span>
          <span className="text-[10px] font-mono uppercase tracking-wider text-gray-400 ml-2">Phase</span>
          <div className="flex-1 mx-3" />
          <span className="text-[10px] font-mono uppercase tracking-wider text-gray-400 w-40 text-right pr-4">Details</span>
        </div>
      </div>

      {/* Phase rows */}
      <div className="flex-1 min-h-0 overflow-auto bg-white">
        {phases.map((p) => (
          <PhaseRow
            key={p.phase}
            phase={p.phase}
            label={p.label}
            count={p.count}
            maxCount={maxCount}
            stat={p.stat}
            skipped={p.skipped}
            defaultOpen={false}
          >
            {p.children}
          </PhaseRow>
        ))}
      </div>
    </div>
  )
}
