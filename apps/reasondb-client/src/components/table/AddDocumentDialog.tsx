import { useState } from 'react'
import { TextT, Link as LinkIcon } from '@phosphor-icons/react'
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
import { useConnectionStore } from '@/stores/connectionStore'
import { useTableStore } from '@/stores/tableStore'
import { useIngestionStore } from '@/stores/ingestionStore'
import { cn } from '@/lib/utils'

type IngestionMode = 'text' | 'url'

interface AddDocumentDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  tableId: string
}

export function AddDocumentDialog({ open, onOpenChange, tableId }: AddDocumentDialogProps) {
  const { activeConnectionId, connections } = useConnectionStore()
  const activeConnection = connections.find(c => c.id === activeConnectionId)
  const tables = useTableStore(s => s.tables)
  const queueJob = useIngestionStore(s => s.queueJob)

  const tableName = tables.find(t => t.id === tableId)?.name ?? tableId

  const [mode, setMode] = useState<IngestionMode>('text')
  const [title, setTitle] = useState('')
  const [content, setContent] = useState('')
  const [tags, setTags] = useState('')
  const [url, setUrl] = useState('')

  const resetForm = () => {
    setTitle('')
    setContent('')
    setTags('')
    setUrl('')
  }

  const handleOpenChange = (nextOpen: boolean) => {
    if (!nextOpen) {
      resetForm()
      setMode('text')
    }
    onOpenChange(nextOpen)
  }

  const handleSubmit = () => {
    if (!activeConnectionId || !activeConnection) return

    if (mode === 'text') {
      const parsedTags = tags.split(',').map(t => t.trim()).filter(Boolean)

      queueJob({
        mode: 'text',
        title: title.trim(),
        tableId,
        tableName,
        connectionId: activeConnectionId,
        payload: {
          title: title.trim(),
          content: content.trim(),
          table_id: tableId,
          tags: parsedTags.length > 0 ? parsedTags : undefined,
        },
      })
    } else {
      const parsedUrl = url.trim()
      const displayTitle = new URL(parsedUrl).hostname

      queueJob({
        mode: 'url',
        title: displayTitle,
        tableId,
        tableName,
        connectionId: activeConnectionId,
        payload: {
          url: parsedUrl,
          table_id: tableId,
        },
      })
    }

    resetForm()
    onOpenChange(false)
  }

  const isValid = mode === 'text'
    ? title.trim().length > 0 && content.trim().length > 0
    : isValidUrl(url.trim())

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent className="max-w-lg max-h-[85vh] overflow-hidden flex flex-col">
        <DialogHeader>
          <DialogTitle>Add Document</DialogTitle>
        </DialogHeader>

        {/* Mode tabs */}
        <div className="flex items-center bg-surface-0 rounded-md p-0.5 gap-0.5">
          <button
            onClick={() => setMode('text')}
            className={cn(
              'flex-1 flex items-center justify-center gap-1.5 px-3 py-1.5 rounded text-sm transition-colors',
              mode === 'text' ? 'bg-surface-1 text-text font-medium' : 'text-overlay-0 hover:text-text'
            )}
          >
            <TextT size={14} />
            Text / Markdown
          </button>
          <button
            onClick={() => setMode('url')}
            className={cn(
              'flex-1 flex items-center justify-center gap-1.5 px-3 py-1.5 rounded text-sm transition-colors',
              mode === 'url' ? 'bg-surface-1 text-text font-medium' : 'text-overlay-0 hover:text-text'
            )}
          >
            <LinkIcon size={14} />
            URL
          </button>
        </div>

        <div className="flex-1 overflow-auto grid gap-4 py-2">
          {mode === 'text' ? (
            <>
              <div className="grid gap-2">
                <Label htmlFor="doc-title">Title</Label>
                <Input
                  id="doc-title"
                  placeholder="e.g. Service Agreement Q1 2026"
                  value={title}
                  onChange={(e) => setTitle(e.target.value)}
                  autoFocus
                />
              </div>

              <div className="grid gap-2">
                <Label htmlFor="doc-content">Content</Label>
                <Textarea
                  id="doc-content"
                  placeholder="Paste your document text or markdown here..."
                  value={content}
                  onChange={(e) => setContent(e.target.value)}
                  rows={10}
                  className="font-mono resize-none"
                />
              </div>

              <div className="grid gap-2">
                <Label htmlFor="doc-tags">
                  Tags <span className="text-overlay-0 font-normal">(optional, comma-separated)</span>
                </Label>
                <Input
                  id="doc-tags"
                  placeholder="e.g. legal, nda, confidential"
                  value={tags}
                  onChange={(e) => setTags(e.target.value)}
                />
              </div>
            </>
          ) : (
            <div className="grid gap-2">
              <Label htmlFor="doc-url">URL</Label>
              <Input
                id="doc-url"
                placeholder="https://example.com/document"
                value={url}
                onChange={(e) => setUrl(e.target.value)}
                autoFocus
              />
              <p className="text-xs text-overlay-0">
                Supports web pages, articles, and other text content.
              </p>
            </div>
          )}
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={() => handleOpenChange(false)}>
            Cancel
          </Button>
          <Button onClick={handleSubmit} disabled={!isValid}>
            Add Document
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

function isValidUrl(str: string): boolean {
  if (!str) return false
  try {
    new URL(str)
    return true
  } catch {
    return false
  }
}
