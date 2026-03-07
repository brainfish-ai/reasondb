"use client"
import { useState, useMemo } from "react"
import { ChevronRight, ChevronDown, FileText, FolderOpen, Folder, CheckCircle2, AlertCircle } from "lucide-react"
import type { MatchedNode } from "@/lib/api"

// ─── Tree building ────────────────────────────────────────────────────────────

interface TreeNode {
  id: string
  label: string
  depth: number
  children: TreeNode[]
  match?: MatchedNode // set only on leaf matched nodes
}

function buildTree(matchedNodes: MatchedNode[]): TreeNode[] {
  const root: TreeNode = { id: "__root__", label: "", depth: -1, children: [] }

  for (const node of matchedNodes) {
    const segments = node.path.length > 0 ? node.path : [node.title]
    let cursor = root

    segments.forEach((seg, i) => {
      const id = segments.slice(0, i + 1).join(" / ")
      let child = cursor.children.find((c) => c.label === seg)
      if (!child) {
        child = { id, label: seg, depth: i, children: [] }
        cursor.children.push(child)
      }
      // Attach match data to the leaf (last segment)
      if (i === segments.length - 1) {
        child.match = node
      }
      cursor = child
    })
  }

  return root.children
}

// ─── Confidence helpers ───────────────────────────────────────────────────────

function confidenceColor(c: number): string {
  if (c >= 0.85) return "text-emerald-600"
  if (c >= 0.65) return "text-amber-600"
  return "text-rose-500"
}

function confidenceBg(c: number): string {
  if (c >= 0.85) return "bg-emerald-50 border-emerald-200 text-emerald-700"
  if (c >= 0.65) return "bg-amber-50 border-amber-200 text-amber-700"
  return "bg-rose-50 border-rose-200 text-rose-600"
}

function confidenceBar(c: number): string {
  if (c >= 0.85) return "bg-emerald-400"
  if (c >= 0.65) return "bg-amber-400"
  return "bg-rose-400"
}

// ─── Subcomponents ────────────────────────────────────────────────────────────

function ConfidencePill({ confidence }: { confidence: number }) {
  return (
    <span className={`inline-flex items-center gap-0.5 px-1.5 py-0.5 rounded-full border text-[10px] font-semibold tabular-nums ${confidenceBg(confidence)}`}>
      {Math.round(confidence * 100)}%
    </span>
  )
}

function ConfidenceBar({ confidence }: { confidence: number }) {
  return (
    <div className="w-full h-1 rounded-full bg-gray-100 mt-1.5">
      <div
        className={`h-1 rounded-full transition-all ${confidenceBar(confidence)}`}
        style={{ width: `${Math.round(confidence * 100)}%` }}
      />
    </div>
  )
}

function ReasoningTrace({ steps }: { steps: Array<{ node_title: string; decision: string; confidence: number }> }) {
  return (
    <div className="mt-2 space-y-1">
      <p className="text-[10px] font-semibold uppercase tracking-wide text-muted-foreground">Reasoning trace</p>
      <div className="space-y-0.5">
        {steps.map((step, i) => (
          <div key={i} className="flex items-start gap-1.5 text-[11px]">
            <span className={`shrink-0 mt-0.5 ${confidenceColor(step.confidence)}`}>
              {step.confidence >= 0.5 ? (
                <CheckCircle2 className="h-3 w-3" />
              ) : (
                <AlertCircle className="h-3 w-3" />
              )}
            </span>
            <span className="text-muted-foreground leading-relaxed">
              <span className="font-medium text-foreground">{step.node_title}</span>
              {" — "}
              {step.decision}
              {" "}
              <span className={`tabular-nums text-[10px] ${confidenceColor(step.confidence)}`}>
                ({Math.round(step.confidence * 100)}%)
              </span>
            </span>
          </div>
        ))}
      </div>
    </div>
  )
}

interface TreeNodeRowProps {
  node: TreeNode
  selectedId: string | null
  onSelect: (id: string | null) => void
  defaultExpanded?: boolean
}

