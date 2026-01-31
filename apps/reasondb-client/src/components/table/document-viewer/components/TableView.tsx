import { type Table as TableType } from '@tanstack/react-table'
import { Copy, PencilSimple, Trash, CheckCircle } from '@phosphor-icons/react'
import { DataTable } from '@/components/shared/data-table'
import type { Document } from '@/stores/tableStore'

interface TableViewProps {
  table: TableType<Document>
  selectedDocumentId: string | null
  copiedId: string | null  // Track which specific document was copied
  onSelectDocument: (id: string) => void
  onCopyDocument: (doc: Document) => void
  onEditDocument?: (doc: Document) => void
  onDeleteDocument?: (doc: Document) => void
}

export function TableView({
  table,
  selectedDocumentId,
  copiedId,
  onSelectDocument,
  onCopyDocument,
  onEditDocument,
  onDeleteDocument,
}: TableViewProps) {
  return (
    <DataTable
      table={table}
      onRowClick={(row) => onSelectDocument(row.id)}
      getRowId={(row) => row.id}
      selectedRowId={selectedDocumentId}
      renderRowActions={(row) => (
        <RowActions
          row={row}
          isCopied={copiedId === row.id}
          onCopy={onCopyDocument}
          onEdit={onEditDocument}
          onDelete={onDeleteDocument}
        />
      )}
    />
  )
}

interface RowActionsProps {
  row: Document
  isCopied: boolean
  onCopy: (doc: Document) => void
  onEdit?: (doc: Document) => void
  onDelete?: (doc: Document) => void
}

function RowActions({ row, isCopied, onCopy, onEdit, onDelete }: RowActionsProps) {
  return (
    <div className="flex items-center justify-end gap-1">
      <button
        onClick={(e) => {
          e.stopPropagation()
          onCopy(row)
        }}
        className="p-1 hover:bg-surface-1 rounded text-overlay-0 hover:text-text"
        title="Copy JSON"
      >
        {isCopied ? (
          <CheckCircle size={14} className="text-green" />
        ) : (
          <Copy size={14} />
        )}
      </button>
      <button
        onClick={(e) => {
          e.stopPropagation()
          onEdit?.(row)
        }}
        className="p-1 hover:bg-surface-1 rounded text-overlay-0 hover:text-text"
        title="Edit"
      >
        <PencilSimple size={14} />
      </button>
      <button
        onClick={(e) => {
          e.stopPropagation()
          onDelete?.(row)
        }}
        className="p-1 hover:bg-surface-1 rounded text-overlay-0 hover:text-red"
        title="Delete"
      >
        <Trash size={14} />
      </button>
    </div>
  )
}
