import { useState, useEffect, useCallback, useRef, lazy, Suspense } from 'react'
import { Panel, Group, Separator } from 'react-resizable-panels'
import {
  Plus,
  X,
  Table as TableIcon,
  Code,
  TreeStructure,
  FileCode,
  CircleNotch,
} from '@phosphor-icons/react'
import { cn } from '@/lib/utils'
import { WelcomeScreen } from '@/components/common/WelcomeScreen'
import { QueryResults } from '@/components/query/QueryResults'
import { JsonViewer } from '@/components/shared/JsonViewer'
import { useQueryStore } from '@/stores/queryStore'
import { useTableStore } from '@/stores/tableStore'
import { useUiStore } from '@/stores/uiStore'
import { useConnectionStore } from '@/stores/connectionStore'
import { useTabsStore } from '@/stores/tabsStore'

const QueryEditor = lazy(() => import('@/components/query/QueryEditor'))
const DocumentViewer = lazy(() =>
  import('@/components/table/DocumentViewer').then(m => ({ default: m.DocumentViewer }))
)
const AgentSettings = lazy(() =>
  import('@/components/settings/AgentSettings').then(m => ({ default: m.AgentSettings }))
)

function LazyFallback() {
  return (
    <div className="flex items-center justify-center h-full text-subtext-0">
      <CircleNotch size={24} className="animate-spin" />
    </div>
  )
}

