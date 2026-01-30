/**
 * ReasonDB API Client
 */

export interface ApiConfig {
  host: string
  port: number
  apiKey?: string
  useSsl?: boolean
}

export interface HealthResponse {
  status: string
  version: string
  uptime_seconds?: number
}

export interface Table {
  name: string
  schema?: string
  row_count?: number
}

export interface Document {
  id: string
  content: string
  metadata?: Record<string, unknown>
}

export interface QueryResult {
  rows: Record<string, unknown>[]
  columns: string[]
  row_count: number
  execution_time_ms: number
}

export interface ApiError {
  error: string
  message: string
  status?: number
}

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

  /**
   * Test connection to the server
   */
  async testConnection(): Promise<{ success: boolean; version?: string; error?: string }> {
    try {
      // Try health endpoint - may return plain text "OK" or JSON
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
      
      // Try to parse as JSON first
      try {
        const health = JSON.parse(text) as HealthResponse
        return {
          success: health.status === 'ok' || health.status === 'healthy',
          version: health.version,
        }
      } catch {
        // Plain text response (e.g., "OK")
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

  /**
   * List all tables
   */
  async listTables(): Promise<Table[]> {
    return this.request<Table[]>('/api/v1/tables')
  }

  /**
   * Get documents from a table
   */
  async getDocuments(
    tableName: string,
    options?: { limit?: number; offset?: number }
  ): Promise<Document[]> {
    const params = new URLSearchParams()
    if (options?.limit) params.set('limit', options.limit.toString())
    if (options?.offset) params.set('offset', options.offset.toString())
    
    const query = params.toString() ? `?${params}` : ''
    return this.request<Document[]>(`/api/v1/tables/${tableName}/documents${query}`)
  }

  /**
   * Execute RQL query
   */
  async executeQuery(query: string): Promise<QueryResult> {
    return this.request<QueryResult>('/api/v1/query', {
      method: 'POST',
      body: JSON.stringify({ query }),
    })
  }

  /**
   * Create a new table
   */
  async createTable(name: string, schema?: Record<string, string>): Promise<Table> {
    return this.request<Table>('/api/v1/tables', {
      method: 'POST',
      body: JSON.stringify({ name, schema }),
    })
  }

  /**
   * Insert a document
   */
  async insertDocument(
    tableName: string,
    content: string,
    metadata?: Record<string, unknown>
  ): Promise<Document> {
    return this.request<Document>(`/api/v1/tables/${tableName}/documents`, {
      method: 'POST',
      body: JSON.stringify({ content, metadata }),
    })
  }

  /**
   * Delete a document
   */
  async deleteDocument(tableName: string, documentId: string): Promise<void> {
    await this.request(`/api/v1/tables/${tableName}/documents/${documentId}`, {
      method: 'DELETE',
    })
  }
}

// Singleton instances per connection
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
