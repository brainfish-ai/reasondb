/**
 * ReasonDB API Client
 * 
 * Provides typed access to the ReasonDB server API endpoints.
 * Includes request caching to reduce API calls and prevent rate limiting.
 */

export interface ApiConfig {
  host: string
  port: number
  apiKey?: string
  useSsl?: boolean
}

// ==================== Request Cache ====================

interface CacheEntry<T> {
  data: T
  timestamp: number
  expiresAt: number
}

interface CacheConfig {
  defaultTTL: number      // Default TTL in milliseconds
  maxEntries: number      // Maximum cache entries
  enabled: boolean        // Whether caching is enabled
}

// Endpoint-specific TTL configurations (in milliseconds)
const ENDPOINT_TTL: Record<string, number> = {
  '/health': 10000,                    // 10 seconds
  '/v1/tables': 30000,                 // 30 seconds
  '/v1/documents': 30000,              // 30 seconds
  'schema/metadata': 300000,           // 5 minutes (metadata schema rarely changes)
  'values': 60000,                     // 1 minute (column values)
  'default': 30000,                    // 30 seconds default
}

class RequestCache {
  private cache = new Map<string, CacheEntry<unknown>>()
  private config: CacheConfig = {
    defaultTTL: 30000,    // 30 seconds default
    maxEntries: 100,
    enabled: true,
  }
  
  // Patterns that should invalidate related cache entries
  private invalidationPatterns: Array<{ method: string; pattern: RegExp; invalidates: RegExp[] }> = [
    // Creating/updating/deleting documents invalidates document lists
    { method: 'POST', pattern: /\/documents/, invalidates: [/\/documents/, /\/tables\/.+\/documents/] },
    { method: 'PUT', pattern: /\/documents/, invalidates: [/\/documents/] },
    { method: 'DELETE', pattern: /\/documents/, invalidates: [/\/documents/, /\/tables\/.+\/documents/] },
    // Creating/updating/deleting tables invalidates table lists
    { method: 'POST', pattern: /\/tables/, invalidates: [/\/tables/] },
    { method: 'PUT', pattern: /\/tables/, invalidates: [/\/tables/] },
    { method: 'DELETE', pattern: /\/tables/, invalidates: [/\/tables/] },
  ]
  
  /**
   * Get TTL for a specific endpoint
   */
  private getTTL(endpoint: string): number {
    // Check for specific endpoint matches
    for (const [pattern, ttl] of Object.entries(ENDPOINT_TTL)) {
      if (pattern !== 'default' && endpoint.includes(pattern)) {
        return ttl
      }
    }
    return this.config.defaultTTL
  }
  
  /**
   * Generate cache key from request details
   */
  private getCacheKey(baseUrl: string, endpoint: string, options?: RequestInit): string {
    const method = options?.method || 'GET'
    const body = options?.body ? JSON.stringify(options.body) : ''
    return `${method}:${baseUrl}${endpoint}:${body}`
  }
  
  /**
   * Check if an endpoint should be cached
   */
  private shouldCache(method: string): boolean {
    // Only cache GET requests
    return this.config.enabled && method === 'GET'
  }
  
  /**
   * Get cached response if valid
   */
  get<T>(baseUrl: string, endpoint: string, options?: RequestInit): T | null {
    if (!this.shouldCache(options?.method || 'GET')) {
      return null
    }
    
    const key = this.getCacheKey(baseUrl, endpoint, options)
    const entry = this.cache.get(key) as CacheEntry<T> | undefined
    
    if (!entry) {
      return null
    }
    
    // Check if expired
    if (Date.now() > entry.expiresAt) {
      this.cache.delete(key)
      return null
    }
    
    return entry.data
  }
  
