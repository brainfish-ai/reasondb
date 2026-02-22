import { useState, useMemo, useRef, useCallback, useEffect } from 'react'
import { useVirtualizer } from '@tanstack/react-virtual'
import { cn } from '@/lib/utils'
import { palette } from '@/lib/monaco-theme'

export interface SyntaxViewerProps {
  content: string
  language?: 'json' | 'text'
  lineNumbers?: boolean
  className?: string
  maxHeight?: string
  onLineClick?: (lineNumber: number) => void
}

interface Token {
  text: string
  color: string
}

const JSON_PATTERNS: [RegExp, string][] = [
  [/^("(?:[^"\\]|\\.)*"\s*:)/, palette.blue],
  [/^("(?:[^"\\]|\\.)*")/, palette.green],
  [/^(-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?)/, palette.peach],
  [/^(true|false)/, palette.peach],
  [/^(null)/, palette.overlay0],
  [/^([{}[\],])/, palette.overlay1],
  [/^(:)/, palette.overlay1],
  [/^(\s+)/, ''],
]

function tokenizeLine(line: string): Token[] {
  const tokens: Token[] = []
  let remaining = line

  while (remaining.length > 0) {
    let matched = false
    for (const [pattern, color] of JSON_PATTERNS) {
      const m = remaining.match(pattern)
      if (m) {
        tokens.push({ text: m[1], color })
        remaining = remaining.slice(m[1].length)
        matched = true
        break
      }
    }
    if (!matched) {
      tokens.push({ text: remaining[0], color: palette.text })
      remaining = remaining.slice(1)
    }
  }

  return tokens
}

const LINE_HEIGHT = 20

export function SyntaxViewer({
  content,
  language = 'json',
  lineNumbers = true,
  className,
  maxHeight,
  onLineClick,
}: SyntaxViewerProps) {
  const parentRef = useRef<HTMLDivElement>(null)
  const lines = useMemo(() => content.split('\n'), [content])
  const gutterWidth = useMemo(() => `${String(lines.length).length + 1}ch`, [lines.length])

  const [collapsedRanges, setCollapsedRanges] = useState<Set<number>>(new Set())

  const foldableLines = useMemo(() => {
    const foldable = new Map<number, number>()
    if (language !== 'json') return foldable

    const stack: number[] = []
    for (let i = 0; i < lines.length; i++) {
      const trimmed = lines[i].trimEnd()
      const lastChar = trimmed[trimmed.length - 1]
      if (lastChar === '{' || lastChar === '[') {
        stack.push(i)
      } else if (trimmed.match(/^\s*[}\]],?\s*$/)) {
        const start = stack.pop()
        if (start !== undefined && i - start > 1) {
          foldable.set(start, i)
        }
      }
    }
    return foldable
  }, [lines, language])

  const visibleLineIndices = useMemo(() => {
    const visible: number[] = []
    let skip = -1
    for (let i = 0; i < lines.length; i++) {
      if (i <= skip) continue
      visible.push(i)
      if (collapsedRanges.has(i)) {
        const endLine = foldableLines.get(i)
        if (endLine !== undefined) {
          skip = endLine
        }
      }
    }
    return visible
  }, [lines, collapsedRanges, foldableLines])

  const virtualizer = useVirtualizer({
    count: visibleLineIndices.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => LINE_HEIGHT,
    overscan: 20,
  })

  const toggleFold = useCallback((lineIndex: number) => {
    setCollapsedRanges((prev) => {
      const next = new Set(prev)
      if (next.has(lineIndex)) {
        next.delete(lineIndex)
      } else {
        next.add(lineIndex)
      }
      return next
    })
  }, [])

  const tokenizedLines = useMemo(() => {
    if (language !== 'json') return null
    const map = new Map<number, Token[]>()
    for (const idx of visibleLineIndices) {
      map.set(idx, tokenizeLine(lines[idx]))
    }
    return map
  }, [lines, visibleLineIndices, language])

  return (
    <div
      ref={parentRef}
      className={cn(
        'overflow-auto font-mono text-[13px] leading-[20px]',
        className,
      )}
      style={{
        maxHeight: maxHeight ?? '100%',
        height: '100%',
        background: palette.base,
        color: palette.text,
      }}
    >
      <div
        style={{
          height: `${virtualizer.getTotalSize()}px`,
          width: '100%',
          position: 'relative',
        }}
      >
        {virtualizer.getVirtualItems().map((virtualRow) => {
          const lineIdx = visibleLineIndices[virtualRow.index]
          const line = lines[lineIdx]
          const isFoldable = foldableLines.has(lineIdx)
          const isCollapsed = collapsedRanges.has(lineIdx)
          const tokens = tokenizedLines?.get(lineIdx)

          return (
            <div
              key={virtualRow.index}
              style={{
                position: 'absolute',
                top: 0,
                left: 0,
                width: '100%',
                height: `${virtualRow.size}px`,
                transform: `translateY(${virtualRow.start}px)`,
              }}
              className="flex hover:bg-white/3"
              onClick={() => onLineClick?.(lineIdx + 1)}
            >
              {lineNumbers && (
                <span
                  className="shrink-0 text-right pr-3 pl-2 select-none"
                  style={{ width: gutterWidth, color: palette.surface2 }}
                >
                  {lineIdx + 1}
                </span>
              )}
              <span className="shrink-0 w-4 select-none text-center" style={{ color: palette.overlay0 }}>
                {isFoldable && (
                  <button
                    onClick={(e) => { e.stopPropagation(); toggleFold(lineIdx) }}
                    className="hover:text-white transition-colors w-full"
                    aria-label={isCollapsed ? 'Expand' : 'Collapse'}
                  >
                    {isCollapsed ? '▸' : '▾'}
                  </button>
                )}
              </span>
              <span className="flex-1 whitespace-pre overflow-hidden text-ellipsis pr-4">
                {tokens ? (
                  <>
                    {tokens.map((tok, i) => (
                      tok.color
                        ? <span key={i} style={{ color: tok.color }}>{tok.text}</span>
                        : <span key={i}>{tok.text}</span>
                    ))}
                    {isCollapsed && (
                      <span style={{ color: palette.overlay0 }}> ... </span>
                    )}
                  </>
                ) : (
                  line
                )}
              </span>
            </div>
          )
        })}
      </div>
    </div>
  )
}
