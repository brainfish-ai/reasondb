import { useState, useEffect, useCallback, useRef } from 'react'
import {
  Table,
  Plus,
  DotsThree,
  MagnifyingGlass,
  Eye,
  ArrowClockwise,
  Pencil,
  Trash,
  Warning,
} from '@phosphor-icons/react'
import { useTableStore, type Table as TableType } from '@/stores/tableStore'
import { useTabsStore } from '@/stores/tabsStore'
import { useConnectionStore } from '@/stores/connectionStore'
import { Button } from '@/components/ui/Button'
import { cn } from '@/lib/utils'
import { createClient, type TableSummary } from '@/lib/api'
import { setValueFetcher, useSchemaStore } from '@/lib/rql-language'
import { CreateTableDialog } from '@/components/table/CreateTableDialog'
import { EditTableDialog } from '@/components/table/EditTableDialog'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from '@/components/ui/Dialog'

function apiTableToStoreTable(apiTable: TableSummary): TableType {
  return {
    id: apiTable.id,
    name: apiTable.name,
    description: apiTable.description,
    metadata: {},
    document_count: apiTable.document_count,
    total_nodes: apiTable.total_nodes,
    created_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
  }
}

interface TableItemProps {
  table: TableType
  isSelected: boolean
  onSelect: () => void
  onViewData: () => void
  onContextMenu: (e: React.MouseEvent) => void
}

function TableItem({ table, isSelected, onSelect, onViewData, onContextMenu }: TableItemProps) {
  return (
    <div className={cn('border-b border-border/30')}>
      <div
        className={cn(
          'flex items-center gap-2 px-3 py-2 cursor-pointer transition-colors group',
          isSelected
            ? 'bg-mauve/20'
            : 'hover:bg-surface-0/50'
        )}
        onClick={onSelect}
        onContextMenu={(e) => onContextMenu(e)}
      >
        <Table
          size={16}
          weight={isSelected ? 'fill' : 'duotone'}
          className={isSelected ? 'text-mauve' : 'text-overlay-1'}
        />

        <span className={cn('flex-1 text-sm truncate', isSelected ? 'text-text font-medium' : 'text-subtext-0')}>
          {table.name}
        </span>

        <span className="text-xs text-overlay-0">
          {table.document_count.toLocaleString()} docs
        </span>

        <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
          <button
            onClick={(e) => {
              e.stopPropagation()
              onViewData()
            }}
            className="p-1 hover:bg-surface-1 rounded text-overlay-0 hover:text-text"
            title="View data"
          >
            <Eye size={14} />
          </button>
          <button
            onClick={(e) => {
              e.stopPropagation()
              onContextMenu(e)
            }}
            className="p-1 hover:bg-surface-1 rounded text-overlay-0 hover:text-text"
            title="More options"
          >
            <DotsThree size={14} weight="bold" />
          </button>
        </div>
      </div>

      {isSelected && table.description && (
        <div className="px-3 pb-2 pl-9 bg-mauve/20">
          <p className="text-xs text-overlay-0">{table.description}</p>
        </div>
      )}
    </div>
  )
}

