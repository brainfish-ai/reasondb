import { Table, Plus } from '@phosphor-icons/react'
import { Button } from '@/components/ui/Button'

/**
 * Loading spinner state
 */
export function LoadingState() {
  return (
    <div className="flex items-center justify-center h-full">
      <div className="w-6 h-6 border-2 border-primary border-t-transparent rounded-full animate-spin" />
    </div>
  )
}

/**
 * Empty table state
 */
export function EmptyState() {
  return (
    <div className="flex flex-col items-center justify-center h-full text-center p-8">
      <Table size={48} weight="duotone" className="text-overlay-0 mb-3" />
      <p className="text-sm text-subtext-0">No documents in this table</p>
      <Button size="sm" variant="secondary" className="mt-4 gap-1.5">
        <Plus size={14} />
        Add Document
      </Button>
    </div>
  )
}

/**
 * Error state with retry
 */
interface ErrorStateProps {
  message: string
  onRetry: () => void
}

export function ErrorState({ message, onRetry }: ErrorStateProps) {
  return (
    <div className="px-4 py-3 mx-4 mt-2 rounded-md bg-red/10 border border-red/20">
      <p className="text-sm text-red">{message}</p>
      <button
        onClick={onRetry}
        className="text-sm text-red underline mt-1 hover:text-red/80"
      >
        Retry
      </button>
    </div>
  )
}

/**
 * No table selected state
 */
export function NoTableState() {
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
