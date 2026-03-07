import { useEffect, useState } from 'react'
import {
  X,
  ArrowsOut,
  ArrowsIn,
  CaretDown,
  Path,
  Target,
  Code,
  Eye,
  GitBranch,
  CheckCircle,
  XCircle,
} from '@phosphor-icons/react'
import Markdown from 'react-markdown'
import { cn } from '@/lib/utils'
import type { MatchedNodeResponse, ReasoningStepResponse } from '@/lib/api'

// ==================== Types ====================

interface ReasonDetailSidebarProps {
  isOpen: boolean
  onClose: () => void
  documentTitle: string
  documentId: string
  confidence?: number
  matchedNodes: MatchedNodeResponse[]
}

type Tab = 'matched' | 'trace'

const MIN_WIDTH = 500
const MAX_WIDTH = window.innerWidth * 0.85
const DEFAULT_WIDTH = 620

// ==================== Sub-components ====================

function ConfidenceBadge({ value, size = 'md' }: { value: number; size?: 'sm' | 'md' }) {
  const pct = Math.round(value * 100)
  const color = pct >= 70 ? 'bg-green/15 text-green' : pct >= 40 ? 'bg-yellow/15 text-yellow' : 'bg-overlay-0/15 text-overlay-1'
  const sizeClass = size === 'sm' ? 'text-[10px] px-1.5 py-0.5' : 'text-xs px-2 py-0.5'
  return (
    <span className={cn('rounded-full font-mono font-medium', color, sizeClass)}>
      {pct}%
    </span>
  )
}

function Breadcrumb({ path }: { path: string[] }) {
  if (path.length === 0) return null
  return (
    <div className="flex items-center gap-1 text-[11px] text-overlay-0 font-mono overflow-x-auto">
      <Path size={11} className="shrink-0 text-overlay-0/60" />
      {path.map((segment, i) => (
        <span key={i} className="flex items-center gap-1 shrink-0">
          {i > 0 && <span className="text-overlay-0/40">›</span>}
          <span className={i === path.length - 1 ? 'text-lavender' : ''}>{segment}</span>
        </span>
      ))}
    </div>
  )
}

function ReasoningSteps({ steps }: { steps: ReasoningStepResponse[] }) {
  if (steps.length === 0) return null

  return (
    <div className="mt-3 rounded-lg bg-surface-0/60 border border-border/40 p-3">
      <span className="text-[11px] font-semibold uppercase tracking-wider text-overlay-0/70">
        Reasoning
      </span>
      <div className="mt-2 flex flex-col gap-3">
        {steps.map((step, i) => {
          const pct = Math.round(step.confidence * 100)
          const isLast = i === steps.length - 1
          return (
            <div key={i} className={cn(
              'rounded-md px-2.5 py-2',
              isLast ? 'bg-mauve/8 border border-mauve/15' : 'bg-surface-1/40'
            )}>
              <div className="flex items-baseline gap-1.5">
                <span className="text-[11px] text-overlay-0/40 font-mono shrink-0">{i + 1}.</span>
                <span className={cn('text-xs font-medium truncate', isLast ? 'text-mauve' : 'text-text')}>
                  {step.node_title}
                </span>
                <span className="text-[10px] font-mono text-overlay-0/50 shrink-0 ml-auto">{pct}%</span>
              </div>
              {step.decision && (
                <p className="text-[11px] text-subtext-0 leading-relaxed mt-1 ml-[18px]">
                  {step.decision}
                </p>
              )}
            </div>
          )
        })}
      </div>
    </div>
  )
}

