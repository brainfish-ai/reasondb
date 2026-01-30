import { useState, useRef, useEffect, useCallback } from 'react'
import {
  MagnifyingGlass,
  X,
  CaretDown,
  Funnel,
  Clock,
  Bookmarks,
  Trash,
} from '@phosphor-icons/react'
import { cn } from '@/lib/utils'
import { useFilterStore } from '@/stores/filterStore'
import { parseSimpleQuery, filterGroupToString } from '@/lib/filter-utils'
import type { ColumnInfo } from '@/lib/filter-types'

interface SearchBarProps {
  columns: ColumnInfo[]
  placeholder?: string
  onSearch: (query: string) => void
  onFilterChange?: () => void
  className?: string
}

export function SearchBar({
  columns,
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
  
  // Sync with store
  useEffect(() => {
    setInputValue(quickSearchText)
  }, [quickSearchText])
  
  // Generate suggestions based on input
  const getSuggestions = useCallback(() => {
    const text = inputValue.toLowerCase().trim()
    if (!text) return []
    
    const suggestions: { type: 'column' | 'operator' | 'example'; value: string; display: string }[] = []
    
    // Check if input looks like start of a query
    const parts = text.split(/\s+/)
    const lastPart = parts[parts.length - 1]
    
    // Column suggestions
    if (parts.length === 1) {
      // Suggest matching columns
      columns.forEach((col) => {
        const colName = col.name.toLowerCase()
        if (colName.includes(text) || col.path.toLowerCase().includes(text)) {
          suggestions.push({
            type: 'column',
            value: col.name,
            display: `${col.name} (${col.type})`,
          })
        }
      })
      
      // Also suggest common query patterns
      suggestions.push(
        { type: 'example', value: `${text} = ""`, display: `${text} = "..." (equals)` },
        { type: 'example', value: `${text} contains ""`, display: `${text} contains "..." (partial match)` }
      )
    }
    
    // Operator suggestions if we have a column
    if (parts.length >= 1 && lastPart === '') {
      suggestions.push(
        { type: 'operator', value: '= ', display: '= (equals)' },
        { type: 'operator', value: '!= ', display: '!= (not equals)' },
        { type: 'operator', value: 'contains ', display: 'contains (partial match)' },
        { type: 'operator', value: '> ', display: '> (greater than)' },
        { type: 'operator', value: '>= ', display: '>= (greater or equal)' },
        { type: 'operator', value: '< ', display: '< (less than)' },
        { type: 'operator', value: '<= ', display: '<= (less or equal)' },
        { type: 'operator', value: 'is null', display: 'is null' },
        { type: 'operator', value: 'is not null', display: 'is not null' },
      )
    }
    
    return suggestions.slice(0, 8)
  }, [inputValue, columns])
  
  const suggestions = getSuggestions()
  
  // Handle input change
  const handleInputChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const value = e.target.value
    setInputValue(value)
    setShowDropdown(true)
    setDropdownMode('suggestions')
    setSelectedIndex(-1)
  }
  
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
          const suggestion = suggestions[selectedIndex]
          if (suggestion.type === 'column') {
            setInputValue(suggestion.value + ' ')
          } else if (suggestion.type === 'operator') {
            setInputValue(inputValue + suggestion.value)
          } else {
            setInputValue(suggestion.value)
          }
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
  
  return (
    <div className={cn('relative flex-1', className)}>
      {/* Input container */}
      <div className="relative flex items-center">
        <MagnifyingGlass
          size={14}
          className="absolute left-3 text-overlay-0 pointer-events-none"
        />
        
        <input
          ref={inputRef}
          type="text"
          value={inputValue}
          onChange={handleInputChange}
          onKeyDown={handleKeyDown}
          onFocus={() => setShowDropdown(true)}
          placeholder={activeFilter ? filterGroupToString(activeFilter) : placeholder}
          className={cn(
            'w-full pl-9 pr-20 py-1.5 text-xs rounded-full',
            'bg-surface-0 border border-transparent',
            'text-text placeholder-overlay-0',
            'focus:border-mauve focus:outline-none',
            activeFilter && 'border-mauve/50'
          )}
        />
        
        {/* Action buttons */}
        <div className="absolute right-1 flex items-center gap-0.5">
          {(inputValue || activeFilter) && (
            <button
              onClick={handleClear}
              className="p-1 rounded-full text-overlay-0 hover:text-text hover:bg-surface-1 transition-colors"
              title="Clear search"
            >
              <X size={12} weight="bold" />
            </button>
          )}
          
          <button
            onClick={() => {
              setShowDropdown(true)
              setDropdownMode('recent')
            }}
            className={cn(
              'p-1 rounded-full transition-colors',
              dropdownMode === 'recent' && showDropdown
                ? 'text-mauve bg-surface-1'
                : 'text-overlay-0 hover:text-text hover:bg-surface-1'
            )}
            title="Recent searches"
          >
            <Clock size={12} weight="bold" />
          </button>
          
          <button
            onClick={() => {
              setShowDropdown(true)
              setDropdownMode('saved')
            }}
            className={cn(
              'p-1 rounded-full transition-colors',
              dropdownMode === 'saved' && showDropdown
                ? 'text-mauve bg-surface-1'
                : 'text-overlay-0 hover:text-text hover:bg-surface-1'
            )}
            title="Saved filters"
          >
            <Bookmarks size={12} weight="bold" />
          </button>
          
          <button
            onClick={toggleFilterBuilder}
            className="p-1 rounded-full text-overlay-0 hover:text-text hover:bg-surface-1 transition-colors"
            title="Advanced filter builder"
          >
            <Funnel size={12} weight="bold" />
          </button>
        </div>
      </div>
      
      {/* Dropdown */}
      {showDropdown && (
        <div
          ref={dropdownRef}
          className="absolute z-50 top-full left-0 right-0 mt-1 bg-surface-0 border border-border rounded-lg shadow-lg overflow-hidden"
        >
          {/* Mode tabs */}
          <div className="flex border-b border-border">
            <button
              onClick={() => setDropdownMode('suggestions')}
              className={cn(
                'flex-1 px-3 py-1.5 text-xs font-medium transition-colors',
                dropdownMode === 'suggestions'
                  ? 'text-text bg-surface-1'
                  : 'text-overlay-0 hover:text-text'
              )}
            >
              Suggestions
            </button>
            <button
              onClick={() => setDropdownMode('recent')}
              className={cn(
                'flex-1 px-3 py-1.5 text-xs font-medium transition-colors',
                dropdownMode === 'recent'
                  ? 'text-text bg-surface-1'
                  : 'text-overlay-0 hover:text-text'
              )}
            >
              Recent
            </button>
            <button
              onClick={() => setDropdownMode('saved')}
              className={cn(
                'flex-1 px-3 py-1.5 text-xs font-medium transition-colors',
                dropdownMode === 'saved'
                  ? 'text-text bg-surface-1'
                  : 'text-overlay-0 hover:text-text'
              )}
            >
              Saved
            </button>
          </div>
          
          {/* Content */}
          <div className="max-h-64 overflow-y-auto">
            {dropdownMode === 'suggestions' && (
              <>
                {suggestions.length > 0 ? (
                  suggestions.map((suggestion, index) => (
                    <button
                      key={index}
                      onClick={() => {
                        if (suggestion.type === 'column') {
                          setInputValue(suggestion.value + ' ')
                          inputRef.current?.focus()
                        } else if (suggestion.type === 'operator') {
                          setInputValue(inputValue + suggestion.value)
                          inputRef.current?.focus()
                        } else {
                          setInputValue(suggestion.value)
                          inputRef.current?.focus()
                        }
                      }}
                      className={cn(
                        'w-full px-3 py-2 text-left text-xs flex items-center gap-2',
                        'hover:bg-surface-1 transition-colors',
                        selectedIndex === index && 'bg-surface-1'
                      )}
                    >
                      <span
                        className={cn(
                          'px-1.5 py-0.5 rounded text-[10px] font-medium',
                          suggestion.type === 'column' && 'bg-mauve/20 text-mauve',
                          suggestion.type === 'operator' && 'bg-green/20 text-green',
                          suggestion.type === 'example' && 'bg-blue/20 text-blue'
                        )}
                      >
                        {suggestion.type}
                      </span>
                      <span className="text-text">{suggestion.display}</span>
                    </button>
                  ))
                ) : inputValue ? (
                  <div className="px-3 py-4 text-center text-xs text-overlay-0">
                    Press Enter to search for "{inputValue}"
                  </div>
                ) : (
                  <div className="px-3 py-4 text-center text-xs text-overlay-0">
                    <p className="mb-2">Try searching with:</p>
                    <div className="space-y-1 text-text font-mono">
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
                        onClick={() => {
                          setInputValue(search)
                          handleSearch()
                        }}
                        className={cn(
                          'w-full px-3 py-2 text-left text-xs flex items-center gap-2',
                          'hover:bg-surface-1 transition-colors',
                          selectedIndex === index && 'bg-surface-1'
                        )}
                      >
                        <Clock size={12} className="text-overlay-0 shrink-0" />
                        <span className="text-text truncate">{search}</span>
                      </button>
                    ))}
                    <button
                      onClick={clearRecentSearches}
                      className="w-full px-3 py-2 text-left text-xs flex items-center gap-2 text-red hover:bg-surface-1"
                    >
                      <Trash size={12} />
                      Clear recent searches
                    </button>
                  </>
                ) : (
                  <div className="px-3 py-4 text-center text-xs text-overlay-0">
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
                      onClick={() => {
                        loadFilter(saved.id)
                        setShowDropdown(false)
                        onFilterChange?.()
                      }}
                      className={cn(
                        'w-full px-3 py-2 text-left text-xs',
                        'hover:bg-surface-1 transition-colors',
                        selectedIndex === index && 'bg-surface-1'
                      )}
                    >
                      <div className="flex items-center gap-2">
                        <Bookmarks size={12} className="text-mauve shrink-0" />
                        <span className="text-text font-medium">{saved.name}</span>
                      </div>
                      <p className="mt-0.5 text-[10px] text-overlay-0 truncate pl-5">
                        {filterGroupToString(saved.filter)}
                      </p>
                    </button>
                  ))
                ) : (
                  <div className="px-3 py-4 text-center text-xs text-overlay-0">
                    No saved filters
                  </div>
                )}
              </>
            )}
          </div>
          
          {/* Footer hint */}
          <div className="px-3 py-1.5 border-t border-border bg-mantle text-[10px] text-overlay-0 flex items-center gap-2">
            <span>↑↓ Navigate</span>
            <span>↵ Select</span>
            <span>Esc Close</span>
          </div>
        </div>
      )}
    </div>
  )
}
