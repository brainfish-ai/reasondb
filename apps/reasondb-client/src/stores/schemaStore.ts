import { create } from 'zustand'

// Schema types for autocompletion
export interface ColumnSchema {
  name: string
  type: string
}

export interface TableSchema {
  name: string
  id: string
  columns: ColumnSchema[]
}

export interface MetadataSchemaField {
  path: string
  field_type: string
  occurrence_count: number
}

// Value cache entry
interface ValueCacheEntry {
  values: string[]
  timestamp: number
}

interface SchemaState {
  // Tables schema for autocompletion
  tables: TableSchema[]
  
  // Pending metadata fields (when base schema not yet set)
  pendingMetadataFields: Map<string, MetadataSchemaField[]>
  
  // Value cache for column values (keyed by "tableId:column")
  valueCache: Map<string, ValueCacheEntry>
  valueCacheTTL: number
  
  // Actions
  setTables: (tables: TableSchema[]) => void
  addMetadataFields: (tableName: string, fields: MetadataSchemaField[]) => void
  getTableById: (tableId: string) => TableSchema | undefined
  getTableByName: (tableName: string) => TableSchema | undefined
  
  // Value cache actions
  getCachedValues: (tableId: string, column: string) => string[] | null
  setCachedValues: (tableId: string, column: string, values: string[]) => void
  
  // Computed getters for sql-completion
  getSchema: () => { tables: Array<{ name: string; columns: ColumnSchema[] }> }
  
  reset: () => void
}

const VALUE_CACHE_TTL = 60000 // 1 minute

const initialState = {
  tables: [] as TableSchema[],
  pendingMetadataFields: new Map<string, MetadataSchemaField[]>(),
  valueCache: new Map<string, ValueCacheEntry>(),
  valueCacheTTL: VALUE_CACHE_TTL,
}

export const useSchemaStore = create<SchemaState>((set, get) => ({
  ...initialState,
  
  setTables: (tables) => {
    const { pendingMetadataFields, tables: existingTables } = get()
    
    // Merge incoming tables with any existing metadata columns
    const updatedTables = tables.map(table => {
      const existingTable = existingTables.find(t => t.id === table.id)
      if (existingTable) {
        // Preserve existing metadata columns from the current state
        const existingMetadataColumns = existingTable.columns.filter(
          col => col.name.startsWith('metadata.')
        )
        if (existingMetadataColumns.length > 0) {
          return {
            ...table,
            columns: [...table.columns, ...existingMetadataColumns],
          }
        }
      }
      return table
    })
    
    // Apply any pending metadata fields
    if (pendingMetadataFields.size > 0) {
      for (const [tableName, fields] of pendingMetadataFields) {
        const tableIndex = updatedTables.findIndex(
          t => t.name.toLowerCase() === tableName.toLowerCase()
        )
        
        if (tableIndex !== -1) {
          // Get existing columns (excluding old metadata.* columns to avoid duplicates)
          const existingColumns = updatedTables[tableIndex].columns.filter(
            col => !col.name.startsWith('metadata.')
          )
          
          // Create new columns for metadata fields
          const metadataColumns: ColumnSchema[] = fields.map(field => ({
            name: `metadata.${field.path}`,
            type: field.field_type,
          }))
          
          updatedTables[tableIndex] = {
            ...updatedTables[tableIndex],
            columns: [...existingColumns, ...metadataColumns],
          }
        }
      }
      
      set({ 
        tables: updatedTables, 
        pendingMetadataFields: new Map() 
      })
    } else {
      set({ tables: updatedTables })
    }
  },
  
  addMetadataFields: (tableName, fields) => {
    const { tables, pendingMetadataFields } = get()
    
    const tableIndex = tables.findIndex(
      t => t.name.toLowerCase() === tableName.toLowerCase()
    )
    
    if (tableIndex === -1) {
      // Table not found yet, store as pending
      const newPending = new Map(pendingMetadataFields)
      newPending.set(tableName, fields)
      set({ pendingMetadataFields: newPending })
      return
    }
    
    // Apply metadata fields immediately
    const updatedTables = [...tables]
    const existingColumns = updatedTables[tableIndex].columns.filter(
      col => !col.name.startsWith('metadata.')
    )
    
    const metadataColumns: ColumnSchema[] = fields.map(field => ({
      name: `metadata.${field.path}`,
      type: field.field_type,
    }))
    
    updatedTables[tableIndex] = {
      ...updatedTables[tableIndex],
      columns: [...existingColumns, ...metadataColumns],
    }
    
    set({ tables: updatedTables })
  },
  
  getTableById: (tableId) => {
    return get().tables.find(t => t.id === tableId)
  },
  
  getTableByName: (tableName) => {
    return get().tables.find(
      t => t.name.toLowerCase() === tableName.toLowerCase()
    )
  },
  
  getCachedValues: (tableId, column) => {
    const { valueCache, valueCacheTTL } = get()
    const cacheKey = `${tableId}:${column}`
    const cached = valueCache.get(cacheKey)
    
    if (cached && Date.now() - cached.timestamp < valueCacheTTL) {
      return cached.values
    }
    return null
  },
  
  setCachedValues: (tableId, column, values) => {
    const { valueCache } = get()
    const newCache = new Map(valueCache)
    newCache.set(`${tableId}:${column}`, {
      values,
      timestamp: Date.now(),
    })
    set({ valueCache: newCache })
  },
  
  getSchema: () => {
    const { tables } = get()
    return {
      tables: tables.map(t => ({
        name: t.name,
        columns: t.columns,
      }))
    }
  },
  
  reset: () => set({
    ...initialState,
    pendingMetadataFields: new Map(),
    valueCache: new Map(),
  }),
}))
