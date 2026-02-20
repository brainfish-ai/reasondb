import { useState } from 'react'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from '@/components/ui/Dialog'
import { Button } from '@/components/ui/Button'
import { Input } from '@/components/ui/Input'
import { Label } from '@/components/ui/Label'
import { useTableStore } from '@/stores/tableStore'
import { useConnectionStore } from '@/stores/connectionStore'
import { createClient } from '@/lib/api'

interface CreateTableDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
}

export function CreateTableDialog({ open, onOpenChange }: CreateTableDialogProps) {
  const { addTable } = useTableStore()
  const { activeConnectionId, connections } = useConnectionStore()

  const [tableName, setTableName] = useState('')
  const [description, setDescription] = useState('')
  const [isCreating, setIsCreating] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const activeConnection = connections.find(c => c.id === activeConnectionId)

  const resetForm = () => {
    setTableName('')
    setDescription('')
    setError(null)
  }

  const handleCreate = async () => {
    if (!tableName.trim() || !activeConnection) return

    setIsCreating(true)
    setError(null)

    try {
      const client = createClient({
        host: activeConnection.host,
        port: activeConnection.port,
        apiKey: activeConnection.apiKey,
        useSsl: activeConnection.ssl,
      })

      const response = await client.createTable(tableName.trim(), {
        description: description.trim() || undefined,
      })

      addTable({
        id: response.id,
        name: response.name,
        description: response.description,
        metadata: response.metadata,
        document_count: response.document_count,
        total_nodes: response.total_nodes,
        created_at: response.created_at,
        updated_at: response.updated_at,
      })

      resetForm()
      onOpenChange(false)
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to create table')
    } finally {
      setIsCreating(false)
    }
  }

  const handleOpenChange = (nextOpen: boolean) => {
    if (!nextOpen) resetForm()
    onOpenChange(nextOpen)
  }

  const isValid = tableName.trim().length > 0

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>Create New Table</DialogTitle>
        </DialogHeader>

        <div className="space-y-4 py-4">
          <div className="space-y-2">
            <Label htmlFor="tableName">Table Name</Label>
            <Input
              id="tableName"
              placeholder="e.g. contracts, research_papers"
              value={tableName}
              onChange={(e) => setTableName(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter' && isValid && !isCreating) handleCreate()
              }}
              autoFocus
            />
          </div>

          <div className="space-y-2">
            <Label htmlFor="description">
              Description <span className="text-overlay-0 font-normal">(optional)</span>
            </Label>
            <textarea
              id="description"
              placeholder="What kind of documents will this table store?"
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              rows={3}
              className="w-full rounded-md bg-surface-0 border border-border px-3 py-2 text-sm text-text placeholder-overlay-0 focus:outline-none focus:ring-1 focus:ring-primary focus:border-transparent resize-none"
            />
          </div>

          {error && (
            <div className="rounded-md bg-red/10 border border-red/20 px-3 py-2">
              <p className="text-xs text-red">{error}</p>
            </div>
          )}
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={() => handleOpenChange(false)}>
            Cancel
          </Button>
          <Button onClick={handleCreate} disabled={!isValid || isCreating}>
            {isCreating ? 'Creating...' : 'Create Table'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
