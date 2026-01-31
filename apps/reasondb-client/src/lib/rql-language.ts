import type * as Monaco from 'monaco-editor'
import { 
  getCompletions, 
  updateTableMetadataFields, 
  updateTableMetadataFieldsFromSchema,
  setValueFetcher,
  type DatabaseSchema, 
} from './sql-completion'
import { useSchemaStore, type MetadataSchemaField } from '@/stores/schemaStore'

// RQL Language Definition for Monaco Editor
export const RQL_LANGUAGE_ID = 'rql'

// Re-export for convenience
export { 
  updateTableMetadataFields, 
  updateTableMetadataFieldsFromSchema,
  setValueFetcher,
  type DatabaseSchema, 
  type MetadataSchemaField,
}

// Re-export store for direct access
export { useSchemaStore }

// Update tables for autocompletion (converts to store format)
export function updateRqlTables(tables: { id: string; name: string; fields: { name: string; type: string }[] }[]) {
  useSchemaStore.getState().setTables(
    tables.map(t => ({
      id: t.id,
      name: t.name,
      columns: t.fields.map(f => ({ name: f.name, type: f.type }))
    }))
  )
}

export const rqlLanguageConfig: Monaco.languages.LanguageConfiguration = {
  comments: {
    lineComment: '--',
    blockComment: ['/*', '*/'],
  },
  brackets: [
    ['{', '}'],
    ['[', ']'],
    ['(', ')'],
  ],
  autoClosingPairs: [
    { open: '{', close: '}' },
    { open: '[', close: ']' },
    { open: '(', close: ')' },
    { open: '"', close: '"' },
    { open: "'", close: "'" },
  ],
  surroundingPairs: [
    { open: '{', close: '}' },
    { open: '[', close: ']' },
    { open: '(', close: ')' },
    { open: '"', close: '"' },
    { open: "'", close: "'" },
  ],
}

