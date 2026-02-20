import { create } from 'zustand'

export interface Table {
  id: string
  name: string
  description?: string
  metadata: Record<string, unknown>
  document_count: number
  total_nodes: number
  created_at: string
  updated_at: string
}

export interface Document {
  id: string
  data: Record<string, unknown>
  metadata?: {
    createdAt?: string
    updatedAt?: string
    version?: number
  }
}

interface TableState {
  // Tables
  tables: Table[]
  selectedTableId: string | null
  isLoadingTables: boolean
  tablesError: string | null

  // Documents
  documents: Document[]
  selectedDocumentId: string | null
  isLoadingDocuments: boolean
  documentsError: string | null
  totalDocuments: number
  currentPage: number
  pageSize: number

  // Actions - Tables
  setTables: (tables: Table[]) => void
  selectTable: (id: string | null) => void
  setLoadingTables: (loading: boolean) => void
  setTablesError: (error: string | null) => void
  addTable: (table: Table) => void
  updateTable: (id: string, updates: Partial<Table>) => void
  deleteTable: (id: string) => void

  // Actions - Documents
  setDocuments: (documents: Document[], total: number) => void
  selectDocument: (id: string | null) => void
  setLoadingDocuments: (loading: boolean) => void
  setDocumentsError: (error: string | null) => void
  addDocument: (document: Document) => void
  updateDocument: (id: string, data: Record<string, unknown>) => void
  deleteDocument: (id: string) => void
  setPage: (page: number) => void
  setPageSize: (size: number) => void

  // Helpers
  getSelectedTable: () => Table | undefined
  getSelectedDocument: () => Document | undefined
  reset: () => void
}

const initialState = {
  tables: [],
  selectedTableId: null,
  isLoadingTables: false,
  tablesError: null,
  documents: [],
  selectedDocumentId: null,
  isLoadingDocuments: false,
  documentsError: null,
  totalDocuments: 0,
  currentPage: 1,
  pageSize: 50,
}

export const useTableStore = create<TableState>((set, get) => ({
  ...initialState,

  // Table actions
  setTables: (tables) => set({ tables, tablesError: null }),
  
  selectTable: (id) => set({ 
    selectedTableId: id, 
    // Don't clear documents here - let DocumentViewer manage its own state
    // Documents will be fetched when the DocumentViewer mounts with new tableId
    selectedDocumentId: null,
    currentPage: 1,
  }),
  
  setLoadingTables: (isLoadingTables) => set({ isLoadingTables }),
  
  setTablesError: (tablesError) => set({ tablesError, isLoadingTables: false }),
  
  addTable: (table) => set((state) => ({ 
    tables: [...state.tables, table] 
  })),
  
  updateTable: (id, updates) => set((state) => ({
    tables: state.tables.map((t) => 
      t.id === id ? { ...t, ...updates } : t
    ),
  })),
  
  deleteTable: (id) => set((state) => ({
    tables: state.tables.filter((t) => t.id !== id),
    selectedTableId: state.selectedTableId === id ? null : state.selectedTableId,
  })),

  // Document actions
  setDocuments: (documents, totalDocuments) => set({ 
    documents, 
    totalDocuments,
    documentsError: null,
  }),
  
  selectDocument: (id) => set({ selectedDocumentId: id }),
  
  setLoadingDocuments: (isLoadingDocuments) => set({ isLoadingDocuments }),
  
  setDocumentsError: (documentsError) => set({ 
    documentsError, 
    isLoadingDocuments: false,
  }),
  
  addDocument: (document) => set((state) => ({
    documents: [document, ...state.documents],
    totalDocuments: state.totalDocuments + 1,
  })),
  
  updateDocument: (id, data) => set((state) => ({
    documents: state.documents.map((d) =>
      d.id === id ? { ...d, data, metadata: { ...d.metadata, updatedAt: new Date().toISOString() } } : d
    ),
  })),
  
  deleteDocument: (id) => set((state) => ({
    documents: state.documents.filter((d) => d.id !== id),
    totalDocuments: state.totalDocuments - 1,
    selectedDocumentId: state.selectedDocumentId === id ? null : state.selectedDocumentId,
  })),
  
  setPage: (currentPage) => set({ currentPage }),
  
  setPageSize: (pageSize) => set({ pageSize, currentPage: 1 }),

  // Helpers
  getSelectedTable: () => {
    const { tables, selectedTableId } = get()
    return tables.find((t) => t.id === selectedTableId)
  },
  
  getSelectedDocument: () => {
    const { documents, selectedDocumentId } = get()
    return documents.find((d) => d.id === selectedDocumentId)
  },
  
  reset: () => set(initialState),
}))
