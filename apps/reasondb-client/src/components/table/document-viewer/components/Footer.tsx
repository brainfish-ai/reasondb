import { CaretLeft, CaretRight } from '@phosphor-icons/react'
import { Button } from '@/components/ui/Button'
import type { Table } from '@tanstack/react-table'
import type { Document } from '@/stores/tableStore'

interface FooterProps {
  table: Table<Document>
  totalDocuments: number
  filteredCount: number
  pageSize: number
  isFiltered: boolean
}

export function Footer({
  table,
  totalDocuments,
  filteredCount,
  pageSize,
  isFiltered,
}: FooterProps) {
  const pageIndex = table.getState().pagination.pageIndex
  const pageCount = table.getPageCount()

  return (
    <div className="flex items-center justify-between px-4 py-2 border-t border-border bg-mantle">
      {/* Row count */}
      <div className="text-xs text-subtext-0">
        {isFiltered ? (
          <>
            <span className="font-medium text-mauve">{filteredCount.toLocaleString()}</span>
            <span className="text-overlay-0"> of </span>
            <span className="text-text">{totalDocuments.toLocaleString()}</span>
            <span> matching</span>
          </>
        ) : (
          <>
            <span className="font-medium text-text">{totalDocuments.toLocaleString()}</span> rows
          </>
        )}
        {pageCount > 1 && (
          <span className="ml-2 text-overlay-0">
            · Showing {pageIndex * pageSize + 1}-
            {Math.min((pageIndex + 1) * pageSize, filteredCount)}
          </span>
        )}
      </div>

      {/* Pagination */}
      {pageCount > 1 && (
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
            Page {pageIndex + 1} of {pageCount}
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
  )
}
