import { useState, useEffect, useCallback, useRef } from 'react'
import { X, Copy, CheckCircle, BracketsCurly, TreeStructure, CircleNotch, WarningCircle } from '@phosphor-icons/react'
import { cn } from '@/lib/utils'
import { JsonViewer } from '@/components/shared/JsonViewer'
import { NodeSplitViewer } from '@/components/shared/NodeSplitViewer'
import { createClient, type TreeNode } from '@/lib/api'
import type { Document } from '@/stores/tableStore'
import type { Connection } from '@/stores/connectionStore'

type Tab = 'record' | 'nodes'

interface RecordSidebarProps {
  document: Document
  connection: Connection
  onClose: () => void
}

export function RecordSidebar({ document, connection, onClose }: RecordSidebarProps) {
  const [activeTab, setActiveTab] = useState<Tab>('record')
  const [copied, setCopied] = useState(false)

  // Lazy tree loading — only fetched when the Nodes tab is first opened
  const [treeData, setTreeData] = useState<TreeNode | null>(null)
  const [treeLoading, setTreeLoading] = useState(false)
  const [treeError, setTreeError] = useState<string | null>(null)
  const fetchedDocIdRef = useRef<string | null>(null)

  // Reset tree cache when the selected document changes
  useEffect(() => {
    if (document.id !== fetchedDocIdRef.current) {
      setTreeData(null)
      setTreeError(null)
      setTreeLoading(false)
      fetchedDocIdRef.current = null
    }
  }, [document.id])

  // Reset to record tab when document changes
  useEffect(() => {
    setActiveTab('record')
  }, [document.id])

  const fetchTree = useCallback(async () => {
    if (fetchedDocIdRef.current === document.id) return
    const docId = document.data.id as string
    if (!docId) return

    setTreeLoading(true)
    setTreeError(null)
    try {
      const client = createClient({
        host: connection.host,
        port: connection.port,
        apiKey: connection.apiKey,
        useSsl: connection.ssl,
      })
      const tree = await client.getDocumentTree(docId)
      setTreeData(tree)
      fetchedDocIdRef.current = document.id
    } catch (err) {
      setTreeError(err instanceof Error ? err.message : 'Failed to load document tree')
    } finally {
      setTreeLoading(false)
    }
  }, [document.id, document.data.id, connection])

  const handleTabChange = (tab: Tab) => {
    setActiveTab(tab)
    if (tab === 'nodes' && fetchedDocIdRef.current !== document.id) {
      fetchTree()
    }
  }

  const handleCopy = async () => {
    await navigator.clipboard.writeText(JSON.stringify(document.data, null, 2))
    setCopied(true)
    setTimeout(() => setCopied(false), 2000)
  }

  // Escape to close
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose()
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [onClose])

  const title = String(document.data.title || document.id)
  const totalNodes = document.data.total_nodes as number | undefined

  return (
    <div className="flex flex-col h-full bg-mantle border-l border-border">
      {/* Header */}
      <div className="flex items-center justify-between px-3 py-2 border-b border-border bg-surface-0/30">
        <div className="flex-1 min-w-0 mr-2">
          <h3 className="text-sm font-semibold text-text truncate">{title}</h3>
        </div>
        <div className="flex items-center gap-1 shrink-0">
          <button
            onClick={handleCopy}
            className="p-1.5 rounded transition-colors hover:bg-surface-1 text-overlay-0 hover:text-text"
            aria-label="Copy document JSON"
          >
            {copied ? (
              <CheckCircle size={15} className="text-green" aria-hidden="true" />
            ) : (
              <Copy size={15} aria-hidden="true" />
            )}
          </button>
          <button
            onClick={onClose}
            className="p-1.5 rounded transition-colors hover:bg-surface-1 text-overlay-0 hover:text-text"
            aria-label="Close detail sidebar"
          >
            <X size={15} weight="bold" aria-hidden="true" />
          </button>
        </div>
      </div>

      {/* Tabs */}
      <div
        role="tablist"
        aria-label="Document detail tabs"
        className="flex border-b border-border bg-mantle"
      >
        <TabButton
          active={activeTab === 'record'}
          onClick={() => handleTabChange('record')}
          id="tab-record"
          controls="panel-record"
        >
          <BracketsCurly size={14} aria-hidden="true" />
          Record
        </TabButton>
        <TabButton
          active={activeTab === 'nodes'}
          onClick={() => handleTabChange('nodes')}
          id="tab-nodes"
          controls="panel-nodes"
        >
          <TreeStructure size={14} aria-hidden="true" />
          Nodes
          {totalNodes != null && (
            <span className="ml-1 text-[10px] px-1.5 py-0.5 rounded-full bg-surface-1 text-overlay-0">
              {totalNodes}
            </span>
          )}
        </TabButton>
      </div>

      {/* Tab panels */}
      <div className="flex-1 min-h-0">
        {activeTab === 'record' && (
          <div
            role="tabpanel"
            id="panel-record"
            aria-labelledby="tab-record"
            className="h-full"
          >
            <JsonViewer data={document.data} lineNumbers={false} />
          </div>
        )}

        {activeTab === 'nodes' && (
          <div
            role="tabpanel"
            id="panel-nodes"
            aria-labelledby="tab-nodes"
            className="h-full"
          >
            {treeLoading ? (
              <div className="flex flex-col items-center justify-center h-full gap-3">
                <CircleNotch size={24} className="animate-spin text-mauve" aria-hidden="true" />
                <span className="text-sm text-overlay-0">Loading document tree...</span>
              </div>
            ) : treeError ? (
              <div className="flex flex-col items-center justify-center h-full gap-3 p-4 text-center">
                <WarningCircle size={24} className="text-red" aria-hidden="true" />
                <span className="text-sm text-red">{treeError}</span>
                <button
                  onClick={fetchTree}
                  className="text-xs text-mauve hover:underline"
                >
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
  )
}

function TabButton({
  active,
  onClick,
  id,
  controls,
  children,
}: {
  active: boolean
  onClick: () => void
  id: string
  controls: string
  children: React.ReactNode
}) {
  return (
    <button
      role="tab"
      id={id}
      aria-selected={active}
      aria-controls={controls}
      tabIndex={active ? 0 : -1}
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
        <div className="absolute bottom-0 left-0 right-0 h-[2px] bg-mauve" aria-hidden="true" />
      )}
    </button>
  )
}
