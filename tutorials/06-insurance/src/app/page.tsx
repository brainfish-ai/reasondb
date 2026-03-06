"use client"
import { useState, useEffect } from "react"
import { Shield, ChevronRight, Search, Brain, Layers } from "lucide-react"
import { ConnectionBar } from "@/components/ConnectionBar"
import { DataSetupPanel } from "@/components/DataSetupPanel"
import { QueryPlayground, type ExampleQuery } from "@/components/QueryPlayground"
import { ResultsDisplay } from "@/components/ResultsDisplay"
import { Badge } from "@/components/ui/badge"
import { Separator } from "@/components/ui/separator"
import { initializeDataset } from "./actions"
import type { QueryResult } from "@/lib/api"

const EXAMPLES: ExampleQuery[] = [
  // SQL
  {
    label: "All policies",
    badge: "SQL",
    query: "SELECT title, metadata.policy, metadata.year, metadata.type FROM aia_insurance ORDER BY metadata.year ASC",
  },
  {
    label: "Income Care Plus",
    badge: "SQL",
    query: "SELECT * FROM aia_insurance WHERE metadata.policy = 'income-care-plus'",
  },
  // BM25
  {
    label: "SEARCH disability",
    badge: "BM25",
    query: "SELECT * FROM aia_insurance SEARCH 'partial disability benefit waiting period monthly income'",
  },
  {
    label: "SEARCH eligibility",
    badge: "BM25",
    query: "SELECT * FROM aia_insurance SEARCH 'pre-existing condition exclusion eligibility definition'",
  },
  // REASON
  {
    label: "REASON — Partial Disability formula",
    badge: "REASON",
    query: "SELECT * FROM aia_insurance REASON 'How is the Partial Disability Benefit calculated? What is the formula and what do the variables A and B represent?'",
  },
  {
    label: "REASON — Benefit termination",
    badge: "REASON",
    query: "SELECT * FROM aia_insurance REASON 'Under what conditions does the Total Disability Benefit stop being paid? List all termination events.'",
  },
  {
    label: "REASON — Super Continuance (condition 4.3)",
    badge: "REASON",
    query: "SELECT * FROM aia_insurance REASON 'What does condition 4.3 specify about the Super Continuance Monthly Benefit and how is it paid to a superannuation plan?'",
  },
  {
    label: "REASON — Waiting period rules",
    badge: "REASON",
    query: "SELECT * FROM aia_insurance REASON 'How do waiting periods work? When does a waiting period start and what events pause or reset it?'",
  },
  {
    label: "REASON — Policy evolution 2011 vs 2025",
    badge: "REASON",
    query: "SELECT * FROM aia_insurance REASON 'How has the income protection benefit calculation methodology changed between the 2011 Income Care Plus policy and the 2025 Priority Protection plans?'",
  },
  {
    label: "REASON — 2025 enhancements",
    badge: "REASON",
    query: "SELECT * FROM aia_insurance REASON 'What new benefits and enhancements were introduced in the November 2025 Priority Protection policy update, including the Healthier Life Reward and Financial Planning Benefit changes?'",
  },
  // COMBO
  {
    label: "COMBO — Income Care eligibility",
    badge: "COMBO",
    query: "SELECT * FROM aia_insurance WHERE metadata.policy = 'income-care-plus' REASON 'What are all the conditions and events that affect whether and how long the Total Disability Benefit is paid under this policy?'",
  },
  {
    label: "COMBO — Waiting & benefit periods",
    badge: "COMBO",
    query: "SELECT * FROM aia_insurance SEARCH 'waiting period benefit period payment monthly' REASON 'How do waiting periods and benefit periods interact to determine when payments start and how long they continue?'",
  },
]

type StepGroup = "search" | "reason" | "combo"

interface Step {
  num: number
  title: string
  badge: string
  desc: string
  exIdx: number
  group: StepGroup
}

