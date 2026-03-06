export interface JobStatus {
  job_id: string
  status: "queued" | "processing" | "completed" | "failed"
  progress?: string
  result?: { document_id: string; title: string; total_nodes: number }
  error?: string
}

export interface MatchedNode {
  node_id: string
  title: string
  content: string
  path: string[]
  confidence: number
  highlights?: string[]
  reasoning_trace?: Array<{ node_title: string; decision: string; confidence: number }>
}

export interface QueryResult {
  rows: Record<string, unknown>[]
  columns: string[]
  rowCount: number
  executionTimeMs: number
  /** Populated for REASON queries — the matched nodes with confidence scores */
  matchedNodes?: MatchedNode[]
  /** The natural-language question extracted from the REASON clause */
  question?: string
}

interface QueryServerResponse {
  documents?: Array<Record<string, unknown>>
  total_count?: number
  execution_time_ms: number
  aggregates?: Array<{ name: string; value: unknown }>
}

export interface IngestJobResponse {
  job_id: string
  status: string
}

export class ReasonDBClient {
  private baseUrl: string
  private headers: Record<string, string>

  constructor(serverUrl: string, apiKey?: string) {
    this.baseUrl = serverUrl.replace(/\/$/, "")
    this.headers = {
      "Content-Type": "application/json",
      ...(apiKey ? { "X-API-Key": apiKey } : {}),
    }
  }

  async health(): Promise<{ ok: boolean; version?: string }> {
    try {
      const res = await fetch(`${this.baseUrl}/health`, {
        headers: this.headers,
        signal: AbortSignal.timeout(5000),
      })
      if (!res.ok) return { ok: false }
      const data = await res.json()
      return { ok: data.status === "ok", version: data.version }
    } catch {
      return { ok: false }
    }
  }

  async tableExists(tableName: string): Promise<boolean> {
    try {
      const res = await fetch(`${this.baseUrl}/v1/tables/${encodeURIComponent(tableName)}`, {
        headers: this.headers,
      })
      return res.ok
    } catch {
      return false
    }
  }

  async getTableDocCount(tableName: string): Promise<number> {
    try {
      // /v1/tables/:id requires a UUID; use the list endpoint and match by name instead
      const res = await fetch(`${this.baseUrl}/v1/tables`, {
        headers: this.headers,
        signal: AbortSignal.timeout(5000),
      })
      if (!res.ok) return 0
      const data = await res.json()
      const table = (data.tables as Array<{ name: string; document_count: number }>)
        ?.find((t) => t.name === tableName)
      return table?.document_count ?? 0
    } catch {
      return 0
    }
  }

  async getJobStatus(jobId: string): Promise<JobStatus> {
    const res = await fetch(`${this.baseUrl}/v1/jobs/${encodeURIComponent(jobId)}`, {
      headers: this.headers,
    })
    if (!res.ok) throw new Error(`Failed to get job ${jobId}: ${res.status}`)
    return res.json()
  }

  async executeQuery(query: string): Promise<QueryResult> {
    const res = await fetch(`${this.baseUrl}/v1/query`, {
      method: "POST",
      headers: this.headers,
      body: JSON.stringify({ query: query.trim().replace(/;+$/, "") }),
    })
    if (!res.ok) {
      const err = await res.json().catch(() => ({ message: res.statusText }))
      throw new Error(err.message ?? err.error ?? "Query failed")
    }
    const data: QueryServerResponse = await res.json()
    return this.transformResponse(data, query)
  }

  async executeQueryStream(
    query: string,
    onProgress: (msg: string) => void
  ): Promise<QueryResult> {
    const res = await fetch(`${this.baseUrl}/v1/query/stream`, {
      method: "POST",
      headers: this.headers,
      body: JSON.stringify({ query: query.trim().replace(/;+$/, "") }),
    })
    if (!res.ok) {
      const err = await res.json().catch(() => ({ message: res.statusText }))
      throw new Error(err.message ?? "Stream query failed")
    }
    return new Promise((resolve, reject) => {
      const reader = res.body?.getReader()
      if (!reader) { reject(new Error("No response body")); return }
      const decoder = new TextDecoder()
      let buffer = ""
      const pump = async () => {
        try {
          while (true) {
            const { done, value } = await reader.read()
            if (done) break
            buffer += decoder.decode(value, { stream: true })
            const lines = buffer.split("\n")
            buffer = lines.pop() ?? ""
            let eventType = ""
            let eventData = ""
            for (const line of lines) {
              if (line.startsWith("event:")) eventType = line.slice(6).trim()
              else if (line.startsWith("data:")) eventData = line.slice(5).trim()
              else if (line === "" && eventType && eventData) {
                if (eventType === "progress") {
                  const p = JSON.parse(eventData)
                  onProgress(p.message ?? "")
                } else if (eventType === "complete") {
                  resolve(this.transformResponse(JSON.parse(eventData), query))
                  return
                } else if (eventType === "error") {
                  reject(new Error(eventData)); return
                }
                eventType = ""; eventData = ""
              }
            }
          }
          reject(new Error("Stream ended without result"))
        } catch (e) { reject(e) }
      }
      pump()
    })
  }

  private transformResponse(data: QueryServerResponse, query?: string): QueryResult {
    if (data.documents && data.documents.length > 0) {
      // Extract matched_nodes from all rows for REASON queries
      const matchedNodes: MatchedNode[] = []
      for (const doc of data.documents) {
        const nodes = doc.matched_nodes
        if (Array.isArray(nodes)) {
          for (const n of nodes) {
            matchedNodes.push(n as MatchedNode)
          }
        }
      }

      return {
        columns: Object.keys(data.documents[0]),
        rows: data.documents,
        rowCount: data.total_count ?? data.documents.length,
        executionTimeMs: data.execution_time_ms,
        ...(matchedNodes.length > 0 && {
          matchedNodes,
          question: query ? extractReasonQuestion(query) : undefined,
        }),
      }
    }
    if (data.aggregates && data.aggregates.length > 0) {
      const row: Record<string, unknown> = {}
      data.aggregates.forEach((a) => { row[a.name] = a.value })
      return {
        columns: data.aggregates.map((a) => a.name),
        rows: [row],
        rowCount: 1,
        executionTimeMs: data.execution_time_ms,
      }
    }
    return { columns: [], rows: [], rowCount: 0, executionTimeMs: data.execution_time_ms }
  }
}

/** Extract the natural-language question from a REASON clause, e.g. REASON 'What is...' */
function extractReasonQuestion(query: string): string | undefined {
  const match = query.match(/REASON\s+['"](.+?)['"]/i)
  return match?.[1]
}
