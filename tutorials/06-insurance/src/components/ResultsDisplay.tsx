"use client"
import { Clock, Rows, Copy, Check, Sparkles } from "lucide-react"
import { useState, useMemo, useEffect } from "react"
import dynamic from "next/dynamic"
import type { Monaco } from "@monaco-editor/react"
import { Tabs, TabsList, TabsTrigger, TabsContent } from "@/components/ui/tabs"
import { ScrollArea } from "@/components/ui/scroll-area"
import { Button } from "@/components/ui/button"
import type { QueryResult } from "@/lib/api"
import { ensureTheme, THEME_NAME } from "@reasondb/rql-editor"
import { AnswerPanel } from "@/components/AnswerPanel"

const MonacoEditor = dynamic(() => import("@monaco-editor/react"), { ssr: false })

interface Props {
  result: QueryResult | null
  error: string | null
}

export function ResultsDisplay({ result, error }: Props) {
  const [copied, setCopied] = useState(false)
  const isReason = !!(result?.matchedNodes && result.matchedNodes.length > 0)

  // Auto-switch to Answer tab when a REASON result arrives
  const [activeTab, setActiveTab] = useState("table")
  useEffect(() => {
    setActiveTab(isReason ? "answer" : "table")
  }, [isReason, result])

  const jsonString = useMemo(
    () => (result ? JSON.stringify(result.rows, null, 2) : "[]"),
    [result]
  )

  // Auto-size the editor height: ~20px per line, capped between 160px and 480px
  const editorHeight = useMemo(() => {
    const lines = jsonString.split("\n").length
    return Math.min(Math.max(lines * 20, 160), 480)
  }, [jsonString])

  const copy = () => {
    if (!result) return
    navigator.clipboard.writeText(jsonString)
    setCopied(true)
    setTimeout(() => setCopied(false), 1500)
  }

  if (error) {
    return (
      <div className="rounded-lg border border-destructive/40 bg-destructive/5 p-4">
        <p className="text-sm font-medium text-destructive mb-1">Query Error</p>
        <pre className="text-xs text-destructive/80 whitespace-pre-wrap font-mono">{error}</pre>
      </div>
    )
  }

  if (!result) {
    return (
      <div className="rounded-lg border border-dashed bg-muted/30 p-8 text-center">
        <p className="text-sm text-muted-foreground">Run a query to see results here</p>
      </div>
    )
  }

  return (
    <div className="space-y-2">
      <div className="flex items-center gap-4 text-xs text-muted-foreground">
        <span className="flex items-center gap-1">
          <Rows className="h-3.5 w-3.5" />
          {result.rowCount} row{result.rowCount !== 1 ? "s" : ""}
        </span>
        <span className="flex items-center gap-1">
          <Clock className="h-3.5 w-3.5" />
          {result.executionTimeMs}ms
        </span>
        <Button variant="ghost" size="sm" className="h-6 px-2 text-xs ml-auto gap-1" onClick={copy}>
          {copied ? <Check className="h-3 w-3" /> : <Copy className="h-3 w-3" />}
          {copied ? "Copied" : "Copy JSON"}
        </Button>
      </div>

      <Tabs value={activeTab} onValueChange={setActiveTab}>
        <TabsList className="h-8">
          <TabsTrigger value="table" className="text-xs h-7">Table</TabsTrigger>
          <TabsTrigger value="json" className="text-xs h-7">JSON</TabsTrigger>
          {isReason && (
            <TabsTrigger value="answer" className="text-xs h-7 gap-1">
              <Sparkles className="h-3 w-3 text-purple-500" />
              Answer
            </TabsTrigger>
          )}
        </TabsList>

        <TabsContent value="table">
          {result.rows.length === 0 ? (
            <div className="rounded-md border p-6 text-center text-sm text-muted-foreground">
              No rows returned
            </div>
          ) : (
            <ScrollArea className="h-80 rounded-md border">
              <table className="w-full text-xs">
                <thead className="sticky top-0 bg-muted/80 backdrop-blur">
                  <tr>
                    {result.columns.map((col, ci) => (
                      <th
                        key={`${col}-${ci}`}
                        className="text-left px-3 py-2 font-medium text-muted-foreground whitespace-nowrap border-b"
                      >
                        {col}
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {result.rows.map((row, i) => (
                    <tr key={i} className="border-b hover:bg-muted/30 transition-colors">
                      {result.columns.map((col, ci) => {
                        const val = row[col]
                        const display =
                          val === null || val === undefined ? (
                            <span className="text-muted-foreground/50">null</span>
                          ) : typeof val === "object" ? (
                            <span className="text-blue-600">{JSON.stringify(val).slice(0, 80)}</span>
                          ) : String(val).length > 120 ? (
                            String(val).slice(0, 120) + "…"
                          ) : (
                            String(val)
                          )
                        return (
                          <td key={`${col}-${ci}`} className="px-3 py-2 max-w-xs truncate align-top">
                            {display}
                          </td>
                        )
                      })}
                    </tr>
                  ))}
                </tbody>
              </table>
            </ScrollArea>
          )}
        </TabsContent>

        <TabsContent value="json">
          <div className="rounded-md overflow-hidden border border-slate-700">
            <MonacoEditor
              height={editorHeight}
              language="json"
              theme={THEME_NAME}
              onMount={(_, monaco: Monaco) => ensureTheme(monaco)}
              value={jsonString}
              options={{
                readOnly: true,
                minimap: { enabled: false },
                fontSize: 12,
                lineNumbers: "on",
                wordWrap: "off",
                scrollBeyondLastLine: false,
                folding: true,
                foldingHighlight: true,
                padding: { top: 8, bottom: 8 },
                overviewRulerLanes: 0,
                hideCursorInOverviewRuler: true,
                scrollbar: {
                  vertical: "auto",
                  horizontal: "auto",
                  verticalScrollbarSize: 6,
                  horizontalScrollbarSize: 6,
                },
                renderLineHighlight: "none",
                contextmenu: false,
                glyphMargin: false,
                lineDecorationsWidth: 4,
                lineNumbersMinChars: 3,
              }}
            />
          </div>
        </TabsContent>

        {isReason && (
          <TabsContent value="answer">
            <AnswerPanel result={result} />
          </TabsContent>
        )}
      </Tabs>
    </div>
  )
}
