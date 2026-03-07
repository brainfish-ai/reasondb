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
import { Label } from '@/components/ui/Label'
import { Plus, Trash } from '@phosphor-icons/react'
import { useTableStore, type Document } from '@/stores/tableStore'
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

interface EditDocumentDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  document: Document | null
}

export function EditDocumentDialog({ open, onOpenChange, document }: EditDocumentDialogProps) {
  const { updateDocument } = useTableStore()
  const { activeConnectionId, connections } = useConnectionStore()

  const [title, setTitle] = useState('')
  const [tagsInput, setTagsInput] = useState('')
  const [metadataEntries, setMetadataEntries] = useState<MetadataEntry[]>([])
  const [isSaving, setIsSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const activeConnection = connections.find((c) => c.id === activeConnectionId)

  useEffect(() => {
    if (!open || !document) return

    const data = document.data as Record<string, unknown>
    setTitle(typeof data.title === 'string' ? data.title : '')
    const tags = Array.isArray(data.tags) ? (data.tags as string[]).join(', ') : ''
    setTagsInput(tags)
    const meta = data.metadata && typeof data.metadata === 'object'
      ? (data.metadata as Record<string, unknown>)
      : {}
    setMetadataEntries(metadataToEntries(meta))
    setError(null)
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, document?.id])

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
    if (!title.trim() || !activeConnection || !document) return

    setIsSaving(true)
    setError(null)

    try {
      const client = createClient({
        host: activeConnection.host,
        port: activeConnection.port,
        apiKey: activeConnection.apiKey,
        useSsl: activeConnection.ssl,
      })

      const tags = tagsInput
        .split(',')
        .map((t) => t.trim())
        .filter(Boolean)

      const metadata = entriesToMetadata(metadataEntries)

      await client.updateDocument(document.id, {
        title: title.trim(),
        tags,
        metadata,
      })

      // Reflect changes immediately in the local store
      updateDocument(document.id, {
        ...document.data,
        title: title.trim(),
        tags,
        metadata,
      })

      onOpenChange(false)
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to update document')
    } finally {
      setIsSaving(false)
    }
  }

  const handleOpenChange = (nextOpen: boolean) => {
    if (!nextOpen) setError(null)
    onOpenChange(nextOpen)
  }

  const isValid = title.trim().length > 0

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent className="max-w-md max-h-[85vh] flex flex-col">
        <DialogHeader>
          <DialogTitle>Edit Document</DialogTitle>
        </DialogHeader>

        <div className="grid gap-4 py-2 overflow-y-auto flex-1 pr-1">
          {/* Title */}
          <div className="grid gap-2">
            <Label htmlFor="editDocTitle">Title</Label>
            <Input
              id="editDocTitle"
              placeholder="Document title"
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter' && isValid && !isSaving) handleSave()
              }}
              autoFocus
            />
          </div>

          {/* Tags */}
          <div className="grid gap-2">
            <Label htmlFor="editDocTags">
              Tags <span className="text-overlay-0 font-normal">(comma-separated, optional)</span>
            </Label>
            <Input
              id="editDocTags"
              placeholder="e.g. insurance, aia, policy"
              value={tagsInput}
              onChange={(e) => setTagsInput(e.target.value)}
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

            {metadataEntries.length === 0 ? (
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
          <Button onClick={handleSave} disabled={!isValid || isSaving}>
            {isSaving ? 'Saving...' : 'Save Changes'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
