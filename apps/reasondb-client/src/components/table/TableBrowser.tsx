import { useState, useEffect, useCallback } from 'react'
import {
  Table,
  Plus,
  DotsThree,
  MagnifyingGlass,
  Eye,
  ArrowClockwise,
} from '@phosphor-icons/react'
import { useTableStore, type Table as TableType } from '@/stores/tableStore'
import { useConnectionStore } from '@/stores/connectionStore'
import { Button } from '@/components/ui/Button'
import { cn } from '@/lib/utils'
import { createClient, type TableSummary } from '@/lib/api'
import { setValueFetcher, useSchemaStore } from '@/lib/rql-language'
import { CreateTableDialog } from '@/components/table/CreateTableDialog'

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
}

function TableItem({ table, isSelected, onSelect, onViewData }: TableItemProps) {
  return (
    <div className={cn('border-b border-border/30', isSelected && 'bg-surface-0/50')}>
      <div
        className={cn(
          'flex items-center gap-2 px-3 py-2 cursor-pointer',
          'hover:bg-surface-0/50 transition-colors group'
        )}
        onClick={onSelect}
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
            onClick={(e) => e.stopPropagation()}
            className="p-1 hover:bg-surface-1 rounded text-overlay-0 hover:text-text"
          >
            <DotsThree size={14} weight="bold" />
          </button>
        </div>
      </div>

      {isSelected && table.description && (
        <div className="px-3 pb-2 pl-8">
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
    selectedTableId,
    setTables,
    selectTable,
    isLoadingTables,
    setLoadingTables,
    setTablesError,
    tablesError,
  } = useTableStore()

  const [searchQuery, setSearchQuery] = useState('')
  const [showCreateDialog, setShowCreateDialog] = useState(false)

  const activeConnection = connections.find(c => c.id === activeConnectionId)

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
              isSelected={selectedTableId === table.id}
              onSelect={() => selectTable(table.id)}
              onViewData={() => handleViewData(table.id)}
            />
          ))
        )}
      </div>

      <CreateTableDialog
        open={showCreateDialog}
        onOpenChange={setShowCreateDialog}
      />
    </div>
  )
}
