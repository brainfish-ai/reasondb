import { useState, useRef, useEffect, useCallback, useMemo } from 'react'
import {
  MagnifyingGlass,
  X,
  Funnel,
  Clock,
  Bookmarks,
  Trash,
  Warning,
} from '@phosphor-icons/react'
import { cn } from '@/lib/utils'
import { useFilterStore } from '@/stores/filterStore'
import { parseSimpleQuery, filterGroupToString } from '@/lib/filter-utils'
import type { ColumnInfo } from '@/lib/filter-types'

// Search context types
type SearchContext = 'column' | 'operator' | 'value' | 'text'

interface ParsedContext {
  context: SearchContext
  column?: string      // The column being filtered (for value suggestions)
  columnType?: string  // The type of the column
  prefix: string       // What the user has typed so far for current context
}

// Value fetcher type for autocomplete
export type ValueFetcher = (column: string) => Promise<string[]>

interface SearchBarProps {
  columns: ColumnInfo[]
  tableId?: string               // Table ID (for context, not used directly)
  valueFetcher?: ValueFetcher    // Function to fetch column values
  placeholder?: string
  onSearch: (query: string) => void
  onFilterChange?: () => void
  className?: string
}

/**
 * Parse the search input to detect the current context
 * Supports patterns like: "column operator value"
 */