  /**
   * Store response in cache
   */
  set<T>(baseUrl: string, endpoint: string, data: T, options?: RequestInit): void {
    if (!this.shouldCache(options?.method || 'GET')) {
      return
    }
    
    // Enforce max entries
    if (this.cache.size >= this.config.maxEntries) {
      // Remove oldest entries
      const entriesToRemove = Math.floor(this.config.maxEntries * 0.2)
      const keys = Array.from(this.cache.keys()).slice(0, entriesToRemove)
      keys.forEach(key => this.cache.delete(key))
    }
    
    const key = this.getCacheKey(baseUrl, endpoint, options)
    const ttl = this.getTTL(endpoint)
    const now = Date.now()
    
    this.cache.set(key, {
      data,
      timestamp: now,
      expiresAt: now + ttl,
    })
  }
  
  /**
   * Invalidate cache entries based on mutation
   */
  invalidate(method: string, endpoint: string): void {
    // Check if this mutation should invalidate any cache entries
    for (const rule of this.invalidationPatterns) {
      if (rule.method === method && rule.pattern.test(endpoint)) {
        // Invalidate matching cache entries
        for (const key of this.cache.keys()) {
          for (const invalidatePattern of rule.invalidates) {
            if (invalidatePattern.test(key)) {
              this.cache.delete(key)
            }
          }
        }
      }
    }
  }
  
  /**
   * Invalidate specific endpoint
   */
  invalidateEndpoint(baseUrl: string, endpoint: string): void {
    const keyPrefix = `GET:${baseUrl}${endpoint}`
    for (const key of this.cache.keys()) {
      if (key.startsWith(keyPrefix)) {
        this.cache.delete(key)
      }
    }
  }
  
  /**
   * Clear all cache entries
   */
  clear(): void {
    this.cache.clear()
  }
  
  /**
   * Get cache statistics
   */
  getStats(): { size: number; enabled: boolean } {
    return {
      size: this.cache.size,
      enabled: this.config.enabled,
    }
  }
  
  /**
   * Enable or disable caching
   */
  setEnabled(enabled: boolean): void {
    this.config.enabled = enabled
    if (!enabled) {
      this.clear()
    }
  }
}

// Global cache instance
const requestCache = new RequestCache()

// Export for debugging/testing
export { requestCache }

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
  content?: string  // Original content (only present for leaf nodes)
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
  title: string
  document_id: string
  path: PathNode[]
  content: string
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
  traceId?: string
}

export interface QueryValidationResult {
  index: number
  valid: boolean
  error?: string
  line?: number
  column?: number
}

// SSE progress events from REASON query execution
export interface ReasonProgressEvent {
  phase: 'candidates' | 'ranking' | 'reasoning'
  status: 'started' | 'progress' | 'completed'
  message: string
  detail?: {
    count?: number
    total?: number
    current?: number
    doc_title?: string
    matches?: number
  }
}

export interface ReasoningStepResponse {
  node_title: string
  decision: string
  confidence: number
}

export interface MatchedNodeResponse {
  node_id: string
  title: string
  content: string
  path: string[]
  confidence: number
  reasoning_trace: ReasoningStepResponse[]
}

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
    matched_nodes?: MatchedNodeResponse[]
    confidence?: number
  }>
  total_count: number
  execution_time_ms: number
  trace_id?: string
  aggregates?: Array<{
    name: string
    value: unknown
    group_key?: Array<[string, unknown]>
  }>
}

// ==================== Trace Types ====================

export interface SubQueryTrace {
  text: string
  rationale: string
  bm25_hits: number
}

export interface DomainContextTrace {
  table_name: string
  description?: string
  vocab_hints: string[]
}

export interface DecompositionTrace {
  domain_context?: DomainContextTrace
  sub_queries: SubQueryTrace[]
}

export interface Bm25HitTrace {
  document_id: string
  document_title: string
  score: number
  matched_node_count: number
  sub_query_index: number
}

export interface Bm25SelectionTrace {
  total_candidates: number
  hits: Bm25HitTrace[]
}

export interface TreeGrepScoreTrace {
  document_id: string
  document_title: string
  combined_score: number
  matched_sections: string[]
}

export interface StructuralFilterTrace {
  terms: string[]
  filtered_count: number
  scores: TreeGrepScoreTrace[]
}

export interface DocumentRankingTrace {
  document_id: string
  document_title: string
  relevance: number
  reasoning: string
}

export interface LlmRankingTrace {
  input_count: number
  selected_count: number
  skipped_llm: boolean
  rankings: DocumentRankingTrace[]
}

