import { useState, useMemo, useCallback } from 'react'
import {
  useReactTable,
  getCoreRowModel,
  getSortedRowModel,
  getFilteredRowModel,
  getPaginationRowModel,
  type ColumnDef,
  type SortingState,
} from '@tanstack/react-table'
import {
  CaretUp,
  CaretDown,
  BracketsCurly,
  TreeStructure,
} from '@phosphor-icons/react'
import { cn } from '@/lib/utils'
import { DataTable } from './DataTable'
import { TableToolbar } from './TableToolbar'
import { TablePagination } from './TablePagination'
import { JsonDetailSidebar } from '@/components/table/JsonDetailSidebar'
import { useConnectionStore } from '@/stores/connectionStore'
import { createClient } from '@/lib/api'

// ==================== Types ====================

export interface RecordTableProps {
  /** Records to display */
  records: Record<string, unknown>[]
  /** Column names to display (auto-detected if not provided) */
  columns?: string[]
  /** Total record count (for server-side pagination) */
  totalCount?: number
  /** Execution time in ms (shown in toolbar) */
  executionTime?: number
  /** Whether this is a query result (affects toolbar styling) */
  isQueryResult?: boolean
  /** Page size */
  pageSize?: number
  /** Custom class */
  className?: string
}

interface SidebarState {
  isOpen: boolean
  title: string
  path: string
  data: unknown
  isLoading: boolean
}

// Preferred column order for regular queries
const COLUMN_ORDER = ['id', 'title', 'total_nodes', 'tags', 'metadata', 'created_at']

// Preferred column order when REASON fields are present (answer first)
const REASON_COLUMN_ORDER = ['title', 'answer', 'confidence', 'id', 'total_nodes', 'tags', 'metadata', 'created_at']

// Columns always excluded
const BASE_EXCLUDED = ['table_id', 'score', 'highlights']

// Additional columns excluded only when NOT a REASON result
const NON_REASON_EXCLUDED = ['answer', 'confidence']

// ==================== Cell Renderers ====================

function SortableHeader({
  column,
  label,
}: {
  column: { toggleSorting: () => void; getIsSorted: () => false | 'asc' | 'desc' }
  label: string
}) {
  const displayLabels: Record<string, string> = {
    total_nodes: 'content',
    created_at: 'created_at',
  }
  const displayLabel = displayLabels[label] ?? label
  
  return (
    <button
      className="flex items-center gap-1 hover:text-text transition-colors font-medium"
      onClick={() => column.toggleSorting()}
    >
      {displayLabel}
      {column.getIsSorted() === 'asc' && <CaretUp size={12} weight="bold" />}
      {column.getIsSorted() === 'desc' && <CaretDown size={12} weight="bold" />}
    </button>
  )
}

interface CellRendererProps {
  columnName: string
  value: unknown
  row: Record<string, unknown>
  onMetadataClick: (row: Record<string, unknown>) => void
  onContentClick: (row: Record<string, unknown>) => void
}

