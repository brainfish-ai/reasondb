import { useEffect } from 'react'
import { TitleBar } from '@/components/layout/TitleBar'
import { Sidebar } from '@/components/layout/Sidebar'
import { MainPanel } from '@/components/layout/MainPanel'
import { StatusBar } from '@/components/layout/StatusBar'
import { IngestionStatus } from '@/components/ingestion/IngestionStatus'
import { useUiStore } from '@/stores/uiStore'
import { useConnectionStore } from '@/stores/connectionStore'

function App() {
  const { theme, sidebarOpen } = useUiStore()
  const { activeConnectionId, connections } = useConnectionStore()

  const activeConnection = connections.find((c) => c.id === activeConnectionId)

  useEffect(() => {
    const root = document.documentElement
    if (theme === 'system') {
      const systemTheme = window.matchMedia('(prefers-color-scheme: dark)')
        .matches
        ? 'dark'
        : 'light'
      root.setAttribute('data-theme', systemTheme)
    } else {
      root.setAttribute('data-theme', theme)
    }
  }, [theme])

  return (
    <>
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
          <div className="flex-1 overflow-hidden transition-all duration-300 ease-in-out">
            <MainPanel />
          </div>
        </div>

        <StatusBar connection={activeConnection} />
      </div>

      <IngestionStatus />
    </>
  )
}

export default App
