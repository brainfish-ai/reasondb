import { useEffect, useRef, useState } from 'react'
import Editor, { type Monaco, loader } from '@monaco-editor/react'
import { X, Copy, CheckCircle, ArrowsOut, ArrowsIn } from '@phosphor-icons/react'
import { cn } from '@/lib/utils'

// Catppuccin Mocha theme colors
const catppuccinMocha = {
  base: '#1e1e2e',
  mantle: '#181825',
  crust: '#11111b',
  surface0: '#313244',
  surface1: '#45475a',
  surface2: '#585b70',
  overlay0: '#6c7086',
  overlay1: '#7f849c',
  overlay2: '#9399b2',
  text: '#cdd6f4',
  subtext0: '#a6adc8',
  subtext1: '#bac2de',
  rosewater: '#f5e0dc',
  flamingo: '#f2cdcd',
  pink: '#f5c2e7',
  mauve: '#cba6f7',
  red: '#f38ba8',
  maroon: '#eba0ac',
  peach: '#fab387',
  yellow: '#f9e2af',
  green: '#a6e3a1',
  teal: '#94e2d5',
  sky: '#89dceb',
  sapphire: '#74c7ec',
  blue: '#89b4fa',
  lavender: '#b4befe',
}

// Define custom Monaco theme
const defineTheme = (monaco: Monaco) => {
  monaco.editor.defineTheme('catppuccin-mocha', {
    base: 'vs-dark',
    inherit: false,
    rules: [
      // JSON specific
      { token: 'string.key.json', foreground: catppuccinMocha.blue.slice(1) },
      { token: 'string.value.json', foreground: catppuccinMocha.green.slice(1) },
      { token: 'number', foreground: catppuccinMocha.peach.slice(1) },
      { token: 'keyword', foreground: catppuccinMocha.mauve.slice(1) },
      { token: 'keyword.json', foreground: catppuccinMocha.peach.slice(1) }, // true, false, null
      { token: 'delimiter', foreground: catppuccinMocha.overlay2.slice(1) },
      { token: 'delimiter.bracket', foreground: catppuccinMocha.overlay2.slice(1) },
      // General
      { token: 'comment', foreground: catppuccinMocha.overlay0.slice(1), fontStyle: 'italic' },
      { token: 'string', foreground: catppuccinMocha.green.slice(1) },
      { token: 'variable', foreground: catppuccinMocha.text.slice(1) },
      { token: 'type', foreground: catppuccinMocha.yellow.slice(1) },
    ],
    colors: {
      'editor.background': catppuccinMocha.mantle,
      'editor.foreground': catppuccinMocha.text,
      'editor.lineHighlightBackground': catppuccinMocha.surface0 + '40',
      'editor.selectionBackground': catppuccinMocha.surface2 + '80',
      'editor.inactiveSelectionBackground': catppuccinMocha.surface1 + '60',
      'editorLineNumber.foreground': catppuccinMocha.surface2,
      'editorLineNumber.activeForeground': catppuccinMocha.lavender,
      'editorCursor.foreground': catppuccinMocha.rosewater,
      'editorWhitespace.foreground': catppuccinMocha.surface2,
      'editorIndentGuide.background': catppuccinMocha.surface1,
      'editorIndentGuide.activeBackground': catppuccinMocha.surface2,
      'editorBracketMatch.background': catppuccinMocha.surface2 + '40',
      'editorBracketMatch.border': catppuccinMocha.mauve,
      'editor.foldBackground': catppuccinMocha.surface0 + '40',
      'scrollbar.shadow': catppuccinMocha.crust,
      'scrollbarSlider.background': catppuccinMocha.surface2 + '80',
      'scrollbarSlider.hoverBackground': catppuccinMocha.overlay0,
      'scrollbarSlider.activeBackground': catppuccinMocha.overlay1,
      'editorGutter.background': catppuccinMocha.mantle,
      'editorWidget.background': catppuccinMocha.surface0,
      'editorWidget.border': catppuccinMocha.surface1,
      'editorBracketHighlight.foreground1': catppuccinMocha.red,
      'editorBracketHighlight.foreground2': catppuccinMocha.peach,
      'editorBracketHighlight.foreground3': catppuccinMocha.yellow,
      'editorBracketHighlight.foreground4': catppuccinMocha.green,
      'editorBracketHighlight.foreground5': catppuccinMocha.sapphire,
      'editorBracketHighlight.foreground6': catppuccinMocha.mauve,
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

interface JsonDetailSidebarProps {
  isOpen: boolean
  onClose: () => void
  title: string
  data: unknown
  path?: string // The path to the data (e.g., "metadata.employee")
  isLoading?: boolean
}

const MIN_WIDTH = 300
const MAX_WIDTH = window.innerWidth * 0.8
const DEFAULT_WIDTH = 400

export function JsonDetailSidebar({ isOpen, onClose, title, data, path, isLoading }: JsonDetailSidebarProps) {
  const [copied, setCopied] = useState(false)
  const [isExpanded, setIsExpanded] = useState(false)
  const [isVisible, setIsVisible] = useState(false)
  const [width, setWidth] = useState(DEFAULT_WIDTH)
  const [isDragging, setIsDragging] = useState(false)
  const editorRef = useRef<unknown>(null)

  // Check if data indicates loading
  const showLoading = isLoading || (data && typeof data === 'object' && 'loading' in (data as Record<string, unknown>))

  // Format JSON with proper indentation (handle undefined data)
  const formattedJson = showLoading || data === undefined ? '' : JSON.stringify(data, null, 2)

  // Handle open/close animation
  useEffect(() => {
    if (isOpen) {
      // Small delay to trigger CSS transition
      requestAnimationFrame(() => {
        setIsVisible(true)
      })
    } else {
      setIsVisible(false)
    }
  }, [isOpen])

  // Handle drag resize
  useEffect(() => {
    if (!isDragging) return

    const handleMouseMove = (e: MouseEvent) => {
      const newWidth = window.innerWidth - e.clientX
      setWidth(Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, newWidth)))
    }

    const handleMouseUp = () => {
      setIsDragging(false)
    }

    document.addEventListener('mousemove', handleMouseMove)
    document.addEventListener('mouseup', handleMouseUp)
    document.body.style.cursor = 'col-resize'
    document.body.style.userSelect = 'none'

    return () => {
      document.removeEventListener('mousemove', handleMouseMove)
      document.removeEventListener('mouseup', handleMouseUp)
      document.body.style.cursor = ''
      document.body.style.userSelect = ''
    }
  }, [isDragging])

  const handleDragStart = (e: React.MouseEvent) => {
    e.preventDefault()
    setIsDragging(true)
  }

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(formattedJson)
      setCopied(true)
      setTimeout(() => setCopied(false), 2000)
    } catch (err) {
      console.error('Failed to copy:', err)
    }
  }

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

  // Close on escape key
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && isOpen) {
        onClose()
      }
    }
    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [isOpen, onClose])

  if (!isOpen && !isVisible) return null

  return (
    <>
      {/* Backdrop - always show when sidebar is open */}
      <div 
        className={cn(
          'fixed inset-0 bg-black/30 z-40 transition-opacity duration-300',
          isVisible ? 'opacity-100' : 'opacity-0 pointer-events-none'
        )}
        onClick={isExpanded ? () => setIsExpanded(false) : onClose}
      />
      
      {/* Sidebar */}
      <div
        className={cn(
          'fixed inset-y-0 right-0 flex flex-col bg-mantle border-l border-border shadow-2xl z-50',
          'transition-transform duration-300 ease-out',
          isVisible ? 'translate-x-0' : 'translate-x-full'
        )}
        style={{ width: isExpanded ? '60vw' : width }}
      >
        {/* Drag handle */}
        <div
          onMouseDown={handleDragStart}
          className={cn(
            'absolute left-0 top-0 bottom-0 w-1 cursor-col-resize z-10',
            'hover:bg-mauve/50 transition-colors',
            isDragging && 'bg-mauve'
          )}
        />
        {/* Header */}
        <div className="flex items-center justify-between px-4 py-3 border-b border-border bg-surface-0/30">
          <div className="flex flex-col gap-0.5 min-w-0 flex-1">
            <h3 className="text-sm font-semibold text-text truncate">{title}</h3>
            {path && (
              <span className="text-xs text-overlay-0 font-mono truncate">{path}</span>
            )}
          </div>
          <div className="flex items-center gap-1 ml-2">
            <button
              onClick={handleCopy}
              className={cn(
                'p-1.5 rounded transition-colors',
                'hover:bg-surface-1 text-overlay-1 hover:text-text'
              )}
              title="Copy JSON"
            >
              {copied ? (
                <CheckCircle size={16} weight="fill" className="text-green" />
              ) : (
                <Copy size={16} />
              )}
            </button>
            <button
              onClick={() => setIsExpanded(!isExpanded)}
              className={cn(
                'p-1.5 rounded transition-colors',
                'hover:bg-surface-1 text-overlay-1 hover:text-text'
              )}
              title={isExpanded ? 'Collapse' : 'Expand'}
            >
              {isExpanded ? <ArrowsIn size={16} /> : <ArrowsOut size={16} />}
            </button>
            <button
              onClick={onClose}
              className={cn(
                'p-1.5 rounded transition-colors',
                'hover:bg-surface-1 text-overlay-1 hover:text-text'
              )}
              title="Close (Esc)"
            >
              <X size={16} weight="bold" />
            </button>
          </div>
        </div>

        {/* Type info */}
        {!showLoading && (
          <div className="px-4 py-2 border-b border-border bg-surface-0/20">
            <div className="flex items-center gap-2 text-xs">
              <span className="text-overlay-0">Type:</span>
              <span className={cn(
                'px-1.5 py-0.5 rounded font-mono',
                Array.isArray(data) ? 'bg-blue/20 text-blue' :
                typeof data === 'object' ? 'bg-mauve/20 text-mauve' :
                typeof data === 'string' ? 'bg-green/20 text-green' :
                typeof data === 'number' ? 'bg-peach/20 text-peach' :
                'bg-overlay-0/20 text-overlay-1'
              )}>
                {Array.isArray(data) ? `array[${data.length}]` : typeof data}
              </span>
              {typeof data === 'object' && data !== null && !Array.isArray(data) && (
                <>
                  <span className="text-overlay-0">•</span>
                  <span className="text-overlay-0">
                    {Object.keys(data).length} keys
                  </span>
                </>
              )}
            </div>
          </div>
        )}

        {/* Monaco Editor or Loading State */}
        <div className="flex-1 min-h-0">
          {showLoading ? (
            <div className="flex flex-col items-center justify-center h-full gap-3">
              <div className="w-8 h-8 border-2 border-mauve border-t-transparent rounded-full animate-spin" />
              <span className="text-sm text-overlay-1">Loading content...</span>
            </div>
          ) : (
            <Editor
              height="100%"
              language="json"
              value={formattedJson}
              onMount={handleEditorDidMount}
              options={{
                readOnly: true,
                minimap: { enabled: false },
                fontSize: 13,
                fontFamily: 'JetBrains Mono, Menlo, Monaco, monospace',
                lineNumbers: 'on',
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
              theme="catppuccin-mocha"
            />
          )}
        </div>

        {/* Footer with quick stats */}
        {!showLoading && (
          <div className="px-4 py-2 border-t border-border bg-surface-0/20">
            <div className="flex items-center justify-between text-xs text-overlay-0">
              <span>{formattedJson.split('\n').length} lines</span>
              <span>{new Blob([formattedJson]).size} bytes</span>
            </div>
          </div>
        )}
      </div>
    </>
  )
}
