import { useState, useEffect, useCallback } from 'react'
import { Gear, FloppyDisk, ArrowCounterClockwise, CircleNotch, CheckCircle, WarningCircle, Plugs } from '@phosphor-icons/react'
import { cn } from '@/lib/utils'
import { useConnectionStore } from '@/stores/connectionStore'
import { getClient, createClient, setClient, type LlmModelConfig, type LlmSettings as LlmSettingsType, type LlmTestStatus } from '@/lib/api'
import { useLlmHealthStore } from '@/stores/llmHealthStore'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/Select'

const PROVIDERS = [
  { value: 'openai', label: 'OpenAI' },
  { value: 'anthropic', label: 'Anthropic' },
  { value: 'gemini', label: 'Google Gemini' },
  { value: 'cohere', label: 'Cohere' },
  { value: 'glm', label: 'GLM (Zhipu AI)' },
  { value: 'kimi', label: 'Kimi (Moonshot)' },
  { value: 'ollama', label: 'Ollama (Local)' },
]

function StatusBadge({ status, testing }: { status?: LlmTestStatus; testing: boolean }) {
  if (testing) {
    return (
      <span className="inline-flex items-center gap-1 text-xs text-overlay-0">
        <CircleNotch size={12} className="animate-spin" />
        Testing…
      </span>
    )
  }
  if (!status) return null
  if (status.ok) {
    return (
      <span className="inline-flex items-center gap-1 text-xs text-green">
        <CheckCircle size={14} weight="fill" />
        Connected{status.latency_ms != null && ` (${status.latency_ms}ms)`}
      </span>
    )
  }
  return (
    <span className="inline-flex items-center gap-1 text-xs text-peach cursor-default" title={status.error ?? 'Connection test failed'}>
      <WarningCircle size={14} weight="fill" />
      Unhealthy
    </span>
  )
}

