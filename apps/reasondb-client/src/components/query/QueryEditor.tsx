import { useRef, useEffect, useCallback, useState, useMemo } from 'react'
import type * as Monaco from 'monaco-editor'
import { useHotkeys } from 'react-hotkeys-hook'
import { Play, FloppyDisk, Clock, CircleNotch, Command, Warning, X } from '@phosphor-icons/react'
import { updateRqlTables } from '@/lib/rql-language'
import { useQueryStore } from '@/stores/queryStore'
import { useConnectionStore } from '@/stores/connectionStore'
import { useTableStore } from '@/stores/tableStore'
import { createClient } from '@/lib/api'
import { Button } from '@/components/ui/Button'
import { useMonacoEditor } from '@/providers/useMonacoHooks'

interface QueryEditorProps {
  onExecute?: (query: string) => Promise<void>
  tabId: string
  initialQuery?: string
  onQueryChange?: (query: string) => void
}

export default function QueryEditor({ onExecute, tabId, initialQuery = '', onQueryChange }: QueryEditorProps) {
  const { currentQuery, setCurrentQuery, isExecuting, setIsExecuting, setResults, setError, setReasonProgress, addToHistory } = useQueryStore()
  
  const query = currentQuery || initialQuery
  const { activeConnectionId, connections } = useConnectionStore()
  const { tables } = useTableStore()
  
  const activeConnection = connections.find((c) => c.id === activeConnectionId)
  const [warningDismissed, setWarningDismissed] = useState(false)

  const handleContentChange = useCallback((value: string) => {
    setCurrentQuery(value)
    onQueryChange?.(value)
  }, [setCurrentQuery, onQueryChange])

  const { containerRef, editor, monaco, isReady } = useMonacoEditor({
    modelId: `query-${tabId}`,
    initialContent: initialQuery,
    language: 'rql',
    onContentChange: handleContentChange,
  })

  const showSemicolonWarning = useMemo(() => {
    if (warningDismissed) return false
    const text = query.trim()
    if (!text) return false
    const segments = text.split(';').map((s) => s.trim()).filter((s) => s.length > 0)
    if (segments.length > 1) return false
    const selectCount = (text.match(/\bSELECT\b/gi) || []).length
    return selectCount > 1
  }, [query, warningDismissed])

  useEffect(() => {
    setWarningDismissed(false)
  }, [query])

  useEffect(() => {
    if (tables.length === 0) return
    
    const defaultColumns = [
      { name: 'id', type: 'uuid' },
      { name: 'title', type: 'text' },
      { name: 'content', type: 'text' },
      { name: 'total_nodes', type: 'integer' },
      { name: 'tags', type: 'text[]' },
      { name: 'metadata', type: 'jsonb' },
      { name: 'created_at', type: 'timestamp' },
      { name: 'updated_at', type: 'timestamp' },
    ]
    
    const tableSchemas = tables.map((table) => ({
      id: table.id,
      name: table.name,
      fields: defaultColumns,
    }))
    
    updateRqlTables(tableSchemas)
  }, [tables])

  // Register execute action once editor is ready
  const executeRef = useRef<() => void>(() => {})
  useEffect(() => {
    if (!editor || !monaco) return

    const action = editor.addAction({
      id: 'execute-query',
      label: 'Execute Query at Cursor',
      keybindings: [monaco.KeyMod.CtrlCmd | monaco.KeyCode.Enter],
      run: () => executeRef.current(),
    })

    return () => action.dispose()
  }, [editor, monaco])

  interface QuerySegment { query: string; start: number; end: number; hitStart: number; hitEnd: number }

  const getQuerySegments = useCallback((fullText: string): QuerySegment[] => {
    const raw: { query: string; start: number; end: number }[] = []

    if (fullText.includes(';')) {
      let pos = 0
      for (const part of fullText.split(';')) {
        const trimmed = part.trim()
        if (trimmed.length > 0) {
          const trimStart = pos + (part.length - part.trimStart().length)
          raw.push({ query: trimmed, start: trimStart, end: trimStart + trimmed.length })
        }
        pos += part.length + 1
      }
    } else {
      const kwStart = /^\s*(SELECT|EXPLAIN)\b/i
      const lines = fullText.split('\n')
      let curStart = -1
      let curEnd = 0
      let lineOff = 0

      const flush = () => {
        if (curStart < 0) return
        const slice = fullText.substring(curStart, curEnd)
        const trimmed = slice.trim()
        if (trimmed.length > 0) {
          const leadWs = slice.length - slice.trimStart().length
          const tStart = curStart + leadWs
          raw.push({ query: trimmed, start: tStart, end: tStart + trimmed.length })
        }
      }

      for (const line of lines) {
        if (kwStart.test(line) && curStart >= 0) {
          flush()
          curStart = lineOff
        }
        if (curStart < 0 && line.trim().length > 0) curStart = lineOff
        curEnd = lineOff + line.length
        lineOff += line.length + 1
      }
      flush()
    }

    if (raw.length === 0) return []

    const segments: QuerySegment[] = raw.map((r, i) => {
      const hitStart = i === 0 ? 0 : Math.ceil((raw[i - 1].end + r.start) / 2)
      const hitEnd = i === raw.length - 1 ? fullText.length : Math.floor((r.end + raw[i + 1].start) / 2)
      return { ...r, hitStart, hitEnd }
    })

    return segments
  }, [])

  const splitQueries = useCallback((text: string): string[] => {
    return getQuerySegments(text).map((s) => s.query)
  }, [getQuerySegments])

  const getQueryAtCursor = useCallback((): string | null => {
    if (!editor) return null
    const model = editor.getModel()
    if (!model) return null
    const position = editor.getPosition()
    if (!position) return null

    const fullText = model.getValue()
    const cursorOffset = model.getOffsetAt(position)
    const segments = getQuerySegments(fullText)

    for (const seg of segments) {
      if (cursorOffset >= seg.hitStart && cursorOffset <= seg.hitEnd) {
        return seg.query
      }
    }
    return segments.length > 0 ? segments[0].query : null
  }, [editor, getQuerySegments])

  const executeQueries = useCallback(async (queries: string[]) => {
    if (queries.length === 0 || isExecuting || !activeConnectionId || !activeConnection) return

    setIsExecuting(true)
    setError(null)

    try {
      if (onExecute) {
        await onExecute(queries.join(';\n'))
      } else {
        const client = createClient({
          host: activeConnection.host,
          port: activeConnection.port,
          apiKey: activeConnection.apiKey,
          useSsl: activeConnection.ssl,
        })

        const allResults: import('@/stores/queryStore').QueryResult[] = []

        for (let i = 0; i < queries.length; i++) {
          const singleQuery = queries[i]
          const startTime = Date.now()
          const isReasonQuery = /\bREASON\b/i.test(singleQuery)

          try {
            let result
            if (isReasonQuery) {
              setReasonProgress(null)
              result = await client.executeQueryStream(singleQuery, (progress) => {
                setReasonProgress(progress)
              })
            } else {
              result = await client.executeQuery(singleQuery)
            }

            const executionTime = Date.now() - startTime

            allResults.push({
              columns: result.columns || [],
              rows: result.rows || [],
              rowCount: result.rowCount || result.rows?.length || 0,
              executionTime,
            })

            addToHistory({
              query: singleQuery,
              connectionId: activeConnectionId,
              executedAt: new Date().toISOString(),
              executionTime,
              rowCount: result.rowCount || result.rows?.length || 0,
            })
          } catch (err) {
            const executionTime = Date.now() - startTime
            const errorMessage = err instanceof Error ? err.message : 'Query execution failed'
            const label = queries.length > 1 ? `Query ${i + 1} failed: ${errorMessage}` : errorMessage

            setError(label)

            addToHistory({
              query: singleQuery,
              connectionId: activeConnectionId,
              executedAt: new Date().toISOString(),
              executionTime,
              rowCount: 0,
              error: errorMessage,
            })

            if (allResults.length > 0) {
              setResults(allResults)
            }
            return
          }
        }

        setResults(allResults)
      }
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Query execution failed'
      setError(errorMessage)
    } finally {
      setIsExecuting(false)
      setReasonProgress(null)
    }
  }, [isExecuting, activeConnectionId, activeConnection, onExecute, setIsExecuting, setError, setResults, setReasonProgress, addToHistory])

  const handleExecute = useCallback(() => {
    const cursorQuery = getQueryAtCursor()
    if (cursorQuery) {
      executeQueries([cursorQuery])
    } else {
      const all = splitQueries(query.trim())
      if (all.length > 0) executeQueries(all)
    }
  }, [getQueryAtCursor, executeQueries, splitQueries, query])

  // Keep executeRef in sync for the Monaco action callback
  executeRef.current = handleExecute

  useHotkeys('mod+enter', () => handleExecute(), {
    enableOnFormTags: true,
    preventDefault: true,
  })

  // Statement highlight decorations
  const decorationsRef = useRef<string[]>([])
  useEffect(() => {
    if (!editor || !monaco) return

    const updateDecoration = () => {
      const model = editor.getModel()
      const position = editor.getPosition()
      if (!model || !position) return

      const fullText = model.getValue()
      const cursorOffset = model.getOffsetAt(position)
      const segments = getQuerySegments(fullText)

      let rangeToHighlight: Monaco.Range | null = null
      for (const seg of segments) {
        if (cursorOffset >= seg.hitStart && cursorOffset <= seg.hitEnd) {
          const startPos = model.getPositionAt(seg.start)
          const endPos = model.getPositionAt(seg.end)
          rangeToHighlight = new monaco.Range(
            startPos.lineNumber, startPos.column,
            endPos.lineNumber, endPos.column,
          )
          break
        }
      }

      if (rangeToHighlight) {
        decorationsRef.current = editor.deltaDecorations(decorationsRef.current, [{
          range: rangeToHighlight,
          options: { isWholeLine: true, className: 'active-statement-highlight' },
        }])
      } else {
        decorationsRef.current = editor.deltaDecorations(decorationsRef.current, [])
      }
    }

    updateDecoration()
    const d1 = editor.onDidChangeCursorPosition(updateDecoration)
    const d2 = editor.onDidChangeModelContent(updateDecoration)
    return () => { d1.dispose(); d2.dispose() }
  }, [editor, monaco, getQuerySegments])

  // Real-time syntax validation
  useEffect(() => {
    if (!activeConnection || !editor || !monaco) return

    const timer = setTimeout(() => {
      const model = editor.getModel()
      if (!model) return
      const text = model.getValue().trim()

      if (!text) {
        monaco.editor.setModelMarkers(model, 'rql-validation', [])
        return
      }

      const segments = getQuerySegments(text)
      if (segments.length === 0) return

      const client = createClient({
        host: activeConnection.host,
        port: activeConnection.port,
        apiKey: activeConnection.apiKey,
        useSsl: activeConnection.ssl,
      })

      client.validateQueries(segments.map((s) => s.query)).then((results) => {
        const currentModel = editor.getModel()
        if (!currentModel) return

        const fullText = currentModel.getValue()
        const markers: Monaco.editor.IMarkerData[] = []
        const fullSegments = getQuerySegments(fullText)

        for (const r of results) {
          if (r.valid) continue
          const seg = fullSegments[r.index]
          if (!seg) continue

          const startPos = currentModel.getPositionAt(seg.start)
          const endPos = currentModel.getPositionAt(seg.end)

          markers.push({
            severity: monaco.MarkerSeverity.Error,
            message: r.error ?? 'Invalid query syntax',
            startLineNumber: startPos.lineNumber + (r.line ? r.line - 1 : 0),
            startColumn: r.line === 1 && r.column ? startPos.column + r.column - 1 : r.column ?? startPos.column,
            endLineNumber: endPos.lineNumber,
            endColumn: endPos.column,
          })
        }

        monaco.editor.setModelMarkers(currentModel, 'rql-validation', markers)
      }).catch(() => {})
    }, 500)

    return () => clearTimeout(timer)
  }, [query, activeConnection, editor, monaco, getQuerySegments])

  return (
    <div className="flex flex-col h-full bg-base">
      {/* Toolbar */}
      <div className="flex items-center justify-between px-3 py-2 border-b border-border bg-mantle">
        <div className="flex items-center gap-3">
          <Button
            size="sm"
            onClick={handleExecute}
            disabled={isExecuting || !activeConnectionId || !query.trim()}
            className="gap-2 pr-3"
            title="Run statement at cursor (⌘+Enter)"
          >
            {isExecuting ? (
              <CircleNotch size={16} className="animate-spin" />
            ) : (
              <Play size={16} weight="fill" />
            )}
            Run
          </Button>

          <div className="h-4 w-px bg-border" />
          
          <Button
            size="sm"
            variant="ghost"
            disabled={!query.trim()}
            className="gap-2"
            title="Save query"
          >
            <FloppyDisk size={16} />
            Save
          </Button>
          
          <Button
            size="sm"
            variant="ghost"
            className="gap-2"
            title="View query history"
          >
            <Clock size={16} />
            History
          </Button>
        </div>

        <div className="flex items-center gap-3 text-xs">
          <div className="hidden sm:flex items-center gap-1 text-overlay-0">
            <Command size={12} />
            <span>+ Enter to run</span>
          </div>
          
          {activeConnection ? (
            <span className="flex items-center gap-1.5 text-subtext-0">
              <span className="w-2 h-2 rounded-full bg-green animate-pulse" />
              {activeConnection.name}
            </span>
          ) : (
            <span className="text-overlay-0">Not connected</span>
          )}
        </div>
      </div>

      {/* Semicolon warning */}
      {showSemicolonWarning && (
        <div className="flex items-center gap-2 px-3 py-1.5 bg-yellow/10 border-b border-yellow/30 text-yellow">
          <Warning size={14} weight="fill" className="shrink-0" />
          <span className="text-xs">
            It looks like you have multiple queries. Separate them with <code className="px-1 py-0.5 bg-yellow/10 rounded font-mono">;</code> to run them individually.
          </span>
          <button
            onClick={() => setWarningDismissed(true)}
            className="ml-auto shrink-0 p-0.5 rounded hover:bg-yellow/20 transition-colors"
            aria-label="Dismiss warning"
          >
            <X size={12} weight="bold" />
          </button>
        </div>
      )}

      {/* Editor container - the singleton Monaco editor attaches here */}
      <div ref={containerRef} className="flex-1 min-h-0">
        {!isReady && (
          <div className="flex items-center justify-center h-full text-subtext-0">
            <CircleNotch size={24} className="animate-spin" />
          </div>
        )}
      </div>
    </div>
  )
}
