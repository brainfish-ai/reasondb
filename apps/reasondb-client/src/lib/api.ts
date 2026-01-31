/**
 * ReasonDB API Client
 * 
 * Provides typed access to the ReasonDB server API endpoints.
 */

export interface ApiConfig {
  host: string
  port: number
  apiKey?: string
  useSsl?: boolean
}

// ==================== Response Types ====================

export interface HealthResponse {
  status: string
  version: string
  uptime_seconds?: number
}

// Tables
export interface TableSummary {
  id: string
  name: string
  description?: string
  document_count: number
  total_nodes: number
}

export interface TableResponse {
  id: string
  name: string
  description?: string
  metadata: Record<string, unknown>
  document_count: number
  total_nodes: number
  created_at: string
  updated_at: string
}

export interface ListTablesResponse {
  tables: TableSummary[]
  total: number
}

// Documents
export interface DocumentSummary {
  id: string
  title: string
  total_nodes: number
  max_depth: number
  source_path: string
  mime_type?: string
  file_size?: number
  table_id?: string
  tags: string[]
  created_at: string
}

export interface DocumentDetail {
  id: string
  title: string
  root_node_id: string
  total_nodes: number
  max_depth: number
  source_path: string
  mime_type?: string
  file_size?: number
  created_at: string
  updated_at: string
}

export interface TableDocumentSummary {
  id: string
  title: string
  total_nodes: number
  tags: string[]
  metadata: Record<string, unknown>
  created_at: string
}

export interface TableDocumentsResponse {
  table_id: string
  documents: TableDocumentSummary[]
  total: number
}

// Metadata Schema (for autocompletion)
export interface MetadataField {
  path: string
  field_type: string
  occurrence_count: number
}

export interface MetadataSchemaResponse {
  table_id: string
  fields: MetadataField[]
  documents_sampled: number
  total_documents: number
}

// Column Values (for autocompletion)
export interface ColumnValue {
  value: string
  count: number
}

export interface ColumnValuesResponse {
  table_id: string
  column: string
  values: ColumnValue[]
  documents_sampled: number
}

// Nodes
export interface NodeSummary {
  id: string
  title: string
  summary: string
  depth: number
  is_leaf: boolean
  children_count: number
}

export interface TreeNode {
  id: string
  title: string
  summary: string
  depth: number
  is_leaf: boolean
  children: TreeNode[]
}

// Search
export interface SearchRequest {
  query: string
  document_id?: string
  table_id?: string
  tags?: string[]
  metadata?: Record<string, unknown>
  max_depth?: number
  beam_width?: number
  min_confidence?: number
  limit?: number
}

export interface PathNode {
  node_id: string
  title: string
  reasoning: string
}

export interface SearchResult {
  node_id: string
  document_id: string
  path: PathNode[]
  content: string
  answer?: string
  confidence: number
}

export interface SearchStats {
  nodes_visited: number
  nodes_pruned: number
  llm_calls: number
  total_time_ms: number
}

export interface SearchResponse {
  results: SearchResult[]
  stats: SearchStats
}

// Query (RQL)
export interface QueryResult {
  rows: Record<string, unknown>[]
  columns: string[]
  rowCount: number
  executionTime: number
}

// Server query response (internal)
interface QueryServerResponse {
  documents: Array<{
    id: string
    title: string
    table_id: string
    tags: string[]
    metadata: Record<string, unknown>
    total_nodes: number
    created_at: string
    score?: number
    highlights?: string[]
    answer?: string
    confidence?: number
  }>
  total_count: number
  execution_time_ms: number
  aggregates?: Array<{
    name: string
    value: unknown
    group_key?: Array<[string, unknown]>
  }>
}

// Errors
export interface ApiError {
  error: string
  message: string
  status?: number
}

// ==================== API Client ====================

class ReasonDBClient {
  private baseUrl: string
  private apiKey?: string