export const rqlTokensProvider: Monaco.languages.IMonarchLanguage = {
  defaultToken: '',
  tokenPostfix: '.rql',
  ignoreCase: true,

  keywords: [
    // Query operations
    'SELECT', 'FROM', 'WHERE', 'INSERT', 'INTO', 'UPDATE', 'DELETE',
    'CREATE', 'DROP', 'ALTER', 'TABLE', 'INDEX',
    // Clauses
    'SET', 'VALUES', 'AND', 'OR', 'NOT', 'IN', 'LIKE', 'BETWEEN',
    'IS', 'NULL', 'TRUE', 'FALSE',
    'ORDER', 'BY', 'ASC', 'DESC', 'LIMIT', 'OFFSET',
    'GROUP', 'HAVING', 'JOIN', 'LEFT', 'RIGHT', 'INNER', 'OUTER', 'ON',
    'AS', 'DISTINCT', 'ALL', 'EXISTS',
    // ReasonDB specific
    'REASON', 'ABOUT', 'SEARCH', 'SEMANTIC', 'EMBED', 'SIMILAR', 'TO',
    'SUMMARIZE', 'EXTRACT', 'CHUNK', 'RELATE', 'LINK',
    'WITH', 'CONTEXT', 'THRESHOLD', 'TOP', 'VECTOR', 'CONTAINS',
  ],

  operators: [
    '=', '>', '<', '!', '~', '?', ':', '==', '<=', '>=', '!=',
    '&&', '||', '++', '--', '+', '-', '*', '/', '&', '|', '^', '%',
    '<<', '>>', '>>>', '+=', '-=', '*=', '/=', '&=', '|=', '^=',
    '%=', '<<=', '>>=', '>>>=', '->',
  ],

  builtinFunctions: [
    // Text functions
    'LOWER', 'UPPER', 'TRIM', 'LENGTH', 'SUBSTRING', 'CONCAT', 'REPLACE',
    // Numeric functions
    'ABS', 'CEIL', 'FLOOR', 'ROUND', 'SQRT', 'POW', 'MOD',
    // Aggregate functions
    'COUNT', 'SUM', 'AVG', 'MIN', 'MAX',
    // Date functions
    'NOW', 'DATE', 'TIME', 'YEAR', 'MONTH', 'DAY',
    // ReasonDB specific
    'SIMILARITY', 'DISTANCE', 'EMBEDDING', 'TOKENS', 'CHUNKS',
  ],

  symbols: /[=><!~?:&|+\-*\/\^%]+/,
  escapes: /\\(?:[abfnrtv\\"']|x[0-9A-Fa-f]{1,4}|u[0-9A-Fa-f]{4}|U[0-9A-Fa-f]{8})/,

  tokenizer: {
    root: [
      // Identifiers and keywords
      [
        /[a-zA-Z_$][\w$]*/,
        {
          cases: {
            '@keywords': 'keyword',
            '@builtinFunctions': 'predefined',
            '@default': 'identifier',
          },
        },
      ],

      // Whitespace
      { include: '@whitespace' },

      // Delimiters and operators
      [/[{}()\[\]]/, '@brackets'],
      [/[<>](?!@symbols)/, '@brackets'],
      [
        /@symbols/,
        {
          cases: {
            '@operators': 'operator',
            '@default': '',
          },
        },
      ],

      // Numbers
      [/\d*\.\d+([eE][\-+]?\d+)?/, 'number.float'],
      [/0[xX][0-9a-fA-F]+/, 'number.hex'],
      [/\d+/, 'number'],

      // Delimiter
      [/[;,.]/, 'delimiter'],

      // Strings
      [/"([^"\\]|\\.)*$/, 'string.invalid'],
      [/'([^'\\]|\\.)*$/, 'string.invalid'],
      [/"/, 'string', '@string_double'],
      [/'/, 'string', '@string_single'],
    ],

    whitespace: [
      [/[ \t\r\n]+/, 'white'],
      [/--.*$/, 'comment'],
      [/\/\*/, 'comment', '@comment'],
    ],

    comment: [
      [/[^\/*]+/, 'comment'],
      [/\*\//, 'comment', '@pop'],
      [/[\/*]/, 'comment'],
    ],

    string_double: [
      [/[^\\"]+/, 'string'],
      [/@escapes/, 'string.escape'],
      [/\\./, 'string.escape.invalid'],
      [/"/, 'string', '@pop'],
    ],

    string_single: [
      [/[^\\']+/, 'string'],
      [/@escapes/, 'string.escape'],
      [/\\./, 'string.escape.invalid'],
      [/'/, 'string', '@pop'],
    ],
  },
}

// RQL Theme colors (Catppuccin Mocha)
export const rqlTheme: Monaco.editor.IStandaloneThemeData = {
  base: 'vs-dark',
  inherit: true,
  rules: [
    { token: 'keyword', foreground: 'cba6f7', fontStyle: 'bold' }, // Mauve
    { token: 'predefined', foreground: '89b4fa' }, // Blue
    { token: 'identifier', foreground: 'cdd6f4' }, // Text
    { token: 'string', foreground: 'a6e3a1' }, // Green
    { token: 'string.escape', foreground: 'f5c2e7' }, // Pink
    { token: 'number', foreground: 'fab387' }, // Peach
    { token: 'number.float', foreground: 'fab387' },
    { token: 'number.hex', foreground: 'fab387' },
    { token: 'operator', foreground: '89dceb' }, // Sky
    { token: 'delimiter', foreground: '9399b2' }, // Overlay2
    { token: 'comment', foreground: '6c7086', fontStyle: 'italic' }, // Overlay0
    { token: 'white', foreground: 'cdd6f4' },
  ],
  colors: {
    'editor.background': '#1e1e2e', // Base
    'editor.foreground': '#cdd6f4', // Text
    'editor.lineHighlightBackground': '#313244', // Surface0
    'editor.selectionBackground': '#45475a', // Surface1
    'editorCursor.foreground': '#f5e0dc', // Rosewater
    'editorLineNumber.foreground': '#6c7086', // Overlay0
    'editorLineNumber.activeForeground': '#cdd6f4', // Text
    'editorIndentGuide.background': '#313244', // Surface0
    'editorIndentGuide.activeBackground': '#45475a', // Surface1
    'editor.selectionHighlightBackground': '#45475a80',
    'editorBracketMatch.background': '#45475a',
    'editorBracketMatch.border': '#89b4fa',
  },
}

// Register RQL language with Monaco
export function registerRqlLanguage(monaco: typeof Monaco) {
  // Register language
  monaco.languages.register({ id: RQL_LANGUAGE_ID })

  // Set language configuration
  monaco.languages.setLanguageConfiguration(RQL_LANGUAGE_ID, rqlLanguageConfig)

  // Set tokenizer
  monaco.languages.setMonarchTokensProvider(RQL_LANGUAGE_ID, rqlTokensProvider)

  // Register theme
  monaco.editor.defineTheme('rql-catppuccin', rqlTheme)

  // Register completion provider using new SQL completion engine
  monaco.languages.registerCompletionItemProvider(RQL_LANGUAGE_ID, {
    triggerCharacters: [' ', '.', ',', "'"],
    provideCompletionItems: async (model, position) => {
      const word = model.getWordUntilPosition(position)
      const range: Monaco.IRange = {
        startLineNumber: position.lineNumber,
        endLineNumber: position.lineNumber,
        startColumn: word.startColumn,
        endColumn: word.endColumn,
      }
      
      // Get full text and cursor offset
      const fullText = model.getValue()
      const cursorOffset = model.getOffsetAt(position)
      
      const suggestions = await getCompletions(monaco, fullText, cursorOffset, range)
      return { suggestions }
    },
  })
}

// Re-export for testing (kept for backward compatibility)
export { detectContext as getCompletionContext } from './sql-completion'
