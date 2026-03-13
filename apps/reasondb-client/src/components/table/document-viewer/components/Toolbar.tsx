import { useState, useRef } from 'react'
import {
  Table,
  Code,
  Rows,
  ArrowsClockwise,
  DownloadSimple,
  Plus,
} from '@phosphor-icons/react'
import { Button } from '@/components/ui/Button'
import { SearchBar, type ValueFetcher } from '@/components/search'
import { cn } from '@/lib/utils'
import type { ColumnInfo } from '@/lib/filter-types'
import type { ViewMode } from '../types'

interface ToolbarProps {
  columns: ColumnInfo[]
  tableId?: string
  valueFetcher?: ValueFetcher
  viewMode: ViewMode
  isLoading: boolean
  onViewModeChange: (mode: ViewMode) => void
  onRefresh: () => void
  onSearch: (text: string) => void
  onAddDocument?: () => void
}

const MIN_SPIN_MS = 600

export function Toolbar({
  columns,
  tableId,
  valueFetcher,
  viewMode,
  isLoading,
  onViewModeChange,
  onRefresh,
  onSearch,
  onAddDocument,
}: ToolbarProps) {
  const [isSpinning, setIsSpinning] = useState(false)
  const spinTimer = useRef<ReturnType<typeof setTimeout> | null>(null)

  const handleRefresh = () => {
    setIsSpinning(true)
    if (spinTimer.current) clearTimeout(spinTimer.current)
    spinTimer.current = setTimeout(() => setIsSpinning(false), MIN_SPIN_MS)
    onRefresh()
  }

  return (
    <div className="flex items-center gap-3 px-4 py-2 border-b border-border bg-mantle">
      {/* Table icon */}
      <div className="flex items-center gap-2 shrink-0">
        <Table size={18} weight="duotone" className="text-mauve" />
      </div>

      {/* Search */}
      <SearchBar
        columns={columns}
        tableId={tableId}
        valueFetcher={valueFetcher}
        placeholder='Search... (e.g., title = "doc" or content contains "text")'
        onSearch={onSearch}
      />

      <div className="h-5 w-px bg-border shrink-0" />

      {/* View mode toggle */}
      <div className="flex items-center bg-surface-0 rounded-md p-0.5 shrink-0">
        <button
          onClick={() => onViewModeChange('table')}
          className={cn(
            'p-1.5 rounded transition-colors',
            viewMode === 'table'
              ? 'bg-surface-1 text-text'
              : 'text-overlay-0 hover:text-text'
          )}
          title="Table view"
        >
          <Rows size={16} />
        </button>
        <button
          onClick={() => onViewModeChange('json')}
          className={cn(
            'p-1.5 rounded transition-colors',
            viewMode === 'json'
              ? 'bg-surface-1 text-text'
              : 'text-overlay-0 hover:text-text'
          )}
          title="JSON view"
        >
          <Code size={16} />
        </button>
      </div>

      <div className="h-5 w-px bg-border shrink-0" />

      {/* Actions */}
      <div className="flex items-center gap-1 shrink-0">
        <Button size="sm" variant="ghost" onClick={handleRefresh} disabled={isLoading} title="Refresh">
          <ArrowsClockwise size={16} className={isLoading || isSpinning ? 'animate-spin' : ''} />
        </Button>

        <Button size="sm" variant="ghost" title="Export">
          <DownloadSimple size={16} />
        </Button>

        <Button size="sm" variant="ghost" className="gap-1.5" onClick={onAddDocument}>
          <Plus size={14} />
          Add
        </Button>
      </div>
    </div>
  )
}
