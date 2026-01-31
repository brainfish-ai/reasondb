/**
 * SQL Completion Engine
 * 
 * Uses node-sql-parser for AST-based context detection
 * and provides intelligent autocompletion suggestions.
 * 
 * Schema state is managed by schemaStore for reactivity.
 */

import { Parser } from 'node-sql-parser'
import type * as Monaco from 'monaco-editor'
import { useSchemaStore } from '@/stores/schemaStore'

// Re-export types from store
export type { ColumnSchema, TableSchema, MetadataSchemaField } from '@/stores/schemaStore'

// Legacy types for backwards compatibility
export interface DatabaseSchema {
  tables: Array<{ name: string; columns: Array<{ name: string; type: string }> }>
}

// Completion context
export type CompletionContext = 
  | 'keyword'      // Start of query or after complete clause
  | 'table'        // After FROM, INTO, UPDATE, JOIN
  | 'column'       // After SELECT, WHERE field position
  | 'operator'     // After column in WHERE
  | 'value'        // After operator
  | 'alias'        // After table reference with alias

// Parser instance (reused)
const parser = new Parser()

// Table aliases in current query
let tableAliases: Map<string, string> = new Map()

// Value fetcher function (set by frontend)
type ValueFetcher = (tableId: string, column: string) => Promise<string[]>
let valueFetcher: ValueFetcher | null = null

/**
 * Set the value fetcher function (called from frontend with API client)
 */
export function setValueFetcher(fetcher: ValueFetcher) {
  valueFetcher = fetcher
}

/**
 * Get cached values or fetch from server
 */
async function getColumnValues(tableName: string, column: string): Promise<string[]> {
  const store = useSchemaStore.getState()
  
  if (!valueFetcher) {
    return []
  }
  
  const table = store.getTableByName(tableName)
  if (!table) {
    return []
  }
  
  // Check cache
  const cached = store.getCachedValues(table.id, column)
  if (cached) {
    return cached
  }
  
  try {
    const values = await valueFetcher(table.id, column)
    store.setCachedValues(table.id, column, values)
    return values
  } catch {
    return []
  }
}

/**
 * Get current schema from store
 */
function getCurrentSchema(): DatabaseSchema {
  return useSchemaStore.getState().getSchema()
}

/**
 * Extract all field paths from a nested object
 * Returns paths like "metadata.author", "metadata.author.name", etc.
 */
function extractFieldPaths(obj: unknown, prefix = '', maxDepth = 4): string[] {
  if (maxDepth <= 0 || obj === null || typeof obj !== 'object') {
    return []
  }
  
  const paths: string[] = []
  
  for (const [key, value] of Object.entries(obj as Record<string, unknown>)) {
    const fullPath = prefix ? `${prefix}.${key}` : key
    paths.push(fullPath)
    
    // Recurse into nested objects (but not arrays)
    if (value !== null && typeof value === 'object' && !Array.isArray(value)) {
      paths.push(...extractFieldPaths(value, fullPath, maxDepth - 1))
    }
  }
  
  return paths
}

/**
 * Infer type from a value
 */
function inferType(value: unknown): string {
  if (value === null || value === undefined) return 'unknown'
  if (typeof value === 'string') return 'text'
  if (typeof value === 'number') return Number.isInteger(value) ? 'integer' : 'float'
  if (typeof value === 'boolean') return 'boolean'
  if (Array.isArray(value)) return 'array'
  if (typeof value === 'object') return 'jsonb'
  return 'unknown'
}

/**
 * Get value at a nested path
 */
function getValueAtPath(obj: unknown, path: string): unknown {
  const parts = path.split('.')
  let current: unknown = obj
  
  for (const part of parts) {
    if (current === null || typeof current !== 'object') return undefined
    current = (current as Record<string, unknown>)[part]
  }
  
  return current
}

/**
 * Update metadata fields for a specific table from server-side schema response
 * Delegates to schemaStore for state management
 */
export function updateTableMetadataFieldsFromSchema(
  tableName: string,
  fields: Array<{ path: string; field_type: string; occurrence_count?: number }>
) {
  useSchemaStore.getState().addMetadataFields(tableName, fields.map(f => ({
    path: f.path,
    field_type: f.field_type,
    occurrence_count: f.occurrence_count ?? 0,
  })))
}

/**
 * Update metadata fields for a specific table based on document data (client-side extraction)
 * Use updateTableMetadataFieldsFromSchema for server-side extraction when available
 */
