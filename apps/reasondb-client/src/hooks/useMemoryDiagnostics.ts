import { useEffect, useRef } from 'react'
import { useTabsStore } from '@/stores/tabsStore'
import { useQueryStore } from '@/stores/queryStore'

let monacoInstanceCount = 0

export function trackMonacoMount(label: string) {
  monacoInstanceCount++
  console.log(`[mem] Monaco MOUNT: ${label} (total: ${monacoInstanceCount})`)
  return () => {
    monacoInstanceCount--
    console.log(`[mem] Monaco UNMOUNT: ${label} (total: ${monacoInstanceCount})`)
  }
}

interface MemoryInfo {
  usedJSHeapSize: number
  totalJSHeapSize: number
  jsHeapSizeLimit: number
}

export function useMemoryDiagnostics(enabled = import.meta.env.DEV) {
  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null)

  useEffect(() => {
    if (!enabled) return

    const perf = performance as Performance & { memory?: MemoryInfo }
    if (!perf.memory) {
      console.log('[mem] performance.memory not available (non-Chromium WebView)')
      return
    }

    const poll = () => {
      const mem = perf.memory!
      const tabs = useTabsStore.getState().tabs
      const { results, history, savedQueries } = useQueryStore.getState()
      const totalRows = results.reduce((sum, r) => sum + r.rows.length, 0)

      console.table({
        'Heap Used (MB)': (mem.usedJSHeapSize / 1048576).toFixed(1),
        'Heap Total (MB)': (mem.totalJSHeapSize / 1048576).toFixed(1),
        'Heap Limit (MB)': (mem.jsHeapSizeLimit / 1048576).toFixed(1),
        'Monaco Instances': monacoInstanceCount,
        'Open Tabs': tabs.length,
        'Query Tabs': tabs.filter(t => t.type === 'query').length,
        'Result Rows': totalRows,
        'History Items': history.length,
        'Saved Queries': savedQueries.length,
      })
    }

    poll()
    intervalRef.current = setInterval(poll, 10_000)

    return () => {
      if (intervalRef.current) clearInterval(intervalRef.current)
    }
  }, [enabled])
}
