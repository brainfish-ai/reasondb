import { useMemo } from 'react'
import { cn } from '@/lib/utils'
import { SyntaxViewer } from './SyntaxViewer'

export interface JsonViewerProps {
  data: unknown
  height?: string | number
  lineNumbers?: boolean
  minimap?: boolean
  className?: string
  emptyMessage?: string
}

export function JsonViewer({
  data,
  lineNumbers = true,
  className,
  emptyMessage = 'No data to display',
}: JsonViewerProps) {
  const formattedJson = useMemo(
    () => (data !== undefined ? JSON.stringify(data, null, 2) : ''),
    [data],
  )

  if (data === undefined || data === null || (Array.isArray(data) && data.length === 0)) {
    return (
      <div className={cn('flex items-center justify-center h-full text-overlay-0 text-sm', className)}>
        {emptyMessage}
      </div>
    )
  }

  return (
    <div className={cn('h-full', className)}>
      <SyntaxViewer
        content={formattedJson}
        language="json"
        lineNumbers={lineNumbers}
      />
    </div>
  )
}
