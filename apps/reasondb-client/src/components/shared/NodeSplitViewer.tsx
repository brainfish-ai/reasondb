import { useState, useMemo, useRef, useCallback, useEffect, forwardRef } from 'react'
import { MagnifyingGlass, X, Article, ArticleNyTimes } from '@phosphor-icons/react'
import Editor, { type Monaco } from '@monaco-editor/react'
import type * as monacoEditor from 'monaco-editor'
import { cn } from '@/lib/utils'
import { THEME_NAME, ensureTheme } from '@/lib/monaco-theme'

// Types
export interface TreeNode {
  id: string
  title: string
  summary: string
  content?: string
  depth: number
  is_leaf: boolean
  children: TreeNode[]
}

interface TreeStats {
  totalNodes: number
  leafNodes: number
  totalChars: number
  maxDepth: number
}

interface LeafPosition {
  node: TreeNode
  index: number
  lineNumber: number
}

// Helper functions
function extractLeafNodes(node: TreeNode): TreeNode[] {
  if (!node) return []
  if (node.is_leaf) return [node]
  const children = node.children || []
  return children.flatMap(extractLeafNodes)
}

function countNodes(node: TreeNode): number {
  if (!node) return 0
  const children = node.children || []
  return 1 + children.reduce((sum, child) => sum + countNodes(child), 0)
}

function calculateStats(node: TreeNode): TreeStats {
  if (!node) {
    return { totalNodes: 0, leafNodes: 0, totalChars: 0, maxDepth: 0 }
  }
  const leaves = extractLeafNodes(node)
  return {
    totalNodes: countNodes(node),
    leafNodes: leaves.length,
    totalChars: leaves.reduce((sum, n) => sum + (n.content?.length || 0), 0),
    maxDepth: leaves.length > 0 ? Math.max(...leaves.map(n => n.depth)) : 0,
  }
}

function findLeafPositions(json: string, leafNodes: TreeNode[]): LeafPosition[] {
  const positions: LeafPosition[] = []
  const lines = json.split('\n')
  
  leafNodes.forEach((node, index) => {
    let nodeStartLine = -1
    for (let i = 0; i < lines.length; i++) {
      if (lines[i].includes(`"id": "${node.id}"`)) {
        nodeStartLine = i
        break
      }
    }
    
    if (nodeStartLine === -1) return
    
    for (let i = nodeStartLine; i < Math.min(nodeStartLine + 15, lines.length); i++) {
      if (lines[i].includes('"content":')) {
        positions.push({ node, index, lineNumber: i + 1 })
        return
      }
    }
    
    positions.push({ node, index, lineNumber: nodeStartLine + 1 })
  })
  
  return positions
}

const ACCENT_COLOR = '#60a5fa'

// Content Block Component
interface ContentBlockProps {
  node: TreeNode
  index: number
  isSelected: boolean
  isHovered: boolean
  searchQuery: string
  onSelect: () => void
  onHover: (hovered: boolean) => void
}

const ContentBlock = forwardRef<HTMLDivElement, ContentBlockProps>(
  ({ node, index, isSelected, isHovered, searchQuery, onSelect, onHover }, ref) => {
    const highlightContent = (text: string) => {
      if (!searchQuery.trim() || !text) return text
      const regex = new RegExp(`(${searchQuery.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')})`, 'gi')
      const parts = text.split(regex)
      return parts.map((part, i) =>
        regex.test(part) ? (
          <mark key={i} className="bg-yellow/20 text-yellow rounded px-0.5">
            {part}
          </mark>
        ) : (
          part
        )
      )
    }

    return (
      <div
        ref={ref}
        className={cn(
          'group relative transition-all duration-150 cursor-pointer',
          'rounded-r-lg pl-4 pr-4 py-3',
          'border-l-2',
          isSelected 
            ? 'border-l-blue bg-blue/10' 
            : isHovered 
              ? 'border-l-lavender bg-surface-0/40'
              : 'border-l-overlay-0 hover:border-l-subtext-0'
        )}
        onClick={onSelect}
        onMouseEnter={() => onHover(true)}
        onMouseLeave={() => onHover(false)}
      >
        <p className={cn(
          'text-[15px] leading-[1.7] tracking-[-0.01em]',
          'font-normal whitespace-pre-wrap transition-colors duration-150',
          isSelected 
            ? 'text-text' 
            : isHovered 
              ? 'text-text'
              : 'text-subtext-0'
        )}>
          {node.content ? (
            highlightContent(node.content)
          ) : (
            <span className="italic text-subtext-0">No content</span>
          )}
        </p>
      </div>
    )
  }
)

ContentBlock.displayName = 'ContentBlock'

// Main Component
export interface NodeSplitViewerProps {
  treeData: TreeNode
  className?: string
}

