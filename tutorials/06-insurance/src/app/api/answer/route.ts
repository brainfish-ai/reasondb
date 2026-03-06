interface ContextNode {
  title: string
  content: string
  confidence: number
  path?: string[]
}

const SYSTEM_PROMPT = `You are an expert insurance analyst delivering a precise, professional briefing on insurance policy documents. Answer directly and authoritatively using only the provided policy document context.

CITATION RULES (mandatory):
- Each source is labelled [Source N] in the context. After every claim or fact you state, add an inline citation using ONLY the format [N] — e.g. "The Partial Disability Benefit is calculated using the formula (A − B) / A × Monthly Benefit [1], where A is the Pre-Disability Income [1]."
- Place the citation immediately after the relevant phrase, before any punctuation.
- Use multiple citations when a claim is supported by more than one source: [1][2].
- Do not include a bibliography or reference list at the end — inline citations only.

OPENING RULES (mandatory):
- Your very first word must be a content word. NEVER start with "Based on", "According to", "The provided", "From the", "These documents", "The sources", or any similar meta-commentary.

INSURANCE DOMAIN RULES:
- When quoting formulas, render them clearly — e.g. "(A − B) / A × Monthly Benefit" with a plain-text explanation of each variable immediately after.
- When listing eligibility conditions or termination events, use a numbered list for clarity.
- When a cross-reference exists between policy sections (e.g. "see condition 4.3"), explicitly state what that referenced section says if the context contains it.
- Be precise about dollar amounts, percentages, and time periods — never round or approximate unless the source does.
- Do not add legal disclaimers or recommend the reader seek advice; this is a factual policy analysis briefing.

STYLE RULES:
- Use the supplied context as your sole factual grounding — do not contradict it or add outside facts.
- Write in a confident, expert voice. Be clear, concise, and well-structured.
- Use bullet points or numbered lists only when they genuinely improve clarity (e.g. listing termination conditions).
- Do not repeat the question.`

function buildPrompt(question: string, context: ContextNode[]): string {
  const contextBlock = context
    .map((node, i) => {
      const path = node.path?.join(" > ") ?? node.title
      return `[Source ${i + 1}] ${path} (confidence: ${(node.confidence * 100).toFixed(0)}%)\n${node.content}`
    })
    .join("\n\n---\n\n")

  return `Context:\n\n${contextBlock}\n\n---\n\nQuestion: ${question}`
}

export async function POST(req: Request) {
  const apiKey = process.env.OPENROUTER_API_KEY
  if (!apiKey) {
    return Response.json({ error: "OPENROUTER_API_KEY is not configured" }, { status: 500 })
  }

  let question: string
  let context: ContextNode[]
  let requestedModel: string | undefined
  try {
    const body = await req.json()
    question = body.question
    context = body.context
    requestedModel = body.model
    if (!question || !Array.isArray(context) || context.length === 0) {
      return Response.json({ error: "question and context are required" }, { status: 400 })
    }
  } catch {
    return Response.json({ error: "Invalid request body" }, { status: 400 })
  }

  const model = requestedModel ?? process.env.OPENROUTER_MODEL ?? "google/gemini-2.0-flash-001"

  try {
    const upstream = await fetch("https://openrouter.ai/api/v1/chat/completions", {
      method: "POST",
      headers: {
        "Authorization": `Bearer ${apiKey}`,
        "Content-Type": "application/json",
        "HTTP-Referer": "https://reasondb.io",
        "X-Title": "ReasonDB Insurance Tutorial",
      },
      body: JSON.stringify({
        model,
        stream: true,
        messages: [
          { role: "system", content: SYSTEM_PROMPT },
          { role: "user", content: buildPrompt(question, context) },
        ],
      }),
    })

    if (!upstream.ok) {
      const err = await upstream.json().catch(() => ({}))
      const msg = (err as { error?: { message?: string } }).error?.message ?? `OpenRouter ${upstream.status}`
      return Response.json({ error: msg }, { status: upstream.status })
    }

    const readable = new ReadableStream({
      async start(controller) {
        const reader = upstream.body!.getReader()
        const decoder = new TextDecoder()
        let buffer = ""
        try {
          while (true) {
            const { done, value } = await reader.read()
            if (done) break
            buffer += decoder.decode(value, { stream: true })
            const lines = buffer.split("\n")
            buffer = lines.pop() ?? ""
            for (const line of lines) {
              if (!line.startsWith("data:")) continue
              const data = line.slice(5).trim()
              if (data === "[DONE]") { controller.close(); return }
              try {
                const json = JSON.parse(data)
                const text: string = json.choices?.[0]?.delta?.content ?? ""
                if (text) controller.enqueue(new TextEncoder().encode(text))
              } catch { /* skip malformed chunks */ }
            }
          }
        } finally {
          controller.close()
        }
      },
    })

    return new Response(readable, {
      headers: { "Content-Type": "text/plain; charset=utf-8" },
    })
  } catch (err) {
    const message = err instanceof Error ? err.message : "OpenRouter request failed"
    return Response.json({ error: message }, { status: 502 })
  }
}
