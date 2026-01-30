// Filter operator types by data type
export type TextOperator =
  | 'equals'
  | 'not_equals'
  | 'contains'
  | 'not_contains'
  | 'starts_with'
  | 'ends_with'
  | 'is_null'
  | 'is_not_null'

export type NumberOperator =
  | 'equals'
  | 'not_equals'
  | 'greater_than'
  | 'greater_equal'
  | 'less_than'
  | 'less_equal'
  | 'between'
  | 'is_null'
  | 'is_not_null'

export type BooleanOperator = 'equals' | 'not_equals' | 'is_null' | 'is_not_null'

export type DateOperator =
  | 'equals'
  | 'not_equals'
  | 'greater_than'
  | 'greater_equal'
  | 'less_than'
  | 'less_equal'
  | 'between'
  | 'is_null'
  | 'is_not_null'

export type JsonOperator =
  | 'contains'
  | 'has_key'
  | 'is_null'
  | 'is_not_null'
  | 'is_empty'

export type FilterOperator =
  | TextOperator
  | NumberOperator
  | BooleanOperator
  | DateOperator
  | JsonOperator

export type ColumnType = 'text' | 'number' | 'boolean' | 'date' | 'json' | 'array' | 'vector' | 'unknown'

export interface ColumnInfo {
  name: string
  type: ColumnType
  path: string // e.g., 'data.title' or 'metadata.tags'
}

export interface SearchFilter {
  id: string
  column: string
  operator: FilterOperator
  value: string | number | boolean | null
  valueEnd?: string | number // For BETWEEN operator
}

export interface FilterGroup {
  id: string
  logic: 'AND' | 'OR'
  filters: (SearchFilter | FilterGroup)[]
}

export interface SavedFilter {
  id: string
  name: string
  filter: FilterGroup
  createdAt: string
}

// Operator display names and symbols
export const operatorLabels: Record<FilterOperator, string> = {
  equals: '=',
  not_equals: '≠',
  contains: 'contains',
  not_contains: 'not contains',
  starts_with: 'starts with',
  ends_with: 'ends with',
  greater_than: '>',
  greater_equal: '≥',
  less_than: '<',
  less_equal: '≤',
  between: 'between',
  is_null: 'is null',
  is_not_null: 'is not null',
  has_key: 'has key',
  is_empty: 'is empty',
}

// Get available operators for a column type
export function getOperatorsForType(type: ColumnType): FilterOperator[] {
  switch (type) {
    case 'text':
      return [
        'equals',
        'not_equals',
        'contains',
        'not_contains',
        'starts_with',
        'ends_with',
        'is_null',
        'is_not_null',
      ]
    case 'number':
      return [
        'equals',
        'not_equals',
        'greater_than',
        'greater_equal',
        'less_than',
        'less_equal',
        'between',
        'is_null',
        'is_not_null',
      ]
    case 'boolean':
      return ['equals', 'not_equals', 'is_null', 'is_not_null']
    case 'date':
      return [
        'equals',
        'not_equals',
        'greater_than',
        'greater_equal',
        'less_than',
        'less_equal',
        'between',
        'is_null',
        'is_not_null',
      ]
    case 'json':
    case 'array':
      return ['contains', 'has_key', 'is_null', 'is_not_null', 'is_empty']
    case 'vector':
      return ['is_null', 'is_not_null']
    default:
      return ['equals', 'not_equals', 'contains', 'is_null', 'is_not_null']
  }
}

// Detect column type from value
export function detectColumnType(value: unknown): ColumnType {
  if (value === null || value === undefined) return 'unknown'
  if (typeof value === 'boolean') return 'boolean'
  if (typeof value === 'number') return 'number'
  if (typeof value === 'string') {
    // Check if it's a date
    if (value.match(/^\d{4}-\d{2}-\d{2}/)) return 'date'
    // Check if it's a vector representation
    if (value.startsWith('[') && value.includes('...')) return 'vector'
    return 'text'
  }
  if (Array.isArray(value)) return 'array'
  if (typeof value === 'object') return 'json'
  return 'unknown'
}

// Check if operator needs a value input
export function operatorNeedsValue(operator: FilterOperator): boolean {
  return !['is_null', 'is_not_null', 'is_empty'].includes(operator)
}

// Check if operator needs two values (BETWEEN)
export function operatorNeedsTwoValues(operator: FilterOperator): boolean {
  return operator === 'between'
}
