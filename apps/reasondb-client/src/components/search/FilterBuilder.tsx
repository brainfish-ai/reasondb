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
    <div className="flex items-center gap-2 p-2 bg-surface-0 rounded-lg">
      {/* Column selector */}
      <div className="relative">
        <select
          value={filter.column}
          onChange={(e) => onUpdate({ ...filter, column: e.target.value })}
          className="appearance-none px-2 py-1.5 pr-7 text-xs rounded bg-base border border-border text-text focus:border-mauve focus:outline-none cursor-pointer"
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
        />
      </div>
      
      {/* Operator selector */}
      <div className="relative">
        <select
          value={filter.operator}
          onChange={(e) => onUpdate({ ...filter, operator: e.target.value as FilterOperator })}
          className="appearance-none px-2 py-1.5 pr-7 text-xs rounded bg-base border border-border text-text focus:border-mauve focus:outline-none cursor-pointer min-w-[100px]"
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
            className="flex-1 h-7 text-xs min-w-[120px]"
          />
          
          {operatorNeedsTwoValues(filter.operator) && (
            <>
              <span className="text-xs text-overlay-0">and</span>
              <Input
                type={columnType === 'number' ? 'number' : columnType === 'date' ? 'date' : 'text'}
                value={filter.valueEnd?.toString() || ''}
                onChange={(e) => {
                  const value = columnType === 'number' ? Number(e.target.value) : e.target.value
                  onUpdate({ ...filter, valueEnd: value })
                }}
                placeholder="End value..."
                className="flex-1 h-7 text-xs min-w-[120px]"
              />
            </>
          )}
        </>
      )}
      
      {/* Remove button */}
      <button
        onClick={onRemove}
        className="p-1.5 rounded text-overlay-0 hover:text-red hover:bg-red/10 transition-colors"
        title="Remove filter"
      >
        <Trash size={14} />
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
    >
      {/* Group header */}
      {!isRoot && (
        <div className="flex items-center justify-between mb-2">
          <button
            onClick={toggleLogic}
            className={cn(
              'px-2 py-0.5 text-xs font-semibold rounded transition-colors',
              group.logic === 'AND'
                ? 'bg-blue/20 text-blue'
                : 'bg-peach/20 text-peach'
            )}
          >
            {group.logic}
          </button>
          {onRemove && (
            <button
              onClick={onRemove}
              className="p-1 rounded text-overlay-0 hover:text-red hover:bg-red/10 transition-colors"
              title="Remove group"
            >
              <X size={14} />
            </button>
          )}
        </div>
      )}
      
      {/* Filters */}
      <div className="space-y-2">
        {group.filters.map((filter, index) => (
          <div key={'id' in filter ? filter.id : index} className="relative">
            {/* Logic connector */}
            {index > 0 && (
              <div className="absolute -top-3 left-4 flex items-center">
                <button
                  onClick={toggleLogic}
                  className={cn(
                    'px-1.5 py-0.5 text-[10px] font-medium rounded transition-colors',
                    group.logic === 'AND'
                      ? 'bg-blue/10 text-blue hover:bg-blue/20'
                      : 'bg-peach/10 text-peach hover:bg-peach/20'
                  )}
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
      <div className="flex items-center gap-2 pt-1">
        <Button
          variant="ghost"
          size="sm"
          onClick={addFilter}
          className="text-xs gap-1.5"
        >
          <Plus size={12} weight="bold" />
          Add Filter
        </Button>
        <Button
          variant="ghost"
          size="sm"
          onClick={addGroup}
          className="text-xs gap-1.5"
        >
          <Plus size={12} weight="bold" />
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
        <div className="border-b border-border bg-base p-4">
          <div className="flex items-center justify-between mb-4">
            <h3 className="text-sm font-semibold text-text">Filter Builder</h3>
            <div className="flex items-center gap-2">
              <Button
                variant="ghost"
                size="sm"
                onClick={() => setShowSaveDialog(true)}
                disabled={currentFilter.filters.length === 0}
                className="text-xs gap-1.5"
              >
                <FloppyDisk size={14} />
                Save
              </Button>
              <Button
                variant="ghost"
                size="sm"
                onClick={handleClear}
                className="text-xs"
              >
                Clear
              </Button>
              <Button size="sm" onClick={handleApply} className="text-xs">
                Apply Filter
              </Button>
              <button
                onClick={() => setFilterBuilderOpen(false)}
                className="p-1 rounded text-overlay-0 hover:text-text hover:bg-surface-0 transition-colors"
              >
                <X size={16} />
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
        <DialogContent className="max-w-sm">
          <DialogHeader>
            <DialogTitle>Save Filter</DialogTitle>
          </DialogHeader>
          <div className="space-y-4 pt-2">
            <Input
              value={filterName}
              onChange={(e) => setFilterName(e.target.value)}
              placeholder="Filter name..."
              onKeyDown={(e) => e.key === 'Enter' && handleSave()}
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
