import { useState, useEffect, useMemo } from 'react'
import {
  useReactTable,
  getCoreRowModel,
  getSortedRowModel,
  getPaginationRowModel,
  flexRender,
  type ColumnDef,
  type SortingState,
} from '@tanstack/react-table'
import {
  Table,
  CaretUp,
  CaretDown,
  CaretLeft,
  CaretRight,
  Plus,
  Trash,
  PencilSimple,
  Copy,
  Code,
  Rows,
  ArrowsClockwise,
  DownloadSimple,
  CheckCircle,
} from '@phosphor-icons/react'
import { useTableStore, type Document } from '@/stores/tableStore'
import { useFilterStore } from '@/stores/filterStore'
import { Button } from '@/components/ui/Button'
import { SearchBar, FilterBuilder } from '@/components/search'
import { cn } from '@/lib/utils'
import { filterDocuments } from '@/lib/filter-utils'
import { detectColumnType, type ColumnInfo } from '@/lib/filter-types'

// Mock documents for demo
function generateMockDocuments(tableId: string): Document[] {
  const count = Math.floor(Math.random() * 50) + 20
  return Array.from({ length: count }, (_, i) => ({
    id: `doc_${tableId}_${i + 1}`,
    data: {
      id: `${tableId}-${crypto.randomUUID().slice(0, 8)}`,
      title: `Document ${i + 1}`,
      content: `This is the content of document ${i + 1}. It contains some sample text for demonstration purposes.`,
      embedding: '[0.123, 0.456, 0.789, ...]',
      metadata: { tags: ['sample', 'demo'], priority: Math.floor(Math.random() * 5) + 1 },
      created_at: new Date(Date.now() - Math.random() * 30 * 24 * 60 * 60 * 1000).toISOString(),
    },
    metadata: {
      createdAt: new Date(Date.now() - Math.random() * 30 * 24 * 60 * 60 * 1000).toISOString(),
      updatedAt: new Date(Date.now() - Math.random() * 7 * 24 * 60 * 60 * 1000).toISOString(),
      version: Math.floor(Math.random() * 5) + 1,
    },
  }))
}

type ViewMode = 'table' | 'json' | 'card'

interface DocumentViewerProps {
  tableId: string
}

