"use client"
import { useState, useEffect } from "react"
import { Scale, ChevronRight } from "lucide-react"
import { ConnectionBar } from "@/components/ConnectionBar"
import { DataSetupPanel } from "@/components/DataSetupPanel"
import { QueryPlayground, type ExampleQuery } from "@/components/QueryPlayground"
import { ResultsDisplay } from "@/components/ResultsDisplay"
import { Badge } from "@/components/ui/badge"
import { Separator } from "@/components/ui/separator"
import { initializeDataset } from "./actions"
import type { QueryResult } from "@/lib/api"

const EXAMPLES: ExampleQuery[] = [
  { label: "SELECT all", badge: "SQL", query: "SELECT * FROM regulations LIMIT 5" },
  { label: "By topic", badge: "SQL", query: "SELECT title, metadata.topic, metadata.publication_date FROM regulations WHERE metadata.topic = 'ai_policy'" },
  { label: "SEARCH AI bias", badge: "BM25", query: "SELECT * FROM regulations SEARCH 'artificial intelligence bias discrimination fairness'" },
  { label: "SEARCH copyright", badge: "BM25", query: "SELECT * FROM regulations SEARCH 'generative AI copyright intellectual property'" },
  { label: "REASON liability", badge: "LLM", query: "SELECT * FROM regulations REASON 'What are the key AI safety and liability requirements mentioned across these regulations?'" },
  { label: "REASON compliance", badge: "LLM", query: "SELECT * FROM regulations REASON 'What steps must organizations take to comply with AI regulations?'" },
  { label: "COUNT by topic", badge: "AGG", query: "SELECT COUNT(*) FROM regulations GROUP BY metadata.topic" },
  { label: "REASON bias & fairness", badge: "LLM", query: "SELECT * FROM regulations REASON 'How do these regulations address algorithmic bias and require fairness in AI decision-making systems?'" },
  { label: "REASON high-risk AI", badge: "LLM", query: "SELECT * FROM regulations REASON 'How do these regulations define high-risk AI applications and what special obligations apply to them?'" },
  { label: "REASON individual rights", badge: "LLM", query: "SELECT * FROM regulations REASON 'What rights do individuals have when subject to automated AI decision-making under these regulations?'" },
]

const STEPS = [
  { num: 1, title: "Browse Documents", badge: "SQL", desc: "List all AI/ML regulatory documents from the Federal Register.", exIdx: 0 },
  { num: 2, title: "Filter by Topic", badge: "SQL", desc: "Filter regulations by topic — ai_policy, ai_ethics, ai_safety, ai_copyright.", exIdx: 1 },
  { num: 3, title: "SEARCH Keywords", badge: "BM25", desc: "Full-text BM25 search across regulation text for specific legal terms.", exIdx: 2 },
  { num: 4, title: "REASON — Legal Q&A", badge: "LLM", desc: "Ask a legal question. ReasonDB synthesizes answers across all documents.", exIdx: 4 },
  { num: 5, title: "Compliance Check", badge: "LLM", desc: "Ask about compliance requirements across all regulations at once.", exIdx: 5 },
  { num: 6, title: "Aggregate by Topic", badge: "AGG", desc: "Count documents grouped by regulatory topic.", exIdx: 6 },
  { num: 7, title: "REASON — Bias & Fairness", badge: "LLM", desc: "Explore how regulations address algorithmic bias and mandate fairness in AI systems.", exIdx: 7 },
  { num: 8, title: "REASON — High-Risk AI", badge: "LLM", desc: "Understand how high-risk AI applications are defined and what special obligations apply.", exIdx: 8 },
  { num: 9, title: "REASON — Individual Rights", badge: "LLM", desc: "Discover what rights individuals hold when subject to automated AI decision-making.", exIdx: 9 },
]

const BADGE_COLORS: Record<string, string> = {
  SQL: "bg-slate-100 text-slate-700", BM25: "bg-amber-100 text-amber-800",
  LLM: "bg-purple-100 text-purple-800", AGG: "bg-blue-100 text-blue-800",
}

