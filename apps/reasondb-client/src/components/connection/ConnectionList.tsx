import { useState, useMemo, useEffect, useRef, useCallback } from 'react'
import {
  Database,
  DotsThree,
  Pencil,
  Trash,
  Plugs,
  PlugsConnected,
  CaretRight,
  FolderSimple,
} from '@phosphor-icons/react'
import { useConnectionStore, type Connection } from '@/stores/connectionStore'
import { cn } from '@/lib/utils'

interface ConnectionListProps {
  onEdit: (connection: Connection) => void
  onConnect: (connection: Connection) => void
}

interface GroupedConnections {
  [key: string]: Connection[]
}

export function ConnectionList({ onEdit, onConnect }: ConnectionListProps) {
  const { connections, activeConnectionId, deleteConnection, setActiveConnection } =
    useConnectionStore()
  const [expandedGroups, setExpandedGroups] = useState<Set<string>>(new Set(['ungrouped']))
  const [contextMenu, setContextMenu] = useState<{
    connection: Connection
    x: number
    y: number
  } | null>(null)
  const menuRef = useRef<HTMLDivElement>(null)

  const groupedConnections = useMemo(() => {
    const grouped: GroupedConnections = {}
    
    connections.forEach((conn) => {
      const group = conn.group || 'ungrouped'
      if (!grouped[group]) {
        grouped[group] = []
      }
      grouped[group].push(conn)
    })

    Object.keys(grouped).forEach((group) => {
      grouped[group].sort((a, b) => a.name.localeCompare(b.name))
    })

    return grouped
  }, [connections])

  const sortedGroups = useMemo(() => {
    const groups = Object.keys(groupedConnections)
    return groups.sort((a, b) => {
      if (a === 'ungrouped') return 1
      if (b === 'ungrouped') return -1
      return a.localeCompare(b)
    })
  }, [groupedConnections])

  const toggleGroup = (group: string) => {
    setExpandedGroups((prev) => {
      const next = new Set(prev)
      if (next.has(group)) {
        next.delete(group)
      } else {
        next.add(group)
      }
      return next
    })
  }

  const closeContextMenu = useCallback(() => {
    setContextMenu(null)
  }, [])

  useEffect(() => {
    if (!contextMenu) return
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') closeContextMenu()
    }
    document.addEventListener('keydown', onKey)
    return () => document.removeEventListener('keydown', onKey)
  }, [contextMenu, closeContextMenu])

  useEffect(() => {
    if (contextMenu && menuRef.current) {
      const firstButton = menuRef.current.querySelector<HTMLButtonElement>('button')
      firstButton?.focus()
    }
  }, [contextMenu])

  const handleContextMenu = (e: React.MouseEvent, connection: Connection) => {
    e.preventDefault()
    setContextMenu({
      connection,
      x: e.clientX,
      y: e.clientY,
    })
  }

  const handleConnect = (connection: Connection) => {
    if (activeConnectionId === connection.id) {
      setActiveConnection(null)
    } else {
      onConnect(connection)
    }
    closeContextMenu()
  }

  const handleEdit = (connection: Connection) => {
    onEdit(connection)
    closeContextMenu()
  }

  const handleDelete = (connection: Connection) => {
    deleteConnection(connection.id)
    closeContextMenu()
  }

  const handleMenuKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault()
      const buttons = menuRef.current?.querySelectorAll<HTMLButtonElement>('button')
      if (!buttons) return
      const focused = document.activeElement as HTMLElement
      const idx = Array.from(buttons).indexOf(focused as HTMLButtonElement)
      const next = e.key === 'ArrowDown'
        ? buttons[(idx + 1) % buttons.length]
        : buttons[(idx - 1 + buttons.length) % buttons.length]
      next.focus()
    }
  }

  if (connections.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center p-6 text-center">
        <Database size={48} className="text-overlay-0 mb-3" weight="duotone" aria-hidden="true" />
        <p className="text-sm text-subtext-0">No connections yet</p>
        <p className="text-xs text-overlay-0 mt-1">
          Click "New Connection" to get started
        </p>
      </div>
    )
  }

  return (
    <div className="relative" onClick={closeContextMenu}>
      {sortedGroups.map((group) => (
        <div key={group} className="mb-1">
          {group !== 'ungrouped' && (
            <button
              onClick={() => toggleGroup(group)}
              className={cn(
                'w-full flex items-center gap-2 px-2 py-1.5 text-xs font-medium',
                'text-subtext-0 hover:text-text hover:bg-surface-0/50 rounded-md transition-colors'
              )}
              aria-expanded={expandedGroups.has(group)}
            >
              <CaretRight
                size={12}
                weight="bold"
                className={cn(
                  'transition-transform duration-200',
                  expandedGroups.has(group) && 'rotate-90'
                )}
                aria-hidden="true"
              />
              <FolderSimple size={14} weight="duotone" aria-hidden="true" />
              <span>{group}</span>
              <span className="ml-auto text-overlay-0">
                {groupedConnections[group].length}
              </span>
            </button>
          )}

          <div
            className={cn(
              'overflow-hidden transition-all duration-200',
              group !== 'ungrouped' && !expandedGroups.has(group) && 'h-0'
            )}
          >
            {groupedConnections[group].map((connection) => (
              <ConnectionItem
                key={connection.id}
                connection={connection}
                isActive={activeConnectionId === connection.id}
                isGrouped={group !== 'ungrouped'}
                onConnect={() => handleConnect(connection)}
                onEdit={() => handleEdit(connection)}
                onContextMenu={(e) => handleContextMenu(e, connection)}
              />
            ))}
          </div>
        </div>
      ))}

      {/* Context Menu */}
      {contextMenu && (
        <div
          ref={menuRef}
          role="menu"
          aria-label={`Actions for ${contextMenu.connection.name}`}
          className={cn(
            'fixed z-50 min-w-[160px] rounded-md border border-border',
            'bg-mantle shadow-lg py-1'
          )}
          style={{ left: contextMenu.x, top: contextMenu.y }}
          onClick={(e) => e.stopPropagation()}
          onKeyDown={handleMenuKeyDown}
        >
          <button
            role="menuitem"
            onClick={() => handleConnect(contextMenu.connection)}
            className="w-full flex items-center gap-2 px-3 py-1.5 text-sm hover:bg-surface-0 text-left focus:bg-surface-0 focus:outline-none"
          >
            {activeConnectionId === contextMenu.connection.id ? (
              <>
                <Plugs size={14} aria-hidden="true" />
                Disconnect
              </>
            ) : (
              <>
                <PlugsConnected size={14} aria-hidden="true" />
                Connect
              </>
            )}
          </button>
          <button
            role="menuitem"
            onClick={() => handleEdit(contextMenu.connection)}
            className="w-full flex items-center gap-2 px-3 py-1.5 text-sm hover:bg-surface-0 text-left focus:bg-surface-0 focus:outline-none"
          >
            <Pencil size={14} aria-hidden="true" />
            Edit
          </button>
          <div className="h-px bg-border my-1" role="separator" />
          <button
            role="menuitem"
            onClick={() => handleDelete(contextMenu.connection)}
            className="w-full flex items-center gap-2 px-3 py-1.5 text-sm hover:bg-surface-0 text-left text-red focus:bg-surface-0 focus:outline-none"
          >
            <Trash size={14} aria-hidden="true" />
            Delete
          </button>
        </div>
      )}
    </div>
  )
}

