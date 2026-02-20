import { useCallback, useEffect } from 'react'
import { useTableStore, type Document } from '@/stores/tableStore'
import { useConnectionStore } from '@/stores/connectionStore'
import { createClient, type TableDocumentSummary } from '@/lib/api'
import { updateTableMetadataFieldsFromSchema } from '@/lib/rql-language'

// Track which tables have had their metadata schema fetched to prevent repeated requests
const fetchedSchemas = new Set<string>()

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
    tables,
    setDocuments,
    selectDocument,
    setLoadingDocuments,
    setDocumentsError,
  } = useTableStore()

  const activeConnection = connections.find(c => c.id === activeConnectionId)
  const currentTable = tables.find(t => t.id === tableId)

  const fetchDocuments = useCallback(async (forceRefresh: boolean = false) => {
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

      const response = await client.getTableDocuments(tableId, { forceRefresh })
      const storeDocs = response.documents.map(apiDocumentToStoreDocument)
      setDocuments(storeDocs, response.total)
      
      // Fetch metadata schema from server for autocompletion
      // Force refresh if explicitly requested, otherwise only once per table
      const shouldFetchSchema = forceRefresh || !fetchedSchemas.has(tableId)
      if (currentTable && shouldFetchSchema) {
        fetchedSchemas.add(tableId) // Mark as fetched before request to prevent race conditions
        try {
          const schemaResponse = await client.getTableMetadataSchema(tableId, forceRefresh)
          updateTableMetadataFieldsFromSchema(currentTable.name, schemaResponse.fields)
        } catch (schemaError) {
          // Schema endpoint might not exist on older servers, fail silently
          // Remove from set so it can be retried later
          fetchedSchemas.delete(tableId)
        }
      }
    } catch (error) {
      console.error('Failed to fetch documents:', error)
      setDocumentsError(error instanceof Error ? error.message : 'Failed to fetch documents')
      setDocuments([], 0)
    } finally {
      setLoadingDocuments(false)
    }
  }, [activeConnection, tableId, currentTable, setLoadingDocuments, setDocuments, setDocumentsError])

  // Load documents when table is selected
  useEffect(() => {
    if (tableId && activeConnection) {
      fetchDocuments()
    }
  }, [tableId, activeConnection, fetchDocuments])

  // Listen for background ingestion completions
  useEffect(() => {
    const handler = (e: Event) => {
      const detail = (e as CustomEvent).detail
      if (detail?.tableId === tableId) {
        fetchDocuments(true)
      }
    }
    window.addEventListener('reasondb:documents-changed', handler)
    return () => window.removeEventListener('reasondb:documents-changed', handler)
  }, [tableId, fetchDocuments])

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