function ContentBlock({ content }: { content: string }) {
  const [expanded, setExpanded] = useState(false)
  const [raw, setRaw] = useState(false)
  const isLong = content.length > 300

  return (
    <div>
      <div className="flex items-center justify-between mb-2">
        <span className="text-[11px] font-semibold uppercase tracking-wider text-overlay-0/70">
          Content
        </span>
        <div className="flex items-center gap-0.5 rounded-md bg-surface-1/60 p-0.5">
          <button
            onClick={() => setRaw(false)}
            className={cn(
              'flex items-center gap-1 px-2 py-0.5 rounded text-[10px] font-medium transition-colors',
              !raw ? 'bg-surface-0 text-text shadow-sm' : 'text-overlay-0 hover:text-text'
            )}
          >
            <Eye size={10} />
            Preview
          </button>
          <button
            onClick={() => setRaw(true)}
            className={cn(
              'flex items-center gap-1 px-2 py-0.5 rounded text-[10px] font-medium transition-colors',
              raw ? 'bg-surface-0 text-text shadow-sm' : 'text-overlay-0 hover:text-text'
            )}
          >
            <Code size={10} />
            Raw
          </button>
        </div>
      </div>
      <div className="rounded-md bg-base/40 border border-border/30 p-3 relative">
        {raw ? (
          <pre
            className={cn(
              'text-xs text-subtext-0 font-mono leading-relaxed whitespace-pre-wrap',
              !expanded && isLong && 'max-h-[140px] overflow-hidden'
            )}
          >
            {content}
          </pre>
        ) : (
          <div
            className={cn(
              'prose-sm prose-invert max-w-none',
              'prose-headings:text-text prose-headings:font-semibold prose-headings:mt-3 prose-headings:mb-1.5',
              'prose-p:text-subtext-0 prose-p:text-xs prose-p:leading-relaxed prose-p:my-1.5',
              'prose-strong:text-text prose-em:text-subtext-1',
              'prose-code:text-[11px] prose-code:bg-surface-1 prose-code:px-1 prose-code:py-0.5 prose-code:rounded prose-code:text-mauve',
              'prose-pre:bg-base prose-pre:border prose-pre:border-border/50 prose-pre:rounded-md prose-pre:text-[11px] prose-pre:p-3',
              'prose-li:text-xs prose-li:text-subtext-0',
              'prose-a:text-mauve prose-a:no-underline hover:prose-a:underline',
              !expanded && isLong && 'max-h-[140px] overflow-hidden'
            )}
          >
            <Markdown>{content}</Markdown>
          </div>
        )}
        {!expanded && isLong && (
          <div className="absolute inset-x-0 bottom-0 h-12 bg-linear-to-t from-base/80 to-transparent rounded-b-md" />
        )}
      </div>
      {isLong && (
        <button
          onClick={() => setExpanded(!expanded)}
          className="mt-1.5 text-[11px] text-mauve hover:text-lavender transition-colors"
        >
          {expanded ? 'Show less' : 'Show more'}
        </button>
      )}
    </div>
  )
}

function MatchedNodeCard({ node, index }: { node: MatchedNodeResponse; index: number }) {
  const [open, setOpen] = useState(index === 0)

  return (
    <div className="rounded-xl border border-border bg-surface-0/50 shadow-sm overflow-hidden">
      {/* Collapsible header */}
      <button
        onClick={() => setOpen(!open)}
        className="w-full px-4 py-3 flex items-center gap-2 text-left hover:bg-surface-0/80 transition-colors"
      >
        <CaretDown
          size={12}
          className={cn('shrink-0 text-overlay-0 transition-transform', !open && '-rotate-90')}
        />
        <span className="font-semibold text-sm text-text truncate">{node.title}</span>
        <ConfidenceBadge value={node.confidence} size="sm" />
      </button>

      {/* Collapsible body */}
      {open && (
        <div className="border-t border-border/40">
          {/* Breadcrumb */}
          <div className="px-4 pt-2 pb-1">
            <Breadcrumb path={node.path} />
          </div>

          {/* Content */}
          <div className="px-4 py-3">
            <ContentBlock content={node.content} />
          </div>

          {/* Reasoning */}
          <div className="px-4 pb-4">
            <ReasoningSteps steps={node.reasoning_trace} />
          </div>
        </div>
      )}
    </div>
  )
}