function CellRenderer({ columnName, value, row, onMetadataClick, onContentClick }: CellRendererProps) {
  // ID column - monospace, muted
  if (columnName === 'id') {
    return (
      <span className="font-mono text-xs text-overlay-1">
        {String(value || '')}
      </span>
    )
  }

  // Title column - bold
  if (columnName === 'title') {
    return (
      <span className="font-medium text-text">
        {String(value || '')}
      </span>
    )
  }

  // Tags column - comma-separated badges
  if (columnName === 'tags') {
    const tags = value as string[] | undefined
    if (!tags || tags.length === 0) {
      return <span className="text-overlay-0 italic">—</span>
    }
    const displayTags = tags.slice(0, 3)
    const remaining = tags.length - 3
    return (
      <span className="text-blue font-mono text-xs">
        {displayTags.join(', ')}{remaining > 0 ? ` +${remaining}` : ''}
      </span>
    )
  }

  // Metadata column - clickable JSON badge
  if (columnName === 'metadata') {
    const metadata = value as Record<string, unknown> | undefined
    if (!metadata || Object.keys(metadata).length === 0) {
      return <span className="text-overlay-0 italic">—</span>
    }
    const keys = Object.keys(metadata)
    const preview = keys.slice(0, 2).join(', ')
    const hasMore = keys.length > 2
    return (
      <button
        onClick={(e) => {
          e.stopPropagation()
          onMetadataClick(row)
        }}
        className={cn(
          'inline-flex items-center gap-1 px-1.5 rounded',
          'bg-mauve/10 hover:bg-mauve/20 text-mauve transition-colors',
          'font-mono text-xs'
        )}
        title="Click to view metadata"
      >
        <BracketsCurly size={11} className="shrink-0" />
        <span className="truncate max-w-[120px]">
          {preview}{hasMore ? ` +${keys.length - 2}` : ''}
        </span>
      </button>
    )
  }

  // Total nodes column - clickable tree badge (shown as "content")
  if (columnName === 'total_nodes') {
    const totalNodes = value as number
    return (
      <button
        onClick={(e) => {
          e.stopPropagation()
          onContentClick(row)
        }}
        className={cn(
          'inline-flex items-center gap-1 px-1.5 rounded',
          'bg-teal/10 hover:bg-teal/20 text-teal transition-colors',
          'font-mono text-xs'
        )}
        title="Click to view document tree"
      >
        <TreeStructure size={11} className="shrink-0" />
        <span>{totalNodes} nodes</span>
      </button>
    )
  }

  // Answer column (REASON) - the extracted answer text
  if (columnName === 'answer') {
    if (!value) return <span className="text-overlay-0 italic">—</span>
    const text = String(value)
    return (
      <span className="text-sm text-text leading-snug" title={text}>
        {text.length > 200 ? text.slice(0, 200) + '...' : text}
      </span>
    )
  }

  // Confidence column (REASON) - percentage badge
  if (columnName === 'confidence') {
    if (value == null) return <span className="text-overlay-0 italic">—</span>
    const pct = Math.round((value as number) * 100)
    const color = pct >= 70 ? 'text-green' : pct >= 40 ? 'text-yellow' : 'text-overlay-1'
    return <span className={`font-mono text-xs ${color}`}>{pct}%</span>
  }

  // Date columns - formatted and colored
  if (columnName === 'created_at' || columnName.endsWith('_at')) {
    if (!value) return <span className="text-overlay-0 italic">—</span>
    return (
      <span className="text-sky text-sm">
        {new Date(value as string).toLocaleDateString()}
      </span>
    )
  }

  // Default rendering
  if (value === null) return <span className="text-overlay-0 italic">null</span>
  if (value === undefined) return <span className="text-overlay-0 italic">—</span>
  if (typeof value === 'boolean') {
    return <span className={value ? 'text-green' : 'text-red'}>{String(value)}</span>
  }
  if (typeof value === 'number') {
    return <span className="text-peach font-mono">{value}</span>
  }
  if (Array.isArray(value)) {
    return (
      <span className="text-overlay-0 font-mono text-xs">
        [{value.length} items]
      </span>
    )
  }
  if (typeof value === 'object') {
    const keys = Object.keys(value)
    return (
      <span className="text-overlay-0 font-mono text-xs">
        {`{${keys.length} fields}`}
      </span>
    )
  }
  const strValue = String(value)
  if (strValue.length > 100) {
    return (
      <span className="truncate max-w-[300px]" title={strValue}>
        {strValue}
      </span>
    )
  }
  return <span>{strValue}</span>
}

// ==================== Main Component ====================

