import {
  Sidebar as SidebarIcon,
  Database,
  Plugs,
  PlugsConnected,
  Sun,
  Moon,
  Monitor,
} from '@phosphor-icons/react'
import { useUiStore, type Theme } from '@/stores/uiStore'
import { cn } from '@/lib/utils'
import type { Connection } from '@/stores/connectionStore'

interface TitleBarProps {
  connection?: Connection
}

const THEME_CYCLE: Theme[] = ['dark', 'light', 'system']
const THEME_META: Record<Theme, { Icon: typeof Moon; label: string }> = {
  dark: { Icon: Moon, label: 'Dark' },
  light: { Icon: Sun, label: 'Light' },
  system: { Icon: Monitor, label: 'System' },
}

export function TitleBar({ connection }: TitleBarProps) {
  const { theme, setTheme, toggleSidebar, sidebarOpen } = useUiStore()

  const cycleTheme = () => {
    const idx = THEME_CYCLE.indexOf(theme)
    setTheme(THEME_CYCLE[(idx + 1) % THEME_CYCLE.length])
  }

  const { Icon: ThemeIcon, label: themeLabel } = THEME_META[theme]

  return (
    <div
      className="h-10 bg-mantle border-b border-border flex items-center justify-between select-none"
      role="banner"
    >
      {/* Left section */}
      <div className="flex items-center gap-2 px-3">
        <button
          onClick={toggleSidebar}
          className="p-1.5 rounded-md hover:bg-surface-0 text-subtext-0 hover:text-text transition-all duration-200 focus-visible:ring-2 focus-visible:ring-primary"
          aria-label={sidebarOpen ? 'Collapse sidebar' : 'Expand sidebar'}
          aria-expanded={sidebarOpen}
        >
          <SidebarIcon
            size={18}
            weight="bold"
            aria-hidden="true"
          />
        </button>

        <div className="flex items-center gap-2 text-sm">
          <Database size={18} weight="duotone" className="text-mauve" aria-hidden="true" />
          <span className="font-semibold text-text">ReasonDB</span>
        </div>
      </div>

      {/* Center - Connection status */}
      <div className="flex-1 flex items-center justify-center gap-2">
        {connection ? (
          <div className="flex items-center gap-2 px-3 py-1 rounded-md bg-surface-0" role="status">
            <PlugsConnected size={14} weight="fill" className="text-green" aria-hidden="true" />
            <span className="text-xs text-subtext-1">{connection.name}</span>
            <span className="text-xs text-overlay-0">
              ({connection.host}:{connection.port})
            </span>
          </div>
        ) : (
          <div className="flex items-center gap-2 px-3 py-1 rounded-md bg-surface-0/50" role="status">
            <Plugs size={14} weight="bold" className="text-overlay-0" aria-hidden="true" />
            <span className="text-xs text-overlay-0">Not connected</span>
          </div>
        )}
      </div>

      {/* Right section — theme toggle */}
      <div className="flex items-center px-2">
        <button
          onClick={cycleTheme}
          className={cn(
            'p-2 rounded-md',
            'text-subtext-0 hover:text-text hover:bg-surface-0 transition-colors',
            'focus-visible:ring-2 focus-visible:ring-primary'
          )}
          aria-label={`Theme: ${themeLabel}. Click to switch.`}
        >
          <ThemeIcon size={16} weight="bold" aria-hidden="true" />
        </button>
      </div>
    </div>
  )
}
