import { useState, useEffect, useCallback } from 'react'
import {
  Table,
  CaretRight,
  CaretDown,
  Columns,
  Key,
  Hash,
  TextT,
  Calendar,
  ToggleLeft,
  ListNumbers,
  Plus,
  DotsThree,
  MagnifyingGlass,
  Eye,
  ArrowClockwise,
} from '@phosphor-icons/react'
import { useTableStore, type Table as TableType, type TableColumn } from '@/stores/tableStore'
import { useConnectionStore } from '@/stores/connectionStore'
import { Button } from '@/components/ui/Button'
import { cn } from '@/lib/utils'
import { createClient, type TableSummary } from '@/lib/api'
import { setValueFetcher, useSchemaStore } from '@/lib/rql-language'

function getTypeIcon(type: string) {
  const lowerType = type.toLowerCase()
  if (lowerType.includes('uuid') || lowerType.includes('id')) return Key
  if (lowerType.includes('text') || lowerType.includes('varchar') || lowerType.includes('char')) return TextT
  if (lowerType.includes('int') || lowerType.includes('numeric') || lowerType.includes('decimal')) return ListNumbers
  if (lowerType.includes('timestamp') || lowerType.includes('date') || lowerType.includes('time')) return Calendar
  if (lowerType.includes('bool')) return ToggleLeft
  if (lowerType.includes('vector')) return Hash
  return Columns
}

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B'
  const k = 1024
  const sizes = ['B', 'KB', 'MB', 'GB']
  const i = Math.floor(Math.log(bytes) / Math.log(k))
  return `${parseFloat((bytes / Math.pow(k, i)).toFixed(1))} ${sizes[i]}`
}

// Standard document fields in ReasonDB
const DOCUMENT_FIELDS: TableColumn[] = [
  { name: 'id', type: 'uuid', nullable: false, primaryKey: true },
  { name: 'title', type: 'text', nullable: false, primaryKey: false },
  { name: 'total_nodes', type: 'integer', nullable: false, primaryKey: false },
  { name: 'tags', type: 'text[]', nullable: true, primaryKey: false },
  { name: 'metadata', type: 'jsonb', nullable: true, primaryKey: false, description: 'Custom key-value pairs' },
  { name: 'created_at', type: 'timestamp', nullable: false, primaryKey: false },
]

// Convert API response to table store format
function apiTableToStoreTable(apiTable: TableSummary): TableType {
  return {
    id: apiTable.id,
    name: apiTable.name,
    schema: 'default',
    columns: DOCUMENT_FIELDS, // Standard document fields
    indexes: [],
    rowCount: apiTable.document_count,
    sizeBytes: 0,
    createdAt: new Date().toISOString(),
    updatedAt: new Date().toISOString(),
    description: apiTable.description || '',
  }
}

interface TableItemProps {
  table: TableType
  isSelected: boolean
  onSelect: () => void
  onViewData: () => void
}

function TableItem({ table, isSelected, onSelect, onViewData }: TableItemProps) {
  const [isExpanded, setIsExpanded] = useState(false)
  const [showMenu, setShowMenu] = useState(false)

  return (
    <div className={cn('border-b border-border/30', isSelected && 'bg-surface-0/50')}>
      {/* Table header */}
      <div
        className={cn(
          'flex items-center gap-2 px-3 py-2 cursor-pointer',
          'hover:bg-surface-0/50 transition-colors group'
        )}
        onClick={() => {
          onSelect()
          setIsExpanded(!isExpanded)
        }}
      >
        <button className="p-0.5 hover:bg-surface-1 rounded">
          {isExpanded ? (
            <CaretDown size={12} weight="bold" className="text-overlay-0" />
          ) : (
            <CaretRight size={12} weight="bold" className="text-overlay-0" />
          )}
        </button>
        
        <Table 
          size={16} 
          weight={isSelected ? 'fill' : 'duotone'} 
          className={isSelected ? 'text-mauve' : 'text-overlay-1'} 
        />
        
        <span className={cn('flex-1 text-sm truncate', isSelected ? 'text-text font-medium' : 'text-subtext-0')}>
          {table.name}
        </span>
        
        <span className="text-xs text-overlay-0">
          {table.rowCount.toLocaleString()} docs
        </span>

        {/* Actions */}
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
              setShowMenu(!showMenu)
            }}
            className="p-1 hover:bg-surface-1 rounded text-overlay-0 hover:text-text"
          >
            <DotsThree size={14} weight="bold" />
          </button>
        </div>
      </div>

      {/* Expanded columns */}
      {isExpanded && (
        <div className="pl-8 pr-3 pb-2 space-y-0.5">
          {table.columns.length > 0 ? (
            table.columns.map((col) => (
              <ColumnItem key={col.name} column={col} />
            ))
          ) : (
            <p className="text-xs text-overlay-0 py-2">{table.description}</p>
          )}
          
          {/* Table info */}
          {table.sizeBytes > 0 && (
            <div className="mt-2 pt-2 border-t border-border/30 flex items-center gap-4 text-xs text-overlay-0">
              <span>{formatBytes(table.sizeBytes)}</span>
              <span>{table.indexes.length} indexes</span>
            </div>
          )}
        </div>
      )}
    </div>
  )
}

function ColumnItem({ column }: { column: TableColumn }) {
  const TypeIcon = getTypeIcon(column.type)
  
  return (
    <div className="flex items-center gap-2 py-1 text-xs group">
      <TypeIcon 
        size={12} 
        weight="duotone" 
        className={column.primaryKey ? 'text-yellow' : 'text-overlay-0'} 
      />
      <span className={cn('flex-1', column.primaryKey ? 'text-text font-medium' : 'text-subtext-0')}>
        {column.name}
        {column.primaryKey && <Key size={10} weight="fill" className="inline ml-1 text-yellow" />}
      </span>
      <span className="text-overlay-0 font-mono">{column.type}</span>
      {column.nullable && <span className="text-overlay-0 italic">null</span>}
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

  // Get active connection details
  const activeConnection = connections.find(c => c.id === activeConnectionId)

  // Fetch tables from server
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
      // Metadata schema fetching is handled by the separate useEffect below
    } catch (error) {
      console.error('Failed to fetch tables:', error)
      setTablesError(error instanceof Error ? error.message : 'Failed to fetch tables')
      setTables([])
    } finally {
      setLoadingTables(false)
    }
  }, [activeConnection, setLoadingTables, setTables, setTablesError])

  // Load tables when connected
  useEffect(() => {
    if (activeConnectionId && activeConnection) {
      fetchTables()
    } else {
      setTables([])
    }
  }, [activeConnectionId, activeConnection, fetchTables, setTables])

  // Fetch metadata schema when tables are available (for autocompletion)
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

    // Small delay to let QueryEditor set base schema first
    const timer = setTimeout(fetchMetadataSchemas, 300)
    return () => clearTimeout(timer)
  }, [activeConnection, tables])

  // Set up value fetcher for autocompletion
  useEffect(() => {
    if (!activeConnection) return

    const client = createClient({
      host: activeConnection.host,
      port: activeConnection.port,
      apiKey: activeConnection.apiKey,
      useSsl: activeConnection.ssl,
    })

    // Set up value fetcher (store handles table ID resolution)
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
    // This would open the document viewer for this table
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
    </div>
  )
}
