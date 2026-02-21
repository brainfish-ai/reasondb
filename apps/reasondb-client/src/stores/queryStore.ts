import { create } from 'zustand'
import { persist } from 'zustand/middleware'
import type { ReasonProgressEvent } from '@/lib/api'

export interface QueryResult {
  columns: string[]
  rows: Record<string, unknown>[]
  rowCount: number
  executionTime: number
  error?: string
}

export interface QueryHistoryItem {
  id: string
  query: string
  connectionId: string
  executedAt: string
  executionTime: number
  rowCount: number
  error?: string
}

export interface SavedQuery {
  id: string
  name: string
  query: string
  description?: string
  connectionId?: string
  createdAt: string
  updatedAt: string
}

interface QueryState {
  // Current query
  currentQuery: string
  isExecuting: boolean
  result: QueryResult | null
  error: string | null

  // REASON progress tracking
  reasonProgress: ReasonProgressEvent | null

  // History
  history: QueryHistoryItem[]
  maxHistoryItems: number

  // Saved queries
  savedQueries: SavedQuery[]

  // Actions
  setCurrentQuery: (query: string) => void
  setIsExecuting: (isExecuting: boolean) => void
  setResult: (result: QueryResult | null) => void
  setError: (error: string | null) => void
  setReasonProgress: (progress: ReasonProgressEvent | null) => void

  // History actions
  addToHistory: (item: Omit<QueryHistoryItem, 'id'>) => void
  clearHistory: () => void
  removeFromHistory: (id: string) => void

  // Saved queries actions
  saveQuery: (query: Omit<SavedQuery, 'id' | 'createdAt' | 'updatedAt'>) => void
  updateSavedQuery: (id: string, updates: Partial<SavedQuery>) => void
  deleteSavedQuery: (id: string) => void
  loadSavedQuery: (id: string) => void

  // Reset
  reset: () => void
}

const initialState = {
  currentQuery: '',
  isExecuting: false,
  result: null,
  error: null,
  reasonProgress: null,
  history: [],
  maxHistoryItems: 100,
  savedQueries: [],
}

export const useQueryStore = create<QueryState>()(
  persist(
    (set, get) => ({
      ...initialState,

      setCurrentQuery: (query) => set({ currentQuery: query }),
      setIsExecuting: (isExecuting) => set({ isExecuting }),
      setResult: (result) => set({ result, error: null, reasonProgress: null }),
      setError: (error) => set({ error, result: null, reasonProgress: null }),
      setReasonProgress: (progress) => set({ reasonProgress: progress }),

      addToHistory: (item) =>
        set((state) => {
          const newItem: QueryHistoryItem = {
            ...item,
            id: crypto.randomUUID(),
          }
          const history = [newItem, ...state.history].slice(0, state.maxHistoryItems)
          return { history }
        }),

      clearHistory: () => set({ history: [] }),

      removeFromHistory: (id) =>
        set((state) => ({
          history: state.history.filter((item) => item.id !== id),
        })),

      saveQuery: (query) =>
        set((state) => {
          const now = new Date().toISOString()
          const newQuery: SavedQuery = {
            ...query,
            id: crypto.randomUUID(),
            createdAt: now,
            updatedAt: now,
          }
          return { savedQueries: [...state.savedQueries, newQuery] }
        }),

      updateSavedQuery: (id, updates) =>
        set((state) => ({
          savedQueries: state.savedQueries.map((q) =>
            q.id === id ? { ...q, ...updates, updatedAt: new Date().toISOString() } : q
          ),
        })),

      deleteSavedQuery: (id) =>
        set((state) => ({
          savedQueries: state.savedQueries.filter((q) => q.id !== id),
        })),

      loadSavedQuery: (id) => {
        const query = get().savedQueries.find((q) => q.id === id)
        if (query) {
          set({ currentQuery: query.query })
        }
      },

      reset: () => set(initialState),
    }),
    {
      name: 'reasondb-queries',
      partialize: (state) => ({
        history: state.history,
        savedQueries: state.savedQueries,
        maxHistoryItems: state.maxHistoryItems,
      }),
    }
  )
)
