"use client"
import { useState, useEffect, useCallback } from "react"
import { Wifi, WifiOff, Key, Server, RefreshCw } from "lucide-react"
import { Input } from "@/components/ui/input"
import { Button } from "@/components/ui/button"
import { ReasonDBClient } from "@/lib/api"

interface Props {
  serverUrl: string
  apiKey: string
  onServerUrlChange: (url: string) => void
  onApiKeyChange: (key: string) => void
}

type Status = "idle" | "checking" | "connected" | "error"

export function ConnectionBar({ serverUrl, apiKey, onServerUrlChange, onApiKeyChange }: Props) {
  const [status, setStatus] = useState<Status>("idle")
  const [version, setVersion] = useState<string>()
  const [showKey, setShowKey] = useState(false)

  const checkHealth = useCallback(async () => {
    if (!serverUrl) return
    setStatus("checking")
    const client = new ReasonDBClient(serverUrl, apiKey || undefined)
    const result = await client.health()
    if (result.ok) {
      setStatus("connected")
      setVersion(result.version)
    } else {
      setStatus("error")
      setVersion(undefined)
    }
  }, [serverUrl, apiKey])

  useEffect(() => {
    const timer = setTimeout(checkHealth, 600)
    return () => clearTimeout(timer)
  }, [checkHealth])

  return (
    <div className="flex items-center gap-3 px-4 py-2.5 border-b bg-background/95 backdrop-blur">
      <div className="flex items-center gap-1.5 text-sm font-semibold text-foreground shrink-0">
        {/* eslint-disable-next-line @next/next/no-img-element */}
        <img src="/logo.svg" alt="ReasonDB" className="w-6 h-6 object-contain" />
        ReasonDB
      </div>

      <div className="h-4 w-px bg-border" />

      <div className="flex items-center gap-2 flex-1 max-w-sm">
        <Server className="h-3.5 w-3.5 text-muted-foreground shrink-0" />
        <Input
          value={serverUrl}
          onChange={(e) => onServerUrlChange(e.target.value)}
          placeholder="http://localhost:4444"
          className="h-7 text-xs font-mono"
        />
      </div>

      <div className="flex items-center gap-2 max-w-xs">
        <Key className="h-3.5 w-3.5 text-muted-foreground shrink-0" />
        <Input
          value={apiKey}
          onChange={(e) => onApiKeyChange(e.target.value)}
          type={showKey ? "text" : "password"}
          placeholder="API key (optional)"
          className="h-7 text-xs"
          onFocus={() => setShowKey(true)}
          onBlur={() => setShowKey(false)}
        />
      </div>

      <Button variant="ghost" size="icon" className="h-7 w-7" onClick={checkHealth} title="Test connection">
        <RefreshCw className={`h-3.5 w-3.5 ${status === "checking" ? "animate-spin" : ""}`} />
      </Button>

      <div className="flex items-center gap-1.5 text-xs shrink-0">
        {status === "connected" ? (
          <>
            <Wifi className="h-3.5 w-3.5 text-emerald-500" />
            <span className="text-emerald-600 font-medium">Connected{version ? ` · v${version}` : ""}</span>
          </>
        ) : status === "error" ? (
          <>
            <WifiOff className="h-3.5 w-3.5 text-destructive" />
            <span className="text-destructive">Disconnected</span>
          </>
        ) : status === "checking" ? (
          <span className="text-muted-foreground">Checking…</span>
        ) : (
          <span className="text-muted-foreground">Not checked</span>
        )}
      </div>
    </div>
  )
}
