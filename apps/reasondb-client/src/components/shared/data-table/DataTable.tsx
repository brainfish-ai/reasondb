import { flexRender, type Table, type RowData } from '@tanstack/react-table'
import { cn } from '@/lib/utils'

// Value formatting helper
export function formatCellValue(value: unknown): React.ReactNode {
  if (value === null) return <span className="text-overlay-0 italic">null</span>
  if (value === undefined) return <span className="text-overlay-0 italic">undefined</span>
  if (typeof value === 'boolean') {
    return <span className={value ? 'text-green' : 'text-red'}>{String(value)}</span>
  }
  if (typeof value === 'number') {
    return <span className="text-peach font-mono">{value}</span>
  }
  if (Array.isArray(value)) {
    return (
      <div className="flex flex-wrap gap-1">
        {value.slice(0, 3).map((item, i) => (
          <span key={i} className="px-1.5 py-0.5 text-xs bg-surface-1 rounded text-subtext-0">
            {String(item)}
          </span>
        ))}
        {value.length > 3 && (
          <span className="text-xs text-overlay-0">+{value.length - 3}</span>
        )}
      </div>
    )
  }
  if (typeof value === 'object' && value !== null) {
    const keys = Object.keys(value)
    if (keys.length === 0) return <span className="text-overlay-0 italic">empty</span>
    return (
      <span className="text-overlay-0 font-mono text-xs">
        {`{${keys.length} fields}`}
      </span>
    )
  }
  // Date string
  if (typeof value === 'string' && value.match(/^\d{4}-\d{2}-\d{2}/)) {
    return <span className="text-sky">{new Date(value).toLocaleString()}</span>
  }
  const strValue = String(value)
  if (strValue.length > 100) {
    return (
      <span className="block max-w-[300px] truncate" title={strValue}>
        {strValue}
      </span>
    )
  }
  return strValue
}

export interface DataTableProps<TData extends RowData> {
  table: Table<TData>
  /** Optional row click handler */
  onRowClick?: (row: TData) => void
  /** Get row ID for selection highlighting */
  getRowId?: (row: TData) => string
  /** Currently selected row ID */
  selectedRowId?: string | null
  /** Optional render function for row actions */
  renderRowActions?: (row: TData) => React.ReactNode
  /** Whether to show zebra striping */
  striped?: boolean
  /** Custom class for table */
  className?: string
  /** Sticky header */
  stickyHeader?: boolean
}

export function DataTable<TData extends RowData>({
  table,
  onRowClick,
  getRowId,
  selectedRowId,
  renderRowActions,
  striped = true,
  className,
  stickyHeader = true,
}: DataTableProps<TData>) {
  return (
    <table className={cn('w-full text-sm', className)}>
      <thead className={cn(
        'bg-mantle border-b border-border',
        stickyHeader && 'sticky top-0 z-10'
      )}>
        {table.getHeaderGroups().map((headerGroup) => (
          <tr key={headerGroup.id}>
            {headerGroup.headers.map((header) => (
              <th
                key={header.id}
                className="px-3 py-2 text-left text-xs font-semibold text-subtext-0"
              >
                {header.isPlaceholder
                  ? null
                  : flexRender(header.column.columnDef.header, header.getContext())}
              </th>
            ))}
            {renderRowActions && (
              <th className="px-3 py-2 w-24 text-right text-xs font-semibold text-subtext-0">
                actions
              </th>
            )}
          </tr>
        ))}
      </thead>
      <tbody>
        {table.getRowModel().rows.map((row, idx) => {
          const rowId = getRowId ? getRowId(row.original) : row.id
          const isSelected = selectedRowId === rowId

          return (
            <tr
              key={row.id}
              onClick={onRowClick ? () => onRowClick(row.original) : undefined}
              className={cn(
                'border-b border-border/50 transition-colors',
                onRowClick && 'cursor-pointer',
                isSelected
                  ? 'bg-mauve/20'
                  : striped
                  ? idx % 2 === 0
                    ? 'bg-base hover:bg-surface-0/50'
                    : 'bg-mantle/30 hover:bg-surface-0/50'
                  : 'bg-base hover:bg-surface-0/50'
              )}
            >
              {row.getVisibleCells().map((cell) => (
                <td key={cell.id} className="px-3 py-2 text-text max-w-[200px]">
                  <div className="truncate">
                    {flexRender(cell.column.columnDef.cell, cell.getContext())}
                  </div>
                </td>
              ))}
              {renderRowActions && (
                <td className="px-3 py-2">
                  {renderRowActions(row.original)}
                </td>
              )}
            </tr>
          )
        })}
      </tbody>
    </table>
  )
}
