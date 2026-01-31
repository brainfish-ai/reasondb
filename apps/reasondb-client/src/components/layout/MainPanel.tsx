import { useState, useEffect } from 'react'
import { Panel, Group, Separator } from 'react-resizable-panels'
import {
  Plus,
  X,
  Table as TableIcon,
  Code,
  TreeStructure,
  FileCode,
} from '@phosphor-icons/react'
import { cn } from '@/lib/utils'
import { WelcomeScreen } from '@/components/common/WelcomeScreen'
import { QueryEditor } from '@/components/query/QueryEditor'
import { QueryResults } from '@/components/query/QueryResults'
import { DocumentViewer } from '@/components/table/DocumentViewer'
import { useQueryStore } from '@/stores/queryStore'
import { useTableStore } from '@/stores/tableStore'
import { useUiStore } from '@/stores/uiStore'
import { useConnectionStore } from '@/stores/connectionStore'

interface Tab {
  id: string
  title: string
  type: 'query' | 'table'
  tableId?: string // For table tabs
}

export function MainPanel() {
  const [tabs, setTabs] = useState<Tab[]>([])
  const [activeTabId, setActiveTabId] = useState<string | null>(null)
  const [resultView, setResultView] = useState<'table' | 'json' | 'tree'>('table')
  const { result } = useQueryStore()
  const { selectedTableId, tables, selectTable } = useTableStore()
  const { openConnectionForm } = useUiStore()
  const { activeConnectionId } = useConnectionStore()

  const addNewTab = () => {
    const newTab: Tab = {
      id: crypto.randomUUID(),
      title: `Query ${tabs.length + 1}`,
      type: 'query',
    }
    setTabs([...tabs, newTab])
    setActiveTabId(newTab.id)
  }

  const addTableTab = (tableId: string) => {
    const table = tables.find((t) => t.id === tableId)
    if (!table) return

    // Check if tab already exists
    const existingTab = tabs.find((t) => t.type === 'table' && t.tableId === tableId)
    if (existingTab) {
      setActiveTabId(existingTab.id)
      return
    }

    const newTab: Tab = {
      id: crypto.randomUUID(),
      title: table.name,
      type: 'table',
      tableId,
    }
    setTabs([...tabs, newTab])
    setActiveTabId(newTab.id)
  }

  // Open table tab when selectedTableId changes
  useEffect(() => {
    if (selectedTableId) {
      addTableTab(selectedTableId)
      // Clear the selection to allow re-selecting the same table
      selectTable(null)
    }
  }, [selectedTableId])

  const closeTab = (id: string, e: React.MouseEvent) => {
    e.stopPropagation()
    const newTabs = tabs.filter((t) => t.id !== id)
    setTabs(newTabs)
    if (activeTabId === id) {
      setActiveTabId(newTabs.length > 0 ? newTabs[newTabs.length - 1].id : null)
    }
  }

  const activeTab = tabs.find((t) => t.id === activeTabId)

  // Show welcome screen only when no connection is active and no tabs are open
  if (tabs.length === 0 && !activeConnectionId) {
    return <WelcomeScreen onNewQuery={addNewTab} onNewConnection={openConnectionForm} />
  }

  return (
    <div className="h-full flex flex-col bg-base">
      {/* Tab bar */}
      <div className="flex items-center bg-mantle border-b border-border min-h-[40px]">
        <div className="flex-1 flex items-center overflow-x-auto scrollbar-none">
          {tabs.map((tab) => {
            const isActive = activeTabId === tab.id
            return (
              <div
                key={tab.id}
                onClick={() => setActiveTabId(tab.id)}
                className={cn(
                  'group relative flex items-center h-[40px] px-3 text-sm cursor-pointer select-none',
                  'transition-colors duration-150',
                  isActive
                    ? 'bg-base text-text'
                    : 'text-subtext-0 hover:text-text hover:bg-surface-0/50'
                )}
              >
                {/* Active indicator */}
                {isActive && (
                  <div className="absolute bottom-0 left-0 right-0 h-[2px] bg-mauve" />
                )}
                
                {/* Tab content */}
                {tab.type === 'table' ? (
                  <TableIcon 
                    size={14} 
                    weight={isActive ? 'fill' : 'regular'} 
                    className={cn('shrink-0 mr-2', isActive ? 'text-green' : 'text-overlay-0')} 
                  />
                ) : (
                  <FileCode 
                    size={14} 
                    weight={isActive ? 'fill' : 'regular'} 
                    className={cn('shrink-0 mr-2', isActive ? 'text-mauve' : 'text-overlay-0')} 
                  />
                )}
                <span className="truncate max-w-[120px]">{tab.title}</span>
                
                {/* Close button */}
                <button
                  onClick={(e) => closeTab(tab.id, e)}
                  className={cn(
                    'ml-2 p-1 rounded-sm transition-all',
                    'hover:bg-surface-1 active:bg-surface-2',
                    isActive 
                      ? 'text-overlay-1 hover:text-text' 
                      : 'text-transparent group-hover:text-overlay-0 hover:!text-text'
                  )}
                  title="Close tab"
                >
                  <X size={12} weight="bold" />
                </button>
              </div>
            )
          })}
        </div>
        
        {/* New tab button */}
        <button
          onClick={addNewTab}
          className={cn(
            'flex items-center justify-center w-[40px] h-[40px]',
            'text-overlay-0 hover:text-text hover:bg-surface-0/50',
            'transition-colors'
          )}
          title="New Query Tab (⌘T)"
        >
          <Plus size={16} weight="bold" />
        </button>
      </div>

      {/* Main content */}
      {tabs.length === 0 ? (
        // Empty state when connected but no tabs
        <div className="flex-1 flex flex-col items-center justify-center bg-base text-center p-8">
          <FileCode size={48} weight="duotone" className="text-overlay-0 mb-4" />
          <h3 className="text-lg font-medium text-text mb-2">No Tabs Open</h3>
          <p className="text-sm text-subtext-0 mb-6 max-w-sm">
            Select a table from the sidebar or create a new query tab to get started
          </p>
          <button
            onClick={addNewTab}
            className={cn(
              'flex items-center gap-2 px-4 py-2 rounded-lg',
              'bg-mauve text-base font-medium',
              'hover:bg-mauve/90 transition-colors'
            )}
          >
            <Plus size={16} weight="bold" />
            New Query
          </button>
        </div>
      ) : activeTab?.type === 'table' && activeTab.tableId ? (
        <div className="flex-1 overflow-hidden">
          <DocumentViewer tableId={activeTab.tableId} />
        </div>
      ) : (
        <Group orientation="vertical" className="flex-1">
          {/* Editor panel */}
          <Panel defaultSize={55} minSize={20}>
            <QueryEditor />
          </Panel>

          <Separator className="h-1 bg-border hover:bg-primary/50 transition-colors cursor-row-resize" />

          {/* Results panel */}
          <Panel defaultSize={45} minSize={15}>
            <div className="h-full flex flex-col">
              {/* Results header with view toggles */}
              <div className="flex items-center justify-between px-3 py-1.5 bg-surface-0/30 border-b border-border">
                <div className="flex items-center gap-4">
                  <span className="text-sm text-text font-medium">Results</span>
                  {result && (
                    <span className="text-xs text-overlay-0">
                      {result.rowCount} rows · {result.executionTime}ms
                    </span>
                  )}
                </div>
                <div className="flex items-center gap-1">
                  <button
                    onClick={() => setResultView('table')}
                    className={cn(
                      'p-1.5 rounded transition-colors',
                      resultView === 'table'
                        ? 'bg-surface-1 text-text'
                        : 'text-overlay-0 hover:text-text hover:bg-surface-0'
                    )}
                    title="Table View"
                  >
                    <TableIcon size={16} weight="bold" />
                  </button>
                  <button
                    onClick={() => setResultView('json')}
                    className={cn(
                      'p-1.5 rounded transition-colors',
                      resultView === 'json'
                        ? 'bg-surface-1 text-text'
                        : 'text-overlay-0 hover:text-text hover:bg-surface-0'
                    )}
                    title="JSON View"
                  >
                    <Code size={16} weight="bold" />
                  </button>
                  <button
                    onClick={() => setResultView('tree')}
                    className={cn(
                      'p-1.5 rounded transition-colors',
                      resultView === 'tree'
                        ? 'bg-surface-1 text-text'
                        : 'text-overlay-0 hover:text-text hover:bg-surface-0'
                    )}
                    title="Tree View"
                  >
                    <TreeStructure size={16} weight="bold" />
                  </button>
                </div>
              </div>

              {/* Results content */}
              <div className="flex-1 min-h-0">
                {resultView === 'table' && <QueryResults />}
                {resultView === 'json' && (
                  <div className="h-full overflow-auto p-4 font-mono text-xs">
                    {result ? (
                      <pre className="text-text">
                        {JSON.stringify(result.rows, null, 2)}
                      </pre>
                    ) : (
                      <span className="text-overlay-0">Run a query to see results</span>
                    )}
                  </div>
                )}
                {resultView === 'tree' && (
                  <div className="flex items-center justify-center h-full text-overlay-0 text-sm">
                    Tree view coming soon...
                  </div>
                )}
              </div>
            </div>
          </Panel>
        </Group>
      )}
    </div>
  )
}
