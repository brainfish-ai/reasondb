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
import { createClient } from '@/lib/api'
import type { Document } from '@/stores/tableStore'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from '@/components/ui/Dialog'
import { Button } from '@/components/ui/Button'

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
  const [copiedId, setCopiedId] = useState<string | null>(null)
  const [selectedCell, setSelectedCell] = useState<SelectedCellData | null>(null)
  const [deleteTarget, setDeleteTarget] = useState<Document | null>(null)
  const [isDeleting, setIsDeleting] = useState(false)

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

  // Load document content (tree structure)
  const handleLoadContent = useCallback(
    async (documentId: string, documentTitle: string) => {
      if (!activeConnection) return

      // Show loading state
      setSelectedCell({
        title: `${documentTitle} → content`,
        path: 'document tree',
        data: { loading: true },
        isLoading: true,
      })

      try {
        const client = createClient({
          host: activeConnection.host,
          port: activeConnection.port,
          apiKey: activeConnection.apiKey,
          useSsl: activeConnection.ssl,
        })

        const tree = await client.getDocumentTree(documentId)
        
        setSelectedCell({
          title: `${documentTitle} → content`,
          path: 'document tree',
          data: tree,
        })
      } catch (error) {
        console.error('Failed to load document tree:', error)
        setSelectedCell({
          title: `${documentTitle} → content`,
          path: 'document tree',
          data: { error: error instanceof Error ? error.message : 'Failed to load' },
        })
      }
    },
    [activeConnection]
  )

  // Column definitions
  const columns = useMemo(
    () => createColumns({ 
      onSelectCell: setSelectedCell,
      onLoadContent: handleLoadContent,
    }),
    [handleLoadContent]
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
  // Create value fetcher for autocomplete
  const valueFetcher = useCallback(
    async (column: string): Promise<string[]> => {
      if (!activeConnection || !tableId) return []
      
      try {
        const client = createClient({
          host: activeConnection.host,
          port: activeConnection.port,
          apiKey: activeConnection.apiKey,
          useSsl: activeConnection.ssl,
        })
        const response = await client.getColumnValues(tableId, column)
        return response.values.map(v => v.value)
      } catch {
        return []
      }
    },
    [activeConnection, tableId]
  )

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

  const handleCopyDocument = useCallback(async (doc: Document) => {
    await navigator.clipboard.writeText(JSON.stringify(doc.data, null, 2))
    setCopiedId(doc.id)
    setTimeout(() => setCopiedId(null), 2000)
  }, [])

  // Handle delete document
  const handleDeleteDocument = useCallback((doc: Document) => {
    setDeleteTarget(doc)
  }, [])

  const confirmDelete = useCallback(async () => {
    if (!deleteTarget || !activeConnection) return

    setIsDeleting(true)
    try {
      const client = createClient({
        host: activeConnection.host,
        port: activeConnection.port,
        apiKey: activeConnection.apiKey,
        useSsl: activeConnection.ssl,
      })

      await client.deleteDocument(deleteTarget.id)
      setDeleteTarget(null)
      // Refresh the document list
      fetchDocuments(true)
    } catch (error) {
      console.error('Failed to delete document:', error)
      alert(error instanceof Error ? error.message : 'Failed to delete document')
    } finally {
      setIsDeleting(false)
    }
  }, [deleteTarget, activeConnection, fetchDocuments])

  // Handle edit document - opens document in sidebar for viewing
  const handleEditDocument = useCallback((doc: Document) => {
    setSelectedCell({
      title: `${doc.data.title || doc.id}`,
      path: 'document',
      data: doc.data,
    })
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
          tableId={tableId}
          valueFetcher={valueFetcher}
          viewMode={viewMode}
          isLoading={isLoadingDocuments}
          onViewModeChange={setViewMode}
          onRefresh={() => fetchDocuments(true)}
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
              copiedId={copiedId}
              onSelectDocument={selectDocument}
              onCopyDocument={handleCopyDocument}
              onEditDocument={handleEditDocument}
              onDeleteDocument={handleDeleteDocument}
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
        isLoading={selectedCell?.isLoading}
      />

      {/* Delete Confirmation Dialog */}
      <Dialog open={deleteTarget !== null} onOpenChange={(open) => !open && setDeleteTarget(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Delete Document</DialogTitle>
            <DialogDescription>
              Are you sure you want to delete "{deleteTarget?.data.title || deleteTarget?.id}"? 
              This action cannot be undone.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button
              variant="ghost"
              onClick={() => setDeleteTarget(null)}
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
    </div>
  )
}
