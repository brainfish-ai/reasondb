import { useState, useEffect, useCallback } from 'react'
import { Gear, FloppyDisk, ArrowCounterClockwise, CircleNotch, CheckCircle, WarningCircle, Plugs } from '@phosphor-icons/react'
import { cn } from '@/lib/utils'
import { useConnectionStore } from '@/stores/connectionStore'
import { getClient, createClient, setClient, type LlmModelConfig, type LlmSettings as LlmSettingsType, type LlmTestStatus, type ChunkStrategy } from '@/lib/api'
import { useLlmHealthStore } from '@/stores/llmHealthStore'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/Select'

const CHUNK_STRATEGIES: { value: ChunkStrategy; label: string; description: string }[] = [
  {
    value: 'agentic',
    label: 'Agentic (LLM-Guided)',
    description: 'Uses the Ingestion LLM to decide which lines belong together. Produces the most semantically coherent chunks. Falls back to Markdown-Aware if no LLM is configured.',
  },
  {
    value: 'markdown_aware',
    label: 'Markdown-Aware',
    description: 'Splits at CommonMark structural boundaries (headings, paragraphs, code blocks). Fully deterministic — no LLM required.',
  },
]

export const CHUNK_STRATEGY_KEY = 'reasondb:chunk_strategy'

const PROVIDERS = [
  { value: 'openai', label: 'OpenAI' },
  { value: 'anthropic', label: 'Anthropic' },
  { value: 'gemini', label: 'Google Gemini' },
  { value: 'cohere', label: 'Cohere' },
  { value: 'glm', label: 'GLM (Zhipu AI)' },
  { value: 'kimi', label: 'Kimi (Moonshot)' },
  { value: 'ollama', label: 'Ollama (Local)' },
  { value: 'vertex', label: 'Google Vertex AI' },
  { value: 'bedrock', label: 'AWS Bedrock' },
]

