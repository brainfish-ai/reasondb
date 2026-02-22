import { create } from 'zustand'
import { persist } from 'zustand/middleware'

const MAX_TABS = 20

export interface Tab {
  id: string
  title: string
  type: 'query' | 'table' | 'settings'
  tableId?: string
  query?: string
}

interface TabsState {
  tabs: Tab[]
  activeTabId: string | null
  
  addTab: (tab: Omit<Tab, 'id'>) => string
  closeTab: (id: string) => void
  setActiveTab: (id: string | null) => void
  updateTab: (id: string, updates: Partial<Tab>) => void
  updateTabQuery: (id: string, query: string) => void
  getActiveTab: () => Tab | undefined
  reset: () => void
}

const initialState = {
  tabs: [] as Tab[],
  activeTabId: null as string | null,
}

export const useTabsStore = create<TabsState>()(
  persist(
    (set, get) => ({
      ...initialState,
      
      addTab: (tab) => {
        const id = crypto.randomUUID()
        const newTab: Tab = { ...tab, id }
        set((state) => {
          let tabs = [...state.tabs, newTab]

          while (tabs.length > MAX_TABS) {
            const evictIdx = tabs.findIndex((t) => t.id !== id && t.id !== state.activeTabId)
            if (evictIdx === -1) break
            tabs = tabs.filter((_, i) => i !== evictIdx)
          }

          return { tabs, activeTabId: id }
        })
        return id
      },
      
      closeTab: (id) => {
        set((state) => {
          const newTabs = state.tabs.filter((t) => t.id !== id)
          const closedIndex = state.tabs.findIndex((t) => t.id === id)
          
          // If closing the active tab, switch to adjacent tab
          let newActiveId = state.activeTabId
          if (state.activeTabId === id) {
            if (newTabs.length > 0) {
              const newIndex = Math.min(closedIndex, newTabs.length - 1)
              newActiveId = newTabs[newIndex].id
            } else {
              newActiveId = null
            }
          }
          
          return { tabs: newTabs, activeTabId: newActiveId }
        })
      },
      
      setActiveTab: (activeTabId) => set({ activeTabId }),
      
      updateTab: (id, updates) => {
        set((state) => ({
          tabs: state.tabs.map((t) => 
            t.id === id ? { ...t, ...updates } : t
          ),
        }))
      },
      
      updateTabQuery: (id, query) => {
        set((state) => ({
          tabs: state.tabs.map((t) => 
            t.id === id ? { ...t, query } : t
          ),
        }))
      },
      
      getActiveTab: () => {
        const { tabs, activeTabId } = get()
        return tabs.find((t) => t.id === activeTabId)
      },
      
      reset: () => set(initialState),
    }),
    {
      name: 'reasondb-tabs',
    }
  )
)
