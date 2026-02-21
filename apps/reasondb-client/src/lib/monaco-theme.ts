import type { Monaco } from '@monaco-editor/react'
import { loader } from '@monaco-editor/react'

/**
 * Centralized Monaco editor palette matching the app's zinc-neutral dark theme.
 * All Monaco instances should reference this instead of defining their own.
 */
export const palette = {
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
  teal: '#5eead4',
  sky: '#38bdf8',
  sapphire: '#22d3ee',
  blue: '#60a5fa',
  lavender: '#c4b5fd',
  pink: '#f9a8d4',
} as const

export const THEME_NAME = 'reasondb-dark'

const jsonTokenRules = [
  { token: 'string.key.json', foreground: palette.blue.slice(1) },
  { token: 'string.value.json', foreground: palette.green.slice(1) },
  { token: 'number', foreground: palette.peach.slice(1) },
  { token: 'keyword', foreground: palette.mauve.slice(1) },
  { token: 'keyword.json', foreground: palette.peach.slice(1) },
  { token: 'delimiter', foreground: palette.overlay1.slice(1) },
  { token: 'delimiter.bracket', foreground: palette.overlay1.slice(1) },
  { token: 'comment', foreground: palette.overlay0.slice(1), fontStyle: 'italic' as const },
  { token: 'string', foreground: palette.green.slice(1) },
  { token: 'variable', foreground: palette.text.slice(1) },
  { token: 'type', foreground: palette.yellow.slice(1) },
]

export const editorColors: Record<string, string> = {
  'editor.background': palette.base,
  'editor.foreground': palette.text,
  'editor.lineHighlightBackground': palette.surface0 + '40',
  'editor.selectionBackground': palette.surface2 + '80',
  'editor.inactiveSelectionBackground': palette.surface1 + '60',
  'editor.selectionHighlightBackground': palette.surface1 + '80',
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
}

function defineThemes(monaco: Monaco) {
  monaco.editor.defineTheme(THEME_NAME, {
    base: 'vs-dark',
    inherit: false,
    rules: jsonTokenRules,
    colors: editorColors,
  })
}

let initialized = false

/**
 * Ensures the centralized Monaco theme is registered.
 * Safe to call multiple times — only registers once.
 */
export function ensureTheme(monaco: Monaco) {
  if (!initialized) {
    defineThemes(monaco)
    initialized = true
  }
}

// Eagerly register as soon as the Monaco loader resolves
loader.init().then((monaco) => {
  ensureTheme(monaco)
})
