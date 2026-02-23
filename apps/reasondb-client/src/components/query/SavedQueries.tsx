import { useState } from 'react'
import { formatDistanceToNow } from 'date-fns'
import {
  Star,
  Play,
  Trash,
  PencilSimple,
  MagnifyingGlass,
} from '@phosphor-icons/react'
import { useQueryStore, type SavedQuery } from '@/stores/queryStore'
import { useConnectionStore } from '@/stores/connectionStore'
import { Input } from '@/components/ui/Input'
import { SaveQueryDialog } from './SaveQueryDialog'

interface SavedQueriesProps {
  onSelectQuery?: (query: string) => void
  onClose?: () => void
}

export function SavedQueries({ onSelectQuery, onClose }: SavedQueriesProps) {
  const { savedQueries, deleteSavedQuery, setCurrentQuery } = useQueryStore()
  const { connections } = useConnectionStore()
  const [search, setSearch] = useState('')
  const [editingQuery, setEditingQuery] = useState<SavedQuery | undefined>()
  const [showEditDialog, setShowEditDialog] = useState(false)

  const handleSelectQuery = (query: SavedQuery) => {
    setCurrentQuery(query.query)
    onSelectQuery?.(query.query)
    onClose?.()
  }

  const handleEdit = (query: SavedQuery) => {
    setEditingQuery(query)
    setShowEditDialog(true)
  }

  const handleDelete = (id: string) => {
    deleteSavedQuery(id)
  }

  const getConnectionName = (connectionId?: string) => {
    if (!connectionId) return null
    return connections.find((c) => c.id === connectionId)?.name || null
  }

  const filtered = search.trim()
    ? savedQueries.filter(
        (q) =>
          q.name.toLowerCase().includes(search.toLowerCase()) ||
          q.query.toLowerCase().includes(search.toLowerCase()) ||
          q.description?.toLowerCase().includes(search.toLowerCase())
      )
    : savedQueries

  if (savedQueries.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center h-full p-6 text-center">
        <Star size={48} weight="duotone" className="text-overlay-0 mb-3" />
        <p className="text-sm text-subtext-0">No saved queries</p>
        <p className="text-xs text-overlay-0 mt-1">
          Save queries from the editor to access them here
        </p>
      </div>
    )
  }

  return (
    <div className="flex flex-col h-full">
      {/* Search */}
      <div className="px-3 py-2 border-b border-border">
        <div className="relative">
          <MagnifyingGlass
            size={14}
            className="absolute left-2.5 top-1/2 -translate-y-1/2 text-overlay-0"
          />
          <Input
            placeholder="Search saved queries..."
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            className="pl-8 h-8 text-xs"
          />
        </div>
      </div>

      {/* List */}
      <div className="flex-1 overflow-auto">
        {filtered.length === 0 ? (
          <div className="p-4 text-center text-xs text-overlay-0">
            No queries match "{search}"
          </div>
        ) : (
          filtered.map((item) => {
            const connectionName = getConnectionName(item.connectionId)
            return (
              <div
                key={item.id}
                className="group border-b border-border/50 hover:bg-surface-0/50 transition-colors"
              >
                <div className="px-3 py-2.5">
                  <div className="flex items-start justify-between gap-2">
                    <div className="flex-1 min-w-0">
                      <p className="text-sm font-medium text-text truncate">
                        {item.name}
                      </p>
                      {item.description && (
                        <p className="text-xs text-subtext-0 mt-0.5 line-clamp-1">
                          {item.description}
                        </p>
                      )}
                    </div>
                    <div className="flex items-center gap-0.5 shrink-0 opacity-0 group-hover:opacity-100 transition-opacity">
                      <button
                        onClick={() => handleSelectQuery(item)}
                        className="p-1 rounded hover:bg-surface-1 text-overlay-0 hover:text-text"
                        title="Load query"
                      >
                        <Play size={12} />
                      </button>
                      <button
                        onClick={() => handleEdit(item)}
                        className="p-1 rounded hover:bg-surface-1 text-overlay-0 hover:text-text"
                        title="Edit"
                      >
                        <PencilSimple size={12} />
                      </button>
                      <button
                        onClick={() => handleDelete(item.id)}
                        className="p-1 rounded hover:bg-surface-1 text-overlay-0 hover:text-red"
                        title="Delete"
                      >
                        <Trash size={12} />
                      </button>
                    </div>
                  </div>

                  <div className="mt-1.5 bg-surface-0 rounded p-1.5">
                    <pre className="text-[11px] font-mono text-subtext-0 whitespace-pre-wrap break-all line-clamp-2">
                      {item.query}
                    </pre>
                  </div>

                  <div className="flex items-center gap-2 mt-1.5 text-[10px] text-overlay-0">
                    <span>
                      {formatDistanceToNow(new Date(item.updatedAt), { addSuffix: true })}
                    </span>
                    {connectionName && (
                      <>
                        <span>·</span>
                        <span>{connectionName}</span>
                      </>
                    )}
                  </div>
                </div>
              </div>
            )
          })
        )}
      </div>

      <SaveQueryDialog
        open={showEditDialog}
        onOpenChange={setShowEditDialog}
        editingQuery={editingQuery}
      />
    </div>
  )
}