interface ConnectionItemProps {
  connection: Connection
  isActive: boolean
  isGrouped: boolean
  onConnect: () => void
  onEdit: () => void
  onContextMenu: (e: React.MouseEvent) => void
}

function ConnectionItem({
  connection,
  isActive,
  isGrouped,
  onConnect,
  onEdit,
  onContextMenu,
}: ConnectionItemProps) {
  const [showActions, setShowActions] = useState(false)

  return (
    <div
      className={cn(
        'group flex items-center gap-2 px-2 py-1.5 rounded-md cursor-pointer',
        'hover:bg-surface-0 transition-colors',
        isActive && 'bg-surface-0',
        isGrouped && 'ml-4'
      )}
      onClick={onConnect}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); onConnect() }
      }}
      onContextMenu={onContextMenu}
      onMouseEnter={() => setShowActions(true)}
      onMouseLeave={() => setShowActions(false)}
      onFocus={() => setShowActions(true)}
      onBlur={() => setShowActions(false)}
      role="button"
      tabIndex={0}
      aria-label={`${connection.name} (${connection.host}:${connection.port})${isActive ? ' — connected' : ''}`}
    >
      {isActive ? (
        <PlugsConnected
          size={16}
          weight="fill"
          className="text-green shrink-0"
          aria-hidden="true"
        />
      ) : (
        <div
          className="w-2.5 h-2.5 rounded-full shrink-0"
          style={{ backgroundColor: connection.color || '#60a5fa' }}
          aria-hidden="true"
        />
      )}

      <div className="flex-1 min-w-0">
        <span className="text-sm font-medium truncate block">{connection.name}</span>
        <span className="text-xs text-overlay-0 truncate block">
          {connection.host}:{connection.port}
        </span>
      </div>

      <div
        className={cn(
          'flex items-center gap-1 transition-opacity',
          showActions ? 'opacity-100' : 'opacity-0'
        )}
      >
        <button
          onClick={(e) => {
            e.stopPropagation()
            onEdit()
          }}
          className="p-1 rounded hover:bg-surface-1 text-overlay-0 hover:text-text"
          aria-label={`Edit ${connection.name}`}
        >
          <Pencil size={14} aria-hidden="true" />
        </button>
        <button
          onClick={(e) => {
            e.stopPropagation()
            onContextMenu(e)
          }}
          className="p-1 rounded hover:bg-surface-1 text-overlay-0 hover:text-text"
          aria-label={`More actions for ${connection.name}`}
        >
          <DotsThree size={14} weight="bold" aria-hidden="true" />
        </button>
      </div>
    </div>
  )
}
