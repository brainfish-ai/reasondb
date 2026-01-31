import { cn } from '@/lib/utils'
import type { Document } from '@/stores/tableStore'

interface JsonViewProps {
  documents: Document[]
  selectedDocumentId: string | null
  onSelectDocument: (id: string) => void
}

export function JsonView({
  documents,
  selectedDocumentId,
  onSelectDocument,
}: JsonViewProps) {
  return (
    <div className="p-4 space-y-2">
      {documents.map((doc) => (
        <div
          key={doc.id}
          onClick={() => onSelectDocument(doc.id)}
          className={cn(
            'p-3 rounded-lg border cursor-pointer transition-colors',
            selectedDocumentId === doc.id
              ? 'border-mauve bg-mauve/5'
              : 'border-border bg-surface-0/50 hover:border-overlay-0'
          )}
        >
          <pre className="text-xs font-mono text-text overflow-auto max-h-48">
            {JSON.stringify(doc.data, null, 2)}
          </pre>
        </div>
      ))}
    </div>
  )
}