export interface BeamReasoningStep {
  node_title: string
  decision: string
  confidence: number
}

export interface LeafVerificationTrace {
  node_id: string
  node_title: string
  is_relevant: boolean
  confidence: number
  path: string[]
  reasoning_steps: BeamReasoningStep[]
}

export interface BeamDocumentTrace {
  document_id: string
  document_title: string
  nodes_visited: number
  nodes_pruned: number
  llm_calls: number
  relevant_leaves: LeafVerificationTrace[]
}

export interface BeamReasoningTrace {
  documents_processed: number
  total_llm_calls: number
  documents: BeamDocumentTrace[]
}

export interface FinalResultTrace {
  document_id: string
  document_title: string
  node_id: string
  node_title: string
  confidence: number
  path: string[]
}

export interface QueryTrace {
  trace_id: string
  query: string
  table_id: string
  created_at: string
  duration_ms: number
  decomposition?: DecompositionTrace
  bm25_selection: Bm25SelectionTrace
  structural_filter: StructuralFilterTrace
  llm_ranking: LlmRankingTrace
  beam_reasoning: BeamReasoningTrace
  final_results: FinalResultTrace[]
}

export interface QueryTraceSummary {
  trace_id: string
  query: string
  table_id: string
  created_at: string
  duration_ms: number
  result_count: number
  sub_query_count: number
  total_llm_calls: number
  candidate_count: number
}

// Ingestion
export interface IngestTextRequest {
  title: string
  content: string
  generate_summaries?: boolean
  tags?: string[]
  metadata?: Record<string, unknown>
}

export interface IngestUrlRequest {
  url: string
  generate_summaries?: boolean
}

export interface IngestStats {
  chars_extracted: number
  chunks_created: number
  nodes_created: number
  summaries_generated: number
  total_time_ms: number
}

export interface IngestResponse {
  document_id: string
  title: string
  total_nodes: number
  max_depth: number
  stats: IngestStats
}

// Jobs
export interface JobStatusResponse {
  job_id: string
  status: 'queued' | 'processing' | 'completed' | 'failed'
  progress?: string
  result?: IngestResponse
  error?: string
  created_at: string
  updated_at: string
}

// LLM Configuration
export interface LlmOptions {
  temperature?: number
  max_tokens?: number
  system_prompt?: string
  top_p?: number
  frequency_penalty?: number
  presence_penalty?: number
  disable_thinking?: boolean
}

export interface LlmModelConfig {
  provider: string
  api_key?: string
  model?: string
  base_url?: string
  region?: string
  options?: LlmOptions
}

export interface LlmSettings {
  ingestion: LlmModelConfig
  retrieval: LlmModelConfig
}

export interface PatchLlmSettings {
  ingestion?: LlmModelConfig
  retrieval?: LlmModelConfig
}

// LLM Health Test
export interface LlmTestStatus {
  ok: boolean
  error?: string
  latency_ms?: number
}

export interface LlmTestResult {
  ingestion: LlmTestStatus
  retrieval: LlmTestStatus
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

  /**
   * Make an API request with optional caching
   * @param endpoint - API endpoint
   * @param options - Fetch options
   * @param skipCache - Force bypass cache for this request
   */
  private async request<T>(
    endpoint: string,
    options: RequestInit = {},
    skipCache: boolean = false,
  ): Promise<T> {
    const method = options.method || 'GET'
    
    if (method === 'GET' && !skipCache) {
      const cached = requestCache.get<T>(this.baseUrl, endpoint, options)
      if (cached !== null) {
        return cached
      }
    }
    
    const headers: Record<string, string> = {
      'Content-Type': 'application/json',
      ...((options.headers as Record<string, string>) || {}),
    }

    if (this.apiKey) {
      headers['X-API-Key'] = this.apiKey
    }

    const url = `${this.baseUrl}${endpoint}`

    const response = await fetch(url, {
      ...options,
      headers,
    })

    if (!response.ok) {
      const errorBody = await response.json().catch(() => ({
        error: 'Unknown error',
        message: response.statusText,
      }))
      const message =
        errorBody.message ??
        (typeof errorBody.error === 'string'
          ? errorBody.error
          : errorBody.error?.message) ??
        'Request failed'
      throw new Error(message)
    }

    const data = await response.json() as T
    
    if (method === 'GET') {
      requestCache.set(this.baseUrl, endpoint, data, options)
    } else {
      requestCache.invalidate(method, endpoint)
    }

    return data
  }
  