export function RecordTable({
  records,
  columns: columnNames,
  totalCount,
  executionTime,
  isQueryResult = false,
  pageSize = 50,
  className,
}: RecordTableProps) {
  const [sorting, setSorting] = useState<SortingState>([])
  const [globalFilter, setGlobalFilter] = useState('')
  const [selectedRowId, setSelectedRowId] = useState<string | null>(null)
  const [sidebar, setSidebar] = useState<SidebarState>({
    isOpen: false,
    title: '',
    path: '',
    data: null,
    isLoading: false,
  })
  
  const { activeConnectionId, connections } = useConnectionStore()
  const activeConnection = connections.find((c) => c.id === activeConnectionId)

  // Close sidebar
  const closeSidebar = useCallback(() => {
    setSidebar(prev => ({ ...prev, isOpen: false }))
    setSelectedRowId(null)
  }, [])

  // Handle metadata click - show metadata in sidebar
  const handleMetadataClick = useCallback((row: Record<string, unknown>) => {
    const title = String(row.title || row.id || 'Record')
    const metadata = row.metadata as Record<string, unknown>
    
    setSidebar({
      isOpen: true,
      title: `${title} → metadata`,
      path: 'metadata',
      data: metadata,
      isLoading: false,
    })
  }, [])

  // Handle content click - load document tree
  const handleContentClick = useCallback(async (row: Record<string, unknown>) => {
    const docId = String(row.id || '')
    const title = String(row.title || row.id || 'Record')
    
    if (!activeConnection || !docId) return

    setSelectedRowId(docId)

    setSidebar({
      isOpen: true,
      title: `${title} → content`,
      path: 'document tree',
      data: null,
      isLoading: true,
    })

    try {
      const client = createClient({
        host: activeConnection.host,
        port: activeConnection.port,
        apiKey: activeConnection.apiKey,
        useSsl: activeConnection.ssl,
      })

      const tree = await client.getDocumentTree(docId)
      
      setSidebar({
        isOpen: true,
        title: `${title} → content`,
        path: 'document tree',
        data: tree,
        isLoading: false,
      })
    } catch (error) {
      console.error('Failed to load document tree:', error)
      setSidebar({
        isOpen: true,
        title: `${title} → content`,
        path: 'document tree',
        data: { error: error instanceof Error ? error.message : 'Failed to load document tree' },
        isLoading: false,
      })
    }
  }, [activeConnection])

  // Row click handler - opens content sidebar for the clicked row
  const handleRowClick = useCallback((row: Record<string, unknown>) => {
    handleContentClick(row)
  }, [handleContentClick])

  // Detect whether results contain REASON fields
  const isReasonResult = useMemo(() => {
    if (records.length === 0) return false
    return records.some(r => r.answer != null || r.confidence != null)
  }, [records])

  // Order and filter columns
  const orderedColumns = useMemo(() => {
    let availableColumns: string[]
    if (columnNames) {
      availableColumns = columnNames
    } else if (records.length > 0) {
      availableColumns = Object.keys(records[0])
    } else {
      return []
    }

    const excluded = isReasonResult
      ? BASE_EXCLUDED
      : [...BASE_EXCLUDED, ...NON_REASON_EXCLUDED]

    const filtered = availableColumns.filter(col => !excluded.includes(col))

    const preferredOrder = isReasonResult ? REASON_COLUMN_ORDER : COLUMN_ORDER
    const ordered: string[] = []
    const remaining: string[] = []

    for (const col of preferredOrder) {
      if (filtered.includes(col)) {
        ordered.push(col)
      }
    }

    for (const col of filtered) {
      if (!preferredOrder.includes(col)) {
        remaining.push(col)
      }
    }

    return [...ordered, ...remaining]
  }, [columnNames, records, isReasonResult])

  // Generate column definitions
  const columns = useMemo<ColumnDef<Record<string, unknown>>[]>(() => {
    return orderedColumns.map((col) => ({
      accessorKey: col,
      header: ({ column }) => <SortableHeader column={column} label={col} />,
      cell: ({ row, getValue }) => (
        <CellRenderer
          columnName={col}
          value={getValue()}
          row={row.original}
          onMetadataClick={handleMetadataClick}
          onContentClick={handleContentClick}
        />
      ),
    }))
  }, [orderedColumns, handleMetadataClick, handleContentClick])

  const table = useReactTable({
    data: records,
    columns,
    state: {
      sorting,
      globalFilter,
    },
    onSortingChange: setSorting,
    onGlobalFilterChange: setGlobalFilter,
    getCoreRowModel: getCoreRowModel(),
    getSortedRowModel: getSortedRowModel(),
    getFilteredRowModel: getFilteredRowModel(),
    getPaginationRowModel: getPaginationRowModel(),
    initialState: {
      pagination: { pageSize },
    },
  })

  const rowCount = totalCount ?? records.length

  return (
    <div className={cn('flex h-full bg-base', className)}>
      {/* Main table area */}
      <div className="flex flex-col flex-1 min-w-0">
        <TableToolbar
          rowCount={rowCount}
          filteredCount={table.getFilteredRowModel().rows.length}
          executionTime={executionTime}
          isQueryResult={isQueryResult}
          filterValue={globalFilter}
          onFilterChange={setGlobalFilter}
          filterPlaceholder="Filter records..."
          columns={orderedColumns}
          rows={records}
        />

        <div className="flex-1 overflow-auto">
          <DataTable
            table={table}
            onRowClick={handleRowClick}
            getRowId={(row) => String(row.id || '')}
            selectedRowId={selectedRowId}
          />
        </div>

        <TablePagination table={table} />
      </div>

      {/* Shared JSON Detail Sidebar */}
      <JsonDetailSidebar
        isOpen={sidebar.isOpen}
        onClose={closeSidebar}
        title={sidebar.title}
        path={sidebar.path}
        data={sidebar.data}
        isLoading={sidebar.isLoading}
      />
    </div>
  )
}
