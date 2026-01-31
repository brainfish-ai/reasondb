import { useState } from 'react'
import {
  Plus,
  Trash,
  X,
  FloppyDisk,
  CaretDown,
} from '@phosphor-icons/react'
import { cn } from '@/lib/utils'
import { useFilterStore } from '@/stores/filterStore'
import {
  type SearchFilter,
  type FilterGroup,
  type ColumnInfo,
  type FilterOperator,
  operatorLabels,
  getOperatorsForType,
  operatorNeedsValue,
  operatorNeedsTwoValues,
} from '@/lib/filter-types'
import { Button } from '@/components/ui/Button'
import { Input } from '@/components/ui/Input'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/Dialog'

interface FilterBuilderProps {
  columns: ColumnInfo[]
  onApply: () => void
}

function FilterRow({
  filter,
  columns,
  onUpdate,
  onRemove,
}: {
  filter: SearchFilter
  columns: ColumnInfo[]
  onUpdate: (filter: SearchFilter) => void
  onRemove: () => void
}) {
  const selectedColumn = columns.find((c) => c.path === filter.column || c.name === filter.column)
  const columnType = selectedColumn?.type || 'text'
  const availableOperators = getOperatorsForType(columnType)
  
  return (
    <div className="flex items-center gap-2 p-2 bg-surface-0 rounded-lg" role="group" aria-label="Filter condition">
      {/* Column selector */}
      <div className="relative">
        <label className="sr-only" htmlFor={`filter-column-${filter.id}`}>Column</label>
        <select
          id={`filter-column-${filter.id}`}
          value={filter.column}
          onChange={(e) => onUpdate({ ...filter, column: e.target.value })}
          className="appearance-none px-2 py-1.5 pr-7 text-xs rounded bg-base border border-border text-text focus:border-mauve focus:outline-none focus:ring-2 focus:ring-mauve/50 cursor-pointer"
        >
          {columns.map((col) => (
            <option key={col.path} value={col.path}>
              {col.name}
            </option>
          ))}
        </select>
        <CaretDown
          size={10}
          weight="bold"
          className="absolute right-2 top-1/2 -translate-y-1/2 text-overlay-0 pointer-events-none"
          aria-hidden="true"
        />
      </div>
      
      {/* Operator selector */}
      <div className="relative">
        <label className="sr-only" htmlFor={`filter-operator-${filter.id}`}>Operator</label>
        <select
          id={`filter-operator-${filter.id}`}
          value={filter.operator}
          onChange={(e) => onUpdate({ ...filter, operator: e.target.value as FilterOperator })}
          className="appearance-none px-2 py-1.5 pr-7 text-xs rounded bg-base border border-border text-text focus:border-mauve focus:outline-none focus:ring-2 focus:ring-mauve/50 cursor-pointer min-w-[100px]"
        >
          {availableOperators.map((op) => (
            <option key={op} value={op}>
              {operatorLabels[op]}
            </option>
          ))}
        </select>
        <CaretDown
          size={10}
          weight="bold"
          className="absolute right-2 top-1/2 -translate-y-1/2 text-overlay-0 pointer-events-none"
          aria-hidden="true"
        />
      </div>
      
      {/* Value input */}
      {operatorNeedsValue(filter.operator) && (
        <>
          <Input
            type={columnType === 'number' ? 'number' : columnType === 'date' ? 'date' : 'text'}
            value={filter.value?.toString() || ''}
            onChange={(e) => {
              const value = columnType === 'number' ? Number(e.target.value) : e.target.value
              onUpdate({ ...filter, value })
            }}
            placeholder="Value..."
            aria-label="Filter value"
            className="flex-1 h-7 text-xs min-w-[120px]"
          />
          
          {operatorNeedsTwoValues(filter.operator) && (
            <>
              <span className="text-xs text-overlay-0" aria-hidden="true">and</span>
              <Input
                type={columnType === 'number' ? 'number' : columnType === 'date' ? 'date' : 'text'}
                value={filter.valueEnd?.toString() || ''}
                onChange={(e) => {
                  const value = columnType === 'number' ? Number(e.target.value) : e.target.value
                  onUpdate({ ...filter, valueEnd: value })
                }}
                placeholder="End value..."
                aria-label="Filter end value"
                className="flex-1 h-7 text-xs min-w-[120px]"
              />
            </>
          )}
        </>
      )}
      
      {/* Remove button */}
      <button
        onClick={onRemove}
        className="p-1.5 rounded text-overlay-0 hover:text-red hover:bg-red/10 transition-colors focus:outline-none focus:ring-2 focus:ring-red/50"
        title="Remove filter"
        aria-label="Remove this filter condition"
      >
        <Trash size={14} aria-hidden="true" />
      </button>
    </div>
  )
}

