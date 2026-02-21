import { useRef } from 'react'
import Editor, { type Monaco } from '@monaco-editor/react'
import { cn } from '@/lib/utils'
import { THEME_NAME, ensureTheme } from '@/lib/monaco-theme'

export interface JsonViewerProps {
  /** Data to display as JSON */
  data: unknown
  /** Height of the editor (default: 100%) */
  height?: string | number
  /** Show line numbers */
  lineNumbers?: boolean
  /** Show minimap */
  minimap?: boolean
  /** Custom class */
  className?: string
  /** Empty state message */
  emptyMessage?: string
}

export function JsonViewer({
  data,
  height = '100%',
  lineNumbers = true,
  minimap = false,
  className,
  emptyMessage = 'No data to display',
}: JsonViewerProps) {
  const editorRef = useRef<unknown>(null)

  const formattedJson = data !== undefined ? JSON.stringify(data, null, 2) : ''

  const handleEditorDidMount = (editor: unknown, monaco: Monaco) => {
    editorRef.current = editor
    ensureTheme(monaco)

    monaco.languages.json.jsonDefaults.setDiagnosticsOptions({
      validate: true,
      allowComments: false,
      schemas: [],
    })
  }

  if (data === undefined || data === null || (Array.isArray(data) && data.length === 0)) {
    return (
      <div className={cn('flex items-center justify-center h-full text-overlay-0 text-sm', className)}>
        {emptyMessage}
      </div>
    )
  }

  return (
    <div className={cn('h-full', className)}>
      <Editor
        height={height}
        language="json"
        value={formattedJson}
        onMount={handleEditorDidMount}
        options={{
          readOnly: true,
          minimap: { enabled: minimap },
          fontSize: 13,
          fontFamily: 'JetBrains Mono, Menlo, Monaco, monospace',
          lineNumbers: lineNumbers ? 'on' : 'off',
          scrollBeyondLastLine: false,
          automaticLayout: true,
          wordWrap: 'on',
          folding: true,
          foldingStrategy: 'indentation',
          showFoldingControls: 'always',
          bracketPairColorization: { enabled: true },
          guides: {
            bracketPairs: true,
            indentation: true,
          },
          renderLineHighlight: 'line',
          scrollbar: {
            vertical: 'auto',
            horizontal: 'auto',
            verticalScrollbarSize: 10,
            horizontalScrollbarSize: 10,
          },
          padding: { top: 12, bottom: 12 },
        }}
        theme={THEME_NAME}
      />
    </div>
  )
}
