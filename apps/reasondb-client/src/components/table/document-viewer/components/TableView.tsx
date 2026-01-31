import { flexRender, type Table as TableType } from '@tanstack/react-table'
import { Copy, PencilSimple, Trash, CheckCircle } from '@phosphor-icons/react'
import { cn } from '@/lib/utils'
import type { Document } from '@/stores/tableStore'

interface TableViewProps {
  table: TableType<Document>
  selectedDocumentId: string | null
  copied: boolean
  onSelectDocument: (id: string) => void
  onCopyDocument: (doc: Document) => void
}

export function TableView({
  table,
  selectedDocumentId,
  copied,
  onSelectDocument,
  onCopyDocument,
}: TableViewProps) {
  return (
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
            onClick={() => onSelectDocument(row.original.id)}
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
              <RowActions
                row={row.original}
                copied={copied}
                onCopy={onCopyDocument}
              />
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  )
}

interface RowActionsProps {
  row: Document
  copied: boolean
  onCopy: (doc: Document) => void
}

function RowActions({ row, copied, onCopy }: RowActionsProps) {
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
        {copied ? (
          <CheckCircle size={14} className="text-green" />
        ) : (
          <Copy size={14} />
        )}
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
  )
}
