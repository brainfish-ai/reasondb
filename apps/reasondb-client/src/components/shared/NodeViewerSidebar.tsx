import { useEffect, useState } from 'react'
import { X, ArrowsOut, ArrowsIn } from '@phosphor-icons/react'
import { cn } from '@/lib/utils'
import { NodeSplitViewer, type TreeNode } from './NodeSplitViewer'

interface NodeViewerSidebarProps {
  isOpen: boolean
  onClose: () => void
  title: string
  documentData?: Record<string, unknown>
  treeData?: TreeNode
  isLoading?: boolean
}

const MIN_WIDTH = 600
const MAX_WIDTH = window.innerWidth * 0.9
const DEFAULT_WIDTH = 800

export function NodeViewerSidebar({
  isOpen,
  onClose,
  title,
  documentData: _documentData,
  treeData,
  isLoading,
}: NodeViewerSidebarProps) {
  const [isExpanded, setIsExpanded] = useState(false)
  const [isVisible, setIsVisible] = useState(false)
  const [width, setWidth] = useState(DEFAULT_WIDTH)
  const [isDragging, setIsDragging] = useState(false)

  // Handle open/close animation
  useEffect(() => {
    if (isOpen) {
      requestAnimationFrame(() => {
        setIsVisible(true)
      })
    } else {
      setIsVisible(false)
    }
  }, [isOpen])

  // Handle drag resize
  useEffect(() => {
    if (!isDragging) return

    const handleMouseMove = (e: MouseEvent) => {
      const newWidth = window.innerWidth - e.clientX
      setWidth(Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, newWidth)))
    }

    const handleMouseUp = () => {
      setIsDragging(false)
    }

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

  const handleDragStart = (e: React.MouseEvent) => {
    e.preventDefault()
    setIsDragging(true)
  }

  // Close on escape key
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && isOpen) {
        onClose()
      }
    }
    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
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
        style={{ width: isExpanded ? '90vw' : width }}
      >
        {/* Drag handle */}
        <div
          onMouseDown={handleDragStart}
          className={cn(
            'absolute left-0 top-0 bottom-0 w-1 cursor-col-resize z-10',
            'hover:bg-mauve/50 transition-colors',
            isDragging && 'bg-mauve'
          )}
        />

        {/* Header */}
        <div className="flex items-center justify-between px-4 py-3 border-b border-border bg-surface-0/30">
          <div className="flex flex-col gap-0.5 min-w-0 flex-1">
            <h3 className="text-sm font-semibold text-text truncate">{title}</h3>
            <span className="text-xs text-overlay-0">Document Node Viewer</span>
          </div>
          <div className="flex items-center gap-1 ml-2">
            <button
              onClick={() => setIsExpanded(!isExpanded)}
              className={cn(
                'p-1.5 rounded transition-colors',
                'hover:bg-surface-1 text-overlay-1 hover:text-text'
              )}
              title={isExpanded ? 'Collapse' : 'Expand'}
            >
              {isExpanded ? <ArrowsIn size={16} /> : <ArrowsOut size={16} />}
            </button>
            <button
              onClick={onClose}
              className={cn(
                'p-1.5 rounded transition-colors',
                'hover:bg-surface-1 text-overlay-1 hover:text-text'
              )}
              title="Close (Esc)"
            >
              <X size={16} weight="bold" />
            </button>
          </div>
        </div>

        {/* Content */}
        <div className="flex-1 min-h-0">
          {isLoading ? (
            <div className="flex flex-col items-center justify-center h-full gap-3">
              <div className="w-8 h-8 border-2 border-mauve border-t-transparent rounded-full animate-spin" />
              <span className="text-sm text-overlay-1">Loading document tree...</span>
            </div>
          ) : treeData ? (
            <NodeSplitViewer treeData={treeData} />
          ) : (
            <div className="flex items-center justify-center h-full text-overlay-0 text-sm">
              No tree data available
            </div>
          )}
        </div>
      </div>
    </>
  )
}