  /**
   * Invalidate cache for a specific endpoint (useful after mutations)
   */
  invalidateCache(endpoint: string): void {
    requestCache.invalidateEndpoint(this.baseUrl, endpoint)
  }
  
  /**
   * Clear all cached requests for this client
   */
  clearCache(): void {
    requestCache.clear()
  }

  // ==================== Health ====================

  /**
   * Test connection to the server
   */
  async testConnection(): Promise<{ success: boolean; version?: string; error?: string }> {
    const url = `${this.baseUrl}/health`

    try {
      const { Command } = await import('@tauri-apps/plugin-shell')
      const args = ['-s', '-m', '5', url]
      if (this.apiKey) {
        args.push('-H', `X-API-Key: ${this.apiKey}`)
      }
      const output = await Command.create('curl', args).execute()

      if (output.code !== 0) {
        return { success: false, error: output.stderr.trim() || 'curl failed' }
      }

      return this.parseHealthBody(output.stdout.trim())
    } catch {
      // Fallback to fetch when running outside Tauri (e.g. browser dev mode)
    }

    const controller = new AbortController()
    const timeoutId = setTimeout(() => controller.abort(), 5000)
    try {
      const response = await fetch(url, {
        headers: this.apiKey ? { 'X-API-Key': this.apiKey } : {},
        signal: controller.signal,
      })

      if (!response.ok) {
        return {
          success: false,
          error: `Server returned ${response.status}: ${response.statusText}`,
        }
      }

      const text = await response.text()
      return this.parseHealthBody(text)
    } catch (error) {
      if (error instanceof DOMException && error.name === 'AbortError') {
        return { success: false, error: 'Connection timed out' }
      }
      return {
        success: false,
        error: error instanceof Error ? error.message : 'Connection failed',
      }
    } finally {
      clearTimeout(timeoutId)
    }
  }