function TabButton({
  active,
  onClick,
  children,
}: {
  active: boolean
  onClick: () => void
  children: React.ReactNode
}) {
  return (
    <button
      onClick={onClick}
      className={cn(
        'relative flex items-center gap-1.5 px-4 py-2 text-xs font-medium transition-colors',
        active
          ? 'text-text'
          : 'text-overlay-0 hover:text-text hover:bg-surface-0/50'
      )}
    >
      {children}
      {active && (
        <div className="absolute bottom-0 left-0 right-0 h-[2px] bg-mauve" />
      )}
    </button>
  )
}

// ==================== Trace Path View ====================

function TracePathNodeRow({
  label,
  isRoot,
  isLeaf,
  confidence,
  decision,
}: {
  label: string
  isRoot?: boolean
  isLeaf?: boolean
  confidence?: number
  decision?: string
}) {
  const [open, setOpen] = useState(false)
  const pct = confidence != null ? Math.round(confidence * 100) : null

  return (
    <div className="flex gap-3">
      {/* Left connector */}
      <div className="flex flex-col items-center shrink-0">
        <div
          className={cn(
            'w-3 h-3 rounded-full border-2 shrink-0',
            isLeaf
              ? 'bg-green/80 border-green'
              : isRoot
              ? 'bg-mauve/80 border-mauve'
              : 'bg-surface-1 border-border'
          )}
        />
        {!isLeaf && <div className="w-px flex-1 bg-border/50 mt-0.5" />}
      </div>

      {/* Content */}
      <div className={cn('pb-3 flex-1 min-w-0', isLeaf && 'pb-0')}>
        <div className="flex items-center gap-2">
          <span
            className={cn(
              'text-xs font-medium truncate',
              isLeaf ? 'text-green' : isRoot ? 'text-mauve' : 'text-text'
            )}
          >
            {label}
          </span>
          {pct != null && (
            <span
              className={cn(
                'text-[10px] font-mono px-1.5 py-0.5 rounded-full shrink-0',
                pct >= 70
                  ? 'bg-green/15 text-green'
                  : pct >= 40
                  ? 'bg-yellow/15 text-yellow'
                  : 'bg-overlay-0/15 text-overlay-1'
              )}
            >
              {pct}%
            </span>
          )}
          {isRoot && (
            <span className="text-[10px] text-overlay-0/50 font-mono shrink-0">root</span>
          )}
          {isLeaf && (
            <span className="text-[10px] text-green/60 font-mono shrink-0">leaf</span>
          )}
          {decision && (
            <button
              onClick={() => setOpen(!open)}
              className="text-[10px] text-overlay-0 hover:text-mauve ml-auto shrink-0"
            >
              {open ? 'hide' : 'why'}
            </button>
          )}
        </div>
        {open && decision && (
          <p className="text-[11px] text-subtext-0 leading-relaxed mt-1">{decision}</p>
        )}
      </div>
    </div>
  )
}