function FilterGroupComponent({
  group,
  columns,
  onUpdate,
  onRemove,
  isRoot = false,
}: {
  group: FilterGroup
  columns: ColumnInfo[]
  onUpdate: (group: FilterGroup) => void
  onRemove?: () => void
  isRoot?: boolean
}) {
  const addFilter = () => {
    const newFilter: SearchFilter = {
      id: crypto.randomUUID(),
      column: columns[0]?.path || 'data.content',
      operator: 'contains',
      value: '',
    }
    onUpdate({
      ...group,
      filters: [...group.filters, newFilter],
    })
  }
  
  const addGroup = () => {
    const newGroup: FilterGroup = {
      id: crypto.randomUUID(),
      logic: group.logic === 'AND' ? 'OR' : 'AND',
      filters: [],
    }
    onUpdate({
      ...group,
      filters: [...group.filters, newGroup],
    })
  }
  
  const updateFilter = (index: number, filter: SearchFilter | FilterGroup) => {
    const newFilters = [...group.filters]
    newFilters[index] = filter
    onUpdate({ ...group, filters: newFilters })
  }
  
  const removeFilter = (index: number) => {
    const newFilters = group.filters.filter((_, i) => i !== index)
    onUpdate({ ...group, filters: newFilters })
  }
  
  const toggleLogic = () => {
    onUpdate({ ...group, logic: group.logic === 'AND' ? 'OR' : 'AND' })
  }
  
  return (
    <div
      className={cn(
        'space-y-2',
        !isRoot && 'p-3 border border-border rounded-lg bg-mantle/50'
      )}
      role="group"
      aria-label={isRoot ? 'Filter conditions' : `${group.logic} filter group`}
    >
      {/* Group header */}
      {!isRoot && (
        <div className="flex items-center justify-between mb-2">
          <button
            onClick={toggleLogic}
            className={cn(
              'px-2 py-0.5 text-xs font-semibold rounded transition-colors focus:outline-none focus:ring-2 focus:ring-mauve',
              group.logic === 'AND'
                ? 'bg-blue/20 text-blue'
                : 'bg-peach/20 text-peach'
            )}
            aria-label={`Toggle logic operator, currently ${group.logic}`}
          >
            {group.logic}
          </button>
          {onRemove && (
            <button
              onClick={onRemove}
              className="p-1 rounded text-overlay-0 hover:text-red hover:bg-red/10 transition-colors focus:outline-none focus:ring-2 focus:ring-red/50"
              title="Remove group"
              aria-label="Remove this filter group"
            >
              <X size={14} aria-hidden="true" />
            </button>
          )}
        </div>
      )}
      
      {/* Filters */}
      <div className="space-y-2" role="list" aria-label="Filter conditions list">
        {group.filters.map((filter, index) => (
          <div key={'id' in filter ? filter.id : index} className="relative" role="listitem">
            {/* Logic connector */}
            {index > 0 && (
              <div className="absolute -top-3 left-4 flex items-center">
                <button
                  onClick={toggleLogic}
                  className={cn(
                    'px-1.5 py-0.5 text-[10px] font-medium rounded transition-colors focus:outline-none focus:ring-2 focus:ring-mauve',
                    group.logic === 'AND'
                      ? 'bg-blue/10 text-blue hover:bg-blue/20'
                      : 'bg-peach/10 text-peach hover:bg-peach/20'
                  )}
                  aria-label={`Toggle logic operator between filters, currently ${group.logic}`}
                >
                  {group.logic}
                </button>
              </div>
            )}
            
            {'logic' in filter ? (
              <FilterGroupComponent
                group={filter as FilterGroup}
                columns={columns}
                onUpdate={(updated) => updateFilter(index, updated)}
                onRemove={() => removeFilter(index)}
              />
            ) : (
              <FilterRow
                filter={filter as SearchFilter}
                columns={columns}
                onUpdate={(updated) => updateFilter(index, updated)}
                onRemove={() => removeFilter(index)}
              />
            )}
          </div>
        ))}
      </div>
      
      {/* Add buttons */}
      <div className="flex items-center gap-2 pt-1" role="toolbar" aria-label="Filter actions">
        <Button
          variant="ghost"
          size="sm"
          onClick={addFilter}
          className="text-xs gap-1.5"
          aria-label="Add a new filter condition"
        >
          <Plus size={12} weight="bold" aria-hidden="true" />
          Add Filter
        </Button>
        <Button
          variant="ghost"
          size="sm"
          onClick={addGroup}
          className="text-xs gap-1.5"
          aria-label="Add a new filter group for nested conditions"
        >
          <Plus size={12} weight="bold" aria-hidden="true" />
          Add Group
        </Button>
      </div>
    </div>
  )
}