export function updateTableMetadataFields(
  tableName: string,
  documents: Array<{ metadata?: Record<string, unknown> }>
) {
  // Extract all unique metadata field paths from documents
  const fieldPaths = new Set<string>()
  const fieldTypes = new Map<string, string>()
  
  for (const doc of documents) {
    if (doc.metadata && typeof doc.metadata === 'object') {
      const paths = extractFieldPaths(doc.metadata)
      for (const path of paths) {
        fieldPaths.add(path)
        // Try to infer type from first non-null value
        if (!fieldTypes.has(path)) {
          const value = getValueAtPath(doc.metadata, path)
          if (value !== undefined && value !== null) {
            fieldTypes.set(path, inferType(value))
          }
        }
      }
    }
  }
  
  if (fieldPaths.size === 0) {
    return
  }
  
  // Convert to MetadataSchemaField format and delegate to store
  const fields = Array.from(fieldPaths).map(path => ({
    path,
    field_type: fieldTypes.get(path) || 'unknown',
    occurrence_count: 1,
  }))
  
  useSchemaStore.getState().addMetadataFields(tableName, fields)
}

/**
 * Get the current schema
 */
export function getSchema(): DatabaseSchema {
  return getCurrentSchema()
}

/**
 * Parse query and extract table aliases
 */
function extractAliases(sql: string): Map<string, string> {
  const aliases = new Map<string, string>()
  
  try {
    // Try to parse - might fail for incomplete queries
    const ast = parser.astify(sql, { database: 'PostgreSQL' })
    
    if (ast && !Array.isArray(ast) && ast.type === 'select' && ast.from && Array.isArray(ast.from)) {
      for (const fromItem of ast.from) {
        if (fromItem && typeof fromItem === 'object' && 'table' in fromItem && 'as' in fromItem) {
          aliases.set(String(fromItem.as), String(fromItem.table))
        }
      }
    }
  } catch {
    // Fallback: regex-based alias extraction for incomplete queries
    const aliasRegex = /\bFROM\s+(\w+)\s+(?:AS\s+)?(\w+)/gi
    let match
    while ((match = aliasRegex.exec(sql)) !== null) {
      aliases.set(match[2], match[1])
    }
    
    const joinAliasRegex = /\bJOIN\s+(\w+)\s+(?:AS\s+)?(\w+)/gi
    while ((match = joinAliasRegex.exec(sql)) !== null) {
      aliases.set(match[2], match[1])
    }
  }
  
  return aliases
}

/**
 * Extract table name from FROM clause
 */
function extractFromTable(sql: string): string | undefined {
  const match = sql.match(/\bFROM\s+(\w+)/i)
  return match ? match[1] : undefined
}

/**
 * Extract column name from WHERE clause before operator
 */
function extractWhereColumn(textBefore: string): string | undefined {
  // Match patterns like "WHERE column =", "AND metadata.field ="
  const match = textBefore.match(/\b(?:WHERE|AND|OR)\s+([\w.]+)\s+(?:=|!=|<>|>|<|>=|<=|LIKE|IN|BETWEEN)\s*$/i)
  return match ? match[1] : undefined
}

/**
 * Detect completion context from cursor position
 */