export function NodeSplitViewer({ treeData, className }: NodeSplitViewerProps) {
  const [selectedIndex, setSelectedIndex] = useState<number | null>(null)
  const [hoveredIndex, setHoveredIndex] = useState<number | null>(null)
  const [searchQuery, setSearchQuery] = useState('')
  const [leafPositions, setLeafPositions] = useState<LeafPosition[]>([])
  const [editorReady, setEditorReady] = useState(false)
  const [showPreview, setShowPreview] = useState(false)

  const contentRefs = useRef<Map<number, HTMLDivElement>>(new Map())
  const editorRef = useRef<monacoEditor.editor.IStandaloneCodeEditor | null>(null)
  const monacoRef = useRef<Monaco | null>(null)
  const leftPanelRef = useRef<HTMLDivElement>(null)
  const rightPanelRef = useRef<HTMLDivElement>(null)

  const [linePositions, setLinePositions] = useState<
    Array<{ leftY: number; rightY: number; rightX: number; index: number }>
  >([])

  const leafNodes = useMemo(() => extractLeafNodes(treeData), [treeData])

  const filteredLeafNodes = useMemo(() => {
    if (!searchQuery.trim()) return leafNodes
    const query = searchQuery.toLowerCase()
    return leafNodes.filter(
      (node) =>
        node.title.toLowerCase().includes(query) ||
        node.content?.toLowerCase().includes(query) ||
        node.summary.toLowerCase().includes(query)
    )
  }, [leafNodes, searchQuery])

  const stats = useMemo(() => calculateStats(treeData), [treeData])
  const fullJson = useMemo(() => JSON.stringify(treeData, null, 2), [treeData])

  useEffect(() => {
    const positions = findLeafPositions(fullJson, leafNodes)
    setLeafPositions(positions)
  }, [fullJson, leafNodes])

  const handleEditorMount = useCallback(
    (editor: monacoEditor.editor.IStandaloneCodeEditor, monaco: Monaco) => {
      editorRef.current = editor
      monacoRef.current = monaco
      ensureTheme(monaco)
      setEditorReady(true)
      updateDecorations()
    },
    []
  )

  const updateDecorations = useCallback(() => {
    if (!editorRef.current || !monacoRef.current || leafPositions.length === 0) return

    const decorations: monacoEditor.editor.IModelDeltaDecoration[] = leafPositions.map(
      (pos, idx) => ({
        range: new monacoRef.current!.Range(pos.lineNumber, 1, pos.lineNumber, 1),
        options: {
          isWholeLine: false,
          glyphMarginClassName: selectedIndex === idx || hoveredIndex === idx ? 'doc-marker-active' : 'doc-marker',
          glyphMarginHoverMessage: { value: `Section ${idx + 1}` },
        },
      })
    )

    editorRef.current.deltaDecorations([], decorations)
  }, [leafPositions, selectedIndex, hoveredIndex])

  useEffect(() => {
    updateDecorations()
  }, [updateDecorations])

  const updateLinePositions = useCallback(() => {
    if (!showPreview || !leftPanelRef.current || !rightPanelRef.current || !editorRef.current) return

    const leftRect = leftPanelRef.current.getBoundingClientRect()
    const containerRect = leftPanelRef.current.parentElement?.getBoundingClientRect()

    if (!containerRect) return

    const newPositions: Array<{ leftY: number; rightY: number; rightX: number; index: number }> = []

    leafPositions.forEach((pos, idx) => {
      const lineTop = editorRef.current?.getTopForLineNumber(pos.lineNumber) || 0
      const scrollTop = editorRef.current?.getScrollTop() || 0
      const editorTop = leftPanelRef.current?.querySelector('.monaco-editor')?.getBoundingClientRect().top || leftRect.top

      const leftY = editorTop - containerRect.top + lineTop - scrollTop + 10

      const contentEl = contentRefs.current.get(idx)
      if (contentEl) {
        const contentRect = contentEl.getBoundingClientRect()
        const rightY = contentRect.top - containerRect.top + (contentRect.height / 2)
        const rightX = contentRect.left - containerRect.left

        newPositions.push({
          leftY: Math.max(0, leftY),
          rightY: Math.max(0, rightY),
          rightX: rightX,
          index: idx,
        })
      }
    })

    setLinePositions(newPositions)
  }, [leafPositions, showPreview])

  // Clear line positions when preview is hidden
  useEffect(() => {
    if (!showPreview) setLinePositions([])
  }, [showPreview])

  useEffect(() => {
    if (!editorReady || !showPreview) return

    const updateLines = () => {
      requestAnimationFrame(updateLinePositions)
    }

    updateLines()

    const editor = editorRef.current
    const rightPanel = rightPanelRef.current
    const leftPanel = leftPanelRef.current
    const container = leftPanel?.parentElement

    const editorDisposable = editor?.onDidScrollChange(updateLines)
    rightPanel?.addEventListener('scroll', updateLines)
    window.addEventListener('resize', updateLines)

    const resizeObserver = new ResizeObserver(() => {
      updateLines()
    })
    
    if (container) resizeObserver.observe(container)
    if (leftPanel) resizeObserver.observe(leftPanel)
    if (rightPanel) resizeObserver.observe(rightPanel)

    const intervalId = setInterval(updateLines, 500)

    return () => {
      editorDisposable?.dispose()
      rightPanel?.removeEventListener('scroll', updateLines)
      window.removeEventListener('resize', updateLines)
      resizeObserver.disconnect()
      clearInterval(intervalId)
    }
  }, [editorReady, showPreview, updateLinePositions])

  const handleSelectContent = useCallback(
    (index: number) => {
      setSelectedIndex(index)

      const pos = leafPositions.find((p) => p.index === index)
      if (pos && editorRef.current) {
        editorRef.current.revealLineInCenter(pos.lineNumber)
      }
    },
    [leafPositions]
  )

  useEffect(() => {
    if (selectedIndex !== null && showPreview) {
      const contentEl = contentRefs.current.get(selectedIndex)
      contentEl?.scrollIntoView({ behavior: 'smooth', block: 'center' })
    }
  }, [selectedIndex, showPreview])

  useEffect(() => {
    const style = document.createElement('style')
    style.id = 'node-split-viewer-styles'
    style.textContent = `
      .doc-marker {
        background: #a1a1aa;
        width: 5px !important;
        height: 5px !important;
        margin-left: 4px;
        margin-top: 6px;
        border-radius: 50%;
      }
      .doc-marker-active {
        background: ${ACCENT_COLOR};
        width: 7px !important;
        height: 7px !important;
        margin-left: 3px;
        margin-top: 5px;
        border-radius: 50%;
      }
    `
    document.head.appendChild(style)
    return () => {
      const existingStyle = document.getElementById('node-split-viewer-styles')
      if (existingStyle) {
        document.head.removeChild(existingStyle)
      }
    }
  }, [])

  return (
    <div className={cn('flex flex-col h-full', className)}>
      {/* Search bar + preview toggle */}
      <div className="px-4 py-3 border-b border-border flex items-center gap-3">
        <div className="relative flex-1 max-w-md">
          <MagnifyingGlass
            size={16}
            className="absolute left-3 top-1/2 -translate-y-1/2 text-overlay-0"
            aria-hidden="true"
          />
          <input
            type="text"
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            placeholder="Search document..."
            className={cn(
              'w-full pl-9 pr-9 py-2 text-sm rounded-lg',
              'bg-surface-0 border border-surface-1',
              'text-text placeholder:text-overlay-0',
              'focus:outline-none focus:ring-2 focus:ring-blue/20 focus:border-blue/50',
              'transition-all duration-150'
            )}
          />
          {searchQuery && (
            <button
              onClick={() => setSearchQuery('')}
              className="absolute right-3 top-1/2 -translate-y-1/2 text-overlay-0 hover:text-text"
              aria-label="Clear search"
            >
              <X size={16} aria-hidden="true" />
            </button>
          )}
        </div>

        <button
          onClick={() => setShowPreview((v) => !v)}
          className={cn(
            'flex items-center gap-1.5 px-3 py-2 text-xs font-medium rounded-lg transition-colors shrink-0',
            showPreview
              ? 'bg-blue/15 text-blue border border-blue/30'
              : 'bg-surface-0 text-overlay-0 hover:text-text border border-surface-1 hover:border-overlay-0'
          )}
          aria-pressed={showPreview}
          aria-label="Toggle document preview"
        >
          {showPreview ? (
            <ArticleNyTimes size={14} weight="bold" aria-hidden="true" />
          ) : (
            <Article size={14} aria-hidden="true" />
          )}
          Preview
        </button>
      </div>

      {searchQuery && (
        <div className="px-4 py-1.5 text-xs text-overlay-0 border-b border-border bg-surface-0/30">
          {filteredLeafNodes.length} of {leafNodes.length} sections
        </div>
      )}

      {/* Split View */}
      <div className="flex-1 flex min-h-0 relative">
        {/* Left Panel — JSON tree (full-width when preview hidden) */}
        <div
          ref={leftPanelRef}
          className={cn(
            'flex flex-col min-h-0 transition-all duration-200',
            showPreview ? 'w-[45%] border-r border-border' : 'w-full'
          )}
        >
          <div className="flex-1 min-h-0">
            <Editor
              height="100%"
              language="json"
              value={fullJson}
              onMount={handleEditorMount}
              options={{
                readOnly: true,
                minimap: { enabled: false },
                fontSize: 12,
                fontFamily: 'JetBrains Mono, Menlo, Monaco, monospace',
                lineNumbers: 'on',
                scrollBeyondLastLine: false,
                automaticLayout: true,
                wordWrap: 'off',
                folding: true,
                foldingStrategy: 'indentation',
                showFoldingControls: 'mouseover',
                renderLineHighlight: 'none',
                glyphMargin: true,
                scrollbar: {
                  vertical: 'auto',
                  horizontal: 'auto',
                  verticalScrollbarSize: 6,
                  horizontalScrollbarSize: 6,
                },
                padding: { top: 12, bottom: 12 },
              }}
              theme={THEME_NAME}
            />
          </div>
        </div>

        {/* Connection Lines — only when preview is visible */}
        {showPreview && linePositions.length > 0 && (
          <svg
            className="absolute inset-0 pointer-events-none"
            style={{ overflow: 'visible', zIndex: 10 }}
          >
            <defs>
              <linearGradient id="line-gradient-inactive" x1="0%" y1="0%" x2="100%" y2="0%">
                <stop offset="0%" stopColor="#a1a1aa" stopOpacity="0.9" />
                <stop offset="100%" stopColor="#a1a1aa" stopOpacity="0.5" />
              </linearGradient>
              <linearGradient id="line-gradient-active" x1="0%" y1="0%" x2="100%" y2="0%">
                <stop offset="0%" stopColor={ACCENT_COLOR} stopOpacity="1" />
                <stop offset="100%" stopColor={ACCENT_COLOR} stopOpacity="0.8" />
              </linearGradient>
            </defs>
            {linePositions.map((pos) => {
              const isActive = selectedIndex === pos.index || hoveredIndex === pos.index
              const leftPanelWidth = leftPanelRef.current?.offsetWidth || 0
              const startX = leftPanelWidth
              const endX = pos.rightX
              const midX = (startX + endX) / 2

              return (
                <g key={pos.index}>
                  <path
                    d={`M ${startX} ${pos.leftY} C ${midX} ${pos.leftY}, ${midX} ${pos.rightY}, ${endX} ${pos.rightY}`}
                    fill="none"
                    stroke={isActive ? 'url(#line-gradient-active)' : 'url(#line-gradient-inactive)'}
                    strokeWidth={isActive ? 2 : 1.5}
                    strokeLinecap="round"
                    className="transition-all duration-150"
                  />
                  <circle
                    cx={startX}
                    cy={pos.leftY}
                    r={isActive ? 4 : 3}
                    fill={isActive ? ACCENT_COLOR : '#a1a1aa'}
                    className="transition-all duration-150"
                  />
                  <circle
                    cx={endX}
                    cy={pos.rightY}
                    r={isActive ? 4 : 3}
                    fill={isActive ? ACCENT_COLOR : '#a1a1aa'}
                    className="transition-all duration-150"
                  />
                </g>
              )
            })}
          </svg>
        )}

        {/* Right Panel — Document preview (toggled) */}
        {showPreview && (
          <div ref={rightPanelRef} className="w-[55%] flex flex-col min-h-0 bg-mantle/30">
            <div className="flex-1 overflow-auto">
              <div className="py-6 px-6 pl-8">
                {filteredLeafNodes.length > 0 ? (
                  <div className="space-y-4">
                    {filteredLeafNodes.map((node) => {
                      const originalIndex = leafNodes.indexOf(node)
                      return (
                        <ContentBlock
                          key={node.id}
                          ref={(el) => {
                            if (el) contentRefs.current.set(originalIndex, el)
                          }}
                          node={node}
                          index={originalIndex}
                          searchQuery={searchQuery}
                          isSelected={selectedIndex === originalIndex}
                          isHovered={hoveredIndex === originalIndex}
                          onSelect={() => handleSelectContent(originalIndex)}
                          onHover={(hovered) => setHoveredIndex(hovered ? originalIndex : null)}
                        />
                      )
                    })}
                  </div>
                ) : (
                  <div className="flex items-center justify-center h-64 text-overlay-0 text-sm">
                    {searchQuery ? 'No matching sections' : 'No content available'}
                  </div>
                )}
              </div>
            </div>
          </div>
        )}
      </div>

      {/* Footer */}
      <div className="px-4 py-2 border-t border-border bg-surface-0/30">
        <div className="flex items-center justify-between text-xs text-overlay-0">
          <span>{stats.leafNodes} sections</span>
          <span>{stats.totalChars.toLocaleString()} characters</span>
        </div>
      </div>
    </div>
  )
}
