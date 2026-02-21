import type * as Monaco from 'monaco-editor'
import { 
  getCompletions, 
  updateTableMetadataFields, 
  updateTableMetadataFieldsFromSchema,
  setValueFetcher,
  type DatabaseSchema, 
} from './sql-completion'
import { useSchemaStore, type MetadataSchemaField } from '@/stores/schemaStore'
import { palette, editorColors } from './monaco-theme'

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

const hex = (color: string) => color.slice(1)

export const rqlTheme: Monaco.editor.IStandaloneThemeData = {
  base: 'vs-dark',
  inherit: false,
  rules: [
    { token: 'keyword', foreground: hex(palette.mauve), fontStyle: 'bold' },
    { token: 'predefined', foreground: hex(palette.blue) },
    { token: 'identifier', foreground: hex(palette.text) },
    { token: 'string', foreground: hex(palette.green) },
    { token: 'string.escape', foreground: hex(palette.pink) },
    { token: 'number', foreground: hex(palette.peach) },
    { token: 'number.float', foreground: hex(palette.peach) },
    { token: 'number.hex', foreground: hex(palette.peach) },
    { token: 'operator', foreground: hex(palette.sky) },
    { token: 'delimiter', foreground: hex(palette.overlay1) },
    { token: 'comment', foreground: hex(palette.overlay0), fontStyle: 'italic' },
    { token: 'white', foreground: hex(palette.text) },
  ],
  colors: editorColors,
}

// Track if language is already registered
let isRegistered = false

// Register RQL language with Monaco
export function registerRqlLanguage(monaco: typeof Monaco) {
  // Prevent multiple registrations
  if (isRegistered) {
    return
  }
  isRegistered = true
  
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
    triggerCharacters: [' ', '.', ','],
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
