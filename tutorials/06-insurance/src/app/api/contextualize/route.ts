interface HistoryTurn {
  role: "user" | "assistant"
  content: string
}

const SYSTEM_PROMPT = `You are a query optimizer for an insurance policy search system.

Given a conversation history and a follow-up question, rewrite the question as a complete, standalone, specific question about insurance policy terms, coverage, exclusions, waiting periods, or conditions.

Rules:
- If the question is already complete and specific (e.g. "What is the waiting period for income protection?"), return it unchanged
- If the question is vague or a follow-up (e.g. "what is commencement", "what about mental health?", "and accidents?"), use the conversation context to expand it into a full, specific question
- Always produce a question that makes sense when searched against AIA Australia insurance policy documents
- Keep the rewritten question to one sentence maximum
- Return ONLY the rewritten question — no preamble, no explanation, no quotes`

export async function POST(req: Request) {
  const apiKey = process.env.OPENROUTER_API_KEY
  if (!apiKey) {
    return Response.json({ error: "OPENROUTER_API_KEY is not configured" }, { status: 500 })
  }

  let history: HistoryTurn[]
  let question: string
  try {
    const body = await req.json()
    history = body.history ?? []
    question = body.question
    if (!question) return Response.json({ error: "question is required" }, { status: 400 })
  } catch {
    return Response.json({ error: "Invalid request body" }, { status: 400 })
  }

  // If no prior conversation, return the question as-is (avoid unnecessary LLM call)
  if (history.length === 0) {
    return Response.json({ contextualQuestion: question })
  }

  const model = process.env.OPENROUTER_MODEL ?? "google/gemini-2.0-flash-001"

  // Build a compact history block (last 3 turns, trim assistant answers to 200 chars)
  const historyBlock = history
    .slice(-6) // last 3 pairs max
    .map((t) => {
      const content = t.role === "assistant" && t.content.length > 200
        ? t.content.slice(0, 200) + "…"
        : t.content
      return `${t.role === "user" ? "User" : "Assistant"}: ${content}`
    })
    .join("\n")

  const userPrompt = `Conversation so far:\n${historyBlock}\n\nNew question: ${question}\n\nRewritten standalone question:`

  try {
    const upstream = await fetch("https://openrouter.ai/api/v1/chat/completions", {
      method: "POST",
      headers: {
        "Authorization": `Bearer ${apiKey}`,
        "Content-Type": "application/json",
        "HTTP-Referer": "https://reasondb.io",
        "X-Title": "ReasonDB Insurance Demo",
      },
      body: JSON.stringify({
        model,
        stream: false,
        max_tokens: 120,
        messages: [
          { role: "system", content: SYSTEM_PROMPT },
          { role: "user", content: userPrompt },
        ],
      }),
    })

    if (!upstream.ok) {
      // Fall back to original question on error
      return Response.json({ contextualQuestion: question })
    }

    const data = await upstream.json()
    const contextualQuestion = (data.choices?.[0]?.message?.content ?? question).trim()
    return Response.json({ contextualQuestion })
  } catch {
    // Fall back gracefully — the original question is always valid
    return Response.json({ contextualQuestion: question })
  }
}
