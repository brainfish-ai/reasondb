import { useEffect, useState, useCallback, useRef } from 'react'
import {
  X,
  ArrowsOut,
  ArrowsIn,
  CaretDown,
  TreeStructure,
  Path,
  CircleNotch,
  WarningCircle,
  Target,
  Code,
  Eye,
} from '@phosphor-icons/react'
import Markdown from 'react-markdown'
import { cn } from '@/lib/utils'
import { NodeSplitViewer, type TreeNode } from '@/components/shared/NodeSplitViewer'
import { createClient } from '@/lib/api'
import type { MatchedNodeResponse, ReasoningStepResponse } from '@/lib/api'

// ==================== Types ====================

interface ReasonDetailSidebarProps {
  isOpen: boolean
  onClose: () => void
  documentTitle: string
  documentId: string
  confidence?: number
  matchedNodes: MatchedNodeResponse[]
  connectionConfig?: {
    host: string
    port: number
    apiKey?: string
    useSsl?: boolean
  }
}

type Tab = 'matched' | 'tree'

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

// ==================== Main Component ====================

export function ReasonDetailSidebar({
  isOpen,
  onClose,
  documentTitle,
  documentId,
  confidence,
  matchedNodes,
  connectionConfig,
}: ReasonDetailSidebarProps) {
  const [activeTab, setActiveTab] = useState<Tab>('matched')
  const [isExpanded, setIsExpanded] = useState(false)
  const [isVisible, setIsVisible] = useState(false)
  const [width, setWidth] = useState(DEFAULT_WIDTH)
  const [isDragging, setIsDragging] = useState(false)

  // Document tree lazy loading
  const [treeData, setTreeData] = useState<TreeNode | null>(null)
  const [treeLoading, setTreeLoading] = useState(false)
  const [treeError, setTreeError] = useState<string | null>(null)
  const fetchedDocIdRef = useRef<string | null>(null)

  // Reset when document changes
  useEffect(() => {
    if (documentId !== fetchedDocIdRef.current) {
      setTreeData(null)
      setTreeError(null)
      setTreeLoading(false)
      fetchedDocIdRef.current = null
    }
  }, [documentId])

  // Reset to matched tab when opened with new data
  useEffect(() => {
    if (isOpen) {
      setActiveTab('matched')
    }
  }, [isOpen, documentId])

  const fetchTree = useCallback(async () => {
    if (!connectionConfig || !documentId || fetchedDocIdRef.current === documentId) return

    setTreeLoading(true)
    setTreeError(null)
    try {
      const client = createClient({
        host: connectionConfig.host,
        port: connectionConfig.port,
        apiKey: connectionConfig.apiKey,
        useSsl: connectionConfig.useSsl,
      })
      const tree = await client.getDocumentTree(documentId)
      setTreeData(tree)
      fetchedDocIdRef.current = documentId
    } catch (err) {
      setTreeError(err instanceof Error ? err.message : 'Failed to load document tree')
    } finally {
      setTreeLoading(false)
    }
  }, [documentId, connectionConfig])

  const handleTabChange = (tab: Tab) => {
    setActiveTab(tab)
    if (tab === 'tree' && fetchedDocIdRef.current !== documentId) {
      fetchTree()
    }
  }

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
          <TabButton active={activeTab === 'matched'} onClick={() => handleTabChange('matched')}>
            <Target size={14} />
            Matched Nodes
            <span className="ml-0.5 text-[10px] px-1.5 py-0.5 rounded-full bg-surface-1 text-overlay-0">
              {matchedNodes.length}
            </span>
          </TabButton>
          <TabButton active={activeTab === 'tree'} onClick={() => handleTabChange('tree')}>
            <TreeStructure size={14} />
            Document Tree
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

          {activeTab === 'tree' && (
            <div className="h-full">
              {treeLoading ? (
                <div className="flex flex-col items-center justify-center h-full gap-3">
                  <CircleNotch size={24} className="animate-spin text-mauve" />
                  <span className="text-sm text-overlay-0">Loading document tree...</span>
                </div>
              ) : treeError ? (
                <div className="flex flex-col items-center justify-center h-full gap-3 p-4 text-center">
                  <WarningCircle size={24} className="text-red" />
                  <span className="text-sm text-red">{treeError}</span>
                  <button onClick={fetchTree} className="text-xs text-mauve hover:underline">
                    Retry
                  </button>
                </div>
              ) : treeData ? (
                <NodeSplitViewer treeData={treeData} />
              ) : (
                <div className="flex items-center justify-center h-full text-overlay-0 text-sm">
                  No tree data available
                </div>
              )}
            </div>
          )}
        </div>
      </div>
    </>
  )
}
