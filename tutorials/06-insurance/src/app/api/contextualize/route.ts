interface HistoryTurn {
  role: "user" | "assistant"
  content: string
}

/** Shape returned to the client */
export type ContextualizeResult =
  | { intent: "query"; contextualQuestion: string }
  | { intent: "direct_answer"; answer: string }

const BASE_SYSTEM_PROMPT = `You are an assistant for AIA Australia insurance policy analysis.

You have access to a tool called search_insurance_policies that searches AIA Australia documents.

Decision rules:
- Call search_insurance_policies when the user asks ANY factual question about policy terms, coverage, exclusions, waiting periods, premiums, benefits, claims, disability definitions, or commencement conditions.
- For follow-up questions (e.g. "what about mental health?", "and accidents?", "what is commencement"), use the conversation context to expand them into a full, specific standalone question before searching.
- DO NOT call the tool for: greetings ("hi", "hello", "thanks"), meta questions about the conversation ("what was my previous question?", "what did you just say?"), or questions you can answer directly from the conversation history.
- For non-search responses keep the answer concise (1–3 sentences).`

function buildSystemPrompt(policyName?: string): string {
  if (!policyName || policyName === "All Policies") {
    return BASE_SYSTEM_PROMPT + `\n\nThe search covers all 4 AIA Australia documents: Income Care Plus (2011), Priority Protection PDS, Priority Protection IBR, and Priority Protection Enhancement Summary (Nov 2025).`
  }
  return BASE_SYSTEM_PROMPT + `\n\nThe user has selected the policy: "${policyName}". Scope your rewritten query and any direct answers specifically to that document. Do not reference other policies unless directly asked.`
}

const SEARCH_TOOL = {
  type: "function" as const,
  function: {
    name: "search_insurance_policies",
    description:
      "Search AIA Australia insurance policy documents to answer questions about coverage, exclusions, waiting periods, premiums, benefits, and policy terms. Always rewrite vague follow-ups into a complete, standalone question before calling this tool.",
    parameters: {
      type: "object",
      properties: {
        query: {
          type: "string",
          description:
            "A specific, self-contained insurance policy question. Expand any follow-ups using conversation context so the question makes sense in isolation.",
        },
      },
      required: ["query"],
    },
  },
}

export async function POST(req: Request) {
  const apiKey = process.env.OPENROUTER_API_KEY
  if (!apiKey) {
    return Response.json({ error: "OPENROUTER_API_KEY is not configured" }, { status: 500 })
  }

  let history: HistoryTurn[]
  let question: string
  let policyName: string | undefined
  try {
    const body = await req.json()
    history = body.history ?? []
    question = body.question
    policyName = body.policyName
    if (!question) return Response.json({ error: "question is required" }, { status: 400 })
  } catch {
    return Response.json({ error: "Invalid request body" }, { status: 400 })
  }

  const model = process.env.OPENROUTER_MODEL ?? "google/gemini-2.0-flash-001"

  // Build messages: system + trimmed history + new user turn
  const historyMessages = history.slice(-6).map((t) => ({
    role: t.role,
    content:
      t.role === "assistant" && t.content.length > 300
        ? t.content.slice(0, 300) + "…"
        : t.content,
  }))

  const messages = [
    { role: "system" as const, content: buildSystemPrompt(policyName) },
    ...historyMessages,
    { role: "user" as const, content: question },
  ]

  try {
    const upstream = await fetch("https://openrouter.ai/api/v1/chat/completions", {
      method: "POST",
      headers: {
        Authorization: `Bearer ${apiKey}`,
        "Content-Type": "application/json",
        "HTTP-Referer": "https://reasondb.io",
        "X-Title": "ReasonDB Insurance Demo",
      },
      body: JSON.stringify({
        model,
        stream: false,
        max_tokens: 200,
        tools: [SEARCH_TOOL],
        // Let the model freely choose: call the tool or answer directly
        tool_choice: "auto",
        messages,
      }),
    })

    if (!upstream.ok) {
      // Fallback: treat as a query so nothing breaks
      return Response.json({ intent: "query", contextualQuestion: question })
    }

    const data = await upstream.json()
    const choice = data.choices?.[0]

    // Model decided to call the search tool
    if (choice?.finish_reason === "tool_calls") {
      const toolCall = choice.message?.tool_calls?.[0]
      if (toolCall?.function?.name === "search_insurance_policies") {
        let args: { query?: string } = {}
        try {
          args = JSON.parse(toolCall.function.arguments ?? "{}")
        } catch { /* ignore */ }
        const contextualQuestion = (args.query ?? question).trim()
        return Response.json({ intent: "query", contextualQuestion })
      }
    }

    // Model answered directly (no tool call)
    const answer = (choice?.message?.content ?? "").trim()
    if (answer) {
      return Response.json({ intent: "direct_answer", answer })
    }

    // Fallback: treat as a query
    return Response.json({ intent: "query", contextualQuestion: question })
  } catch {
    return Response.json({ intent: "query", contextualQuestion: question })
  }
}
