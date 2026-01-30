import type { SearchFilter, FilterGroup, FilterOperator } from './filter-types'

// Get nested value from object using dot notation path
export function getNestedValue(obj: Record<string, unknown>, path: string): unknown {
  const keys = path.split('.')
  let value: unknown = obj
  
  for (const key of keys) {
    if (value === null || value === undefined) return undefined
    if (typeof value !== 'object') return undefined
    value = (value as Record<string, unknown>)[key]
  }
  
  return value
}

// Evaluate a single filter against a value
export function evaluateFilter(value: unknown, filter: SearchFilter): boolean {
  const { operator, value: filterValue, valueEnd } = filter
  
  // Handle null checks first
  if (operator === 'is_null') {
    return value === null || value === undefined
  }
  if (operator === 'is_not_null') {
    return value !== null && value !== undefined
  }
  if (operator === 'is_empty') {
    if (Array.isArray(value)) return value.length === 0
    if (typeof value === 'object' && value !== null) return Object.keys(value).length === 0
    if (typeof value === 'string') return value.length === 0
    return false
  }
  
  // If value is null/undefined and we're not checking for null, filter doesn't match
  if (value === null || value === undefined) return false
  
  const strValue = String(value).toLowerCase()
  const strFilterValue = String(filterValue).toLowerCase()
  
  switch (operator) {
    case 'equals':
      if (typeof value === 'number' && typeof filterValue === 'number') {
        return value === filterValue
      }
      if (typeof value === 'boolean') {
        return value === (filterValue === 'true' || filterValue === true)
      }
      return strValue === strFilterValue
      
    case 'not_equals':
      if (typeof value === 'number' && typeof filterValue === 'number') {
        return value !== filterValue
      }
      return strValue !== strFilterValue
      
    case 'contains':
      if (Array.isArray(value)) {
        return value.some((item) => 
          String(item).toLowerCase().includes(strFilterValue)
        )
      }
      if (typeof value === 'object') {
        return JSON.stringify(value).toLowerCase().includes(strFilterValue)
      }
      return strValue.includes(strFilterValue)
      
    case 'not_contains':
      return !strValue.includes(strFilterValue)
      
    case 'starts_with':
      return strValue.startsWith(strFilterValue)
      
    case 'ends_with':
      return strValue.endsWith(strFilterValue)
      
    case 'greater_than':
      if (typeof value === 'number' && typeof filterValue === 'number') {
        return value > filterValue
      }
      // Date comparison
      if (typeof value === 'string' && typeof filterValue === 'string') {
        return new Date(value) > new Date(filterValue)
      }
      return strValue > strFilterValue
      
    case 'greater_equal':
      if (typeof value === 'number' && typeof filterValue === 'number') {
        return value >= filterValue
      }
      if (typeof value === 'string' && typeof filterValue === 'string') {
        return new Date(value) >= new Date(filterValue)
      }
      return strValue >= strFilterValue
      
    case 'less_than':
      if (typeof value === 'number' && typeof filterValue === 'number') {
        return value < filterValue
      }
      if (typeof value === 'string' && typeof filterValue === 'string') {
        return new Date(value) < new Date(filterValue)
      }
      return strValue < strFilterValue
      
    case 'less_equal':
      if (typeof value === 'number' && typeof filterValue === 'number') {
        return value <= filterValue
      }
      if (typeof value === 'string' && typeof filterValue === 'string') {
        return new Date(value) <= new Date(filterValue)
      }
      return strValue <= strFilterValue
      
    case 'between':
      if (typeof value === 'number' && typeof filterValue === 'number' && typeof valueEnd === 'number') {
        return value >= filterValue && value <= valueEnd
      }
      if (typeof value === 'string' && typeof filterValue === 'string' && typeof valueEnd === 'string') {
        const dateValue = new Date(value)
        return dateValue >= new Date(filterValue) && dateValue <= new Date(valueEnd)
      }
      return false
      
    case 'has_key':
      if (typeof value === 'object' && value !== null && !Array.isArray(value)) {
        return strFilterValue in (value as Record<string, unknown>)
      }
      return false
      
    default:
      return true
  }
}

// Evaluate a filter group against a document
export function evaluateFilterGroup(
  doc: Record<string, unknown>,
  group: FilterGroup
): boolean {
  const results = group.filters.map((filter) => {
    if ('logic' in filter) {
      // It's a nested FilterGroup
      return evaluateFilterGroup(doc, filter as FilterGroup)
    } else {
      // It's a SearchFilter
      const searchFilter = filter as SearchFilter
      const value = getNestedValue(doc, searchFilter.column)
      return evaluateFilter(value, searchFilter)
    }
  })
  
  if (group.logic === 'AND') {
    return results.every(Boolean)
  } else {
    return results.some(Boolean)
  }
}

