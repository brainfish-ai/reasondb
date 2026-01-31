import type { Document } from '@/stores/tableStore'

// Selected cell data for sidebar
export interface SelectedCellData {
  title: string
  path: string
  data: unknown
}

// View modes
export type ViewMode = 'table' | 'json'

// Props for DocumentViewer
export interface DocumentViewerProps {
  tableId: string
}

// Re-export Document type
export type { Document }
