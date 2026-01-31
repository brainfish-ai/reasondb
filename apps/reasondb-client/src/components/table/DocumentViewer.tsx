import { useState, useEffect, useMemo, useCallback } from 'react'
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
import { useConnectionStore } from '@/stores/connectionStore'
import { useFilterStore } from '@/stores/filterStore'
import { Button } from '@/components/ui/Button'
import { SearchBar, FilterBuilder } from '@/components/search'
import { cn } from '@/lib/utils'
import { filterDocuments } from '@/lib/filter-utils'
import { detectColumnType, type ColumnInfo } from '@/lib/filter-types'
import { createClient, type TableDocumentSummary } from '@/lib/api'

// Convert API response to document store format
function apiDocumentToStoreDocument(apiDoc: TableDocumentSummary): Document {
  // Spread custom metadata into the data object for display
  const customMetadata = apiDoc.metadata || {}
  
  return {
    id: apiDoc.id,
    data: {
      id: apiDoc.id,
      title: apiDoc.title,
      total_nodes: apiDoc.total_nodes,
      tags: apiDoc.tags,
      ...customMetadata, // Include custom metadata fields
      created_at: apiDoc.created_at,
    },
    metadata: {
      createdAt: apiDoc.created_at,
      updatedAt: apiDoc.created_at,
      version: 1,
    },
  }
}

type ViewMode = 'table' | 'json' | 'card'

interface DocumentViewerProps {
  tableId: string
}