const PROVIDER_MODELS: Record<string, { value: string; label: string }[]> = {
  openai: [
    // GPT-5 family (current frontier)
    { value: 'gpt-5.4', label: 'GPT-5.4 (Latest)' },
    { value: 'gpt-5.4-pro', label: 'GPT-5.4 Pro' },
    { value: 'gpt-5', label: 'GPT-5' },
    { value: 'gpt-5-mini', label: 'GPT-5 Mini' },
    { value: 'gpt-5-nano', label: 'GPT-5 Nano' },
    // GPT-4.1 family
    { value: 'gpt-4.1', label: 'GPT-4.1' },
    { value: 'gpt-4.1-mini', label: 'GPT-4.1 Mini' },
    { value: 'gpt-4.1-nano', label: 'GPT-4.1 Nano' },
    // o-series reasoning
    { value: 'o3', label: 'o3' },
    { value: 'o3-pro', label: 'o3 Pro' },
    { value: 'o4-mini', label: 'o4-mini' },
    { value: 'o3-mini', label: 'o3-mini' },
    { value: 'o1', label: 'o1' },
    // GPT-4o (still active)
    { value: 'gpt-4o', label: 'GPT-4o' },
    { value: 'gpt-4o-mini', label: 'GPT-4o Mini' },
  ],
  anthropic: [
    // Claude 4.6 (latest, Feb 2026)
    { value: 'claude-opus-4-6', label: 'Claude Opus 4.6 (Latest)' },
    { value: 'claude-sonnet-4-6', label: 'Claude Sonnet 4.6 (Latest)' },
    { value: 'claude-haiku-4-5', label: 'Claude Haiku 4.5 (Latest)' },
    // Claude 4.5
    { value: 'claude-opus-4-5', label: 'Claude Opus 4.5' },
    { value: 'claude-sonnet-4-5', label: 'Claude Sonnet 4.5' },
    // Claude 4.1 / 4.0
    { value: 'claude-opus-4-1', label: 'Claude Opus 4.1' },
    { value: 'claude-sonnet-4-0', label: 'Claude Sonnet 4' },
    { value: 'claude-opus-4-0', label: 'Claude Opus 4' },
    // Claude 3 (legacy, Haiku retiring Apr 2026)
    { value: 'claude-3-haiku-20240307', label: 'Claude 3 Haiku (Retiring Apr 2026)' },
  ],
  gemini: [
    // Gemini 3 (preview, Feb 2026)
    { value: 'gemini-3.1-pro-preview', label: 'Gemini 3.1 Pro (Preview)' },
    { value: 'gemini-3-flash-preview', label: 'Gemini 3 Flash (Preview)' },
    { value: 'gemini-3.1-flash-lite-preview', label: 'Gemini 3.1 Flash Lite (Preview)' },
    // Gemini 2.5 (stable)
    { value: 'gemini-2.5-pro', label: 'Gemini 2.5 Pro' },
    { value: 'gemini-2.5-flash', label: 'Gemini 2.5 Flash' },
    { value: 'gemini-2.5-flash-lite', label: 'Gemini 2.5 Flash Lite' },
    // Gemini 2.0
    { value: 'gemini-2.0-flash', label: 'Gemini 2.0 Flash' },
    { value: 'gemini-2.0-flash-lite', label: 'Gemini 2.0 Flash Lite' },
    // Gemini 1.5
    { value: 'gemini-1.5-pro', label: 'Gemini 1.5 Pro' },
    { value: 'gemini-1.5-flash', label: 'Gemini 1.5 Flash' },
  ],
  cohere: [
    // Command A (latest flagship, 111B)
    { value: 'command-a-03-2025', label: 'Command A (Latest)' },
    // Command R7B (smallest, fastest)
    { value: 'command-r7b-12-2024', label: 'Command R7B' },
    // Command R+ / R (versioned)
    { value: 'command-r-plus-08-2024', label: 'Command R+' },
    { value: 'command-r-08-2024', label: 'Command R' },
  ],
  glm: [
    { value: 'glm-4-plus', label: 'GLM-4 Plus' },
    { value: 'glm-4', label: 'GLM-4' },
    { value: 'glm-4-flash', label: 'GLM-4 Flash' },
    { value: 'glm-4-air', label: 'GLM-4 Air' },
    { value: 'glm-3-turbo', label: 'GLM-3 Turbo' },
  ],
  kimi: [
    { value: 'moonshot-v1-8k', label: 'Moonshot v1 8K' },
    { value: 'moonshot-v1-32k', label: 'Moonshot v1 32K' },
    { value: 'moonshot-v1-128k', label: 'Moonshot v1 128K' },
  ],
  ollama: [
    // Llama 4 (latest Meta, 2025)
    { value: 'llama4:scout', label: 'Llama 4 Scout' },
    { value: 'llama4:maverick', label: 'Llama 4 Maverick' },
    // Llama 3.x
    { value: 'llama3.2', label: 'Llama 3.2' },
    { value: 'llama3.1', label: 'Llama 3.1' },
    // DeepSeek
    { value: 'deepseek-r1', label: 'DeepSeek R1' },
    { value: 'deepseek-v3', label: 'DeepSeek V3' },
    // Qwen
    { value: 'qwen3', label: 'Qwen 3' },
    { value: 'qwen2.5', label: 'Qwen 2.5' },
    // Mistral
    { value: 'mistral-large', label: 'Mistral Large' },
    { value: 'mistral', label: 'Mistral 7B' },
    { value: 'mixtral', label: 'Mixtral' },
    // Google / Microsoft
    { value: 'gemma2', label: 'Gemma 2' },
    { value: 'phi4', label: 'Phi-4' },
    { value: 'phi3', label: 'Phi-3' },
    // Code
    { value: 'codellama', label: 'Code Llama' },
  ],
  vertex: [
    // Gemini 3 (preview)
    { value: 'gemini-3.1-pro-preview', label: 'Gemini 3.1 Pro (Preview)' },
    { value: 'gemini-3-flash-preview', label: 'Gemini 3 Flash (Preview)' },
    // Gemini 2.5
    { value: 'gemini-2.5-pro', label: 'Gemini 2.5 Pro' },
    { value: 'gemini-2.5-flash', label: 'Gemini 2.5 Flash' },
    // Gemini 2.0
    { value: 'gemini-2.0-flash', label: 'Gemini 2.0 Flash' },
    // Claude 4.6 on Vertex
    { value: 'claude-opus-4-6', label: 'Claude Opus 4.6' },
    { value: 'claude-sonnet-4-6', label: 'Claude Sonnet 4.6' },
    { value: 'claude-haiku-4-5@20251001', label: 'Claude Haiku 4.5' },
    // Claude 4.5 on Vertex
    { value: 'claude-opus-4-5@20251101', label: 'Claude Opus 4.5' },
    { value: 'claude-sonnet-4-5@20250929', label: 'Claude Sonnet 4.5' },
  ],
  bedrock: [
    // Claude 4.6 (latest, Feb 2026)
    { value: 'anthropic.claude-opus-4-6-v1', label: 'Claude Opus 4.6 (Latest)' },
    { value: 'anthropic.claude-sonnet-4-6', label: 'Claude Sonnet 4.6 (Latest)' },
    { value: 'anthropic.claude-haiku-4-5-20251001-v1:0', label: 'Claude Haiku 4.5' },
    // Claude 4.5
    { value: 'anthropic.claude-opus-4-5-20251101-v1:0', label: 'Claude Opus 4.5' },
    { value: 'anthropic.claude-sonnet-4-5-20250929-v1:0', label: 'Claude Sonnet 4.5' },
    // Llama 4
    { value: 'meta.llama4-maverick-17b-instruct-v1:0', label: 'Llama 4 Maverick 17B' },
    { value: 'meta.llama4-scout-17b-instruct-v1:0', label: 'Llama 4 Scout 17B' },
    { value: 'meta.llama3-70b-instruct-v1:0', label: 'Llama 3 70B' },
    // Amazon Nova
    { value: 'amazon.nova-pro-v1:0', label: 'Amazon Nova Pro' },
    { value: 'amazon.nova-lite-v1:0', label: 'Amazon Nova Lite' },
    { value: 'amazon.nova-micro-v1:0', label: 'Amazon Nova Micro' },
  ],
}

