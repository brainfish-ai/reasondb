import { useState, useMemo, useCallback } from 'react'
import {
  useReactTable,
  getCoreRowModel,
  getSortedRowModel,
  getPaginationRowModel,
  type SortingState,
} from '@tanstack/react-table'
import { FilterBuilder } from '@/components/search'
import { JsonDetailSidebar } from '../JsonDetailSidebar'

// Hooks
import { useDocuments, useColumnDetection, useDocumentFilter } from './hooks'

// Components
import {
  Toolbar,
  TableView,
  JsonView,
  Footer,
  LoadingState,
  EmptyState,
  ErrorState,
  NoTableState,
} from './components'

// Utils
import { createColumns } from './columns'

// Types
import type { ViewMode, SelectedCellData, DocumentViewerProps } from './types'

/**
 * DocumentViewer - Main component for displaying and managing table documents
 */
export function DocumentViewer({ tableId }: DocumentViewerProps) {
  // State
  const [viewMode, setViewMode] = useState<ViewMode>('table')
  const [sorting, setSorting] = useState<SortingState>([])
  const [copied, setCopied] = useState(false)
  const [selectedCell, setSelectedCell] = useState<SelectedCellData | null>(null)

  // Hooks
  const {
    documents,
    selectedDocumentId,
    isLoadingDocuments,
    totalDocuments,
    pageSize,
    documentsError,
    selectDocument,
    fetchDocuments,
    activeConnection,
  } = useDocuments(tableId)

  const detectedColumns = useColumnDetection(documents)
  
  const { filteredDocuments, isFiltered } = useDocumentFilter(documents)

  // Column definitions
  const columns = useMemo(
    () => createColumns({ onSelectCell: setSelectedCell }),
    []
  )

  // Table instance
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

  // Handlers
  const handleSearch = useCallback(
    async (searchText: string) => {
      if (!activeConnection || !tableId || !searchText.trim()) {
        fetchDocuments()
        return
      }
      // TODO: Implement server-side search
    },
    [activeConnection, tableId, fetchDocuments]
  )

  const handleCopyDocument = useCallback(async (doc: { data: unknown }) => {
    await navigator.clipboard.writeText(JSON.stringify(doc.data, null, 2))
    setCopied(true)
    setTimeout(() => setCopied(false), 2000)
  }, [])

  // Early return for no table selected
  if (!tableId) {
    return <NoTableState />
  }

  return (
    <div className="flex h-full bg-base">
      {/* Main content area */}
      <div className="flex flex-col flex-1 min-w-0">
        {/* Toolbar */}
        <Toolbar
          columns={detectedColumns}
          viewMode={viewMode}
          isLoading={isLoadingDocuments}
          onViewModeChange={setViewMode}
          onRefresh={fetchDocuments}
          onSearch={handleSearch}
        />

        {/* Filter Builder */}
        <FilterBuilder columns={detectedColumns} onApply={() => {}} />

        {/* Error State */}
        {documentsError && (
          <ErrorState message={documentsError} onRetry={fetchDocuments} />
        )}

        {/* Content */}
        <div className="flex-1 min-h-0 overflow-auto">
          {isLoadingDocuments ? (
            <LoadingState />
          ) : documents.length === 0 ? (
            <EmptyState />
          ) : viewMode === 'table' ? (
            <TableView
              table={table}
              selectedDocumentId={selectedDocumentId}
              copied={copied}
              onSelectDocument={selectDocument}
              onCopyDocument={handleCopyDocument}
            />
          ) : (
            <JsonView
              documents={filteredDocuments}
              selectedDocumentId={selectedDocumentId}
              onSelectDocument={selectDocument}
            />
          )}
        </div>

        {/* Footer */}
        {viewMode === 'table' && documents.length > 0 && (
          <Footer
            table={table}
            totalDocuments={totalDocuments}
            filteredCount={filteredDocuments.length}
            pageSize={pageSize}
            isFiltered={isFiltered}
          />
        )}
      </div>

      {/* JSON Detail Sidebar - always render for animation */}
      <JsonDetailSidebar
        isOpen={selectedCell !== null}
        onClose={() => setSelectedCell(null)}
        title={selectedCell?.title ?? ''}
        path={selectedCell?.path}
        data={selectedCell?.data}
      />
    </div>
  )
}