  private parseHealthBody(body: string): { success: boolean; version?: string; error?: string } {
    const health = JSON.parse(body) as HealthResponse
    return { success: health.status === 'ok', version: health.version }
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
  /**
   * List all tables
   * @param forceRefresh - Bypass cache and fetch fresh data
   */
  async listTables(forceRefresh?: boolean): Promise<ListTablesResponse> {
    return this.request<ListTablesResponse>('/v1/tables', {}, forceRefresh)
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
   * @param tableId - Table ID
   * @param options - Query options (limit, offset, forceRefresh)
   */
  async getTableDocuments(
    tableId: string,
    options?: { limit?: number; offset?: number; forceRefresh?: boolean }
  ): Promise<TableDocumentsResponse> {
    const params = new URLSearchParams()
    if (options?.limit) params.set('limit', options.limit.toString())
    if (options?.offset) params.set('offset', options.offset.toString())
    
    const queryString = params.toString()
    const url = `/v1/tables/${encodeURIComponent(tableId)}/documents${queryString ? `?${queryString}` : ''}`
    
    return this.request<TableDocumentsResponse>(url, {}, options?.forceRefresh)
  }

  /**
   * Get metadata schema for a table (samples documents to detect field structure)
   * This is more efficient than fetching all documents for large tables
   * @param tableId - Table ID
   * @param forceRefresh - Bypass cache and fetch fresh data
   */
  async getTableMetadataSchema(tableId: string, forceRefresh?: boolean): Promise<MetadataSchemaResponse> {
    return this.request<MetadataSchemaResponse>(
      `/v1/tables/${encodeURIComponent(tableId)}/schema/metadata`,
      {},
      forceRefresh
    )
  }

  /**
   * Get distinct values for a column (for autocompletion)
   * Supports: title, tags, metadata.field_name
   * @param tableId - Table ID
   * @param column - Column name
   * @param forceRefresh - Bypass cache and fetch fresh data
   */
  async getColumnValues(tableId: string, column: string, forceRefresh?: boolean): Promise<ColumnValuesResponse> {
    return this.request<ColumnValuesResponse>(
      `/v1/tables/${encodeURIComponent(tableId)}/values/${encodeURIComponent(column)}`,
      {},
      forceRefresh
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
   * Update a document's title, tags, and/or metadata (merge patch).
   * Only the supplied fields are changed; metadata keys are merged, not replaced.
   */
  async updateDocument(
    documentId: string,
    updates: { title?: string; tags?: string[]; metadata?: Record<string, unknown> }
  ): Promise<{ updated: boolean; document_id: string }> {
    return this.request<{ updated: boolean; document_id: string }>(
      `/v1/documents/${encodeURIComponent(documentId)}`,
      {
        method: 'PATCH',
        body: JSON.stringify(updates),
      }
    )
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
    const cleanQuery = query.trim().replace(/;+$/, '').trim()
    
    const response = await this.request<QueryServerResponse>('/v1/query', {
      method: 'POST',
      body: JSON.stringify({ query: cleanQuery }),
    })
    
    return this.transformQueryResponse(response)
  }

  /**
   * Execute RQL query with SSE progress streaming (for REASON queries).
   * Emits progress events via the callback and returns the final result.
   */
  async executeQueryStream(
    query: string,
    onProgress: (event: ReasonProgressEvent) => void,
  ): Promise<QueryResult> {
    const cleanQuery = query.trim().replace(/;+$/, '').trim()

    const headers: Record<string, string> = {
      'Content-Type': 'application/json',
    }
    if (this.apiKey) {
      headers['X-API-Key'] = this.apiKey
    }

    const url = `${this.baseUrl}/v1/query/stream`
    const response = await fetch(url, {
      method: 'POST',
      headers,
      body: JSON.stringify({ query: cleanQuery }),
    })

    if (!response.ok) {
      const errorBody = await response.json().catch(() => ({
        error: 'Unknown error',
        message: response.statusText,
      }))
      const message =
        errorBody.message ??
        (typeof errorBody.error === 'string'
          ? errorBody.error
          : errorBody.error?.message) ??
        'Request failed'
      throw new Error(message)
    }

    return new Promise<QueryResult>((resolve, reject) => {
      const reader = response.body?.getReader()
      if (!reader) {
        reject(new Error('No response body'))
        return
      }

      const decoder = new TextDecoder()
      let buffer = ''

      const processChunk = async () => {
        try {
          while (true) {
            const { done, value } = await reader.read()
            if (done) break

            buffer += decoder.decode(value, { stream: true })

            // Parse SSE events from buffer
            const lines = buffer.split('\n')
            buffer = lines.pop() || ''

            let eventType = ''
            let eventData = ''

            for (const line of lines) {
              if (line.startsWith('event:')) {
                eventType = line.slice(6).trim()
              } else if (line.startsWith('data:')) {
                eventData = line.slice(5).trim()
              } else if (line === '' && eventType && eventData) {
                // End of an event block
                try {
                  if (eventType === 'progress') {
                    const progress = JSON.parse(eventData) as ReasonProgressEvent
                    onProgress(progress)
                  } else if (eventType === 'complete') {
                    const serverResponse = JSON.parse(eventData) as QueryServerResponse
                    resolve(this.transformQueryResponse(serverResponse))
                    return
                  } else if (eventType === 'error') {
                    reject(new Error(eventData))
                    return
                  }
                } catch {
                  // Ignore malformed events
                }
                eventType = ''
                eventData = ''
              }
            }
          }

          // Stream ended without a complete event
          reject(new Error('Stream ended without results'))
        } catch (err) {
          reject(err)
        }
      }

      processChunk()
    })
  }

  /**
   * Validate RQL queries without executing them.
   * Returns per-query validation results with error positions for editor markers.
   */
  async validateQueries(queries: string[]): Promise<QueryValidationResult[]> {
    const response = await this.request<{ results: QueryValidationResult[] }>('/v1/query/validate', {
      method: 'POST',
      body: JSON.stringify({ queries }),
    })
    return response.results
  }

  private transformQueryResponse(response: QueryServerResponse): QueryResult {
    const traceId = response.trace_id

    if (response.documents && response.documents.length > 0) {
      const firstDoc = response.documents[0]
      const columns = Object.keys(firstDoc)
      return {
        columns,
        rows: response.documents,
        rowCount: response.total_count,
        executionTime: response.execution_time_ms,
        traceId,
      }
    }

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
        traceId,
      }
    }

    return {
      columns: [],
      rows: [],
      rowCount: 0,
      executionTime: response.execution_time_ms,
      traceId,
    }
  }

  // ==================== Traces ====================

  /**
   * List recent query traces for a table (newest first).
   */
  async listTraces(tableId: string, limit = 50): Promise<QueryTraceSummary[]> {
    return this.request<QueryTraceSummary[]>(
      `/v1/tables/${encodeURIComponent(tableId)}/traces?limit=${limit}`,
      {},
      true,
    )
  }

  /**
   * Get full trace data for a specific trace ID.
   */
  async getTrace(tableId: string, traceId: string): Promise<QueryTrace> {
    return this.request<QueryTrace>(
      `/v1/tables/${encodeURIComponent(tableId)}/traces/${encodeURIComponent(traceId)}`,
      {},
      true,
    )
  }

  // ==================== Ingestion ====================

  async ingestText(tableName: string, request: IngestTextRequest): Promise<JobStatusResponse> {
    return this.request<JobStatusResponse>(`/v1/tables/${encodeURIComponent(tableName)}/ingest/text`, {
      method: 'POST',
      body: JSON.stringify(request),
    })
  }

  async ingestUrl(tableName: string, request: IngestUrlRequest): Promise<JobStatusResponse> {
    return this.request<JobStatusResponse>(`/v1/tables/${encodeURIComponent(tableName)}/ingest/url`, {
      method: 'POST',
      body: JSON.stringify(request),
    })
  }

  // ==================== LLM Configuration ====================

  /**
   * Get current LLM settings from the server (keys masked)
   */
  async getLlmConfig(): Promise<LlmSettings> {
    return this.request<LlmSettings>('/v1/config/llm', {}, true)
  }

  /**
   * Replace both ingestion and retrieval LLM config
   */
  async updateLlmConfig(settings: LlmSettings): Promise<LlmSettings> {
    return this.request<LlmSettings>('/v1/config/llm', {
      method: 'PUT',
      body: JSON.stringify(settings),
    })
  }

  /**
   * Partially update LLM config (ingestion and/or retrieval)
   */
  async patchLlmConfig(patch: PatchLlmSettings): Promise<LlmSettings> {
    return this.request<LlmSettings>('/v1/config/llm', {
      method: 'PATCH',
      body: JSON.stringify(patch),
    })
  }

  /**
   * Update only the ingestion LLM config
   */
  async updateIngestionConfig(config: LlmModelConfig): Promise<LlmSettings> {
    return this.patchLlmConfig({ ingestion: config })
  }

  /**
   * Update only the retrieval LLM config
   */
  async updateRetrievalConfig(config: LlmModelConfig): Promise<LlmSettings> {
    return this.patchLlmConfig({ retrieval: config })
  }

  /**
   * Test both ingestion and retrieval LLM connectivity
   */
  async testLlmConfig(): Promise<LlmTestResult> {
    return this.request<LlmTestResult>('/v1/config/llm/test', {
      method: 'POST',
    })
  }

  // ==================== Jobs ====================

  async getJobStatus(jobId: string): Promise<JobStatusResponse> {
    return this.request<JobStatusResponse>(
      `/v1/jobs/${encodeURIComponent(jobId)}`,
      {},
      true, // always skip cache for job status
    )
  }

  async listJobs(limit = 50): Promise<JobStatusResponse[]> {
    return this.request<JobStatusResponse[]>(
      `/v1/jobs?limit=${limit}`,
      {},
      true,
    )
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
