export interface OpenRouterModel {
  id: string
  name: string
  created?: number
  context_length: number
  pricing: { prompt: string; completion: string }
  architecture: {
    modality?: string
    output_modalities?: string[]
  }
}

export const dynamic = "force-dynamic"

// Only include models created after this date (Jan 1, 2024)
const ACTIVE_CUTOFF_UNIX = 1704067200

// Old versioned snapshot suffixes: -0314, -0613, -1106, -0125, -20240229, etc.
const SNAPSHOT_SUFFIX_RE = /-(0[0-9]{3}|[0-9]{8}|[0-9]{4}[01][0-9][0-3][0-9])(\b|$)/

// Routing/pricing tier suffixes that aren't real model variants
const ROUTING_SUFFIX_RE = /:(extended|floor|beta|thinking-exp)$/

export async function GET() {
  const apiKey = process.env.OPENROUTER_API_KEY
  if (!apiKey) {
    return Response.json({ error: "OPENROUTER_API_KEY is not configured" }, { status: 500 })
  }

  try {
    const res = await fetch("https://openrouter.ai/api/v1/models", {
      headers: { Authorization: `Bearer ${apiKey}` },
      next: { revalidate: 300 }, // cache for 5 minutes
    })
    if (!res.ok) {
      return Response.json({ error: `OpenRouter ${res.status}` }, { status: res.status })
    }

    const data: { data: OpenRouterModel[] } = await res.json()

    const textModels = data.data
      .filter((m) => {
        // Support both old `modality` string and new `output_modalities` array
        const outMods = m.architecture?.output_modalities
        const oldModality = m.architecture?.modality ?? ""
        const isTextOutput = outMods
          ? outMods.includes("text")
          : oldModality.includes("->text")
        if (!isTextOutput) return false

        // Drop old date-stamped snapshot versions (e.g. gpt-4-0314, claude-2.1-20231101)
        if (SNAPSHOT_SUFFIX_RE.test(m.id)) return false

        // Drop routing/pricing tier variants (not real model choices)
        if (ROUTING_SUFFIX_RE.test(m.id)) return false

        // Drop models added before 2024 — they're typically deprecated
        if (m.created && m.created < ACTIVE_CUTOFF_UNIX) return false

        return true
      })
      .map((m) => ({
        id: m.id,
        name: m.name,
        created: m.created ?? 0,
        context_length: m.context_length,
        pricing: {
          prompt: parseFloat(m.pricing.prompt) * 1_000_000,       // per 1M tokens
          completion: parseFloat(m.pricing.completion) * 1_000_000,
        },
      }))
      // Sort newest first so the most relevant models appear at the top
      .sort((a, b) => (b.created ?? 0) - (a.created ?? 0))

    return Response.json({ models: textModels })
  } catch (err) {
    const message = err instanceof Error ? err.message : "Failed to fetch models"
    return Response.json({ error: message }, { status: 502 })
  }
}