export function DocumentViewer({ tableId }: DocumentViewerProps) {
  const { activeConnectionId, connections } = useConnectionStore()
  const {
    documents,
    selectedDocumentId,
    isLoadingDocuments,
    totalDocuments,
    pageSize,
    documentsError,
    setDocuments,
    selectDocument,
    setLoadingDocuments,
    setDocumentsError,
  } = useTableStore()

  const {
    activeFilter,
    setDetectedColumns,
    quickSearchText,
  } = useFilterStore()

  const [viewMode, setViewMode] = useState<ViewMode>('table')
  const [sorting, setSorting] = useState<SortingState>([])
  const [copied, setCopied] = useState(false)

  // Get active connection details
  const activeConnection = connections.find(c => c.id === activeConnectionId)

  // Fetch documents from server
  const fetchDocuments = useCallback(async () => {
    if (!activeConnection || !tableId) return

    setLoadingDocuments(true)
    setDocumentsError(null)
    // Clear documents at the start of fetch to avoid showing stale data
    setDocuments([], 0)

    try {
      const client = createClient({
        host: activeConnection.host,
        port: activeConnection.port,
        apiKey: activeConnection.apiKey,
        useSsl: activeConnection.ssl,
      })

      const response = await client.getTableDocuments(tableId)
      const storeDocs = response.documents.map(apiDocumentToStoreDocument)
      setDocuments(storeDocs, response.total)
    } catch (error) {
      console.error('Failed to fetch documents:', error)
      setDocumentsError(error instanceof Error ? error.message : 'Failed to fetch documents')
      setDocuments([], 0)
    } finally {
      setLoadingDocuments(false)
    }
  }, [activeConnection, tableId, setLoadingDocuments, setDocuments, setDocumentsError])

  // Load documents when table is selected
  useEffect(() => {
    if (tableId && activeConnection) {
      fetchDocuments()
    }
  }, [tableId, activeConnection, fetchDocuments])

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

  // Filter documents based on active filter (client-side filtering for now)
  // TODO: Implement server-side search using the /api/v1/search endpoint
  const filteredDocuments = useMemo(() => {
    if (!activeFilter && !quickSearchText) return documents
    
    // If there's a quick search text but no structured filter, do simple text search
    if (!activeFilter && quickSearchText) {
      const searchLower = quickSearchText.toLowerCase()
      return documents.filter((doc) =>
        JSON.stringify(doc.data).toLowerCase().includes(searchLower)
      )
    }
    
    if (activeFilter) {
      // Filter using the full document structure
      return filterDocuments(documents as unknown as Record<string, unknown>[], activeFilter) as unknown as Document[]
    }
    
    return documents
  }, [documents, activeFilter, quickSearchText])

  // Generate columns from ALL document keys (to include custom metadata)
  const columns = useMemo<ColumnDef<Document>[]>(() => {
    if (documents.length === 0) return []
    
    // Standard fields in preferred order
    const standardFields = ['id', 'title', 'total_nodes', 'tags', 'created_at']
    
    // Collect all unique keys from all documents
    const allKeys = new Set<string>()
    documents.forEach(doc => {
      Object.keys(doc.data).forEach(key => allKeys.add(key))
    })
    
    // Sort keys: standard fields first (in order), then custom metadata fields alphabetically
    const sortedKeys = Array.from(allKeys).sort((a, b) => {
      const aIndex = standardFields.indexOf(a)
      const bIndex = standardFields.indexOf(b)
      
      // Both are standard fields - sort by their standard order
      if (aIndex !== -1 && bIndex !== -1) return aIndex - bIndex
      // Only a is standard - it comes first
      if (aIndex !== -1) return -1
      // Only b is standard - it comes first
      if (bIndex !== -1) return 1
      // Both are custom - sort alphabetically
      return a.localeCompare(b)
    })
    
    return sortedKeys.map((key) => ({
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

  // Handle search - for server-side search we'd call the API
  const handleSearch = useCallback(async (searchText: string) => {
    if (!activeConnection || !tableId || !searchText.trim()) {
      // If search is cleared, just refetch all documents
      fetchDocuments()
      return
    }

    // TODO: Use server-side search when implemented
    // For now, the filtering is done client-side in filteredDocuments
    // 
    // Server-side search would look like:
    // const client = createClient({ ... })
    // const results = await client.search({
    //   query: searchText,
    //   table_id: tableId,
    //   limit: pageSize,
    // })
    // setDocuments(results.map(r => apiToStoreDoc(r)), results.length)
  }, [activeConnection, tableId, fetchDocuments])

  const handleCopyDocument = async (doc: Document) => {
    await navigator.clipboard.writeText(JSON.stringify(doc.data, null, 2))
    setCopied(true)
    setTimeout(() => setCopied(false), 2000)
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
          <Button size="sm" variant="ghost" onClick={fetchDocuments} title="Refresh">
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

      {/* Error state */}
      {documentsError && (
        <div className="px-4 py-3 mx-4 mt-2 rounded-md bg-red/10 border border-red/20">
          <p className="text-sm text-red">{documentsError}</p>
          <button 
            onClick={fetchDocuments}
            className="text-sm text-red underline mt-1 hover:text-red/80"
          >
            Retry
          </button>
        </div>
      )}

      {/* Content */}
      <div className="flex-1 min-h-0 overflow-auto">
        {isLoadingDocuments ? (
          <div className="flex items-center justify-center h-full">
            <div className="w-6 h-6 border-2 border-primary border-t-transparent rounded-full animate-spin" />
          </div>
        ) : documents.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-full text-center p-8">
            <Table size={48} weight="duotone" className="text-overlay-0 mb-3" />
            <p className="text-sm text-subtext-0">No documents in this table</p>
            <Button size="sm" variant="secondary" className="mt-4 gap-1.5">
              <Plus size={14} />
              Add Document
            </Button>
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
            {filteredDocuments.map((doc) => (
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
      {viewMode === 'table' && documents.length > 0 && (
        <div className="flex items-center justify-between px-4 py-2 border-t border-border bg-mantle">
          <div className="text-xs text-subtext-0">
            {activeFilter || quickSearchText ? (
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
  
  if (Array.isArray(value)) {
    if (value.length === 0) return <span className="text-overlay-0 italic">[]</span>
    const str = value.join(', ')
    if (str.length > 50) {
      return (
        <span className="text-blue font-mono text-xs" title={str}>
          [{str.slice(0, 50)}...]
        </span>
      )
    }
    return <span className="text-blue font-mono text-xs">[{str}]</span>
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
