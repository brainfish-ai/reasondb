import { useState, useMemo } from 'react'
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

  // Group connections
  const groupedConnections = useMemo(() => {
    const grouped: GroupedConnections = {}
    
    connections.forEach((conn) => {
      const group = conn.group || 'ungrouped'
      if (!grouped[group]) {
        grouped[group] = []
      }
      grouped[group].push(conn)
    })

    // Sort connections within each group by name
    Object.keys(grouped).forEach((group) => {
      grouped[group].sort((a, b) => a.name.localeCompare(b.name))
    })

    return grouped
  }, [connections])

  // Get sorted group names (ungrouped last)
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

  const handleContextMenu = (e: React.MouseEvent, connection: Connection) => {
    e.preventDefault()
    setContextMenu({
      connection,
      x: e.clientX,
      y: e.clientY,
    })
  }

  const closeContextMenu = () => {
    setContextMenu(null)
  }

  const handleConnect = (connection: Connection) => {
    if (activeConnectionId === connection.id) {
      // Disconnect
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

  if (connections.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center p-6 text-center">
        <Database size={48} className="text-overlay-0 mb-3" weight="duotone" />
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
          {/* Group Header */}
          {group !== 'ungrouped' && (
            <button
              onClick={() => toggleGroup(group)}
              className={cn(
                'w-full flex items-center gap-2 px-2 py-1.5 text-xs font-medium',
                'text-subtext-0 hover:text-text hover:bg-surface-0/50 rounded-md transition-colors'
              )}
            >
              <CaretRight
                size={12}
                weight="bold"
                className={cn(
                  'transition-transform duration-200',
                  expandedGroups.has(group) && 'rotate-90'
                )}
              />
              <FolderSimple size={14} weight="duotone" />
              <span>{group}</span>
              <span className="ml-auto text-overlay-0">
                {groupedConnections[group].length}
              </span>
            </button>
          )}

          {/* Connections */}
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
          className={cn(
            'fixed z-50 min-w-[160px] rounded-md border border-border',
            'bg-mantle shadow-lg py-1'
          )}
          style={{ left: contextMenu.x, top: contextMenu.y }}
          onClick={(e) => e.stopPropagation()}
        >
          <button
            onClick={() => handleConnect(contextMenu.connection)}
            className="w-full flex items-center gap-2 px-3 py-1.5 text-sm hover:bg-surface-0 text-left"
          >
            {activeConnectionId === contextMenu.connection.id ? (
              <>
                <Plugs size={14} />
                Disconnect
              </>
            ) : (
              <>
                <PlugsConnected size={14} />
                Connect
              </>
            )}
          </button>
          <button
            onClick={() => handleEdit(contextMenu.connection)}
            className="w-full flex items-center gap-2 px-3 py-1.5 text-sm hover:bg-surface-0 text-left"
          >
            <Pencil size={14} />
            Edit
          </button>
          <div className="h-px bg-border my-1" />
          <button
            onClick={() => handleDelete(contextMenu.connection)}
            className="w-full flex items-center gap-2 px-3 py-1.5 text-sm hover:bg-surface-0 text-left text-red"
          >
            <Trash size={14} />
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
      onContextMenu={onContextMenu}
      onMouseEnter={() => setShowActions(true)}
      onMouseLeave={() => setShowActions(false)}
    >
      {/* Connection status indicator */}
      {isActive ? (
        <PlugsConnected
          size={16}
          weight="fill"
          className="text-green shrink-0"
        />
      ) : (
        <div
          className="w-2.5 h-2.5 rounded-full shrink-0"
          style={{ backgroundColor: connection.color || '#89b4fa' }}
        />
      )}

      {/* Connection info */}
      <div className="flex-1 min-w-0">
        <span className="text-sm font-medium truncate block">{connection.name}</span>
        <span className="text-xs text-overlay-0 truncate block">
          {connection.host}:{connection.port}
        </span>
      </div>

      {/* Actions */}
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
          title="Edit"
        >
          <Pencil size={14} />
        </button>
        <button
          onClick={(e) => {
            e.stopPropagation()
            onContextMenu(e)
          }}
          className="p-1 rounded hover:bg-surface-1 text-overlay-0 hover:text-text"
          title="More actions"
        >
          <DotsThree size={14} weight="bold" />
        </button>
      </div>
    </div>
  )
}
