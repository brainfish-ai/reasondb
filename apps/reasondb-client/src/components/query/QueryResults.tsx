import {
  Table,
  WarningCircle,
} from '@phosphor-icons/react'
import { useQueryStore } from '@/stores/queryStore'
import { RecordTable } from '@/components/shared/data-table'

export function QueryResults() {
  const { result, error, isExecuting, reasonProgress } = useQueryStore()

  if (isExecuting) {
    const message = reasonProgress?.message ?? 'Executing query...'

    return (
      <div className="flex flex-col items-center justify-center h-full bg-base text-subtext-0">
        <div className="w-6 h-6 border-2 border-primary border-t-transparent rounded-full animate-spin mb-4" />
        <p
          key={message}
          className="text-sm animate-[fadeIn_0.3s_ease-in-out]"
        >
          {message}
        </p>
      </div>
    )
  }

  if (error) {
    return (
      <div className="flex flex-col items-center justify-center h-full bg-base p-6">
        <WarningCircle size={48} weight="duotone" className="text-red mb-3" />
        <p className="text-sm font-medium text-red mb-2">Query Error</p>
        <pre className="text-xs text-subtext-0 bg-surface-0 p-3 rounded-md max-w-full overflow-auto">
          {error}
        </pre>
      </div>
    )
  }

  if (!result) {
    return (
      <div className="flex flex-col items-center justify-center h-full bg-base text-subtext-0">
        <Table size={48} weight="duotone" className="mb-3 opacity-50" />
        <p className="text-sm">Run a query to see results</p>
      </div>
    )
  }

  return (
    <RecordTable
      records={result.rows}
      columns={result.columns}
      totalCount={result.rowCount}
      executionTime={result.executionTime}
      isQueryResult
    />
  )
}