export default function Page() {
  const [serverUrl, setServerUrl] = useState("http://localhost:4444")
  const [apiKey, setApiKey] = useState("")
  const [isDataReady, setIsDataReady] = useState(false)
  const [result, setResult] = useState<QueryResult | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [activeStep, setActiveStep] = useState<number | null>(null)
  const [playgroundIdx, setPlaygroundIdx] = useState(0)

  useEffect(() => {
    const url = localStorage.getItem("reasondb_server_url")
    const key = localStorage.getItem("reasondb_api_key")
    if (url) setServerUrl(url)
    if (key) setApiKey(key)
  }, [])

  const handleUrlChange = (url: string) => { setServerUrl(url); localStorage.setItem("reasondb_server_url", url) }
  const handleKeyChange = (key: string) => { setApiKey(key); localStorage.setItem("reasondb_api_key", key) }

  return (
    <div className="flex flex-col h-screen overflow-hidden">
      <ConnectionBar serverUrl={serverUrl} apiKey={apiKey} onServerUrlChange={handleUrlChange} onApiKeyChange={handleKeyChange} />
      <div className="flex flex-1 overflow-hidden">
        <div className="w-80 shrink-0 border-r flex flex-col overflow-hidden">
          <div className="p-4 border-b bg-gradient-to-br from-amber-50 to-orange-50">
            <div className="flex items-center gap-2 mb-2">
              <div className="p-1.5 rounded-md bg-amber-600"><Scale className="h-4 w-4 text-white" /></div>
              <div>
                <h1 className="text-sm font-bold">Legal Document Search</h1>
                <p className="text-[11px] text-muted-foreground">Tutorial 02 · Intermediate · 45 min</p>
              </div>
            </div>
            <p className="text-xs text-muted-foreground">Search AI/ML regulatory documents from the US Federal Register using BM25 and LLM reasoning.</p>
          </div>
          <div className="p-3 border-b">
            <DataSetupPanel
              tableName="regulations" docCount={5} serverUrl={serverUrl} apiKey={apiKey}
              label="Federal Register Dataset"
              description="5 AI/ML regulatory documents — Executive Orders, FTC guidance, copyright rules, and autonomous vehicle regulations."
              onInitialize={initializeDataset} onReady={() => setIsDataReady(true)}
            />
          </div>
          <div className="flex-1 overflow-y-auto p-3 space-y-1.5">
            <p className="text-[11px] font-semibold text-muted-foreground uppercase tracking-wide px-1 mb-2">Query Steps</p>
            {STEPS.map((step) => (
              <div key={step.num} className={`rounded-md border p-3 space-y-1.5 cursor-pointer transition-colors ${activeStep === step.num ? "border-amber-200 bg-amber-50" : "hover:bg-muted/40"}`}
                onClick={() => { setActiveStep(step.num); setPlaygroundIdx(step.exIdx); setResult(null); setError(null) }}>
                <div className="flex items-center gap-2">
                  <span className="w-5 h-5 rounded-full bg-muted flex items-center justify-center text-[10px] font-bold text-muted-foreground shrink-0">{step.num}</span>
                  <span className="text-xs font-medium flex-1">{step.title}</span>
                  <span className={`text-[10px] px-1.5 py-0.5 rounded font-medium ${BADGE_COLORS[step.badge]}`}>{step.badge}</span>
                </div>
                <p className="text-[11px] text-muted-foreground pl-7">{step.desc}</p>
                <div className="pl-7"><button className="flex items-center gap-1 text-[11px] text-amber-700 hover:text-amber-900 font-medium">Try it <ChevronRight className="h-3 w-3" /></button></div>
              </div>
            ))}
          </div>
        </div>
        <div className="flex-1 flex flex-col overflow-hidden">
          <div className="p-4 border-b">
            <div className="flex items-center gap-2 mb-1">
              <h2 className="text-sm font-semibold">Query Playground</h2>
              <Badge variant="outline" className="text-xs">regulations</Badge>
            </div>
            <p className="text-xs text-muted-foreground">Search Federal Register AI regulations with RQL.</p>
          </div>
          <div className="flex-1 overflow-y-auto p-4 space-y-4">
            <QueryPlayground serverUrl={serverUrl} apiKey={apiKey} examples={EXAMPLES} onResult={setResult} onError={setError} isDataReady={isDataReady} selectedIdx={playgroundIdx} />
            <Separator />
            <div><h3 className="text-sm font-semibold mb-3">Results</h3><ResultsDisplay result={result} error={error} /></div>
          </div>
        </div>
      </div>
    </div>
  )
}