export function detectContext(sql: string, cursorOffset: number): {
  context: CompletionContext
  prefix: string
  tableName?: string  // For alias.column completion or value context
  columnName?: string // For value context - which column we're filtering
  fromTable?: string  // The table in FROM clause
} {
  const textBefore = sql.substring(0, cursorOffset)
  const trimmedText = textBefore.trim()
  const upperText = trimmedText.toUpperCase()
  
  // Update aliases
  tableAliases = extractAliases(sql)
  
  // Extract FROM table for value context
  const fromTable = extractFromTable(sql)
  
  // Check if we're typing after a dot (alias.column or table.column)
  // But NOT if there's a space after it (then we need operator)
  const dotMatch = trimmedText.match(/(\w+)\.(\w*)$/i)
  if (dotMatch && !textBefore.endsWith(' ')) {
    const [, tableOrAlias, columnPrefix] = dotMatch
    // Resolve alias to table name
    const tableName = tableAliases.get(tableOrAlias) || tableOrAlias
    return { context: 'column', prefix: columnPrefix || '', tableName, fromTable }
  }
  
  // Get the word being typed
  const wordMatch = trimmedText.match(/(\w*)$/i)
  const prefix = wordMatch ? wordMatch[1] : ''
  
  // After FROM, JOIN, INTO, UPDATE - expect table
  if (/\b(FROM|JOIN|INTO|UPDATE|TABLE)\s*$/i.test(upperText)) {
    return { context: 'table', prefix, fromTable }
  }
  
  // After FROM/JOIN/INTO/UPDATE + partial table name being typed (no space after)
  // This catches cases like "FROM kno" where "kno" is partial table name
  if (/\b(FROM|JOIN|INTO|UPDATE|TABLE)\s+\w+$/i.test(trimmedText) && !textBefore.endsWith(' ')) {
    return { context: 'table', prefix, fromTable }
  }
  
  // After WHERE/AND/OR + identifier (column or table.column) + space - expect operator
  // This checks if we have a column name followed by space, indicating we need an operator
  if (/\b(WHERE|AND|OR|HAVING)\s+[\w.]+\s+$/i.test(textBefore)) {
    return { context: 'operator', prefix, fromTable }
  }
  
  // After operator - expect value
  if (/\b(WHERE|AND|OR)\s+[\w.]+\s+(=|!=|<>|>|<|>=|<=|LIKE|IN|BETWEEN)\s*$/i.test(upperText)) {
    const columnName = extractWhereColumn(textBefore)
    return { context: 'value', prefix, fromTable, columnName }
  }
  
  // After operator with partial value typed (inside quotes)
  const valueMatch = textBefore.match(/\b(?:WHERE|AND|OR)\s+([\w.]+)\s+(?:=|!=|<>|LIKE)\s+'([^']*)$/i)
  if (valueMatch) {
    return { context: 'value', prefix: valueMatch[2], fromTable, columnName: valueMatch[1] }
  }
  
  // Right after SELECT - expect columns
  if (/\bSELECT\s*$/i.test(upperText)) {
    return { context: 'column', prefix }
  }
  
  // After SELECT with comma - expect more columns
  if (/\bSELECT\s+.*,\s*$/i.test(upperText) && !/\bFROM\b/i.test(upperText)) {
    return { context: 'column', prefix }
  }
  
  // After WHERE/AND/OR only - expect column
  if (/\b(WHERE|AND|OR|HAVING)\s*$/i.test(upperText)) {
    return { context: 'column', prefix }
  }
  
  // After WHERE/AND/OR + partial column name being typed (no space after)
  // This catches cases like "WHERE kno" where "kno" is partial column name
  if (/\b(WHERE|AND|OR|HAVING)\s+\w+$/i.test(trimmedText) && !textBefore.endsWith(' ')) {
    return { context: 'column', prefix }
  }
  
  // After ORDER BY / GROUP BY - expect column
  if (/\b(ORDER\s+BY|GROUP\s+BY)\s*$/i.test(upperText)) {
    return { context: 'column', prefix }
  }
  
  // After ORDER BY / GROUP BY + partial column name being typed
  if (/\b(ORDER\s+BY|GROUP\s+BY)\s+\w+$/i.test(trimmedText) && !textBefore.endsWith(' ')) {
    return { context: 'column', prefix }
  }
  
  // After SELECT columns (has FROM to add) - expect keyword
  if (/\bSELECT\s+.+$/i.test(upperText) && !/\bFROM\b/i.test(upperText)) {
    return { context: 'keyword', prefix }
  }
  
  // After FROM table - expect keyword (WHERE, ORDER BY, etc)
  if (/\bFROM\s+\w+(\s+\w+)?\s*$/i.test(upperText)) {
    return { context: 'keyword', prefix }
  }
  
  // Default: keyword
  return { context: 'keyword', prefix }
}

/**
 * Generate completion items based on context (async for value fetching)
 */
