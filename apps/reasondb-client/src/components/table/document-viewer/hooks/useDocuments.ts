import { useCallback, useEffect } from 'react'
import { useTableStore, type Document } from '@/stores/tableStore'
import { useConnectionStore } from '@/stores/connectionStore'
import { createClient, type TableDocumentSummary } from '@/lib/api'

/**
 * Convert API response to document store format
 */
function apiDocumentToStoreDocument(apiDoc: TableDocumentSummary): Document {
  return {
    id: apiDoc.id,
    data: {
      id: apiDoc.id,
      title: apiDoc.title,
      total_nodes: apiDoc.total_nodes,
      tags: apiDoc.tags,
      metadata: apiDoc.metadata || {},
      created_at: apiDoc.created_at,
    },
    metadata: {
      createdAt: apiDoc.created_at,
      updatedAt: apiDoc.created_at,
      version: 1,
    },
  }
}

/**
 * Hook for fetching and managing documents
 */
export function useDocuments(tableId: string) {
  const { activeConnectionId, connections } = useConnectionStore()
  const {
    documents,
    selectedDocumentId,
    isLoadingDocuments,
    totalDocuments,
    pageSize,
    documentsError,
    setDocuments,
    selectDocument,
    setLoadingDocuments,
    setDocumentsError,
  } = useTableStore()

  const activeConnection = connections.find(c => c.id === activeConnectionId)

  const fetchDocuments = useCallback(async () => {
    if (!activeConnection || !tableId) return

    setLoadingDocuments(true)
    setDocumentsError(null)
    setDocuments([], 0)

    try {
      const client = createClient({
        host: activeConnection.host,
        port: activeConnection.port,
        apiKey: activeConnection.apiKey,
        useSsl: activeConnection.ssl,
      })

      const response = await client.getTableDocuments(tableId)
      const storeDocs = response.documents.map(apiDocumentToStoreDocument)
      setDocuments(storeDocs, response.total)
    } catch (error) {
      console.error('Failed to fetch documents:', error)
      setDocumentsError(error instanceof Error ? error.message : 'Failed to fetch documents')
      setDocuments([], 0)
    } finally {
      setLoadingDocuments(false)
    }
  }, [activeConnection, tableId, setLoadingDocuments, setDocuments, setDocumentsError])

  // Load documents when table is selected
  useEffect(() => {
    if (tableId && activeConnection) {
      fetchDocuments()
    }
  }, [tableId, activeConnection, fetchDocuments])

  return {
    documents,
    selectedDocumentId,
    isLoadingDocuments,
    totalDocuments,
    pageSize,
    documentsError,
    selectDocument,
    fetchDocuments,
    activeConnection,
  }
}