const CUSTOM_MODEL_VALUE = '__custom__'

function ModelSelect({
  provider,
  value,
  onChange,
}: {
  provider: string
  value?: string
  onChange: (model: string | undefined) => void
}) {
  const models = PROVIDER_MODELS[provider] ?? []
  const isKnown = !!value && models.some((m) => m.value === value)
  const [isCustom, setIsCustom] = useState(!isKnown && !!value)
  const [customText, setCustomText] = useState(!isKnown && value ? value : '')

  const selectValue = isCustom ? CUSTOM_MODEL_VALUE : (value ?? '')

  const handleSelect = (v: string) => {
    if (v === CUSTOM_MODEL_VALUE) {
      setIsCustom(true)
      onChange(customText || undefined)
    } else {
      setIsCustom(false)
      onChange(v || undefined)
    }
  }

  return (
    <div className="space-y-2">
      <Select value={selectValue} onValueChange={handleSelect}>
        <SelectTrigger className="w-full">
          <SelectValue placeholder="Select a model…" />
        </SelectTrigger>
        <SelectContent>
          {models.map((m) => (
            <SelectItem key={m.value} value={m.value}>
              {m.label}
            </SelectItem>
          ))}
          <SelectItem value={CUSTOM_MODEL_VALUE}>Custom…</SelectItem>
        </SelectContent>
      </Select>
      {isCustom && (
        <input
          type="text"
          value={customText}
          onChange={(e) => {
            setCustomText(e.target.value)
            onChange(e.target.value || undefined)
          }}
          placeholder="Enter model name"
          autoFocus
          className={cn(
            'w-full h-9 rounded-md border border-border bg-surface-0 px-3 py-2 text-sm',
            'placeholder:text-overlay-0 focus:outline-none focus:ring-2 focus:ring-primary focus:ring-offset-2'
          )}
        />
      )}
    </div>
  )
}

/**
 * Extract a human-readable message from a server error string.
 * Server errors often embed a JSON payload like:
 *   "... ProviderError: {"type":"error","error":{"type":"...","message":"..."},...}"
 * This pulls out the innermost `message` field, or falls back to the
 * text that precedes the JSON if no message field is found.
 */
