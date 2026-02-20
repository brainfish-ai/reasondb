import { useRef, useEffect, useCallback } from 'react'
import Editor, { loader } from '@monaco-editor/react'
import type * as Monaco from 'monaco-editor'
import { useHotkeys } from 'react-hotkeys-hook'
import { Play, FloppyDisk, Clock, CircleNotch, Command } from '@phosphor-icons/react'
import { registerRqlLanguage, RQL_LANGUAGE_ID, updateRqlTables } from '@/lib/rql-language'
import { useQueryStore } from '@/stores/queryStore'
import { useConnectionStore } from '@/stores/connectionStore'
import { useTableStore } from '@/stores/tableStore'
import { createClient } from '@/lib/api'
import { Button } from '@/components/ui/Button'
import { cn } from '@/lib/utils'

interface QueryEditorProps {
  onExecute?: (query: string) => Promise<void>
  initialQuery?: string
  onQueryChange?: (query: string) => void
}

export function QueryEditor({ onExecute, initialQuery, onQueryChange }: QueryEditorProps) {
  const editorRef = useRef<Monaco.editor.IStandaloneCodeEditor | null>(null)
  const monacoRef = useRef<typeof Monaco | null>(null)
  
  const { currentQuery, setCurrentQuery, isExecuting, setIsExecuting, setResult, setError, addToHistory } = useQueryStore()
  
  // Use initialQuery if provided (even if empty), otherwise fall back to global currentQuery
  const query = initialQuery !== undefined ? initialQuery : currentQuery
  const { activeConnectionId, connections } = useConnectionStore()
  const { tables } = useTableStore()
  
  const activeConnection = connections.find((c) => c.id === activeConnectionId)

  // Sync tables for autocompletion - runs whenever tables change
  useEffect(() => {
    if (tables.length === 0) {
      return // Don't clear schema when tables aren't loaded yet
    }
    
    // Convert tables to the format expected by RQL language
    // Use actual columns from table if available, otherwise use default schema
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

  // Register RQL language on Monaco load
  const handleEditorWillMount = useCallback((monaco: typeof Monaco) => {
    registerRqlLanguage(monaco)
    monacoRef.current = monaco
  }, [])

  const handleEditorDidMount = useCallback((editor: Monaco.editor.IStandaloneCodeEditor, monaco: typeof Monaco) => {
    editorRef.current = editor
    monacoRef.current = monaco
    
    // Focus editor on mount
    editor.focus()
    
    // Add keyboard shortcuts
    editor.addAction({
      id: 'execute-query',
      label: 'Execute Query',
      keybindings: [
        monaco.KeyMod.CtrlCmd | monaco.KeyCode.Enter,
      ],
      run: () => {
        handleExecute()
      },
    })
  }, [])

  // Execute query
  const handleExecute = useCallback(async () => {
    const queryText = query.trim()
    if (!queryText || isExecuting || !activeConnectionId || !activeConnection) return

    setIsExecuting(true)
    setError(null)
    
    const startTime = Date.now()

    try {
      if (onExecute) {
        await onExecute(queryText)
      } else {
        // Execute query against the server
        const client = createClient({
          host: activeConnection.host,
          port: activeConnection.port,
          apiKey: activeConnection.apiKey,
          useSsl: activeConnection.ssl,
        })
        
        const result = await client.executeQuery(queryText)
        const executionTime = Date.now() - startTime
        
        setResult({
          columns: result.columns || [],
          rows: result.rows || [],
          rowCount: result.rowCount || result.rows?.length || 0,
          executionTime,
        })
        
        addToHistory({
          query: queryText,
          connectionId: activeConnectionId,
          executedAt: new Date().toISOString(),
          executionTime,
          rowCount: result.rowCount || result.rows?.length || 0,
        })
      }
    } catch (err) {
      const executionTime = Date.now() - startTime
      const errorMessage = err instanceof Error ? err.message : 'Query execution failed'
      
      setError(errorMessage)
      
      addToHistory({
        query: queryText,
        connectionId: activeConnectionId,
        executedAt: new Date().toISOString(),
        executionTime,
        rowCount: 0,
        error: errorMessage,
      })
    } finally {
      setIsExecuting(false)
    }
  }, [query, isExecuting, activeConnectionId, activeConnection, onExecute, setIsExecuting, setError, setResult, addToHistory])

  // Keyboard shortcut for execute
  useHotkeys('mod+enter', () => handleExecute(), {
    enableOnFormTags: true,
    preventDefault: true,
  })

  // Configure Monaco loader
  useEffect(() => {
    loader.config({
      paths: {
        vs: 'https://cdn.jsdelivr.net/npm/monaco-editor@0.55.1/min/vs',
      },
    })
  }, [])

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
            title="Execute query (⌘+Enter)"
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
          {/* Keyboard shortcut hint */}
          <div className="hidden sm:flex items-center gap-1 text-overlay-0">
            <Command size={12} />
            <span>+ Enter to run</span>
          </div>
          
          {/* Connection status */}
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

      {/* Editor */}
      <div className="flex-1 min-h-0">
        <Editor
          height="100%"
          language={RQL_LANGUAGE_ID}
          theme="rql-catppuccin"
          value={query}
          onChange={(value) => {
            const newValue = value || ''
            setCurrentQuery(newValue)
            onQueryChange?.(newValue)
          }}
          beforeMount={handleEditorWillMount}
          onMount={handleEditorDidMount}
          options={{
            minimap: { enabled: false },
            fontSize: 14,
            fontFamily: "'JetBrains Mono', 'Fira Code', 'SF Mono', Consolas, monospace",
            lineNumbers: 'on',
            renderLineHighlight: 'all',
            scrollBeyondLastLine: false,
            wordWrap: 'on',
            automaticLayout: true,
            tabSize: 2,
            padding: { top: 12, bottom: 12 },
            suggestOnTriggerCharacters: true,
            quickSuggestions: true,
            folding: true,
            bracketPairColorization: { enabled: true },
            guides: {
              bracketPairs: true,
              indentation: true,
            },
            scrollbar: {
              verticalScrollbarSize: 8,
              horizontalScrollbarSize: 8,
            },
            placeholder: activeConnectionId 
              ? 'Enter your RQL query here... (⌘+Enter to execute)'
              : 'Connect to a database to start querying...',
          }}
          loading={
            <div className="flex items-center justify-center h-full text-subtext-0">
              <CircleNotch size={24} className="animate-spin" />
            </div>
          }
        />
      </div>
    </div>
  )
}