const STEPS: Step[] = [
  // Search
  { num: 1, title: "List All Policies",         badge: "SQL",    desc: "Browse all 4 insurance policy documents ingested from aia.com.au.",                                                      exIdx: 0, group: "search" },
  { num: 2, title: "Filter by Policy",          badge: "SQL",    desc: "Narrow to the Income Care Plus 2011 document — the one with the Partial Disability formula.",                      exIdx: 1, group: "search" },
  { num: 3, title: "SEARCH Disability Terms",   badge: "BM25",   desc: "Full-text search for disability, waiting period, and benefit terms across all policies.",                          exIdx: 2, group: "search" },
  // Reason
  { num: 4, title: "REASON — Partial Disability Formula",    badge: "REASON", desc: "Extract the (A−B)/A formula and variable definitions from Section 3 of Income Care Plus.",          exIdx: 4, group: "reason" },
  { num: 5, title: "REASON — Benefit Termination Tree",      badge: "REASON", desc: "Surface all conditional events that end the Total Disability Benefit — the eligibility tree.",      exIdx: 5, group: "reason" },
  { num: 6, title: "REASON — Condition 4.3 Cross-ref",       badge: "REASON", desc: "Navigate from Section 3.1 to the cross-referenced condition 4.3 for Super Continuance payment.",    exIdx: 6, group: "reason" },
  { num: 7, title: "REASON — Waiting Period Rules",          badge: "REASON", desc: "Understand how waiting periods start, pause, and reset across the policy documents.",               exIdx: 7, group: "reason" },
  { num: 8, title: "REASON — 2011 vs 2025 Comparison",       badge: "REASON", desc: "Compare benefit calculation methodology across the 2011 and 2025 policy generations.",             exIdx: 8, group: "reason" },
  { num: 9, title: "REASON — 2025 Policy Enhancements",      badge: "REASON", desc: "Identify new benefits added in the November 2025 Priority Protection update.",                      exIdx: 9, group: "reason" },
  // Combo
  { num: 10, title: "COMBO — Income Care Eligibility",       badge: "COMBO",  desc: "Filter to Income Care Plus, then reason through the full eligibility and termination logic.",       exIdx: 10, group: "combo" },
  { num: 11, title: "COMBO — Waiting & Benefit Periods",     badge: "COMBO",  desc: "Search payment-related passages, then reason how waiting and benefit periods interact.",            exIdx: 11, group: "combo" },
]

const BADGE_COLORS: Record<string, string> = {
  SQL:    "bg-slate-100 text-slate-700",
  BM25:   "bg-amber-100 text-amber-800",
  REASON: "bg-purple-100 text-purple-800",
  AGG:    "bg-blue-100 text-blue-800",
  COMBO:  "bg-rose-100 text-rose-800",
}

