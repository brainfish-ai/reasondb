import { create } from 'zustand'
import { persist } from 'zustand/middleware'
import type { FilterGroup, SavedFilter, ColumnInfo } from '@/lib/filter-types'

interface FilterState {
  // Active filter for current table
  activeFilter: FilterGroup | null
  
  // Quick search text
  quickSearchText: string
  
  // Detected columns from current table data
  detectedColumns: ColumnInfo[]
  
  // Saved filters
  savedFilters: SavedFilter[]
  
  // Recent searches
  recentSearches: string[]
  
  // Filter builder open state
  filterBuilderOpen: boolean
  
  // Actions
  setActiveFilter: (filter: FilterGroup | null) => void
  setQuickSearchText: (text: string) => void
  setDetectedColumns: (columns: ColumnInfo[]) => void
  
  // Saved filters
  saveFilter: (name: string, filter: FilterGroup) => void
  deleteFilter: (id: string) => void
  loadFilter: (id: string) => void
  
  // Recent searches
  addRecentSearch: (search: string) => void
  clearRecentSearches: () => void
  
  // UI
  setFilterBuilderOpen: (open: boolean) => void
  toggleFilterBuilder: () => void
  
  // Reset
  clearFilter: () => void
}

export const useFilterStore = create<FilterState>()(
  persist(
    (set, get) => ({
      activeFilter: null,
      quickSearchText: '',
      detectedColumns: [],
      savedFilters: [],
      recentSearches: [],
      filterBuilderOpen: false,
      
      setActiveFilter: (filter) => set({ activeFilter: filter }),
      
      setQuickSearchText: (text) => set({ quickSearchText: text }),
      
      setDetectedColumns: (columns) => set({ detectedColumns: columns }),
      
      saveFilter: (name, filter) => {
        const newFilter: SavedFilter = {
          id: crypto.randomUUID(),
          name,
          filter,
          createdAt: new Date().toISOString(),
        }
        set((state) => ({
          savedFilters: [...state.savedFilters, newFilter],
        }))
      },
      
      deleteFilter: (id) => {
        set((state) => ({
          savedFilters: state.savedFilters.filter((f) => f.id !== id),
        }))
      },
      
      loadFilter: (id) => {
        const { savedFilters } = get()
        const saved = savedFilters.find((f) => f.id === id)
        if (saved) {
          set({ activeFilter: saved.filter })
        }
      },
      
      addRecentSearch: (search) => {
        if (!search.trim()) return
        set((state) => {
          const filtered = state.recentSearches.filter((s) => s !== search)
          return {
            recentSearches: [search, ...filtered].slice(0, 10),
          }
        })
      },
      
      clearRecentSearches: () => set({ recentSearches: [] }),
      
      setFilterBuilderOpen: (open) => set({ filterBuilderOpen: open }),
      
      toggleFilterBuilder: () => {
        set((state) => ({ filterBuilderOpen: !state.filterBuilderOpen }))
      },
      
      clearFilter: () => {
        set({
          activeFilter: null,
          quickSearchText: '',
        })
      },
    }),
    {
      name: 'reasondb-filters',
      partialize: (state) => ({
        savedFilters: state.savedFilters,
        recentSearches: state.recentSearches,
      }),
    }
  )
)
