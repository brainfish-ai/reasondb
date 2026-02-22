import { useEffect } from 'react'
import { TitleBar } from '@/components/layout/TitleBar'
import { Sidebar } from '@/components/layout/Sidebar'
import { MainPanel } from '@/components/layout/MainPanel'
import { StatusBar } from '@/components/layout/StatusBar'
import { IngestionStatus } from '@/components/ingestion/IngestionStatus'
import { useUiStore } from '@/stores/uiStore'
import { useConnectionStore } from '@/stores/connectionStore'
import { useMemoryDiagnostics } from '@/hooks/useMemoryDiagnostics'
import { MonacoProvider } from '@/providers/MonacoProvider'

function App() {
  useMemoryDiagnostics()
  const { theme, sidebarOpen } = useUiStore()
  const { activeConnectionId, connections } = useConnectionStore()

  const activeConnection = connections.find((c) => c.id === activeConnectionId)

  useEffect(() => {
    const root = document.documentElement
    const applyTheme = () => {
      if (theme === 'system') {
        const systemTheme = window.matchMedia('(prefers-color-scheme: dark)')
          .matches
          ? 'dark'
          : 'light'
        root.setAttribute('data-theme', systemTheme)
      } else {
        root.setAttribute('data-theme', theme)
      }
    }

    applyTheme()

    if (theme === 'system') {
      const mq = window.matchMedia('(prefers-color-scheme: dark)')
      const handler = () => applyTheme()
      mq.addEventListener('change', handler)
      return () => mq.removeEventListener('change', handler)
    }
  }, [theme])

  return (
    <MonacoProvider>
      <a href="#main-content" className="skip-nav">
        Skip to main content
      </a>

      <div className="flex flex-col h-screen bg-background text-foreground">
        <TitleBar connection={activeConnection} />

        <div className="flex-1 flex overflow-hidden">
          <div 
            className={`shrink-0 transition-all duration-300 ease-in-out overflow-hidden ${
              sidebarOpen ? 'w-64 opacity-100' : 'w-0 opacity-0'
            }`}
          >
            <div className="w-64 h-full">
              <Sidebar />
            </div>
          </div>
          <main
            id="main-content"
            className="flex-1 overflow-hidden transition-all duration-300 ease-in-out"
          >
            <MainPanel />
          </main>
        </div>

        <StatusBar connection={activeConnection} />
      </div>

      <IngestionStatus />
    </MonacoProvider>
  )
}

export default App