function extractErrorMessage(raw: string): string {
  // Try to parse any embedded JSON object in the string
  const jsonStart = raw.indexOf('{')
  if (jsonStart !== -1) {
    try {
      const json = JSON.parse(raw.slice(jsonStart))
      // Anthropic / OpenAI shape: { error: { message: "..." } }
      if (json?.error?.message) return json.error.message
      // Generic shape: { message: "..." }
      if (json?.message) return json.message
    } catch {
      // not valid JSON — fall through
    }
  }
  // Strip well-known prefixes to shorten the text
  return raw
    .replace(/^Reasoning error:\s*/i, '')
    .replace(/^.*?ProviderError:\s*/i, '')
    .replace(/^.*?CompletionError:\s*/i, '')
    .trim()
}

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
    <span className="inline-flex items-center gap-1 text-xs text-peach">
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
  const isVertex = config.provider === 'vertex'
  const isBedrock = config.provider === 'bedrock'

  const update = (patch: Partial<LlmModelConfig>) => {
    // Clear model when provider changes so the dropdown resets cleanly
    if ('provider' in patch && patch.provider !== config.provider) {
      onChange({ ...config, ...patch, model: undefined })
    } else {
      onChange({ ...config, ...patch })
    }
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

        {!isOllama && !isBedrock && (
          <div>
            <label className="block text-xs font-medium text-subtext-0 mb-1">
              {isVertex ? 'Access token (Google Cloud)' : 'API Key'}
            </label>
            <input
              type="password"
              value={config.api_key || ''}
              onChange={(e) => update({ api_key: e.target.value || undefined })}
              placeholder={isVertex ? 'Bearer token from gcloud auth' : 'sk-...'}
              className={cn(
                'w-full h-9 rounded-md border border-border bg-surface-0 px-3 py-2 text-sm',
                'placeholder:text-overlay-0 focus:outline-none focus:ring-2 focus:ring-primary focus:ring-offset-2'
              )}
            />
          </div>
        )}

        <div>
          <label className="block text-xs font-medium text-subtext-0 mb-1">Model</label>
          <ModelSelect
            key={config.provider}
            provider={config.provider}
            value={config.model}
            onChange={(model) => update({ model })}
          />
        </div>

        {(isOllama || isVertex) && (
          <div>
            <label className="block text-xs font-medium text-subtext-0 mb-1">
              Base URL {isVertex && '(Vertex OpenAI-compatible endpoint)'}
            </label>
            <input
              type="text"
              value={config.base_url || ''}
              onChange={(e) => update({ base_url: e.target.value || undefined })}
              placeholder={
                isVertex
                  ? 'https://LOCATION-aiplatform.googleapis.com/v1/projects/PROJECT/locations/LOCATION/endpoints/openapi'
                  : 'http://localhost:11434/v1'
              }
              className={cn(
                'w-full h-9 rounded-md border border-border bg-surface-0 px-3 py-2 text-sm',
                'placeholder:text-overlay-0 focus:outline-none focus:ring-2 focus:ring-primary focus:ring-offset-2'
              )}
            />
          </div>
        )}

        {isBedrock && (
          <div>
            <label className="block text-xs font-medium text-subtext-0 mb-1">Region</label>
            <input
              type="text"
              value={config.region || ''}
              onChange={(e) => update({ region: e.target.value || undefined })}
              placeholder="e.g. us-east-1"
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

        {status && !status.ok && !testing && (
          <div className="flex items-start gap-2 rounded-md border border-peach/30 bg-peach/10 px-3 py-2">
            <WarningCircle size={14} weight="fill" className="mt-0.5 shrink-0 text-peach" />
            <p className="text-xs text-peach leading-snug">
              {extractErrorMessage(status.error ?? 'Connection test failed')}
            </p>
          </div>
        )}
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
  const [chunkStrategy, setChunkStrategy] = useState<ChunkStrategy>(
    () => (localStorage.getItem(CHUNK_STRATEGY_KEY) as ChunkStrategy | null) ?? 'agentic'
  )

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
      localStorage.setItem(CHUNK_STRATEGY_KEY, chunkStrategy)
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

        {/* Chunking Strategy */}
        <div className="p-4 rounded-lg border border-border bg-surface-0/30 space-y-4">
          <h3 className="text-sm font-semibold text-text uppercase tracking-wide">
            Ingestion Chunking Strategy
          </h3>
          <div className="space-y-3">
            {CHUNK_STRATEGIES.map((s) => (
              <label
                key={s.value}
                className={cn(
                  'flex items-start gap-3 p-3 rounded-md border cursor-pointer transition-colors',
                  chunkStrategy === s.value
                    ? 'border-mauve bg-mauve/10'
                    : 'border-border hover:border-overlay-1'
                )}
              >
                <input
                  type="radio"
                  name="chunk_strategy"
                  value={s.value}
                  checked={chunkStrategy === s.value}
                  onChange={() => setChunkStrategy(s.value)}
                  className="mt-0.5 accent-mauve shrink-0"
                />
                <div>
                  <div className="flex items-center gap-2">
                    <span className="text-sm font-medium text-text">{s.label}</span>
                    {s.value === 'agentic' && (
                      <span className="text-[10px] font-semibold uppercase tracking-wide px-1.5 py-0.5 rounded bg-mauve/20 text-mauve">
                        Default
                      </span>
                    )}
                  </div>
                  <p className="text-xs text-subtext-0 mt-0.5 leading-relaxed">{s.description}</p>
                </div>
              </label>
            ))}
          </div>
        </div>
      </div>
    </div>
  )
}