export function MainPanel() {
  const [resultView, setResultView] = useState<'table' | 'json' | 'tree'>('table')
  const { results, activeResultIndex } = useQueryStore()
  const activeResult = results.length > 0 ? (results[activeResultIndex] ?? results[0]) : null
  const totalStats = results.length > 0
    ? {
        rowCount: results.reduce((sum, r) => sum + r.rowCount, 0),
        executionTime: results.reduce((sum, r) => sum + r.executionTime, 0),
      }
    : null
  const { selectedTableId, tables, selectTable } = useTableStore()
  const { openConnectionForm } = useUiStore()
  const { activeConnectionId } = useConnectionStore()
  const { tabs, activeTabId, addTab, closeTab: closeTabStore, setActiveTab } = useTabsStore()
  const tabListRef = useRef<HTMLDivElement>(null)

  const addNewTab = () => {
    addTab({
      title: `Query ${tabs.length + 1}`,
      type: 'query',
      query: '',
    })
  }

  const addTableTab = (tableId: string) => {
    const table = tables.find((t) => t.id === tableId)
    if (!table) return

    const existingTab = tabs.find((t) => t.type === 'table' && t.tableId === tableId)
    if (existingTab) {
      setActiveTab(existingTab.id)
      return
    }

    addTab({
      title: table.name,
      type: 'table',
      tableId,
    })
  }

  useEffect(() => {
    if (selectedTableId) {
      addTableTab(selectedTableId)
      selectTable(null)
    }
  }, [selectedTableId])

  const handleCloseTab = (id: string, e: React.MouseEvent | React.KeyboardEvent) => {
    e.stopPropagation()
    closeTabStore(id)
  }
  
  const { updateTabQuery } = useTabsStore()
  
  const handleQueryChange = useCallback((query: string) => {
    if (activeTabId) {
      updateTabQuery(activeTabId, query)
    }
  }, [activeTabId, updateTabQuery])

  const handleTabKeyDown = (e: React.KeyboardEvent) => {
    const currentIndex = tabs.findIndex((t) => t.id === activeTabId)
    let nextIndex: number | null = null

    if (e.key === 'ArrowRight') {
      nextIndex = currentIndex < tabs.length - 1 ? currentIndex + 1 : 0
    } else if (e.key === 'ArrowLeft') {
      nextIndex = currentIndex > 0 ? currentIndex - 1 : tabs.length - 1
    } else if (e.key === 'Home') {
      nextIndex = 0
    } else if (e.key === 'End') {
      nextIndex = tabs.length - 1
    }

    if (nextIndex !== null) {
      e.preventDefault()
      setActiveTab(tabs[nextIndex].id)
      const buttons = tabListRef.current?.querySelectorAll<HTMLButtonElement>('[role="tab"]')
      buttons?.[nextIndex]?.focus()
    }
  }

  const activeTab = tabs.find((t) => t.id === activeTabId)

  if (tabs.length === 0 && !activeConnectionId) {
    return <WelcomeScreen onNewQuery={addNewTab} onNewConnection={openConnectionForm} />
  }

  return (
    <div className="h-full flex flex-col bg-base">
      {/* Tab bar */}
      <div className="flex items-center bg-mantle border-b border-border min-h-[40px]">
        <div
          ref={tabListRef}
          role="tablist"
          aria-label="Open tabs"
          className="flex-1 flex items-center overflow-x-auto scrollbar-none"
          onKeyDown={handleTabKeyDown}
        >
          {tabs.map((tab) => {
            const isActive = activeTabId === tab.id
            return (
              <button
                key={tab.id}
                role="tab"
                id={`tab-${tab.id}`}
                aria-selected={isActive}
                aria-controls={`tabpanel-${tab.id}`}
                tabIndex={isActive ? 0 : -1}
                onClick={() => setActiveTab(tab.id)}
                className={cn(
                  'group relative flex items-center h-[40px] px-3 text-sm cursor-pointer select-none',
                  'transition-colors duration-150',
                  isActive
                    ? 'bg-base text-text'
                    : 'text-subtext-0 hover:text-text hover:bg-surface-0/50'
                )}
              >
                {isActive && (
                  <div className="absolute bottom-0 left-0 right-0 h-[2px] bg-mauve" aria-hidden="true" />
                )}
                
                {tab.type === 'table' ? (
                  <TableIcon 
                    size={14} 
                    weight={isActive ? 'fill' : 'regular'} 
                    className={cn('shrink-0 mr-2', isActive ? 'text-green' : 'text-overlay-0')} 
                    aria-hidden="true"
                  />
                ) : tab.type === 'settings' ? (
                  <Code 
                    size={14} 
                    weight={isActive ? 'fill' : 'regular'} 
                    className={cn('shrink-0 mr-2', isActive ? 'text-peach' : 'text-overlay-0')} 
                    aria-hidden="true"
                  />
                ) : (
                  <FileCode 
                    size={14} 
                    weight={isActive ? 'fill' : 'regular'} 
                    className={cn('shrink-0 mr-2', isActive ? 'text-mauve' : 'text-overlay-0')} 
                    aria-hidden="true"
                  />
                )}
                <span className="truncate max-w-[120px]">{tab.title}</span>
                
                <span
                  role="button"
                  tabIndex={0}
                  onClick={(e) => handleCloseTab(tab.id, e)}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter' || e.key === ' ') handleCloseTab(tab.id, e)
                  }}
                  className={cn(
                    'ml-2 p-1 rounded-sm transition-all inline-flex items-center justify-center',
                    'hover:bg-surface-1 active:bg-surface-2',
                    isActive 
                      ? 'text-overlay-1 hover:text-text' 
                      : 'text-transparent group-hover:text-overlay-0 hover:text-text!'
                  )}
                  aria-label={`Close ${tab.title}`}
                >
                  <X size={12} weight="bold" aria-hidden="true" />
                </span>
              </button>
            )
          })}
        </div>
        
        <button
          onClick={addNewTab}
          className={cn(
            'flex items-center justify-center w-[40px] h-[40px]',
            'text-overlay-0 hover:text-text hover:bg-surface-0/50',
            'transition-colors'
          )}
          aria-label="New query tab"
        >
          <Plus size={16} weight="bold" aria-hidden="true" />
        </button>
      </div>

      {/* Tab panel content */}
      {tabs.length === 0 ? (
        <div className="flex-1 flex flex-col items-center justify-center bg-base text-center p-8">
          <FileCode size={48} weight="duotone" className="text-overlay-0 mb-4" aria-hidden="true" />
          <h3 className="text-lg font-medium text-text mb-2">No Tabs Open</h3>
          <p className="text-sm text-subtext-0 mb-6 max-w-sm">
            Select a table from the sidebar or create a new query tab to get started
          </p>
          <button
            onClick={addNewTab}
            className={cn(
              'flex items-center gap-2 px-4 py-2 rounded-lg',
              'bg-surface-0 border border-border text-subtext-0 font-medium',
              'hover:bg-surface-1 hover:text-text hover:border-overlay-0 transition-colors'
            )}
          >
            <Plus size={16} aria-hidden="true" />
            New Query
          </button>
        </div>
      ) : activeTab?.type === 'settings' ? (
        <div
          role="tabpanel"
          id={`tabpanel-${activeTab.id}`}
          aria-labelledby={`tab-${activeTab.id}`}
          className="flex-1 overflow-hidden"
        >
          <Suspense fallback={<LazyFallback />}>
            <AgentSettings />
          </Suspense>
        </div>
      ) : activeTab?.type === 'table' && activeTab.tableId ? (
        <div
          role="tabpanel"
          id={`tabpanel-${activeTab.id}`}
          aria-labelledby={`tab-${activeTab.id}`}
          className="flex-1 overflow-hidden"
        >
          <Suspense fallback={<LazyFallback />}>
            <DocumentViewer tableId={activeTab.tableId} />
          </Suspense>
        </div>
      ) : (
        <div
          role="tabpanel"
          id={`tabpanel-${activeTab?.id}`}
          aria-labelledby={`tab-${activeTab?.id}`}
          className="flex-1 min-h-0"
        >
          <Group orientation="vertical" className="h-full">
            <Panel defaultSize={55} minSize={20}>
              <Suspense fallback={<LazyFallback />}>
                <QueryEditor 
                  tabId={activeTabId || 'default'}
                  initialQuery={activeTab?.query || ''}
                  onQueryChange={handleQueryChange}
                />
              </Suspense>
            </Panel>

            <Separator className="h-1 bg-border hover:bg-primary/50 transition-colors cursor-row-resize" />

            <Panel defaultSize={45} minSize={15}>
              <div className="h-full flex flex-col">
                <div className="flex items-center justify-between px-3 py-1.5 bg-surface-0/30 border-b border-border">
                  <div className="flex items-center gap-4">
                    <span className="text-sm text-text font-medium">Results</span>
                    {totalStats && (
                      <span className="text-xs text-overlay-0">
                        {totalStats.rowCount} rows · {totalStats.executionTime}ms
                        {results.length > 1 && ` · ${results.length} queries`}
                      </span>
                    )}
                  </div>
                  <div className="flex items-center gap-1" role="group" aria-label="Result view mode">
                    <button
                      onClick={() => setResultView('table')}
                      className={cn(
                        'p-1.5 rounded transition-colors',
                        resultView === 'table'
                          ? 'bg-surface-1 text-text'
                          : 'text-overlay-0 hover:text-text hover:bg-surface-0'
                      )}
                      aria-label="Table view"
                      aria-pressed={resultView === 'table'}
                    >
                      <TableIcon size={16} weight="bold" aria-hidden="true" />
                    </button>
                    <button
                      onClick={() => setResultView('json')}
                      className={cn(
                        'p-1.5 rounded transition-colors',
                        resultView === 'json'
                          ? 'bg-surface-1 text-text'
                          : 'text-overlay-0 hover:text-text hover:bg-surface-0'
                      )}
                      aria-label="JSON view"
                      aria-pressed={resultView === 'json'}
                    >
                      <Code size={16} weight="bold" aria-hidden="true" />
                    </button>
                    <button
                      onClick={() => setResultView('tree')}
                      className={cn(
                        'p-1.5 rounded transition-colors',
                        resultView === 'tree'
                          ? 'bg-surface-1 text-text'
                          : 'text-overlay-0 hover:text-text hover:bg-surface-0'
                      )}
                      aria-label="Tree view"
                      aria-pressed={resultView === 'tree'}
                    >
                      <TreeStructure size={16} weight="bold" aria-hidden="true" />
                    </button>
                  </div>
                </div>

                <div className="flex-1 min-h-0">
                  {resultView === 'table' && <QueryResults />}
                  {resultView === 'json' && (
                    <JsonViewer
                      data={activeResult?.rows}
                      emptyMessage="Run a query to see results"
                    />
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
        </div>
      )}
    </div>
  )
}
