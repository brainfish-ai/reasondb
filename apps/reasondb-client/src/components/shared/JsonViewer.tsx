import { useEffect, useRef } from 'react'
import Editor, { type Monaco, loader } from '@monaco-editor/react'
import { cn } from '@/lib/utils'

// Near-black & white theme palette
const palette = {
  base: '#09090b',
  mantle: '#0c0c0e',
  crust: '#050507',
  surface0: '#18181b',
  surface1: '#27272a',
  surface2: '#3f3f46',
  overlay0: '#a1a1aa',
  overlay1: '#d4d4d8',
  text: '#fafafa',
  mauve: '#a78bfa',
  red: '#f87171',
  peach: '#fdba74',
  yellow: '#fde047',
  green: '#4ade80',
  sapphire: '#22d3ee',
  blue: '#60a5fa',
  lavender: '#c4b5fd',
}

const defineTheme = (monaco: Monaco) => {
  monaco.editor.defineTheme('catppuccin-mocha-json', {
    base: 'vs-dark',
    inherit: false,
    rules: [
      { token: 'string.key.json', foreground: palette.blue.slice(1) },
      { token: 'string.value.json', foreground: palette.green.slice(1) },
      { token: 'number', foreground: palette.peach.slice(1) },
      { token: 'keyword', foreground: palette.mauve.slice(1) },
      { token: 'keyword.json', foreground: palette.peach.slice(1) },
      { token: 'delimiter', foreground: palette.overlay1.slice(1) },
      { token: 'delimiter.bracket', foreground: palette.overlay1.slice(1) },
      { token: 'comment', foreground: palette.overlay0.slice(1), fontStyle: 'italic' },
      { token: 'string', foreground: palette.green.slice(1) },
      { token: 'variable', foreground: palette.text.slice(1) },
      { token: 'type', foreground: palette.yellow.slice(1) },
    ],
    colors: {
      'editor.background': palette.base,
      'editor.foreground': palette.text,
      'editor.lineHighlightBackground': palette.surface0 + '40',
      'editor.selectionBackground': palette.surface2 + '80',
      'editor.inactiveSelectionBackground': palette.surface1 + '60',
      'editorLineNumber.foreground': palette.surface2,
      'editorLineNumber.activeForeground': palette.lavender,
      'editorCursor.foreground': palette.text,
      'editorWhitespace.foreground': palette.surface2,
      'editorIndentGuide.background': palette.surface1,
      'editorIndentGuide.activeBackground': palette.surface2,
      'editorBracketMatch.background': palette.surface2 + '40',
      'editorBracketMatch.border': palette.mauve,
      'editor.foldBackground': palette.surface0 + '40',
      'scrollbar.shadow': palette.crust,
      'scrollbarSlider.background': palette.surface2 + '80',
      'scrollbarSlider.hoverBackground': palette.overlay0,
      'scrollbarSlider.activeBackground': palette.overlay1,
      'editorGutter.background': palette.base,
      'editorWidget.background': palette.surface0,
      'editorWidget.border': palette.surface1,
      'editorBracketHighlight.foreground1': palette.red,
      'editorBracketHighlight.foreground2': palette.peach,
      'editorBracketHighlight.foreground3': palette.yellow,
      'editorBracketHighlight.foreground4': palette.green,
      'editorBracketHighlight.foreground5': palette.sapphire,
      'editorBracketHighlight.foreground6': palette.mauve,
    },
  })
}

// Initialize theme once
let themeInitialized = false
loader.init().then((monaco) => {
  if (!themeInitialized) {
    defineTheme(monaco)
    themeInitialized = true
  }
})

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

  // Format JSON with proper indentation
  const formattedJson = data !== undefined ? JSON.stringify(data, null, 2) : ''

  const handleEditorDidMount = (editor: unknown, monaco: Monaco) => {
    editorRef.current = editor

    // Ensure theme is defined
    if (!themeInitialized) {
      defineTheme(monaco)
      themeInitialized = true
    }

    // Configure JSON language features
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
        theme="catppuccin-mocha-json"
      />
    </div>
  )
}