export async function getCompletions(
  monaco: typeof Monaco,
  sql: string,
  cursorOffset: number,
  range: Monaco.IRange
): Promise<Monaco.languages.CompletionItem[]> {
  const { context, prefix, tableName, fromTable, columnName } = detectContext(sql, cursorOffset)
  const items: Monaco.languages.CompletionItem[] = []
  const textBefore = sql.substring(0, cursorOffset).toUpperCase()
  const schema = getCurrentSchema()
  
  switch (context) {
    case 'table': {
      // Show table names
      schema.tables.forEach((table, idx) => {
        if (!prefix || table.name.toLowerCase().startsWith(prefix.toLowerCase())) {
          items.push({
            label: table.name,
            kind: monaco.languages.CompletionItemKind.Class,
            insertText: table.name,
            detail: `Table (${table.columns.length} columns)`,
            documentation: `Columns: ${table.columns.map(c => c.name).join(', ')}`,
            range,
            sortText: idx.toString().padStart(3, '0'),
          })
        }
      })
      break
    }
    
    case 'column': {
      // If specific table, show only its columns
      if (tableName) {
        const table = schema.tables.find(
          t => t.name.toLowerCase() === tableName.toLowerCase()
        )
        if (table) {
          // Found a table with this name - show its columns
          table.columns.forEach((col, idx) => {
            if (!prefix || col.name.toLowerCase().startsWith(prefix.toLowerCase())) {
              items.push({
                label: col.name,
                kind: monaco.languages.CompletionItemKind.Field,
                insertText: col.name,
                detail: col.type,
                range,
                sortText: idx.toString().padStart(3, '0'),
              })
            }
          })
        } else {
          // No table found - might be a column prefix like "metadata."
          // Look for columns that start with "tableName." pattern
          const fullPrefix = prefix ? `${tableName}.${prefix}` : `${tableName}.`
          const fullPrefixLower = fullPrefix.toLowerCase()
          
          schema.tables.forEach((t) => {
            t.columns.forEach((col, idx) => {
              if (col.name.toLowerCase().startsWith(fullPrefixLower)) {
                // Extract the part after the dot for display
                const afterPrefix = col.name.substring(tableName.length + 1)
                items.push({
                  label: afterPrefix,
                  kind: monaco.languages.CompletionItemKind.Field,
                  insertText: afterPrefix,
                  detail: `${col.type} (${col.name})`,
                  range,
                  sortText: idx.toString().padStart(3, '0'),
                })
              }
            })
          })
        }
      } else {
        // Show all columns with table prefix
        const seenColumns = new Set<string>()
        
        // First, add columns without prefix (common ones)
        schema.tables.forEach((table) => {
          table.columns.forEach((col, idx) => {
            if (!prefix || col.name.toLowerCase().startsWith(prefix.toLowerCase())) {
              if (!seenColumns.has(col.name)) {
                seenColumns.add(col.name)
                items.push({
                  label: col.name,
                  kind: monaco.languages.CompletionItemKind.Field,
                  insertText: col.name,
                  detail: col.type,
                  range,
                  sortText: `0${idx.toString().padStart(3, '0')}`,
                })
              }
            }
          })
        })
        
        // Then, add table.column format
        schema.tables.forEach((table) => {
          table.columns.forEach((col, idx) => {
            const fullName = `${table.name}.${col.name}`
            if (!prefix || fullName.toLowerCase().startsWith(prefix.toLowerCase())) {
              items.push({
                label: fullName,
                kind: monaco.languages.CompletionItemKind.Field,
                insertText: fullName,
                detail: `${col.type} from ${table.name}`,
                range,
                sortText: `1${idx.toString().padStart(3, '0')}`,
              })
            }
          })
        })
        
        // Add * for SELECT
        if (/\bSELECT\s*$/i.test(textBefore) || /,\s*$/i.test(textBefore)) {
          items.unshift({
            label: '*',
            kind: monaco.languages.CompletionItemKind.Constant,
            insertText: '* ',
            detail: 'All columns',
            range,
            sortText: '00000',
          })
        }
      }
      break
    }
    
    case 'operator': {
      const operators = [
        { label: '=', detail: 'Equals' },
        { label: '!=', detail: 'Not equals' },
        { label: '<>', detail: 'Not equals' },
        { label: '>', detail: 'Greater than' },
        { label: '<', detail: 'Less than' },
        { label: '>=', detail: 'Greater or equal' },
        { label: '<=', detail: 'Less or equal' },
        { label: 'LIKE', detail: 'Pattern match', insertText: "LIKE '%${1}%'" },
        { label: 'NOT LIKE', detail: 'Not pattern match', insertText: "NOT LIKE '%${1}%'" },
        { label: 'IN', detail: 'In list', insertText: 'IN (${1})' },
        { label: 'NOT IN', detail: 'Not in list', insertText: 'NOT IN (${1})' },
        { label: 'BETWEEN', detail: 'Range', insertText: 'BETWEEN ${1} AND ${2}' },
        { label: 'IS NULL', detail: 'Is null' },
        { label: 'IS NOT NULL', detail: 'Is not null' },
        { label: 'CONTAINS', detail: 'Contains text (ReasonDB)', insertText: "CONTAINS '${1}'" },
      ]
      
      operators.forEach((op, idx) => {
        items.push({
          label: op.label,
          kind: monaco.languages.CompletionItemKind.Operator,
          insertText: op.insertText || `${op.label} `,
          insertTextRules: op.insertText 
            ? monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet 
            : undefined,
          detail: op.detail,
          range,
          sortText: idx.toString().padStart(3, '0'),
        })
      })
      break
    }
    
    case 'keyword': {
      const hasSelect = /\bSELECT\b/i.test(textBefore)
      const hasFrom = /\bFROM\b/i.test(textBefore)
      const hasWhere = /\bWHERE\b/i.test(textBefore)
      
      let keywords: { label: string; insertText: string; detail: string }[] = []
      
      if (!hasSelect) {
        // Start of query
        keywords = [
          { label: 'SELECT', insertText: 'SELECT ', detail: 'Select data' },
          { label: 'INSERT INTO', insertText: 'INSERT INTO ${1:table} (${2:columns}) VALUES (${3:values})', detail: 'Insert data' },
          { label: 'UPDATE', insertText: 'UPDATE ${1:table} SET ${2:column} = ${3:value} WHERE ${4:condition}', detail: 'Update data' },
          { label: 'DELETE FROM', insertText: 'DELETE FROM ${1:table} WHERE ${2:condition}', detail: 'Delete data' },
          { label: 'CREATE TABLE', insertText: 'CREATE TABLE ${1:name} (\n  ${2:columns}\n)', detail: 'Create table' },
          // ReasonDB specific
          { label: 'REASON ABOUT', insertText: 'REASON ABOUT "${1:question}" FROM ${2:table}', detail: 'AI reasoning query' },
          { label: 'SEARCH', insertText: 'SEARCH "${1:query}" IN ${2:table}', detail: 'Semantic search' },
        ]
      } else if (!hasFrom) {
        // After SELECT columns
        keywords = [
          { label: 'FROM', insertText: 'FROM ', detail: 'Specify table' },
        ]
      } else if (!hasWhere) {
        // After FROM table
        keywords = [
          { label: 'WHERE', insertText: 'WHERE ', detail: 'Filter results' },
          { label: 'ORDER BY', insertText: 'ORDER BY ${1:column} ${2|ASC,DESC|}', detail: 'Sort results' },
          { label: 'GROUP BY', insertText: 'GROUP BY ${1:column}', detail: 'Group results' },
          { label: 'LIMIT', insertText: 'LIMIT ${1:10}', detail: 'Limit results' },
          { label: 'JOIN', insertText: 'JOIN ${1:table} ON ${2:condition}', detail: 'Join table' },
          { label: 'LEFT JOIN', insertText: 'LEFT JOIN ${1:table} ON ${2:condition}', detail: 'Left join' },
        ]
      } else {
        // After WHERE
        keywords = [
          { label: 'AND', insertText: 'AND ', detail: 'Add condition' },
          { label: 'OR', insertText: 'OR ', detail: 'Alternative condition' },
          { label: 'ORDER BY', insertText: 'ORDER BY ${1:column} ${2|ASC,DESC|}', detail: 'Sort results' },
          { label: 'GROUP BY', insertText: 'GROUP BY ${1:column}', detail: 'Group results' },
          { label: 'LIMIT', insertText: 'LIMIT ${1:10}', detail: 'Limit results' },
        ]
      }
      
      keywords.forEach((kw, idx) => {
        if (!prefix || kw.label.toLowerCase().startsWith(prefix.toLowerCase())) {
          items.push({
            label: kw.label,
            kind: monaco.languages.CompletionItemKind.Keyword,
            insertText: kw.insertText,
            insertTextRules: kw.insertText.includes('${')
              ? monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet
              : undefined,
            detail: kw.detail,
            range,
            sortText: idx.toString().padStart(3, '0'),
          })
        }
      })
      break
    }
    
    case 'value': {
      // Try to fetch actual values from the database
      if (fromTable && columnName) {
        try {
          const values = await getColumnValues(fromTable, columnName)
          
          values.forEach((value, idx) => {
            if (!prefix || value.toLowerCase().includes(prefix.toLowerCase())) {
              // Escape single quotes in value
              const escapedValue = value.replace(/'/g, "''")
              items.push({
                label: value,
                kind: monaco.languages.CompletionItemKind.Value,
                insertText: `'${escapedValue}'`,
                detail: `Value from ${columnName}`,
                range,
                sortText: idx.toString().padStart(3, '0'),
              })
            }
          })
        } catch {
          // Fallback to generic suggestions
        }
      }
      
      // Add generic suggestions if no values found
      if (items.length === 0) {
        items.push({
          label: "'value'",
          kind: monaco.languages.CompletionItemKind.Value,
          insertText: "'${1}'",
          insertTextRules: monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet,
          detail: 'String value',
          range,
        })
      }
      
      items.push({
        label: 'NULL',
        kind: monaco.languages.CompletionItemKind.Constant,
        insertText: 'NULL',
        detail: 'Null value',
        range,
      })
      break
    }
  }
  
  return items
}