  constructor(config: ApiConfig) {
    const protocol = config.useSsl ? 'https' : 'http'
    this.baseUrl = `${protocol}://${config.host}:${config.port}`
    this.apiKey = config.apiKey
  }

  private async request<T>(
    endpoint: string,
    options: RequestInit = {}
  ): Promise<T> {
    const headers: Record<string, string> = {
      'Content-Type': 'application/json',
      ...((options.headers as Record<string, string>) || {}),
    }

    if (this.apiKey) {
      headers['X-API-Key'] = this.apiKey
    }

    const response = await fetch(`${this.baseUrl}${endpoint}`, {
      ...options,
      headers,
    })

    if (!response.ok) {
      const error = await response.json().catch(() => ({
        error: 'Unknown error',
        message: response.statusText,
      }))
      throw new Error(error.message || error.error || 'Request failed')
    }

    return response.json()
  }

  // ==================== Health ====================

  /**
   * Test connection to the server
   */
  async testConnection(): Promise<{ success: boolean; version?: string; error?: string }> {
    try {
      const response = await fetch(`${this.baseUrl}/health`, {
        headers: this.apiKey ? { 'X-API-Key': this.apiKey } : {},
        signal: AbortSignal.timeout(5000),
      })
      
      if (!response.ok) {
        return {
          success: false,
          error: `Server returned ${response.status}: ${response.statusText}`,
        }
      }
      
      const text = await response.text()
      
      try {
        const health = JSON.parse(text) as HealthResponse
        return {
          success: health.status === 'ok' || health.status === 'healthy',
          version: health.version,
        }
      } catch {
        if (text.toLowerCase().includes('ok') || text.toLowerCase().includes('healthy')) {
          return { success: true }
        }
        return { success: false, error: text }
      }
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : 'Connection failed',
      }
    }
  }

  /**
   * Get server health
   */
  async health(): Promise<HealthResponse> {
    return this.request<HealthResponse>('/health')
  }

  // ==================== Tables ====================

  /**
   * List all tables
   */
  async listTables(): Promise<ListTablesResponse> {
    return this.request<ListTablesResponse>('/v1/tables')
  }

  /**
   * Get table details
   */
  async getTable(tableId: string): Promise<TableResponse> {
    return this.request<TableResponse>(`/v1/tables/${encodeURIComponent(tableId)}`)
  }

  /**
   * Create a new table
   */
  async createTable(
    name: string,
    options?: { description?: string; metadata?: Record<string, unknown> }
  ): Promise<TableResponse> {
    return this.request<TableResponse>('/v1/tables', {
      method: 'POST',
      body: JSON.stringify({ name, ...options }),
    })
  }

  /**
   * Update a table
   */
  async updateTable(
    tableId: string,
    updates: { name?: string; description?: string; metadata?: Record<string, unknown> }
  ): Promise<TableResponse> {
    return this.request<TableResponse>(`/v1/tables/${encodeURIComponent(tableId)}`, {
      method: 'PATCH',
      body: JSON.stringify(updates),
    })
  }

  /**
   * Delete a table
   */
  async deleteTable(tableId: string, cascade = false): Promise<void> {
    await this.request(`/v1/tables/${encodeURIComponent(tableId)}`, {
      method: 'DELETE',
      body: JSON.stringify({ cascade }),
    })
  }

  /**
   * Get documents in a table
   */
  async getTableDocuments(
    tableId: string,
    options?: { limit?: number; offset?: number }
  ): Promise<TableDocumentsResponse> {
    const params = new URLSearchParams()
    if (options?.limit) params.set('limit', options.limit.toString())
    if (options?.offset) params.set('offset', options.offset.toString())
    
    const queryString = params.toString()
    const url = `/v1/tables/${encodeURIComponent(tableId)}/documents${queryString ? `?${queryString}` : ''}`
    
    return this.request<TableDocumentsResponse>(url)
  }

  /**
   * Get metadata schema for a table (samples documents to detect field structure)
   * This is more efficient than fetching all documents for large tables
   */
  async getTableMetadataSchema(tableId: string): Promise<MetadataSchemaResponse> {
    return this.request<MetadataSchemaResponse>(
      `/v1/tables/${encodeURIComponent(tableId)}/schema/metadata`
    )
  }

  /**
   * Get distinct values for a column (for autocompletion)
   * Supports: title, tags, metadata.field_name
   */
  async getColumnValues(tableId: string, column: string): Promise<ColumnValuesResponse> {
    return this.request<ColumnValuesResponse>(
      `/v1/tables/${encodeURIComponent(tableId)}/values/${encodeURIComponent(column)}`
    )
  }

  // ==================== Documents ====================

  /**
   * List all documents
   */
  async listDocuments(): Promise<DocumentSummary[]> {
    return this.request<DocumentSummary[]>('/v1/documents')
  }

  /**
   * Get document details
   */
  async getDocument(documentId: string): Promise<DocumentDetail> {
    return this.request<DocumentDetail>(`/v1/documents/${encodeURIComponent(documentId)}`)
  }

  /**
   * Delete a document
   */
  async deleteDocument(documentId: string): Promise<void> {
    await this.request(`/v1/documents/${encodeURIComponent(documentId)}`, {
      method: 'DELETE',
    })
  }

  /**
   * Get nodes for a document
   */
  async getDocumentNodes(documentId: string): Promise<NodeSummary[]> {
    return this.request<NodeSummary[]>(
      `/v1/documents/${encodeURIComponent(documentId)}/nodes`
    )
  }

  /**
   * Get document tree structure
   */
  async getDocumentTree(documentId: string): Promise<TreeNode> {
    return this.request<TreeNode>(`/v1/documents/${encodeURIComponent(documentId)}/tree`)
  }

  // ==================== Search ====================

  /**
   * Search documents using LLM-guided tree traversal
   */
  async search(request: SearchRequest): Promise<SearchResponse> {
    return this.request<SearchResponse>('/v1/search', {
      method: 'POST',
      body: JSON.stringify(request),
    })
  }

  // ==================== Query (RQL) ====================

  /**
   * Execute RQL query
   */
  async executeQuery(query: string): Promise<QueryResult> {
    // Strip trailing semicolons - RQL doesn't use them
    const cleanQuery = query.trim().replace(/;+$/, '').trim()
    
    const response = await this.request<QueryServerResponse>('/v1/query', {
      method: 'POST',
      body: JSON.stringify({ query: cleanQuery }),
    })
    
    // Transform server response to frontend format
    if (response.documents && response.documents.length > 0) {
      // Get columns from first document
      const firstDoc = response.documents[0]
      const columns = Object.keys(firstDoc)
      
      return {
        columns,
        rows: response.documents,
        rowCount: response.total_count,
        executionTime: response.execution_time_ms,
      }
    }
    
    // Handle aggregate results
    if (response.aggregates && response.aggregates.length > 0) {
      const columns = response.aggregates.map(a => a.name)
      const row: Record<string, unknown> = {}
      response.aggregates.forEach(a => {
        row[a.name] = a.value
      })
      
      return {
        columns,
        rows: [row],
        rowCount: 1,
        executionTime: response.execution_time_ms,
      }
    }
    
    // Empty result
    return {
      columns: [],
      rows: [],
      rowCount: 0,
      executionTime: response.execution_time_ms,
    }
  }
}

// ==================== Client Management ====================

const clients = new Map<string, ReasonDBClient>()

export function createClient(config: ApiConfig): ReasonDBClient {
  return new ReasonDBClient(config)
}

export function getClient(connectionId: string): ReasonDBClient | undefined {
  return clients.get(connectionId)
}

export function setClient(connectionId: string, client: ReasonDBClient): void {
  clients.set(connectionId, client)
}

export function removeClient(connectionId: string): void {
  clients.delete(connectionId)
}

export { ReasonDBClient }