function ModelConfigForm({
  label,
  config,
  onChange,
  status,
  testing,
}: {
  label: string
  config: LlmModelConfig
  onChange: (config: LlmModelConfig) => void
  status?: LlmTestStatus
  testing: boolean
}) {
  const isOllama = config.provider === 'ollama'

  const update = (patch: Partial<LlmModelConfig>) => {
    onChange({ ...config, ...patch })
  }

  const updateOptions = (patch: Record<string, unknown>) => {
    onChange({
      ...config,
      options: { ...config.options, ...patch },
    })
  }

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-semibold text-text uppercase tracking-wide">{label}</h3>
        <StatusBadge status={status} testing={testing} />
      </div>

      <div className="space-y-3">
        <div>
          <label className="block text-xs font-medium text-subtext-0 mb-1">Provider</label>
          <Select value={config.provider} onValueChange={(v) => update({ provider: v })}>
            <SelectTrigger className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {PROVIDERS.map((p) => (
                <SelectItem key={p.value} value={p.value}>
                  {p.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>

        {!isOllama && (
          <div>
            <label className="block text-xs font-medium text-subtext-0 mb-1">API Key</label>
            <input
              type="password"
              value={config.api_key || ''}
              onChange={(e) => update({ api_key: e.target.value || undefined })}
              placeholder="sk-..."
              className={cn(
                'w-full h-9 rounded-md border border-border bg-surface-0 px-3 py-2 text-sm',
                'placeholder:text-overlay-0 focus:outline-none focus:ring-2 focus:ring-primary focus:ring-offset-2'
              )}
            />
          </div>
        )}

        <div>
          <label className="block text-xs font-medium text-subtext-0 mb-1">Model</label>
          <input
            type="text"
            value={config.model || ''}
            onChange={(e) => update({ model: e.target.value || undefined })}
            placeholder="e.g. gpt-4o, claude-sonnet-4-5-20250929"
            className={cn(
              'w-full h-9 rounded-md border border-border bg-surface-0 px-3 py-2 text-sm',
              'placeholder:text-overlay-0 focus:outline-none focus:ring-2 focus:ring-primary focus:ring-offset-2'
            )}
          />
        </div>

        {isOllama && (
          <div>
            <label className="block text-xs font-medium text-subtext-0 mb-1">Base URL</label>
            <input
              type="text"
              value={config.base_url || ''}
              onChange={(e) => update({ base_url: e.target.value || undefined })}
              placeholder="http://localhost:11434/v1"
              className={cn(
                'w-full h-9 rounded-md border border-border bg-surface-0 px-3 py-2 text-sm',
                'placeholder:text-overlay-0 focus:outline-none focus:ring-2 focus:ring-primary focus:ring-offset-2'
              )}
            />
          </div>
        )}

        <div className="grid grid-cols-2 gap-3">
          <div>
            <label className="block text-xs font-medium text-subtext-0 mb-1">Temperature</label>
            <input
              type="number"
              min="0"
              max="2"
              step="0.1"
              value={config.options?.temperature ?? ''}
              onChange={(e) =>
                updateOptions({
                  temperature: e.target.value ? parseFloat(e.target.value) : undefined,
                })
              }
              placeholder="default"
              className={cn(
                'w-full h-9 rounded-md border border-border bg-surface-0 px-3 py-2 text-sm',
                'placeholder:text-overlay-0 focus:outline-none focus:ring-2 focus:ring-primary focus:ring-offset-2'
              )}
            />
          </div>
          <div>
            <label className="block text-xs font-medium text-subtext-0 mb-1">Max Tokens</label>
            <input
              type="number"
              min="1"
              step="1"
              value={config.options?.max_tokens ?? ''}
              onChange={(e) =>
                updateOptions({
                  max_tokens: e.target.value ? parseInt(e.target.value, 10) : undefined,
                })
              }
              placeholder="default"
              className={cn(
                'w-full h-9 rounded-md border border-border bg-surface-0 px-3 py-2 text-sm',
                'placeholder:text-overlay-0 focus:outline-none focus:ring-2 focus:ring-primary focus:ring-offset-2'
              )}
            />
          </div>
        </div>

        <div className="flex items-center gap-2">
          <input
            type="checkbox"
            id={`${label}-disable-thinking`}
            checked={config.options?.disable_thinking ?? false}
            onChange={(e) => updateOptions({ disable_thinking: e.target.checked })}
            className="rounded border-border"
          />
          <label htmlFor={`${label}-disable-thinking`} className="text-xs text-subtext-0">
            Disable extended thinking
          </label>
        </div>
      </div>
    </div>
  )
}

export function AgentSettings() {
  const { activeConnectionId, connections } = useConnectionStore()
  const [settings, setSettings] = useState<LlmSettingsType | null>(null)
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [success, setSuccess] = useState<string | null>(null)

  const { testResult, testing, setTestResult, setTesting } = useLlmHealthStore()

  const getOrCreateClient = useCallback(() => {
    if (!activeConnectionId) return null
    let client = getClient(activeConnectionId)
    if (!client) {
      const conn = connections.find((c) => c.id === activeConnectionId)
      if (!conn) return null
      client = createClient({ host: conn.host, port: conn.port, apiKey: conn.apiKey, useSsl: conn.ssl })
      setClient(activeConnectionId, client)
    }
    return client
  }, [activeConnectionId, connections])

  const runTest = useCallback(async () => {
    const client = getOrCreateClient()
    if (!client) return
    setTesting(true)
    try {
      const result = await client.testLlmConfig()
      setTestResult(result)
    } catch {
      setTestResult({
        ingestion: { ok: false, error: 'Test request failed' },
        retrieval: { ok: false, error: 'Test request failed' },
      })
    } finally {
      setTesting(false)
    }
  }, [getOrCreateClient, setTestResult, setTesting])

  const loadSettings = useCallback(async () => {
    if (!activeConnectionId) {
      setLoading(false)
      return
    }
    const client = getOrCreateClient()
    if (!client) {
      setLoading(false)
      setError('Not connected to server')
      return
    }

    setLoading(true)
    setError(null)
    try {
      const s = await client.getLlmConfig()
      setSettings(s)
    } catch (err) {
      const msg = err instanceof Error ? err.message : 'Failed to load settings'
      if (msg.toLowerCase().includes('not found') || msg.includes('404')) {
        setSettings({
          ingestion: { provider: 'openai', options: {} },
          retrieval: { provider: 'openai', options: {} },
        })
      } else {
        setError(msg)
      }
    } finally {
      setLoading(false)
    }
  }, [activeConnectionId, getOrCreateClient])

  useEffect(() => {
    loadSettings().then(() => {
      runTest()
    })
  }, [loadSettings, runTest])

  const handleSave = async () => {
    if (!activeConnectionId || !settings) return
    const client = getOrCreateClient()
    if (!client) return

    setSaving(true)
    setError(null)
    setSuccess(null)
    try {
      const result = await client.updateLlmConfig(settings)
      setSettings(result)
      setSuccess('Agent settings saved successfully')
      setTimeout(() => setSuccess(null), 3000)
      runTest()
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to save settings')
    } finally {
      setSaving(false)
    }
  }

  if (!activeConnectionId) {
    return (
      <div className="flex items-center justify-center h-full text-overlay-0 text-sm">
        Connect to a server to manage agent settings
      </div>
    )
  }

  if (loading) {
    return (
      <div className="flex items-center justify-center h-full">
        <CircleNotch size={24} className="animate-spin text-overlay-0" />
      </div>
    )
  }

  if (!settings) {
    return (
      <div className="flex items-center justify-center h-full text-overlay-0 text-sm">
        {error || 'Unable to load settings'}
      </div>
    )
  }

  return (
    <div className="h-full overflow-auto">
      <div className="max-w-3xl mx-auto p-6 space-y-6">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <Gear size={20} weight="duotone" className="text-mauve" />
            <h2 className="text-lg font-semibold text-text">Agent Settings</h2>
          </div>
          <div className="flex items-center gap-2">
            <button
              onClick={runTest}
              disabled={testing}
              className={cn(
                'flex items-center gap-1.5 px-3 py-1.5 text-sm rounded-md',
                'border border-border text-subtext-0',
                'hover:bg-surface-0 hover:text-text transition-colors',
                'disabled:opacity-50'
              )}
            >
              {testing ? (
                <CircleNotch size={14} className="animate-spin" />
              ) : (
                <Plugs size={14} />
              )}
              Test Connection
            </button>
            <button
              onClick={loadSettings}
              disabled={loading}
              className={cn(
                'flex items-center gap-1.5 px-3 py-1.5 text-sm rounded-md',
                'border border-border text-subtext-0',
                'hover:bg-surface-0 hover:text-text transition-colors',
                'disabled:opacity-50'
              )}
            >
              <ArrowCounterClockwise size={14} />
              Reload
            </button>
            <button
              onClick={handleSave}
              disabled={saving}
              className={cn(
                'flex items-center gap-1.5 px-3 py-1.5 text-sm rounded-md',
                'bg-mauve text-base font-medium',
                'hover:bg-mauve/90 transition-colors',
                'disabled:opacity-50'
              )}
            >
              {saving ? (
                <CircleNotch size={14} className="animate-spin" />
              ) : (
                <FloppyDisk size={14} />
              )}
              Save
            </button>
          </div>
        </div>

        {error && (
          <div className="p-3 rounded-md bg-red/10 border border-red/20 text-red text-sm">
            {error}
          </div>
        )}

        {success && (
          <div className="p-3 rounded-md bg-green/10 border border-green/20 text-green text-sm">
            {success}
          </div>
        )}

        <p className="text-sm text-subtext-0">
          Configure separate models for ingestion (summarization) and retrieval (search reasoning).
          Changes take effect immediately without a server restart.
        </p>

        <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
          <div className="p-4 rounded-lg border border-border bg-surface-0/30">
            <ModelConfigForm
              label="Ingestion"
              config={settings.ingestion}
              onChange={(ingestion) => setSettings({ ...settings, ingestion })}
              status={testResult?.ingestion}
              testing={testing}
            />
          </div>

          <div className="p-4 rounded-lg border border-border bg-surface-0/30">
            <ModelConfigForm
              label="Retrieval"
              config={settings.retrieval}
              onChange={(retrieval) => setSettings({ ...settings, retrieval })}
              status={testResult?.retrieval}
              testing={testing}
            />
          </div>
        </div>
      </div>
    </div>
  )
}