export function FilterBuilder({ columns, onApply }: FilterBuilderProps) {
  const {
    filterBuilderOpen,
    setFilterBuilderOpen,
    activeFilter,
    setActiveFilter,
    saveFilter,
  } = useFilterStore()
  
  const [showSaveDialog, setShowSaveDialog] = useState(false)
  const [filterName, setFilterName] = useState('')
  
  // Initialize filter group if none exists
  const currentFilter: FilterGroup = activeFilter || {
    id: crypto.randomUUID(),
    logic: 'AND',
    filters: [],
  }
  
  const handleApply = () => {
    if (currentFilter.filters.length === 0) {
      setActiveFilter(null)
    } else {
      setActiveFilter(currentFilter)
    }
    onApply()
    setFilterBuilderOpen(false)
  }
  
  const handleClear = () => {
    setActiveFilter(null)
    onApply()
  }
  
  const handleSave = () => {
    if (filterName.trim() && currentFilter.filters.length > 0) {
      saveFilter(filterName.trim(), currentFilter)
      setFilterName('')
      setShowSaveDialog(false)
    }
  }
  
  return (
    <>
      {/* Filter Builder Panel */}
      {filterBuilderOpen && (
        <div 
          className="border-b border-border bg-base p-4"
          role="region"
          aria-label="Filter builder"
        >
          <div className="flex items-center justify-between mb-4">
            <h3 id="filter-builder-title" className="text-sm font-semibold text-text">Filter Builder</h3>
            <div className="flex items-center gap-2" role="toolbar" aria-label="Filter builder actions">
              <Button
                variant="ghost"
                size="sm"
                onClick={() => setShowSaveDialog(true)}
                disabled={currentFilter.filters.length === 0}
                className="text-xs gap-1.5"
                aria-label="Save current filter"
              >
                <FloppyDisk size={14} aria-hidden="true" />
                Save
              </Button>
              <Button
                variant="ghost"
                size="sm"
                onClick={handleClear}
                className="text-xs"
                aria-label="Clear all filters"
              >
                Clear
              </Button>
              <Button size="sm" onClick={handleApply} className="text-xs" aria-label="Apply filter to results">
                Apply Filter
              </Button>
              <button
                onClick={() => setFilterBuilderOpen(false)}
                className="p-1 rounded text-overlay-0 hover:text-text hover:bg-surface-0 transition-colors focus:outline-none focus:ring-2 focus:ring-mauve"
                aria-label="Close filter builder"
              >
                <X size={16} aria-hidden="true" />
              </button>
            </div>
          </div>
          
          <FilterGroupComponent
            group={currentFilter}
            columns={columns}
            onUpdate={setActiveFilter}
            isRoot
          />
        </div>
      )}
      
      {/* Save Filter Dialog */}
      <Dialog open={showSaveDialog} onOpenChange={setShowSaveDialog}>
        <DialogContent className="max-w-sm" aria-describedby="save-filter-description">
          <DialogHeader>
            <DialogTitle>Save Filter</DialogTitle>
          </DialogHeader>
          <div className="space-y-4 pt-2">
            <p id="save-filter-description" className="sr-only">
              Enter a name for your filter to save it for later use.
            </p>
            <Input
              value={filterName}
              onChange={(e) => setFilterName(e.target.value)}
              placeholder="Filter name..."
              aria-label="Filter name"
              onKeyDown={(e) => e.key === 'Enter' && handleSave()}
              autoFocus
            />
            <div className="flex justify-end gap-2">
              <Button variant="ghost" onClick={() => setShowSaveDialog(false)}>
                Cancel
              </Button>
              <Button onClick={handleSave} disabled={!filterName.trim()}>
                Save
              </Button>
            </div>
          </div>
        </DialogContent>
      </Dialog>
    </>
  )
}