export function DocumentViewer({ tableId }: DocumentViewerProps) {
  const {
    documents,
    selectedDocumentId,
    isLoadingDocuments,
    totalDocuments,
    pageSize,
    setDocuments,
    selectDocument,
    setLoadingDocuments,
  } = useTableStore()

  const {
    activeFilter,
    setDetectedColumns,
  } = useFilterStore()

  const [viewMode, setViewMode] = useState<ViewMode>('table')
  const [sorting, setSorting] = useState<SortingState>([])
  const [copied, setCopied] = useState(false)

  // Load documents when table is selected
  useEffect(() => {
    if (tableId) {
      setLoadingDocuments(true)
      // Simulate API call
      setTimeout(() => {
        const docs = generateMockDocuments(tableId)
        setDocuments(docs, docs.length)
        setLoadingDocuments(false)
      }, 300)
    }
  }, [tableId, setDocuments, setLoadingDocuments])

  // Detect columns from documents
  const detectedColumns = useMemo<ColumnInfo[]>(() => {
    if (documents.length === 0) return []
    
    const sampleDoc = documents[0]
    const cols: ColumnInfo[] = []
    
    // Extract columns from data object
    const extractColumns = (obj: Record<string, unknown>, prefix: string) => {
      Object.entries(obj).forEach(([key, value]) => {
        const path = prefix ? `${prefix}.${key}` : key
        const type = detectColumnType(value)
        
        cols.push({ name: key, type, path })
        
        // Recursively extract nested object columns (one level)
        if (typeof value === 'object' && value !== null && !Array.isArray(value)) {
          Object.entries(value as Record<string, unknown>).forEach(([nestedKey, nestedValue]) => {
            cols.push({
              name: `${key}.${nestedKey}`,
              type: detectColumnType(nestedValue),
              path: `${path}.${nestedKey}`,
            })
          })
        }
      })
    }
    
    extractColumns(sampleDoc.data, 'data')
    
    return cols
  }, [documents])

  // Update filter store with detected columns
  useEffect(() => {
    setDetectedColumns(detectedColumns)
  }, [detectedColumns, setDetectedColumns])

  // Filter documents based on active filter
  const filteredDocuments = useMemo(() => {
    if (!activeFilter) return documents
    return filterDocuments(
      documents.map((doc) => ({ ...doc, ...doc.data })),
      activeFilter
    ).map((filtered) => documents.find((d) => d.id === filtered.id)!).filter(Boolean)
  }, [documents, activeFilter])

  // Generate columns from table schema or document keys
  const columns = useMemo<ColumnDef<Document>[]>(() => {
    if (documents.length === 0) return []
    
    const sampleDoc = documents[0]
    const keys = Object.keys(sampleDoc.data)
    
    return keys.map((key) => ({
      accessorKey: `data.${key}`,
      header: ({ column }) => (
        <button
          className="flex items-center gap-1 hover:text-text transition-colors font-medium"
          onClick={() => column.toggleSorting()}
        >
          {key}
          {column.getIsSorted() === 'asc' && <CaretUp size={12} weight="bold" />}
          {column.getIsSorted() === 'desc' && <CaretDown size={12} weight="bold" />}
        </button>
      ),
      cell: ({ row }) => {
        const value = row.original.data[key]
        return <CellRenderer value={value} />
      },
    }))
  }, [documents])

  const table = useReactTable({
    data: filteredDocuments,
    columns,
    state: { sorting },
    onSortingChange: setSorting,
    getCoreRowModel: getCoreRowModel(),
    getSortedRowModel: getSortedRowModel(),
    getPaginationRowModel: getPaginationRowModel(),
    initialState: { pagination: { pageSize } },
  })

  // Handle search
  const handleSearch = () => {
    // Search triggers filter update through the SearchBar component
  }

  const handleCopyDocument = async (doc: Document) => {
    await navigator.clipboard.writeText(JSON.stringify(doc.data, null, 2))
    setCopied(true)
    setTimeout(() => setCopied(false), 2000)
  }

  const handleRefresh = () => {
    if (tableId) {
      setLoadingDocuments(true)
      setTimeout(() => {
        const docs = generateMockDocuments(tableId)
        setDocuments(docs, docs.length)
        setLoadingDocuments(false)
      }, 300)
    }
  }

  if (!tableId) {
    return (
      <div className="flex flex-col items-center justify-center h-full bg-base text-center p-8">
        <Table size={64} weight="duotone" className="text-overlay-0 mb-4" />
        <h3 className="text-lg font-medium text-text mb-2">No Table Selected</h3>
        <p className="text-sm text-subtext-0 max-w-sm">
          Select a table from the sidebar to view and manage its documents
        </p>
      </div>
    )
  }

  return (
    <div className="flex flex-col h-full bg-base">
      {/* Toolbar */}
      <div className="flex items-center gap-3 px-4 py-2 border-b border-border bg-mantle">
        {/* Table icon */}
        <div className="flex items-center gap-2 shrink-0">
          <Table size={18} weight="duotone" className="text-mauve" />
        </div>

        {/* Search - takes remaining space */}
        <SearchBar
          columns={detectedColumns}
          placeholder="Search... (e.g., title = &quot;doc&quot; or content contains &quot;text&quot;)"
          onSearch={handleSearch}
        />

        <div className="h-5 w-px bg-border shrink-0" />

        {/* View mode toggle */}
        <div className="flex items-center bg-surface-0 rounded-md p-0.5 shrink-0">
          <button
            onClick={() => setViewMode('table')}
            className={cn(
              'p-1.5 rounded transition-colors',
              viewMode === 'table' ? 'bg-surface-1 text-text' : 'text-overlay-0 hover:text-text'
            )}
            title="Table view"
          >
            <Rows size={16} />
          </button>
          <button
            onClick={() => setViewMode('json')}
            className={cn(
              'p-1.5 rounded transition-colors',
              viewMode === 'json' ? 'bg-surface-1 text-text' : 'text-overlay-0 hover:text-text'
            )}
            title="JSON view"
          >
            <Code size={16} />
          </button>
        </div>

        <div className="h-5 w-px bg-border shrink-0" />

        {/* Actions */}
        <div className="flex items-center gap-1 shrink-0">
          <Button size="sm" variant="ghost" onClick={handleRefresh} title="Refresh">
            <ArrowsClockwise size={16} className={isLoadingDocuments ? 'animate-spin' : ''} />
          </Button>
          
          <Button size="sm" variant="ghost" title="Export">
            <DownloadSimple size={16} />
          </Button>

          <Button size="sm" variant="ghost" className="gap-1.5">
            <Plus size={14} />
            Add
          </Button>
        </div>
      </div>

      {/* Filter Builder */}
      <FilterBuilder columns={detectedColumns} onApply={() => {}} />

      {/* Content */}
      <div className="flex-1 min-h-0 overflow-auto">
        {isLoadingDocuments ? (
          <div className="flex items-center justify-center h-full">
            <div className="w-6 h-6 border-2 border-primary border-t-transparent rounded-full animate-spin" />
          </div>
        ) : viewMode === 'table' ? (
          <table className="w-full text-sm">
            <thead className="sticky top-0 bg-mantle border-b border-border z-10">
              {table.getHeaderGroups().map((headerGroup) => (
                <tr key={headerGroup.id}>
                  {headerGroup.headers.map((header) => (
                    <th
                      key={header.id}
                      className="px-4 py-3 text-left text-xs font-medium text-subtext-0 uppercase tracking-wide"
                    >
                      {header.isPlaceholder
                        ? null
                        : flexRender(header.column.columnDef.header, header.getContext())}
                    </th>
                  ))}
                  <th className="px-4 py-3 w-24 text-right text-xs font-medium text-subtext-0 uppercase tracking-wide">
                    Actions
                  </th>
                </tr>
              ))}
            </thead>
              <tbody>
              {table.getRowModel().rows.map((row, idx) => (
                <tr
                  key={row.id}
                  onClick={() => selectDocument(row.original.id)}
                  className={cn(
                    'border-b border-border/50 cursor-pointer transition-colors group',
                    selectedDocumentId === row.original.id
                      ? 'bg-mauve/10'
                      : idx % 2 === 0
                      ? 'bg-base hover:bg-surface-0/50'
                      : 'bg-mantle/30 hover:bg-surface-0/50'
                  )}
                >
                  {row.getVisibleCells().map((cell) => (
                    <td key={cell.id} className="px-4 py-2 max-w-[200px]">
                      <div className="truncate">
                        {flexRender(cell.column.columnDef.cell, cell.getContext())}
                      </div>
                    </td>
                  ))}
                  <td className="px-4 py-2">
                    <div className="flex items-center justify-end gap-1">
                      <button
                        onClick={(e) => {
                          e.stopPropagation()
                          handleCopyDocument(row.original)
                        }}
                        className="p-1 hover:bg-surface-1 rounded text-overlay-0 hover:text-text"
                        title="Copy JSON"
                      >
                        {copied ? <CheckCircle size={14} className="text-green" /> : <Copy size={14} />}
                      </button>
                      <button
                        onClick={(e) => e.stopPropagation()}
                        className="p-1 hover:bg-surface-1 rounded text-overlay-0 hover:text-text"
                        title="Edit"
                      >
                        <PencilSimple size={14} />
                      </button>
                      <button
                        onClick={(e) => e.stopPropagation()}
                        className="p-1 hover:bg-surface-1 rounded text-overlay-0 hover:text-red"
                        title="Delete"
                      >
                        <Trash size={14} />
                      </button>
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        ) : (
          <div className="p-4 space-y-2">
            {documents.map((doc) => (
              <div
                key={doc.id}
                onClick={() => selectDocument(doc.id)}
                className={cn(
                  'p-3 rounded-lg border cursor-pointer transition-colors',
                  selectedDocumentId === doc.id
                    ? 'border-mauve bg-mauve/5'
                    : 'border-border bg-surface-0/50 hover:border-overlay-0'
                )}
              >
                <pre className="text-xs font-mono text-text overflow-auto max-h-48">
                  {JSON.stringify(doc.data, null, 2)}
                </pre>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Footer */}
      {viewMode === 'table' && (
        <div className="flex items-center justify-between px-4 py-2 border-t border-border bg-mantle">
          <div className="text-xs text-subtext-0">
            {activeFilter ? (
              <>
                <span className="font-medium text-mauve">{filteredDocuments.length.toLocaleString()}</span>
                <span className="text-overlay-0"> of </span>
                <span className="text-text">{totalDocuments.toLocaleString()}</span>
                <span> matching</span>
              </>
            ) : (
              <>
                <span className="font-medium text-text">{totalDocuments.toLocaleString()}</span> rows
              </>
            )}
            {table.getPageCount() > 1 && (
              <span className="ml-2 text-overlay-0">
                · Showing {table.getState().pagination.pageIndex * pageSize + 1}-
                {Math.min((table.getState().pagination.pageIndex + 1) * pageSize, filteredDocuments.length)}
              </span>
            )}
          </div>
          {table.getPageCount() > 1 && (
            <div className="flex items-center gap-1">
              <Button
                size="icon"
                variant="ghost"
                onClick={() => table.previousPage()}
                disabled={!table.getCanPreviousPage()}
                className="h-7 w-7"
              >
                <CaretLeft size={14} />
              </Button>
              <span className="text-xs text-subtext-0 px-2">
                Page {table.getState().pagination.pageIndex + 1} of {table.getPageCount()}
              </span>
              <Button
                size="icon"
                variant="ghost"
                onClick={() => table.nextPage()}
                disabled={!table.getCanNextPage()}
                className="h-7 w-7"
              >
                <CaretRight size={14} />
              </Button>
            </div>
          )}
        </div>
      )}
    </div>
  )
}

function CellRenderer({ value }: { value: unknown }) {
  if (value === null) return <span className="text-overlay-0 italic">null</span>
  if (value === undefined) return <span className="text-overlay-0 italic">—</span>
  
  if (typeof value === 'boolean') {
    return <span className={value ? 'text-green' : 'text-red'}>{String(value)}</span>
  }
  
  if (typeof value === 'number') {
    return <span className="text-peach font-mono">{value}</span>
  }
  
  if (typeof value === 'object') {
    const str = JSON.stringify(value)
    if (str.length > 50) {
      return (
        <span className="text-blue font-mono text-xs" title={str}>
          {str.slice(0, 50)}...
        </span>
      )
    }
    return <span className="text-blue font-mono text-xs">{str}</span>
  }
  
  const strValue = String(value)
  
  // Check if it's a date
  if (strValue.match(/^\d{4}-\d{2}-\d{2}/)) {
    return <span className="text-sky">{new Date(strValue).toLocaleString()}</span>
  }
  
  // Check if it's a vector representation
  if (strValue.startsWith('[') && strValue.includes('...')) {
    return <span className="text-teal font-mono text-xs">{strValue}</span>
  }
  
  // Truncate long strings
  if (strValue.length > 100) {
    return (
      <span className="block max-w-[300px] truncate" title={strValue}>
        {strValue}
      </span>
    )
  }
  
  return <span>{strValue}</span>
}