// Filter documents using a filter group
export function filterDocuments<T extends Record<string, unknown>>(
  documents: T[],
  filterGroup: FilterGroup | null
): T[] {
  if (!filterGroup || filterGroup.filters.length === 0) {
    return documents
  }
  
  return documents.filter((doc) => evaluateFilterGroup(doc, filterGroup))
}

// Create a simple text search filter (searches all text fields)
export function createTextSearchFilter(
  searchText: string,
  columns: string[]
): FilterGroup | null {
  if (!searchText.trim()) return null
  
  const filters: SearchFilter[] = columns.map((column) => ({
    id: crypto.randomUUID(),
    column,
    operator: 'contains' as FilterOperator,
    value: searchText.trim(),
  }))
  
  return {
    id: crypto.randomUUID(),
    logic: 'OR',
    filters,
  }
}

// Parse simple query syntax: "column = value" or "column contains value"
export function parseSimpleQuery(query: string, availableColumns: string[]): FilterGroup | null {
  const trimmed = query.trim()
  if (!trimmed) return null
  
  // Try to match patterns like: column = "value", column contains value, etc.
  const patterns = [
    // column = "value" or column = value
    /^(\w+(?:\.\w+)?)\s*=\s*["']?([^"']+)["']?$/i,
    // column != "value"
    /^(\w+(?:\.\w+)?)\s*!=\s*["']?([^"']+)["']?$/i,
    // column contains "value"
    /^(\w+(?:\.\w+)?)\s+contains\s+["']?([^"']+)["']?$/i,
    // column like "%value%"
    /^(\w+(?:\.\w+)?)\s+like\s+["']?%?([^"'%]+)%?["']?$/i,
    // column > value
    /^(\w+(?:\.\w+)?)\s*>\s*["']?([^"']+)["']?$/i,
    // column >= value
    /^(\w+(?:\.\w+)?)\s*>=\s*["']?([^"']+)["']?$/i,
    // column < value
    /^(\w+(?:\.\w+)?)\s*<\s*["']?([^"']+)["']?$/i,
    // column <= value
    /^(\w+(?:\.\w+)?)\s*<=\s*["']?([^"']+)["']?$/i,
    // column is null
    /^(\w+(?:\.\w+)?)\s+is\s+null$/i,
    // column is not null
    /^(\w+(?:\.\w+)?)\s+is\s+not\s+null$/i,
  ]
  
  const operatorMap: Record<number, FilterOperator> = {
    0: 'equals',
    1: 'not_equals',
    2: 'contains',
    3: 'contains',
    4: 'greater_than',
    5: 'greater_equal',
    6: 'less_than',
    7: 'less_equal',
    8: 'is_null',
    9: 'is_not_null',
  }
  
  for (let i = 0; i < patterns.length; i++) {
    const match = trimmed.match(patterns[i])
    if (match) {
      const column = match[1]
      const value = match[2] || null
      
      // Find the full column path
      const fullColumn = availableColumns.find(
        (c) => c === column || c.endsWith(`.${column}`)
      ) || `data.${column}`
      
      return {
        id: crypto.randomUUID(),
        logic: 'AND',
        filters: [
          {
            id: crypto.randomUUID(),
            column: fullColumn,
            operator: operatorMap[i],
            value: value,
          },
        ],
      }
    }
  }
  
  // If no pattern matched, treat as a global text search
  return createTextSearchFilter(trimmed, availableColumns)
}

// Serialize filter group to display string
export function filterGroupToString(group: FilterGroup): string {
  const parts = group.filters.map((filter) => {
    if ('logic' in filter) {
      return `(${filterGroupToString(filter as FilterGroup)})`
    }
    const f = filter as SearchFilter
    const colName = f.column.split('.').pop()
    if (f.operator === 'is_null') return `${colName} IS NULL`
    if (f.operator === 'is_not_null') return `${colName} IS NOT NULL`
    if (f.operator === 'between') return `${colName} BETWEEN ${f.value} AND ${f.valueEnd}`
    
    const opSymbols: Record<string, string> = {
      equals: '=',
      not_equals: '!=',
      contains: 'CONTAINS',
      not_contains: 'NOT CONTAINS',
      starts_with: 'STARTS WITH',
      ends_with: 'ENDS WITH',
      greater_than: '>',
      greater_equal: '>=',
      less_than: '<',
      less_equal: '<=',
    }
    
    return `${colName} ${opSymbols[f.operator] || f.operator} "${f.value}"`
  })
  
  return parts.join(` ${group.logic} `)
}
