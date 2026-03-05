import { useState, useEffect } from 'react'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from '@/components/ui/Dialog'
import { Button } from '@/components/ui/Button'
import { Input } from '@/components/ui/Input'
import { Textarea } from '@/components/ui/Textarea'
import { Label } from '@/components/ui/Label'
import { Plus, Trash } from '@phosphor-icons/react'
import { useTableStore, type Table } from '@/stores/tableStore'
import { useConnectionStore } from '@/stores/connectionStore'
import { createClient } from '@/lib/api'

interface MetadataEntry {
  key: string
  value: string
}

function metadataToEntries(metadata: Record<string, unknown>): MetadataEntry[] {
  return Object.entries(metadata).map(([key, value]) => ({
    key,
    value: typeof value === 'string' ? value : JSON.stringify(value),
  }))
}

function entriesToMetadata(entries: MetadataEntry[]): Record<string, unknown> {
  const result: Record<string, unknown> = {}
  for (const { key, value } of entries) {
    if (!key.trim()) continue
    try {
      result[key.trim()] = JSON.parse(value)
    } catch {
      result[key.trim()] = value
    }
  }
  return result
}

interface EditTableDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  table: Table | null
}

export function EditTableDialog({ open, onOpenChange, table }: EditTableDialogProps) {
  const { updateTable } = useTableStore()
  const { activeConnectionId, connections } = useConnectionStore()

  const [tableName, setTableName] = useState('')
  const [description, setDescription] = useState('')
  const [metadataEntries, setMetadataEntries] = useState<MetadataEntry[]>([])
  const [isFetching, setIsFetching] = useState(false)
  const [isSaving, setIsSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const activeConnection = connections.find(c => c.id === activeConnectionId)

  useEffect(() => {
    if (!open || !table || !activeConnection) return

    setTableName(table.name)
    setDescription(table.description ?? '')
    setMetadataEntries(metadataToEntries(table.metadata))
    setError(null)

    // Fetch full table details to get current metadata
    const fetchDetails = async () => {
      setIsFetching(true)
      try {
        const client = createClient({
          host: activeConnection.host,
          port: activeConnection.port,
          apiKey: activeConnection.apiKey,
          useSsl: activeConnection.ssl,
        })
        const detail = await client.getTable(table.id)
        setMetadataEntries(metadataToEntries(detail.metadata))
      } catch {
        // Non-critical — user can still edit name/description
      } finally {
        setIsFetching(false)
      }
    }

    fetchDetails()
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, table?.id])

  const addMetadataEntry = () => {
    setMetadataEntries((prev) => [...prev, { key: '', value: '' }])
  }

  const updateMetadataEntry = (index: number, field: 'key' | 'value', val: string) => {
    setMetadataEntries((prev) =>
      prev.map((entry, i) => (i === index ? { ...entry, [field]: val } : entry))
    )
  }

  const removeMetadataEntry = (index: number) => {
    setMetadataEntries((prev) => prev.filter((_, i) => i !== index))
  }

  const handleSave = async () => {
    if (!tableName.trim() || !activeConnection || !table) return

    setIsSaving(true)
    setError(null)

    try {
      const client = createClient({
        host: activeConnection.host,
        port: activeConnection.port,
        apiKey: activeConnection.apiKey,
        useSsl: activeConnection.ssl,
      })

      const metadata = entriesToMetadata(metadataEntries)

      const response = await client.updateTable(table.id, {
        name: tableName.trim(),
        description: description.trim() || undefined,
        metadata,
      })

      updateTable(table.id, {
        name: response.name,
        description: response.description,
        metadata: response.metadata,
        updated_at: response.updated_at,
      })

      onOpenChange(false)
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to update table')
    } finally {
      setIsSaving(false)
    }
  }

  const handleOpenChange = (nextOpen: boolean) => {
    if (!nextOpen) setError(null)
    onOpenChange(nextOpen)
  }

  const isValid = tableName.trim().length > 0

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent className="max-w-md max-h-[85vh] flex flex-col">
        <DialogHeader>
          <DialogTitle>Edit Table</DialogTitle>
        </DialogHeader>

        <div className="grid gap-4 py-2 overflow-y-auto flex-1 pr-1">
          <div className="grid gap-2">
            <Label htmlFor="editTableName">Table Name</Label>
            <Input
              id="editTableName"
              placeholder="e.g. contracts, research_papers"
              value={tableName}
              onChange={(e) => setTableName(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter' && isValid && !isSaving) handleSave()
              }}
              autoFocus
            />
          </div>

          <div className="grid gap-2">
            <Label htmlFor="editDescription">
              Description <span className="text-overlay-0 font-normal">(optional)</span>
            </Label>
            <Textarea
              id="editDescription"
              placeholder="What kind of documents will this table store?"
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              rows={3}
              className="resize-none"
            />
          </div>

          {/* Metadata key-value editor */}
          <div className="grid gap-2">
            <div className="flex items-center justify-between">
              <Label>
                Metadata <span className="text-overlay-0 font-normal">(optional)</span>
              </Label>
              <button
                type="button"
                onClick={addMetadataEntry}
                className="flex items-center gap-1 text-xs text-subtext-0 hover:text-text transition-colors"
                title="Add metadata field"
              >
                <Plus size={12} />
                Add field
              </button>
            </div>

            {isFetching ? (
              <div className="flex items-center gap-2 py-2 text-xs text-overlay-0">
                <div className="w-3 h-3 border border-primary border-t-transparent rounded-full animate-spin" />
                Loading metadata...
              </div>
            ) : metadataEntries.length === 0 ? (
              <p className="text-xs text-overlay-0 py-1">
                No metadata fields. Click "Add field" to add one.
              </p>
            ) : (
              <div className="space-y-2">
                {metadataEntries.map((entry, index) => (
                  <div key={index} className="flex items-center gap-2">
                    <Input
                      placeholder="Key"
                      value={entry.key}
                      onChange={(e) => updateMetadataEntry(index, 'key', e.target.value)}
                      className="flex-1 text-xs h-8"
                    />
                    <Input
                      placeholder="Value"
                      value={entry.value}
                      onChange={(e) => updateMetadataEntry(index, 'value', e.target.value)}
                      className="flex-1 text-xs h-8"
                    />
                    <button
                      type="button"
                      onClick={() => removeMetadataEntry(index)}
                      className="p-1 text-overlay-0 hover:text-red transition-colors shrink-0"
                      title="Remove field"
                    >
                      <Trash size={14} />
                    </button>
                  </div>
                ))}
              </div>
            )}
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
          <Button onClick={handleSave} disabled={!isValid || isSaving || isFetching}>
            {isSaving ? 'Saving...' : 'Save Changes'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