export function TableBrowser() {
  const { activeConnectionId, connections } = useConnectionStore()
  const {
    tables,
    setTables,
    selectTable,
    isLoadingTables,
    setLoadingTables,
    setTablesError,
    tablesError,
    deleteTable: deleteTableFromStore,
  } = useTableStore()

  const { tabs, activeTabId } = useTabsStore()
  const activeTab = tabs.find(t => t.id === activeTabId)
  const activeTableId = activeTab?.type === 'table' ? activeTab.tableId : undefined

  const [searchQuery, setSearchQuery] = useState('')
  const [showCreateDialog, setShowCreateDialog] = useState(false)
  const [editingTable, setEditingTable] = useState<TableType | null>(null)
  const [deletingTable, setDeletingTable] = useState<TableType | null>(null)
  const [isDeleting, setIsDeleting] = useState(false)
  const [deleteError, setDeleteError] = useState<string | null>(null)
  const [contextMenu, setContextMenu] = useState<{
    table: TableType
    x: number
    y: number
  } | null>(null)
  const menuRef = useRef<HTMLDivElement>(null)

  const activeConnection = connections.find(c => c.id === activeConnectionId)

  const closeContextMenu = useCallback(() => setContextMenu(null), [])

  useEffect(() => {
    if (!contextMenu) return
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') closeContextMenu()
    }
    const onPointerDown = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        closeContextMenu()
      }
    }
    document.addEventListener('keydown', onKey)
    document.addEventListener('mousedown', onPointerDown)
    return () => {
      document.removeEventListener('keydown', onKey)
      document.removeEventListener('mousedown', onPointerDown)
    }
  }, [contextMenu, closeContextMenu])

  useEffect(() => {
    if (contextMenu && menuRef.current) {
      menuRef.current.querySelector<HTMLButtonElement>('button')?.focus()
    }
  }, [contextMenu])

  const handleContextMenu = useCallback((e: React.MouseEvent, table: TableType) => {
    e.preventDefault()
    e.stopPropagation()
    setContextMenu({ table, x: e.clientX, y: e.clientY })
  }, [])

  const handleEdit = useCallback((table: TableType) => {
    setEditingTable(table)
    closeContextMenu()
  }, [closeContextMenu])

  const handleDelete = useCallback((table: TableType) => {
    closeContextMenu()
    setDeleteError(null)
    setDeletingTable(table)
  }, [closeContextMenu])

  const confirmDelete = useCallback(async () => {
    if (!deletingTable || !activeConnection) return
    setIsDeleting(true)
    setDeleteError(null)
    try {
      const client = createClient({
        host: activeConnection.host,
        port: activeConnection.port,
        apiKey: activeConnection.apiKey,
        useSsl: activeConnection.ssl,
      })
      await client.deleteTable(deletingTable.id)
      deleteTableFromStore(deletingTable.id)
      setDeletingTable(null)
    } catch (err) {
      setDeleteError(err instanceof Error ? err.message : 'Failed to delete table')
    } finally {
      setIsDeleting(false)
    }
  }, [deletingTable, activeConnection, deleteTableFromStore])

  const handleMenuKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault()
      const buttons = menuRef.current?.querySelectorAll<HTMLButtonElement>('button')
      if (!buttons) return
      const focused = document.activeElement as HTMLElement
      const idx = Array.from(buttons).indexOf(focused as HTMLButtonElement)
      const next = e.key === 'ArrowDown'
        ? buttons[(idx + 1) % buttons.length]
        : buttons[(idx - 1 + buttons.length) % buttons.length]
      next.focus()
    }
  }

  const fetchTables = useCallback(async () => {
    if (!activeConnection) return

    setLoadingTables(true)
    setTablesError(null)

    try {
      const client = createClient({
        host: activeConnection.host,
        port: activeConnection.port,
        apiKey: activeConnection.apiKey,
        useSsl: activeConnection.ssl,
      })

      const response = await client.listTables()
      const storeTables = response.tables.map(apiTableToStoreTable)
      setTables(storeTables)
    } catch (error) {
      console.error('Failed to fetch tables:', error)
      setTablesError(error instanceof Error ? error.message : 'Failed to fetch tables')
      setTables([])
    } finally {
      setLoadingTables(false)
    }
  }, [activeConnection, setLoadingTables, setTables, setTablesError])

  useEffect(() => {
    if (activeConnectionId && activeConnection) {
      fetchTables()
    } else {
      setTables([])
    }
  }, [activeConnectionId, activeConnection, fetchTables, setTables])

  // Refresh table list when ingestion completes
  useEffect(() => {
    const handler = () => fetchTables()
    window.addEventListener('reasondb:tables-changed', handler)
    return () => window.removeEventListener('reasondb:tables-changed', handler)
  }, [fetchTables])

  useEffect(() => {
    if (!activeConnection || tables.length === 0) return

    const { addMetadataFields } = useSchemaStore.getState()

    const fetchMetadataSchemas = async () => {
      const client = createClient({
        host: activeConnection.host,
        port: activeConnection.port,
        apiKey: activeConnection.apiKey,
        useSsl: activeConnection.ssl,
      })

      for (const table of tables) {
        try {
          const schemaResponse = await client.getTableMetadataSchema(table.id)
          if (schemaResponse.fields.length > 0) {
            addMetadataFields(table.name, schemaResponse.fields)
          }
        } catch {
          // Silently ignore - endpoint might not exist or table might be empty
        }
      }
    }

    const timer = setTimeout(fetchMetadataSchemas, 300)
    return () => clearTimeout(timer)
  }, [activeConnection, tables])

  useEffect(() => {
    if (!activeConnection) return

    const client = createClient({
      host: activeConnection.host,
      port: activeConnection.port,
      apiKey: activeConnection.apiKey,
      useSsl: activeConnection.ssl,
    })

    setValueFetcher(async (tableId: string, column: string) => {
      const response = await client.getColumnValues(tableId, column)
      return response.values.map(v => v.value)
    })
  }, [activeConnection])

  const filteredTables = tables.filter((t) =>
    t.name.toLowerCase().includes(searchQuery.toLowerCase())
  )

  const handleViewData = (tableId: string) => {
    selectTable(tableId)
  }

  if (!activeConnectionId) {
    return (
      <div className="flex flex-col items-center justify-center h-full p-6 text-center">
        <Table size={48} weight="duotone" className="text-overlay-0 mb-3" />
        <p className="text-sm text-subtext-0">Connect to view tables</p>
      </div>
    )
  }

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <div className="flex items-center justify-between px-3 py-2 border-b border-border">
        <span className="text-xs font-semibold text-overlay-1 uppercase tracking-wide">
          Tables ({tables.length})
        </span>
        <div className="flex items-center gap-1">
          <Button
            size="icon"
            variant="ghost"
            className="h-6 w-6"
            onClick={fetchTables}
            title="Refresh tables"
            disabled={isLoadingTables}
          >
            <ArrowClockwise size={14} className={isLoadingTables ? 'animate-spin' : ''} />
          </Button>
          <Button
            size="icon"
            variant="ghost"
            className="h-6 w-6"
            onClick={() => setShowCreateDialog(true)}
            title="Create table"
          >
            <Plus size={14} />
          </Button>
        </div>
      </div>

      {/* Search */}
      <div className="p-2">
        <div className="relative">
          <MagnifyingGlass
            size={14}
            className="absolute left-2.5 top-1/2 -translate-y-1/2 text-overlay-0"
          />
          <input
            type="text"
            placeholder="Filter tables..."
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            className={cn(
              'w-full pl-8 pr-3 py-1.5 text-xs rounded-md',
              'bg-surface-0 border border-border',
              'text-text placeholder-overlay-0',
              'focus:outline-none focus:ring-1 focus:ring-primary focus:border-transparent'
            )}
          />
        </div>
      </div>

      {/* Error state */}
      {tablesError && (
        <div className="px-3 py-2 mx-2 mb-2 rounded-md bg-red/10 border border-red/20">
          <p className="text-xs text-red">{tablesError}</p>
          <button
            onClick={fetchTables}
            className="text-xs text-red underline mt-1 hover:text-red/80"
          >
            Retry
          </button>
        </div>
      )}

      {/* Table list */}
      <div className="flex-1 overflow-auto">
        {isLoadingTables ? (
          <div className="flex items-center justify-center h-32">
            <div className="w-5 h-5 border-2 border-primary border-t-transparent rounded-full animate-spin" />
          </div>
        ) : filteredTables.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-32 text-center px-4">
            <Table size={32} weight="duotone" className="text-overlay-0 mb-2" />
            <p className="text-xs text-overlay-0">
              {searchQuery ? 'No tables match your search' : 'No tables found'}
            </p>
          </div>
        ) : (
          filteredTables.map((table) => (
            <TableItem
              key={table.id}
              table={table}
              isSelected={activeTableId === table.id}
              onSelect={() => selectTable(table.id)}
              onViewData={() => handleViewData(table.id)}
              onContextMenu={(e) => handleContextMenu(e, table)}
            />
          ))
        )}
      </div>

      <CreateTableDialog
        open={showCreateDialog}
        onOpenChange={setShowCreateDialog}
      />

      <EditTableDialog
        open={editingTable !== null}
        onOpenChange={(open) => { if (!open) setEditingTable(null) }}
        table={editingTable}
      />

      {/* Delete confirmation dialog */}
      <Dialog
        open={deletingTable !== null}
        onOpenChange={(open) => { if (!open) { setDeletingTable(null); setDeleteError(null) } }}
      >
        <DialogContent className="max-w-sm">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <Warning size={18} className="text-red" weight="fill" aria-hidden="true" />
              Delete Table
            </DialogTitle>
          </DialogHeader>
          <div className="py-2 space-y-3">
            <p className="text-sm text-subtext-0">
              Are you sure you want to delete{' '}
              <span className="font-semibold text-text">{deletingTable?.name}</span>?
              This will permanently remove the table and all its documents.
            </p>
            {deleteError && (
              <div className="rounded-md bg-red/10 border border-red/20 px-3 py-2">
                <p className="text-xs text-red">{deleteError}</p>
              </div>
            )}
          </div>
          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => { setDeletingTable(null); setDeleteError(null) }}
              disabled={isDeleting}
            >
              Cancel
            </Button>
            <Button
              variant="destructive"
              onClick={confirmDelete}
              disabled={isDeleting}
            >
              {isDeleting ? 'Deleting...' : 'Delete'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {contextMenu && (
        <div
          ref={menuRef}
          role="menu"
          aria-label={`Actions for ${contextMenu.table.name}`}
          className={cn(
            'fixed z-50 min-w-[160px] rounded-md border border-border',
            'bg-mantle shadow-lg py-1'
          )}
          style={{ left: contextMenu.x, top: contextMenu.y }}
          onKeyDown={handleMenuKeyDown}
        >
          <button
            role="menuitem"
            onClick={() => handleEdit(contextMenu.table)}
            className="w-full flex items-center gap-2 px-3 py-1.5 text-sm hover:bg-surface-0 text-left focus:bg-surface-0 focus:outline-none"
          >
            <Pencil size={14} aria-hidden="true" />
            Edit
          </button>
          <div className="h-px bg-border my-1" role="separator" />
          <button
            role="menuitem"
            onClick={() => handleDelete(contextMenu.table)}
            className="w-full flex items-center gap-2 px-3 py-1.5 text-sm hover:bg-surface-0 text-left text-red focus:bg-surface-0 focus:outline-none"
          >
            <Trash size={14} aria-hidden="true" />
            Delete
          </button>
        </div>
      )}
    </div>
  )
}