function detectSearchContext(text: string, columns: ColumnInfo[]): ParsedContext {
  const trimmed = text.trim()
  
  if (!trimmed) {
    return { context: 'column', prefix: '' }
  }
  
  // Symbol operators (can have optional spaces)
  const symbolOps = ['!=', '>=', '<=', '<>', '=', '>', '<']
  // Word operators (require spaces)
  const wordOps = ['contains', 'not contains', 'is not null', 'is null', 'starts with', 'ends with']
  
  // Try symbol operators first (e.g., "column >= value" or "column>=value")
  for (const op of symbolOps) {
    // Escape special regex chars
    const escapedOp = op.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
    // Allow optional spaces around symbol operators
    const opRegex = new RegExp(`^([\\w.]+)\\s*${escapedOp}\\s*(.*)$`, 'i')
    const match = trimmed.match(opRegex)
    
    if (match) {
      const [, colName, valuePrefix] = match
      const col = columns.find(
        c => c.name.toLowerCase() === colName.toLowerCase() || 
             c.path.toLowerCase() === colName.toLowerCase()
      )
      
      return {
        context: 'value',
        column: col?.path || colName,
        columnType: col?.type,
        prefix: valuePrefix.replace(/^["']|["']$/g, '').trim(),
      }
    }
  }
  
  // Try word operators (require at least one space before)
  for (const op of wordOps) {
    const opRegex = new RegExp(`^([\\w.]+)\\s+${op.replace(/\s+/g, '\\s+')}\\s*(.*)$`, 'i')
    const match = trimmed.match(opRegex)
    
    if (match) {
      const [, colName, valuePrefix] = match
      const col = columns.find(
        c => c.name.toLowerCase() === colName.toLowerCase() || 
             c.path.toLowerCase() === colName.toLowerCase()
      )
      
      return {
        context: 'value',
        column: col?.path || colName,
        columnType: col?.type,
        prefix: valuePrefix.replace(/^["']|["']$/g, '').trim(),
      }
    }
    
    // Check if typing the operator (e.g., "title con")
    if (op.length > 1) {
      const partialOpRegex = new RegExp(`^([\\w.]+)\\s+([a-z]+)$`, 'i')
      const partialMatch = trimmed.match(partialOpRegex)
      if (partialMatch && op.toLowerCase().startsWith(partialMatch[2].toLowerCase())) {
        return {
          context: 'operator',
          column: partialMatch[1],
          prefix: partialMatch[2],
        }
      }
    }
  }
  
  // Check if we have "column " (column followed by space - need operator)
  const columnSpaceMatch = trimmed.match(/^([\w.]+)\s+$/i)
  if (columnSpaceMatch) {
    return {
      context: 'operator',
      column: columnSpaceMatch[1],
      prefix: '',
    }
  }
  
  // Check if we have a partial column name (only letters, numbers, dots, underscores)
  const columnMatch = trimmed.match(/^([\w.]*)$/i)
  if (columnMatch) {
    return {
      context: 'column',
      prefix: columnMatch[1],
    }
  }
  
  // Default: treat as text search
  return { context: 'text', prefix: trimmed }
}

/**
 * Validate a search query and return an error message if invalid
 */
function validateQuery(text: string, columns: ColumnInfo[]): string | null {
  const trimmed = text.trim()
  if (!trimmed) return null
  
  // Check for incomplete operator patterns
  const incompletePatterns = [
    { pattern: /^[\w.]+\s+contains\s*$/i, error: 'Missing value after "contains"' },
    { pattern: /^[\w.]+\s+starts\s+with\s*$/i, error: 'Missing value after "starts with"' },
    { pattern: /^[\w.]+\s+ends\s+with\s*$/i, error: 'Missing value after "ends with"' },
    { pattern: /^[\w.]+\s*[=!<>]=?\s*$/i, error: 'Missing value after operator' },
    { pattern: /^[\w.]+\s+is\s*$/i, error: 'Incomplete "is null" or "is not null"' },
    { pattern: /^[\w.]+\s+is\s+not\s*$/i, error: 'Incomplete "is not null"' },
  ]
  
  for (const { pattern, error } of incompletePatterns) {
    if (pattern.test(trimmed)) {
      return error
    }
  }
  
  // Check if it looks like a filter query but uses an unknown column
  const filterPattern = /^([\w.]+)\s*(?:=|!=|<>|>=?|<=?|contains|like|is\s)/i
  const match = trimmed.match(filterPattern)
  if (match) {
    const colName = match[1].toLowerCase()
    const isKnownColumn = columns.some(
      c => c.name.toLowerCase() === colName || 
           c.path.toLowerCase() === colName ||
           c.path.toLowerCase().endsWith(`.${colName}`)
    )
    if (!isKnownColumn) {
      return `Unknown column "${match[1]}". Available: ${columns.slice(0, 5).map(c => c.name).join(', ')}${columns.length > 5 ? '...' : ''}`
    }
  }
  
  // Check for invalid operator syntax
  const invalidOps = [
    { pattern: /^[\w.]+\s+><\s*/i, error: 'Invalid operator "><"' },
    { pattern: /^[\w.]+\s+<>\s*$/i, error: 'Missing value after "<>"' },
    { pattern: /^[\w.]+\s+==\s*/i, error: 'Use "=" instead of "=="' },
    { pattern: /^[\w.]+\s+===\s*/i, error: 'Use "=" instead of "==="' },
  ]
  
  for (const { pattern, error } of invalidOps) {
    if (pattern.test(trimmed)) {
      return error
    }
  }
  
  return null
}

export function SearchBar({
  columns,
  tableId: _tableId,
  valueFetcher,
  placeholder = 'Search...',
  onSearch,
  onFilterChange,
  className,
}: SearchBarProps) {
  const inputRef = useRef<HTMLInputElement>(null)
  const dropdownRef = useRef<HTMLDivElement>(null)
  
  const [inputValue, setInputValue] = useState('')
  const [showDropdown, setShowDropdown] = useState(false)
  const [selectedIndex, setSelectedIndex] = useState(-1)
  const [dropdownMode, setDropdownMode] = useState<'suggestions' | 'recent' | 'saved'>('suggestions')
  const [columnValues, setColumnValues] = useState<Map<string, string[]>>(new Map())
  const [loadingValues, setLoadingValues] = useState(false)
  const [queryError, setQueryError] = useState<string | null>(null)
  
  const {
    quickSearchText,
    setQuickSearchText,
    activeFilter,
    setActiveFilter,
    recentSearches,
    addRecentSearch,
    clearRecentSearches,
    savedFilters,
    loadFilter,
    toggleFilterBuilder,
  } = useFilterStore()
  
  // Parse current context
  const parsedContext = useMemo(() => detectSearchContext(inputValue, columns), [inputValue, columns])
  
  // Validate query on change
  useEffect(() => {
    const error = validateQuery(inputValue, columns)
    setQueryError(error)
  }, [inputValue, columns])
  
  // Sync with store
  useEffect(() => {
    setInputValue(quickSearchText)
  }, [quickSearchText])
  
  // Fetch column values when in value context (only for text/array columns)
  // Debounced to prevent rate limiting
  useEffect(() => {
    if (!valueFetcher || parsedContext.context !== 'value' || !parsedContext.column) {
      return
    }
    
    // Skip fetching for numeric/date columns - doesn't make sense
    const columnType = parsedContext.columnType
    if (columnType === 'number' || columnType === 'date') {
      return
    }
    
    const columnPath = parsedContext.column
    
    // Check cache first
    if (columnValues.has(columnPath)) {
      return
    }
    
    // Debounce the fetch to prevent rapid API calls
    const timeoutId = setTimeout(() => {
      setLoadingValues(true)
      valueFetcher(columnPath)
        .then((values) => {
          setColumnValues(prev => {
            const next = new Map(prev)
            next.set(columnPath, values)
            return next
          })
        })
        .catch(() => {
          // Mark as fetched (empty) to prevent retries on rate limit
          setColumnValues(prev => {
            const next = new Map(prev)
            next.set(columnPath, [])
            return next
          })
        })
        .finally(() => {
          setLoadingValues(false)
        })
    }, 500) // 500ms debounce
    
    return () => clearTimeout(timeoutId)
  }, [valueFetcher, parsedContext.context, parsedContext.column, parsedContext.columnType, columnValues])
  
  // Generate suggestions based on context
  const suggestions = useMemo(() => {
    const result: { type: 'column' | 'operator' | 'value' | 'example'; value: string; display: string; insertText: string }[] = []
    const { context, column, columnType, prefix } = parsedContext
    const prefixLower = prefix.toLowerCase()
    
    switch (context) {
      case 'column': {
        // Suggest matching columns
        columns.forEach((col) => {
          const colNameLower = col.name.toLowerCase()
          const colPathLower = col.path.toLowerCase()
          if (!prefix || colNameLower.includes(prefixLower) || colPathLower.includes(prefixLower)) {
            result.push({
              type: 'column',
              value: col.name,
              display: `${col.name} (${col.type})`,
              insertText: col.name + ' ',
            })
          }
        })
        break
      }
      
      case 'operator': {
        // Suggest operators based on column type
        const operators = [
          { op: '=', label: '= (equals)', types: ['text', 'number', 'date', 'boolean'] },
          { op: '!=', label: '!= (not equals)', types: ['text', 'number', 'date', 'boolean'] },
          { op: 'contains', label: 'contains (partial match)', types: ['text'] },
          { op: 'starts with', label: 'starts with', types: ['text'] },
          { op: 'ends with', label: 'ends with', types: ['text'] },
          { op: '>', label: '> (greater than)', types: ['number', 'date'] },
          { op: '>=', label: '>= (greater or equal)', types: ['number', 'date'] },
          { op: '<', label: '< (less than)', types: ['number', 'date'] },
          { op: '<=', label: '<= (less or equal)', types: ['number', 'date'] },
          { op: 'is null', label: 'is null', types: ['text', 'number', 'date', 'array', 'object'] },
          { op: 'is not null', label: 'is not null', types: ['text', 'number', 'date', 'array', 'object'] },
        ]
        
        const colInfo = columns.find(c => 
          c.name.toLowerCase() === column?.toLowerCase() || 
          c.path.toLowerCase() === column?.toLowerCase()
        )
        const type = colInfo?.type || 'text'
        
        operators
          .filter(o => o.types.includes(type) || type === 'unknown')
          .filter(o => !prefix || o.op.toLowerCase().startsWith(prefixLower))
          .forEach(o => {
            result.push({
              type: 'operator',
              value: o.op,
              display: o.label,
              insertText: o.op + ' ',
            })
          })
        break
      }
      
      case 'value': {
        // Add type-specific suggestions first
        if (columnType === 'boolean') {
          if (!prefix || 'true'.includes(prefixLower)) {
            result.push({ type: 'value', value: 'true', display: 'true', insertText: 'true' })
          }
          if (!prefix || 'false'.includes(prefixLower)) {
            result.push({ type: 'value', value: 'false', display: 'false', insertText: 'false' })
          }
        } else if (columnType === 'number') {
          // For numbers, just show hint - user should type a number
          if (!prefix) {
            result.push({ type: 'example', value: '100', display: 'Enter a number (e.g., 100, 10000)', insertText: '' })
          }
        } else if (columnType === 'date') {
          // For dates, show format hint
          if (!prefix) {
            const today = new Date().toISOString().split('T')[0]
            result.push({ type: 'example', value: today, display: `Enter a date (e.g., ${today})`, insertText: today })
          }
        } else {
          // For text columns, suggest actual values from the column
          const values = column ? columnValues.get(column) : undefined
          
          if (values && values.length > 0) {
            values
              .filter(v => !prefix || v.toLowerCase().includes(prefixLower))
              .slice(0, 10)
              .forEach(v => {
                const needsQuotes = v.includes(' ') || /[^a-zA-Z0-9_.-]/.test(v)
                const displayValue = v.length > 40 ? v.slice(0, 40) + '...' : v
                result.push({
                  type: 'value',
                  value: v,
                  display: displayValue,
                  insertText: needsQuotes ? `"${v}"` : v,
                })
              })
          }
        }
        break
      }
      
      case 'text': {
        // Suggest common query patterns
        const text = prefix
        result.push(
          { type: 'example', value: `title contains "${text}"`, display: `title contains "${text}"`, insertText: `title contains "${text}"` },
          { type: 'example', value: `content contains "${text}"`, display: `content contains "${text}"`, insertText: `content contains "${text}"` },
        )
        break
      }
    }
    
    return result.slice(0, 10)
  }, [parsedContext, columns, columnValues])
  
  // Handle input change
  const handleInputChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const value = e.target.value
    setInputValue(value)
    setShowDropdown(true)
    setDropdownMode('suggestions')
    setSelectedIndex(-1)
  }
  
  // Apply a suggestion to the input
  const applySuggestion = useCallback((suggestion: { type: string; value: string; insertText: string }) => {
    const { context, prefix } = parsedContext
    
    let newValue = ''
    
    if (context === 'column') {
      // Replace the prefix with the column name
      newValue = suggestion.insertText
    } else if (context === 'operator') {
      // Keep the column, add the operator
      const beforePrefix = inputValue.slice(0, inputValue.length - prefix.length)
      newValue = beforePrefix + suggestion.insertText
    } else if (context === 'value') {
      // Keep the column and operator, add the value
      const beforePrefix = inputValue.slice(0, inputValue.length - prefix.length)
      newValue = beforePrefix + suggestion.insertText
    } else {
      // Replace with the full suggestion
      newValue = suggestion.insertText
    }
    
    setInputValue(newValue)
    inputRef.current?.focus()
    
    // Auto-execute search if we just completed a value
    if (suggestion.type === 'value') {
      // Keep dropdown open for more edits
    }
  }, [parsedContext, inputValue])
  
  // Handle search execution
  const handleSearch = useCallback(() => {
    const value = inputValue.trim()
    
    if (value) {
      // Try to parse as structured query
      const columnPaths = columns.map((c) => c.path)
      const filter = parseSimpleQuery(value, columnPaths)
      
      if (filter) {
        setActiveFilter(filter)
        onFilterChange?.()
      }
      
      addRecentSearch(value)
    } else {
      setActiveFilter(null)
    }
    
    setQuickSearchText(value)
    onSearch(value)
    setShowDropdown(false)
  }, [inputValue, columns, setActiveFilter, addRecentSearch, setQuickSearchText, onSearch, onFilterChange])
  
  // Handle keyboard navigation
  const handleKeyDown = (e: React.KeyboardEvent) => {
    const items = dropdownMode === 'recent' 
      ? recentSearches 
      : dropdownMode === 'saved' 
      ? savedFilters 
      : suggestions
    
    if (e.key === 'ArrowDown') {
      e.preventDefault()
      setSelectedIndex((prev) => Math.min(prev + 1, items.length - 1))
    } else if (e.key === 'ArrowUp') {
      e.preventDefault()
      setSelectedIndex((prev) => Math.max(prev - 1, -1))
    } else if (e.key === 'Tab' && showDropdown) {
      // Tab cycles through modes when dropdown is open
      e.preventDefault()
      const modes: Array<'suggestions' | 'recent' | 'saved'> = ['suggestions', 'recent', 'saved']
      const currentIndex = modes.indexOf(dropdownMode)
      const nextIndex = e.shiftKey 
        ? (currentIndex - 1 + modes.length) % modes.length 
        : (currentIndex + 1) % modes.length
      setDropdownMode(modes[nextIndex])
      setSelectedIndex(-1)
    } else if (e.key === 'Enter') {
      e.preventDefault()
      if (selectedIndex >= 0) {
        if (dropdownMode === 'recent') {
          setInputValue(recentSearches[selectedIndex])
          handleSearch()
        } else if (dropdownMode === 'saved') {
          loadFilter(savedFilters[selectedIndex].id)
          setShowDropdown(false)
        } else if (suggestions[selectedIndex]) {
          applySuggestion(suggestions[selectedIndex])
        }
      } else {
        handleSearch()
      }
    } else if (e.key === 'Escape') {
      setShowDropdown(false)
      inputRef.current?.blur()
    }
  }
  
  // Handle click outside
  useEffect(() => {
    const handleClickOutside = (e: MouseEvent) => {
      if (
        dropdownRef.current &&
        !dropdownRef.current.contains(e.target as Node) &&
        !inputRef.current?.contains(e.target as Node)
      ) {
        setShowDropdown(false)
      }
    }
    
    document.addEventListener('mousedown', handleClickOutside)
    return () => document.removeEventListener('mousedown', handleClickOutside)
  }, [])
  
  // Clear input
  const handleClear = () => {
    setInputValue('')
    setQuickSearchText('')
    setActiveFilter(null)
    onSearch('')
    inputRef.current?.focus()
  }
  
  // Generate unique ID for ARIA
  const listboxId = 'search-listbox'
  const activeDescendantId = selectedIndex >= 0 ? `search-option-${selectedIndex}` : undefined

  return (
    <div className={cn('relative flex-1', className)} role="search">
      {/* Input container */}
      <div className="relative flex items-center">
        {queryError ? (
          <Warning
            size={14}
            weight="fill"
            className="absolute left-3 text-red pointer-events-none"
            aria-hidden="true"
          />
        ) : (
          <MagnifyingGlass
            size={14}
            className="absolute left-3 text-overlay-0 pointer-events-none"
            aria-hidden="true"
          />
        )}
        
        <input
          ref={inputRef}
          type="text"
          role="combobox"
          aria-expanded={showDropdown}
          aria-controls={listboxId}
          aria-activedescendant={activeDescendantId}
          aria-autocomplete="list"
          aria-label="Search documents"
          aria-invalid={!!queryError}
          value={inputValue}
          onChange={handleInputChange}
          onKeyDown={handleKeyDown}
          onFocus={() => setShowDropdown(true)}
          placeholder={activeFilter ? filterGroupToString(activeFilter) : placeholder}
          className={cn(
            'w-full pl-9 pr-20 py-1.5 text-xs rounded-full',
            'bg-surface-0 border',
            'text-text placeholder-overlay-0',
            'focus:outline-none',
            queryError 
              ? 'border-red focus:border-red' 
              : activeFilter 
                ? 'border-mauve/50 focus:border-mauve' 
                : 'border-transparent focus:border-mauve'
          )}
        />
        
        {/* Action buttons */}
        <div className="absolute right-1 flex items-center gap-0.5" role="toolbar" aria-label="Search actions">
          {(inputValue || activeFilter) && (
            <button
              onClick={handleClear}
              className="p-1 rounded-full text-overlay-0 hover:text-text hover:bg-surface-1 transition-colors focus:outline-none focus:ring-2 focus:ring-mauve focus:ring-offset-1 focus:ring-offset-base"
              title="Clear search"
              aria-label="Clear search"
            >
              <X size={12} weight="bold" aria-hidden="true" />
            </button>
          )}
          
          <button
            onClick={() => {
              setShowDropdown(true)
              setDropdownMode('recent')
              setSelectedIndex(-1)
            }}
            className={cn(
              'p-1 rounded-full transition-colors focus:outline-none focus:ring-2 focus:ring-mauve focus:ring-offset-1 focus:ring-offset-base',
              dropdownMode === 'recent' && showDropdown
                ? 'text-mauve bg-surface-1'
                : 'text-overlay-0 hover:text-text hover:bg-surface-1'
            )}
            title="Recent searches"
            aria-label="Recent searches"
            aria-pressed={dropdownMode === 'recent' && showDropdown}
          >
            <Clock size={12} weight="bold" aria-hidden="true" />
          </button>
          
          <button
            onClick={() => {
              setShowDropdown(true)
              setDropdownMode('saved')
              setSelectedIndex(-1)
            }}
            className={cn(
              'p-1 rounded-full transition-colors focus:outline-none focus:ring-2 focus:ring-mauve focus:ring-offset-1 focus:ring-offset-base',
              dropdownMode === 'saved' && showDropdown
                ? 'text-mauve bg-surface-1'
                : 'text-overlay-0 hover:text-text hover:bg-surface-1'
            )}
            title="Saved filters"
            aria-label="Saved filters"
            aria-pressed={dropdownMode === 'saved' && showDropdown}
          >
            <Bookmarks size={12} weight="bold" aria-hidden="true" />
          </button>
          
          <button
            onClick={toggleFilterBuilder}
            className="p-1 rounded-full text-overlay-0 hover:text-text hover:bg-surface-1 transition-colors focus:outline-none focus:ring-2 focus:ring-mauve focus:ring-offset-1 focus:ring-offset-base"
            title="Advanced filter builder"
            aria-label="Open advanced filter builder"
          >
            <Funnel size={12} weight="bold" aria-hidden="true" />
          </button>
        </div>
      </div>
      
      {/* Dropdown */}
      {showDropdown && (
        <div
          ref={dropdownRef}
          id={listboxId}
          role="listbox"
          aria-label="Search suggestions"
          className="absolute z-50 top-full left-0 right-0 mt-1 bg-surface-0 border border-border rounded-lg shadow-lg overflow-hidden"
        >
          {/* Mode tabs */}
          <div className="flex" role="tablist" aria-label="Search modes">
            <button
              role="tab"
              aria-selected={dropdownMode === 'suggestions'}
              aria-controls="suggestions-panel"
              onClick={() => { setDropdownMode('suggestions'); setSelectedIndex(-1) }}
              className={cn(
                'flex-1 px-3 py-1.5 text-xs font-medium transition-colors focus:outline-none border-b-2',
                dropdownMode === 'suggestions'
                  ? 'text-mauve border-mauve'
                  : 'text-overlay-0 border-transparent hover:text-text hover:border-border'
              )}
            >
              Suggestions
            </button>
            <button
              role="tab"
              aria-selected={dropdownMode === 'recent'}
              aria-controls="recent-panel"
              onClick={() => { setDropdownMode('recent'); setSelectedIndex(-1) }}
              className={cn(
                'flex-1 px-3 py-1.5 text-xs font-medium transition-colors focus:outline-none border-b-2',
                dropdownMode === 'recent'
                  ? 'text-mauve border-mauve'
                  : 'text-overlay-0 border-transparent hover:text-text hover:border-border'
              )}
            >
              Recent
            </button>
            <button
              role="tab"
              aria-selected={dropdownMode === 'saved'}
              aria-controls="saved-panel"
              onClick={() => { setDropdownMode('saved'); setSelectedIndex(-1) }}
              className={cn(
                'flex-1 px-3 py-1.5 text-xs font-medium transition-colors focus:outline-none border-b-2',
                dropdownMode === 'saved'
                  ? 'text-mauve border-mauve'
                  : 'text-overlay-0 border-transparent hover:text-text hover:border-border'
              )}
            >
              Saved
            </button>
          </div>
          
          {/* Content */}
          <div className="max-h-64 overflow-y-auto" role="tabpanel" id={`${dropdownMode}-panel`}>
            {dropdownMode === 'suggestions' && (
              <>
                {/* Error message */}
                {queryError && (
                  <div className="px-3 py-2 text-xs text-red bg-red/10 border-b border-red/20 flex items-center gap-2">
                    <span className="font-medium">Error:</span>
                    <span>{queryError}</span>
                  </div>
                )}
                
                {/* Context indicator */}
                {inputValue && !queryError && (
                  <div className="px-3 py-1.5 text-[10px] text-overlay-0 border-b border-border/50 flex items-center gap-2">
                    <span>Context:</span>
                    <span className={cn(
                      'px-1.5 py-0.5 rounded font-medium',
                      parsedContext.context === 'column' && 'bg-mauve/20 text-mauve',
                      parsedContext.context === 'operator' && 'bg-green/20 text-green',
                      parsedContext.context === 'value' && 'bg-peach/20 text-peach',
                      parsedContext.context === 'text' && 'bg-blue/20 text-blue',
                    )}>
                      {parsedContext.context === 'column' && 'Select a column'}
                      {parsedContext.context === 'operator' && `Select an operator for "${parsedContext.column}"`}
                      {parsedContext.context === 'value' && `Enter a value for "${parsedContext.column}"`}
                      {parsedContext.context === 'text' && 'Free text search'}
                    </span>
                  </div>
                )}
                {suggestions.length > 0 ? (
                  suggestions.map((suggestion, index) => (
                    <button
                      key={index}
                      id={`search-option-${index}`}
                      role="option"
                      aria-selected={selectedIndex === index}
                      onClick={() => applySuggestion(suggestion)}
                      className={cn(
                        'w-full px-3 py-2 text-left text-xs flex items-center gap-2',
                        'hover:bg-surface-1 transition-colors focus:outline-none focus:bg-surface-1',
                        selectedIndex === index && 'bg-surface-1'
                      )}
                    >
                      <span
                        className={cn(
                          'px-1.5 py-0.5 rounded text-[10px] font-medium',
                          suggestion.type === 'column' && 'bg-mauve/20 text-mauve',
                          suggestion.type === 'operator' && 'bg-green/20 text-green',
                          suggestion.type === 'value' && 'bg-peach/20 text-peach',
                          suggestion.type === 'example' && 'bg-blue/20 text-blue'
                        )}
                        aria-hidden="true"
                      >
                        {suggestion.type}
                      </span>
                      <span className="text-text truncate">{suggestion.display}</span>
                    </button>
                  ))
                ) : loadingValues ? (
                  <div className="px-3 py-4 text-center text-xs text-overlay-0" role="status">
                    Loading suggestions...
                  </div>
                ) : inputValue ? (
                  <div className="px-3 py-4 text-center text-xs text-overlay-0" role="status">
                    {parsedContext.context === 'value' && !valueFetcher ? (
                      <p>Type a value to filter by</p>
                    ) : parsedContext.context === 'value' ? (
                      <p>No matching values found. Type or press Enter to search.</p>
                    ) : (
                      <p>Press Enter to search for "{inputValue}"</p>
                    )}
                  </div>
                ) : (
                  <div className="px-3 py-4 text-center text-xs text-overlay-0">
                    <p className="mb-2">Try searching with:</p>
                    <div className="space-y-1 text-text font-mono" aria-label="Search examples">
                      <p>title = "document"</p>
                      <p>content contains "search"</p>
                      <p>created_at {'>'} "2024-01-01"</p>
                    </div>
                  </div>
                )}
              </>
            )}
            
            {dropdownMode === 'recent' && (
              <>
                {recentSearches.length > 0 ? (
                  <>
                    {recentSearches.map((search, index) => (
                      <button
                        key={index}
                        id={`search-option-${index}`}
                        role="option"
                        aria-selected={selectedIndex === index}
                        onClick={() => {
                          setInputValue(search)
                          handleSearch()
                        }}
                        className={cn(
                          'w-full px-3 py-2 text-left text-xs flex items-center gap-2',
                          'hover:bg-surface-1 transition-colors focus:outline-none focus:bg-surface-1',
                          selectedIndex === index && 'bg-surface-1'
                        )}
                      >
                        <Clock size={12} className="text-overlay-0 shrink-0" aria-hidden="true" />
                        <span className="text-text truncate">{search}</span>
                      </button>
                    ))}
                    <button
                      onClick={clearRecentSearches}
                      className="w-full px-3 py-2 text-left text-xs flex items-center gap-2 text-red hover:bg-surface-1 focus:outline-none focus:bg-surface-1"
                      aria-label="Clear all recent searches"
                    >
                      <Trash size={12} aria-hidden="true" />
                      Clear recent searches
                    </button>
                  </>
                ) : (
                  <div className="px-3 py-4 text-center text-xs text-overlay-0" role="status">
                    No recent searches
                  </div>
                )}
              </>
            )}
            
            {dropdownMode === 'saved' && (
              <>
                {savedFilters.length > 0 ? (
                  savedFilters.map((saved, index) => (
                    <button
                      key={saved.id}
                      id={`search-option-${index}`}
                      role="option"
                      aria-selected={selectedIndex === index}
                      onClick={() => {
                        loadFilter(saved.id)
                        setShowDropdown(false)
                        onFilterChange?.()
                      }}
                      className={cn(
                        'w-full px-3 py-2 text-left text-xs',
                        'hover:bg-surface-1 transition-colors focus:outline-none focus:bg-surface-1',
                        selectedIndex === index && 'bg-surface-1'
                      )}
                    >
                      <div className="flex items-center gap-2">
                        <Bookmarks size={12} className="text-mauve shrink-0" aria-hidden="true" />
                        <span className="text-text font-medium">{saved.name}</span>
                      </div>
                      <p className="mt-0.5 text-[10px] text-overlay-0 truncate pl-5">
                        {filterGroupToString(saved.filter)}
                      </p>
                    </button>
                  ))
                ) : (
                  <div className="px-3 py-4 text-center text-xs text-overlay-0" role="status">
                    No saved filters
                  </div>
                )}
              </>
            )}
          </div>
          
          {/* Footer hint */}
          <div 
            className="px-3 py-1 border-t border-border/50 text-[9px] text-overlay-0/60 flex items-center gap-3"
            aria-hidden="true"
          >
            <span>↑↓</span>
            <span>Tab</span>
            <span>↵</span>
            <span>Esc</span>
          </div>
        </div>
      )}
    </div>
  )
}
