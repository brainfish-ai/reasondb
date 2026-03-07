import { useState, useMemo, useCallback } from 'react'
import {
  useReactTable,
  getCoreRowModel,
  getSortedRowModel,
  getPaginationRowModel,
  type SortingState,
} from '@tanstack/react-table'
import { Panel, Group, Separator } from 'react-resizable-panels'
import { FilterBuilder } from '@/components/search'
import { JsonDetailSidebar } from '../JsonDetailSidebar'
import { AddDocumentDialog } from '../AddDocumentDialog'
import { EditDocumentDialog } from '../EditDocumentDialog'
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
import { createClient } from '@/lib/api'

import { useDocuments, useColumnDetection, useDocumentFilter } from './hooks'

import {
  Toolbar,
  TableView,
  JsonView,
  Footer,
  LoadingState,
  EmptyState,
  ErrorState,
  NoTableState,
  RecordSidebar,
} from './components'

import { createColumns } from './columns'

import type { ViewMode, SelectedCellData, DocumentViewerProps } from './types'

export function DocumentViewer({ tableId }: DocumentViewerProps) {
  const [viewMode, setViewMode] = useState<ViewMode>('table')
  const [sorting, setSorting] = useState<SortingState>([])
  const [copiedId, setCopiedId] = useState<string | null>(null)
  const [selectedCell, setSelectedCell] = useState<SelectedCellData | null>(null)
  const [deleteTarget, setDeleteTarget] = useState<Document | null>(null)
  const [isDeleting, setIsDeleting] = useState(false)
  const [showAddDocument, setShowAddDocument] = useState(false)
  const [editTarget, setEditTarget] = useState<Document | null>(null)

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

  const selectedDocument = useMemo(
    () => filteredDocuments.find((d) => d.id === selectedDocumentId) ?? null,
    [filteredDocuments, selectedDocumentId]
  )

  // Close the record sidebar by deselecting the document
  const handleCloseRecordSidebar = useCallback(() => {
    selectDocument(null)
  }, [selectDocument])

  // Row click: toggle selection (click again to deselect)
  const handleRowClick = useCallback(
    (id: string) => {
      selectDocument(selectedDocumentId === id ? null : id)
    },
    [selectDocument, selectedDocumentId]
  )

  // Load document content for the "nodes" column button — now just selects row
  const handleLoadContent = useCallback(
    async (documentId: string, _documentTitle: string) => {
      selectDocument(documentId)
    },
    [selectDocument]
  )

  const columns = useMemo(
    () =>
      createColumns({
        onSelectCell: setSelectedCell,
        onLoadContent: handleLoadContent,
      }),
    [handleLoadContent]
  )

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
        return response.values.map((v) => v.value)
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
    },
    [activeConnection, tableId, fetchDocuments]
  )

  const handleCopyDocument = useCallback(async (doc: Document) => {
    await navigator.clipboard.writeText(JSON.stringify(doc.data, null, 2))
    setCopiedId(doc.id)
    setTimeout(() => setCopiedId(null), 2000)
  }, [])

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
      fetchDocuments(true)
    } catch (error) {
      console.error('Failed to delete document:', error)
      alert(error instanceof Error ? error.message : 'Failed to delete document')
    } finally {
      setIsDeleting(false)
    }
  }, [deleteTarget, activeConnection, fetchDocuments])

  const handleEditDocument = useCallback((doc: Document) => {
    setEditTarget(doc)
  }, [])

  if (!tableId) {
    return <NoTableState />
  }

  const showRecordSidebar = selectedDocument !== null && activeConnection !== undefined

  return (
    <div className="flex h-full bg-base">
      <Group orientation="horizontal" className="flex-1">
        {/* Left panel — table list */}
        <Panel defaultSize={showRecordSidebar ? 55 : 100} minSize={30}>
          <div className="flex flex-col h-full min-w-0">
            <Toolbar
              columns={detectedColumns}
              tableId={tableId}
              valueFetcher={valueFetcher}
              viewMode={viewMode}
              isLoading={isLoadingDocuments}
              onViewModeChange={setViewMode}
              onRefresh={() => fetchDocuments(true)}
              onSearch={handleSearch}
              onAddDocument={() => setShowAddDocument(true)}
            />

            <FilterBuilder columns={detectedColumns} onApply={() => {}} />

            {documentsError && (
              <ErrorState message={documentsError} onRetry={fetchDocuments} />
            )}

            <div className="flex-1 min-h-0 overflow-auto">
              {isLoadingDocuments ? (
                <LoadingState />
              ) : documents.length === 0 ? (
                <EmptyState onAddDocument={() => setShowAddDocument(true)} />
              ) : viewMode === 'table' ? (
                <TableView
                  table={table}
                  selectedDocumentId={selectedDocumentId}
                  copiedId={copiedId}
                  onSelectDocument={handleRowClick}
                  onCopyDocument={handleCopyDocument}
                  onEditDocument={handleEditDocument}
                  onDeleteDocument={handleDeleteDocument}
                />
              ) : (
                <JsonView
                  documents={filteredDocuments}
                  selectedDocumentId={selectedDocumentId}
                  onSelectDocument={handleRowClick}
                />
              )}
            </div>

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
        </Panel>

        {/* Right panel — record detail sidebar */}
        {showRecordSidebar && (
          <>
            <Separator className="w-1 bg-border hover:bg-primary/50 transition-colors cursor-col-resize" />
            <Panel defaultSize={45} minSize={25}>
              <RecordSidebar
                document={selectedDocument}
                connection={activeConnection}
                onClose={handleCloseRecordSidebar}
              />
            </Panel>
          </>
        )}
      </Group>

      {/* JSON Detail Sidebar — for metadata cell click */}
      <JsonDetailSidebar
        isOpen={selectedCell !== null}
        onClose={() => setSelectedCell(null)}
        title={selectedCell?.title ?? ''}
        path={selectedCell?.path}
        data={selectedCell?.data}
        isLoading={selectedCell?.isLoading}
      />

      <AddDocumentDialog
        open={showAddDocument}
        onOpenChange={setShowAddDocument}
        tableId={tableId}
      />

      <EditDocumentDialog
        open={editTarget !== null}
        onOpenChange={(open) => { if (!open) setEditTarget(null) }}
        document={editTarget}
      />

      <Dialog
        open={deleteTarget !== null}
        onOpenChange={(open) => !open && setDeleteTarget(null)}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Delete Document</DialogTitle>
            <DialogDescription>
              Are you sure you want to delete "
              {String(deleteTarget?.data?.title ?? deleteTarget?.id)}"? This action cannot
              be undone.
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
