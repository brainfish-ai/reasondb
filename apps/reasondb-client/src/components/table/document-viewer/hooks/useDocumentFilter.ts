import { useMemo } from 'react'
import { useFilterStore } from '@/stores/filterStore'
import { filterDocuments } from '@/lib/filter-utils'
import type { Document } from '@/stores/tableStore'

/**
 * Hook for filtering documents based on active filter and search
 */
export function useDocumentFilter(documents: Document[]) {
  const { activeFilter, quickSearchText } = useFilterStore()

  const filteredDocuments = useMemo(() => {
    if (!activeFilter && !quickSearchText) return documents

    // Simple text search if no structured filter
    if (!activeFilter && quickSearchText) {
      const searchLower = quickSearchText.toLowerCase()
      return documents.filter((doc) =>
        JSON.stringify(doc.data).toLowerCase().includes(searchLower)
      )
    }

    // Apply structured filter
    if (activeFilter) {
      return filterDocuments(
        documents as unknown as Record<string, unknown>[],
        activeFilter
      ) as unknown as Document[]
    }

    return documents
  }, [documents, activeFilter, quickSearchText])

  return {
    filteredDocuments,
    activeFilter,
    quickSearchText,
    isFiltered: Boolean(activeFilter || quickSearchText),
  }
}