function TreeNodeRow({ node, selectedId, onSelect, defaultExpanded = false }: TreeNodeRowProps) {
  const [expanded, setExpanded] = useState(defaultExpanded || node.match != null)
  const isSelected = selectedId === node.id
  const isMatch = node.match != null
  const hasChildren = node.children.length > 0

  const isDocument = node.depth === 0

  const toggle = () => {
    if (hasChildren) setExpanded((v) => !v)
    if (isMatch) onSelect(isSelected ? null : node.id)
  }

  return (
    <div>
      {/* Row */}
      <button
        onClick={toggle}
        className={`w-full flex items-center gap-1.5 px-2 py-1.5 rounded-md text-left transition-colors group
          ${isSelected ? "bg-blue-50 border border-blue-200" : "hover:bg-muted/50"}
          ${isMatch && !isSelected ? "border border-transparent" : ""}
        `}
        style={{ paddingLeft: `${node.depth * 16 + 8}px` }}
      >
        {/* Expand / leaf icon */}
        <span className="shrink-0 text-muted-foreground">
          {hasChildren ? (
            expanded
              ? <ChevronDown className="h-3.5 w-3.5" />
              : <ChevronRight className="h-3.5 w-3.5" />
          ) : (
            <span className="w-3.5 h-3.5 inline-block" />
          )}
        </span>

        {/* Folder / file icon */}
        <span className={`shrink-0 ${isDocument ? "text-blue-500" : isMatch ? "text-amber-500" : "text-muted-foreground"}`}>
          {isDocument ? (
            expanded ? <FolderOpen className="h-3.5 w-3.5" /> : <Folder className="h-3.5 w-3.5" />
          ) : isMatch ? (
            <FileText className="h-3.5 w-3.5" />
          ) : (
            expanded ? <FolderOpen className="h-3.5 w-3.5" /> : <Folder className="h-3.5 w-3.5" />
          )}
        </span>

        {/* Label */}
        <span className={`flex-1 text-xs truncate ${isMatch ? "font-medium" : "text-muted-foreground"} ${isDocument ? "font-semibold text-foreground" : ""}`}>
          {node.label}
        </span>

        {/* Confidence pill on matched leaf */}
        {isMatch && node.match && (
          <ConfidencePill confidence={node.match.confidence} />
        )}
      </button>

      {/* Expanded leaf: content + reasoning */}
      {isMatch && isSelected && node.match && (
        <div
          className="mx-2 mb-2 rounded-md border bg-muted/30 p-3 space-y-2"
          style={{ marginLeft: `${node.depth * 16 + 28}px` }}
        >
          {/* Confidence bar */}
          <div>
            <div className="flex items-center justify-between text-[11px]">
              <span className="text-muted-foreground">Confidence</span>
              <span className={`font-semibold tabular-nums ${confidenceColor(node.match.confidence)}`}>
                {Math.round(node.match.confidence * 100)}%
              </span>
            </div>
            <ConfidenceBar confidence={node.match.confidence} />
          </div>

          {/* Content excerpt */}
          {node.match.content && (
            <div>
              <p className="text-[10px] font-semibold uppercase tracking-wide text-muted-foreground mb-1">Content</p>
              <p className="text-[12px] leading-relaxed text-foreground line-clamp-6">
                {node.match.content}
              </p>
            </div>
          )}

          {/* Reasoning trace */}
          {node.match.reasoning_trace && node.match.reasoning_trace.length > 0 && (
            <ReasoningTrace steps={node.match.reasoning_trace} />
          )}
        </div>
      )}

      {/* Children */}
      {hasChildren && expanded && (
        <div>
          {node.children.map((child) => (
            <TreeNodeRow
              key={child.id}
              node={child}
              selectedId={selectedId}
              onSelect={onSelect}
              defaultExpanded={child.depth < 1}
            />
          ))}
        </div>
      )}
    </div>
  )
}

// ─── Public component ─────────────────────────────────────────────────────────

interface MatchedNodesTreeProps {
  matchedNodes: MatchedNode[]
}

export function MatchedNodesTree({ matchedNodes }: MatchedNodesTreeProps) {
  const [selectedId, setSelectedId] = useState<string | null>(null)

  const tree = useMemo(() => buildTree(matchedNodes), [matchedNodes])

  // Summary stats
  const avgConfidence = matchedNodes.reduce((s, n) => s + n.confidence, 0) / matchedNodes.length
  const docCount = tree.length

  if (matchedNodes.length === 0) {
    return (
      <div className="flex items-center justify-center h-full text-sm text-muted-foreground">
        No matched nodes
      </div>
    )
  }

  return (
    <div className="flex flex-col">
      {/* Summary bar */}
      <div className="flex items-center gap-4 px-3 py-2 border-b bg-muted/20">
        <span className="text-[11px] text-muted-foreground">
          <span className="font-semibold text-foreground">{matchedNodes.length}</span> matched node{matchedNodes.length !== 1 ? "s" : ""}
        </span>
        <span className="text-[11px] text-muted-foreground">
          across <span className="font-semibold text-foreground">{docCount}</span> document{docCount !== 1 ? "s" : ""}
        </span>
        <span className={`text-[11px] font-semibold tabular-nums ${confidenceColor(avgConfidence)}`}>
          avg {Math.round(avgConfidence * 100)}% confidence
        </span>
      </div>

      {/* Tree */}
      <div className="py-2 px-1">
        {tree.map((node) => (
          <TreeNodeRow
            key={node.id}
            node={node}
            selectedId={selectedId}
            onSelect={setSelectedId}
            defaultExpanded
          />
        ))}
      </div>
    </div>
  )
}