function NodeTracePath({ node, index }: { node: MatchedNodeResponse; index: number }) {
  const [open, setOpen] = useState(index === 0)
  const steps = node.reasoning_trace ?? []

  // Prefer explicit path titles; fall back to reasoning_trace node titles
  const pathSegments: Array<{ label: string; confidence?: number; decision?: string }> =
    node.path.length > 0
      ? node.path.map((segment, i) => ({
          label: segment,
          confidence: steps[i]?.confidence ?? (i === node.path.length - 1 ? node.confidence : undefined),
          decision: steps[i]?.decision,
        }))
      : steps.length > 0
      ? steps.map((step) => ({
          label: step.node_title,
          confidence: step.confidence,
          decision: step.decision,
        }))
      : []

  const hopLabel =
    pathSegments.length > 0
      ? `${pathSegments.length} step${pathSegments.length !== 1 ? 's' : ''}`
      : 'direct match'

  return (
    <div className="rounded-xl border border-border bg-surface-0/50 overflow-hidden">
      <button
        onClick={() => setOpen(!open)}
        className="w-full px-4 py-3 flex items-center gap-2 text-left hover:bg-surface-0/80 transition-colors"
      >
        <CaretDown
          size={12}
          className={cn('shrink-0 text-overlay-0 transition-transform', !open && '-rotate-90')}
        />
        <span className="font-semibold text-sm text-text truncate flex-1">{node.title}</span>
        <span className="text-[10px] font-mono text-overlay-0 shrink-0">{hopLabel}</span>
        <ConfidenceBadge value={node.confidence} size="sm" />
      </button>

      {open && (
        <div className="border-t border-border/40 px-4 py-3">
          {pathSegments.length === 0 ? (
            <div className="flex flex-col gap-2">
              <div className="flex items-start gap-2 rounded-lg bg-surface-1/50 border border-border/40 px-3 py-2.5">
                <CheckCircle size={14} className="text-green shrink-0 mt-0.5" />
                <div>
                  <p className="text-xs font-medium text-text">Found via direct text match</p>
                  <p className="text-[11px] text-subtext-0 leading-relaxed mt-0.5">
                    This node was identified directly by BM25 full-text search. No tree traversal was needed, so there are no intermediate reasoning steps.
                  </p>
                </div>
              </div>
            </div>
          ) : (
            <div className="flex flex-col">
              {pathSegments.map((seg, i) => (
                <TracePathNodeRow
                  key={i}
                  label={seg.label}
                  isRoot={i === 0}
                  isLeaf={i === pathSegments.length - 1}
                  confidence={seg.confidence}
                  decision={seg.decision}
                />
              ))}
            </div>
          )}

          {/* Verdict */}
          <div className={cn(
            'mt-2 flex items-center gap-2 px-3 py-2 rounded-lg text-xs font-medium',
            node.confidence >= 0.7
              ? 'bg-green/10 text-green border border-green/20'
              : 'bg-yellow/10 text-yellow border border-yellow/20'
          )}>
            {node.confidence >= 0.7
              ? <CheckCircle size={13} />
              : <XCircle size={13} />}
            Confidence: {Math.round(node.confidence * 100)}%
          </div>
        </div>
      )}
    </div>
  )
}

function TracePathView({ matchedNodes }: { matchedNodes: MatchedNodeResponse[] }) {
  if (matchedNodes.length === 0) {
    return (
      <div className="flex items-center justify-center h-32 text-overlay-0 text-sm">
        No matched nodes
      </div>
    )
  }

  return (
    <div className="p-4 flex flex-col gap-4">
      <p className="text-[11px] font-semibold uppercase tracking-wider text-overlay-0 mb-1">
        Agent Reasoning — {matchedNodes.length} matched node{matchedNodes.length !== 1 ? 's' : ''}
      </p>
      {matchedNodes.map((node, i) => (
        <NodeTracePath key={node.node_id || i} node={node} index={i} />
      ))}
    </div>
  )
}

// ==================== Main Component ====================