const GROUP_META: Record<StepGroup, { label: string; icon: React.ReactNode; color: string }> = {
  search: { label: "Search",      icon: <Search className="h-3 w-3" />, color: "text-slate-500" },
  reason: { label: "Reason",      icon: <Brain className="h-3 w-3" />,  color: "text-purple-600" },
  combo:  { label: "Combination", icon: <Layers className="h-3 w-3" />, color: "text-rose-600" },
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

  const groups: StepGroup[] = ["search", "reason", "combo"]

  return (
    <div className="flex flex-col h-screen overflow-hidden">
      <ConnectionBar serverUrl={serverUrl} apiKey={apiKey} onServerUrlChange={handleUrlChange} onApiKeyChange={handleKeyChange} />
      <div className="flex flex-1 overflow-hidden">
        <div className="w-80 shrink-0 border-r flex flex-col overflow-hidden">
          <div className="p-4 border-b bg-gradient-to-br from-indigo-50 to-blue-50">
            <div className="flex items-center gap-2 mb-2">
              <div className="p-1.5 rounded-md bg-indigo-600"><Shield className="h-4 w-4 text-white" /></div>
              <div>
                <h1 className="text-sm font-bold">Insurance Policy Analysis</h1>
                <p className="text-[11px] text-muted-foreground">Tutorial 06 · Advanced · 45 min</p>
              </div>
            </div>
            <p className="text-xs text-muted-foreground">
              Analyze income protection and life insurance policies using ReasonDB structural reasoning.
              Demonstrate formula extraction, cross-section references, and conditional eligibility trees.
            </p>
          </div>
          <div className="p-3 border-b">
            <DataSetupPanel
              tableName="aia_insurance"
              docCount={4}
              serverUrl={serverUrl}
              apiKey={apiKey}
              label="Insurance Policy Documents"
              description="4 PDFs: Income Care Plus (2011) · Priority Protection PDS, Incorporated by Reference, and Enhancement Summary (all Nov 2025). Fetched directly from aia.com.au."
              onInitialize={initializeDataset}
              onReady={() => setIsDataReady(true)}
            />
          </div>
          <div className="flex-1 overflow-y-auto p-3 space-y-3">
            {groups.map((group) => {
              const meta = GROUP_META[group]
              const groupSteps = STEPS.filter((s) => s.group === group)
              return (
                <div key={group}>
                  <div className={`flex items-center gap-1.5 px-1 mb-1.5 ${meta.color}`}>
                    {meta.icon}
                    <p className="text-[11px] font-semibold uppercase tracking-wide">{meta.label}</p>
                  </div>
                  <div className="space-y-1.5">
                    {groupSteps.map((step) => (
                      <div
                        key={step.num}
                        className={`rounded-md border p-3 space-y-1.5 cursor-pointer transition-colors ${activeStep === step.num ? "border-indigo-200 bg-indigo-50" : "hover:bg-muted/40"}`}
                        onClick={() => { setActiveStep(step.num); setPlaygroundIdx(step.exIdx); setResult(null); setError(null) }}
                      >
                        <div className="flex items-center gap-2">
                          <span className="w-5 h-5 rounded-full bg-muted flex items-center justify-center text-[10px] font-bold text-muted-foreground shrink-0">{step.num}</span>
                          <span className="text-xs font-medium flex-1">{step.title}</span>
                          <span className={`text-[10px] px-1.5 py-0.5 rounded font-medium ${BADGE_COLORS[step.badge]}`}>{step.badge}</span>
                        </div>
                        <p className="text-[11px] text-muted-foreground pl-7">{step.desc}</p>
                        <div className="pl-7">
                          <button className="flex items-center gap-1 text-[11px] text-indigo-700 hover:text-indigo-900 font-medium">Try it <ChevronRight className="h-3 w-3" /></button>
                        </div>
                      </div>
                    ))}
                  </div>
                </div>
              )
            })}
          </div>
        </div>
        <div className="flex-1 flex flex-col overflow-hidden">
          <div className="p-4 border-b">
            <div className="flex items-center gap-2 mb-1">
              <h2 className="text-sm font-semibold">Query Playground</h2>
              <Badge variant="outline" className="text-xs">aia_insurance</Badge>
              <Badge className="text-xs bg-indigo-100 text-indigo-700 border-indigo-200 hover:bg-indigo-100">Insurance</Badge>
            </div>
            <p className="text-xs text-muted-foreground">
              Query across Income Care Plus (2011) and Priority Protection suite (2025) — formula extraction, eligibility trees, cross-section references.
            </p>
          </div>
          <div className="flex-1 overflow-y-auto p-4 space-y-4">
            <QueryPlayground
              serverUrl={serverUrl}
              apiKey={apiKey}
              examples={EXAMPLES}
              onResult={setResult}
              onError={setError}
              isDataReady={isDataReady}
              selectedIdx={playgroundIdx}
            />
            <Separator />
            <div><h3 className="text-sm font-semibold mb-3">Results</h3><ResultsDisplay result={result} error={error} /></div>
          </div>
        </div>
      </div>
    </div>
  )
}