export function ReasonDetailSidebar({
  isOpen,
  onClose,
  documentTitle,
  documentId,
  confidence,
  matchedNodes,
}: ReasonDetailSidebarProps) {
  const [activeTab, setActiveTab] = useState<Tab>('matched')
  const [isExpanded, setIsExpanded] = useState(false)
  const [isVisible, setIsVisible] = useState(false)
  const [width, setWidth] = useState(DEFAULT_WIDTH)
  const [isDragging, setIsDragging] = useState(false)


  // Reset to matched tab when opened with new data
  useEffect(() => {
    if (isOpen) {
      setActiveTab('matched')
    }
  }, [isOpen, documentId])


  // Open/close animation
  useEffect(() => {
    if (isOpen) {
      requestAnimationFrame(() => setIsVisible(true))
    } else {
      setIsVisible(false)
    }
  }, [isOpen])

  // Drag resize
  useEffect(() => {
    if (!isDragging) return

    const handleMouseMove = (e: MouseEvent) => {
      const newWidth = window.innerWidth - e.clientX
      setWidth(Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, newWidth)))
    }
    const handleMouseUp = () => setIsDragging(false)

    document.addEventListener('mousemove', handleMouseMove)
    document.addEventListener('mouseup', handleMouseUp)
    document.body.style.cursor = 'col-resize'
    document.body.style.userSelect = 'none'

    return () => {
      document.removeEventListener('mousemove', handleMouseMove)
      document.removeEventListener('mouseup', handleMouseUp)
      document.body.style.cursor = ''
      document.body.style.userSelect = ''
    }
  }, [isDragging])

  // Escape to close
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && isOpen) onClose()
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [isOpen, onClose])

  if (!isOpen && !isVisible) return null

  return (
    <>
      {/* Backdrop */}
      <div
        className={cn(
          'fixed inset-0 bg-black/30 z-40 transition-opacity duration-300',
          isVisible ? 'opacity-100' : 'opacity-0 pointer-events-none'
        )}
        onClick={isExpanded ? () => setIsExpanded(false) : onClose}
      />

      {/* Sidebar */}
      <div
        className={cn(
          'fixed inset-y-0 right-0 flex flex-col bg-mantle border-l border-border shadow-2xl z-50',
          'transition-transform duration-300 ease-out',
          isVisible ? 'translate-x-0' : 'translate-x-full'
        )}
        style={{ width: isExpanded ? '85vw' : width }}
      >
        {/* Drag handle */}
        <div
          onMouseDown={(e) => { e.preventDefault(); setIsDragging(true) }}
          className={cn(
            'absolute left-0 top-0 bottom-0 w-1 cursor-col-resize z-10',
            'hover:bg-mauve/50 transition-colors',
            isDragging && 'bg-mauve'
          )}
        />

        {/* Header */}
        <div className="flex items-center justify-between px-4 py-3 border-b border-border bg-surface-0/30">
          <div className="flex items-center gap-2.5 min-w-0 flex-1">
            <div className="min-w-0 flex-1">
              <h3 className="text-sm font-semibold text-text truncate">{documentTitle}</h3>
              <span className="text-[11px] text-overlay-0">
                {matchedNodes.length} matched node{matchedNodes.length !== 1 ? 's' : ''} found
              </span>
            </div>
            {confidence != null && <ConfidenceBadge value={confidence} />}
          </div>
          <div className="flex items-center gap-1 ml-2">
            <button
              onClick={() => setIsExpanded(!isExpanded)}
              className="p-1.5 rounded transition-colors hover:bg-surface-1 text-overlay-1 hover:text-text"
              title={isExpanded ? 'Collapse' : 'Expand'}
            >
              {isExpanded ? <ArrowsIn size={16} /> : <ArrowsOut size={16} />}
            </button>
            <button
              onClick={onClose}
              className="p-1.5 rounded transition-colors hover:bg-surface-1 text-overlay-1 hover:text-text"
              title="Close (Esc)"
            >
              <X size={16} weight="bold" />
            </button>
          </div>
        </div>

        {/* Tabs */}
        <div className="flex border-b border-border bg-mantle">
          <TabButton active={activeTab === 'matched'} onClick={() => setActiveTab('matched')}>
            <Target size={14} />
            Matched Nodes
            <span className="ml-0.5 text-[10px] px-1.5 py-0.5 rounded-full bg-surface-1 text-overlay-0">
              {matchedNodes.length}
            </span>
          </TabButton>
          <TabButton active={activeTab === 'trace'} onClick={() => setActiveTab('trace')}>
            <GitBranch size={14} />
            Trace Path
          </TabButton>
        </div>

        {/* Content */}
        <div className="flex-1 min-h-0 overflow-auto">
          {activeTab === 'matched' && (
            <div className="p-4 flex flex-col gap-4">
              {matchedNodes.length === 0 ? (
                <div className="flex items-center justify-center h-32 text-overlay-0 text-sm">
                  No matched nodes
                </div>
              ) : (
                matchedNodes.map((node, i) => (
                  <MatchedNodeCard key={node.node_id || i} node={node} index={i} />
                ))
              )}
            </div>
          )}

          {activeTab === 'trace' && (
            <TracePathView matchedNodes={matchedNodes} />
          )}

        </div>
      </div>
    </>
  )
}
