import { useState } from 'react'
import {
  Clock,
  Star,
  Plus,
  CaretLeft,
} from '@phosphor-icons/react'
import { cn } from '@/lib/utils'
import { useConnectionStore, type Connection } from '@/stores/connectionStore'
import { useUiStore } from '@/stores/uiStore'
import { ConnectionList } from '@/components/connection/ConnectionList'
import { ConnectionForm } from '@/components/connection/ConnectionForm'
import { TableBrowser } from '@/components/table/TableBrowser'
import { createClient, setClient, removeClient } from '@/lib/api'

export function Sidebar() {
  const { 
    activeConnectionId, 
    setActiveConnection, 
    setConnecting, 
    setConnectionError,
  } = useConnectionStore()
  const { showConnectionForm, setShowConnectionForm } = useUiStore()
  const [editingConnection, setEditingConnection] = useState<Connection | undefined>()

  const handleConnect = async (connection: Connection) => {
    setConnecting(true)
    setConnectionError(null)

    try {
      const client = createClient({
        host: connection.host,
        port: connection.port,
        apiKey: connection.apiKey,
        useSsl: connection.ssl,
      })

      const result = await client.testConnection()
      
      if (result.success) {
        // Store the client for this connection
        setClient(connection.id, client)
        setActiveConnection(connection.id)
      } else {
        setConnectionError(result.error || 'Connection failed')
      }
    } catch (error) {
      setConnectionError(error instanceof Error ? error.message : 'Connection failed')
    } finally {
      setConnecting(false)
    }
  }

  const handleDisconnect = () => {
    if (activeConnectionId) {
      removeClient(activeConnectionId)
      setActiveConnection(null)
    }
  }

  const handleEditConnection = (connection: Connection) => {
    setEditingConnection(connection)
    setShowConnectionForm(true)
  }

  const handleNewConnection = () => {
    setEditingConnection(undefined)
    setShowConnectionForm(true)
  }

  return (
    <div className="h-full bg-mantle flex flex-col border-r border-border min-w-[200px]">
      {/* Show Tables when connected, otherwise show Connections */}
      {activeConnectionId ? (
        <>
          {/* Back to connections link */}
          <div className="px-3 pt-3 pb-2">
            <button
              onClick={handleDisconnect}
              className="flex items-center gap-1.5 text-xs text-overlay-0 hover:text-text transition-colors"
            >
              <CaretLeft size={12} weight="bold" />
              <span>Connections</span>
            </button>
          </div>

          {/* Tables browser */}
          <div className="flex-1 overflow-auto">
            <TableBrowser />
          </div>
        </>
      ) : (
        <>
          {/* Connections header */}
          <div className="px-3 pt-3 pb-2">
            <div className="flex items-center justify-between">
              <div className="text-xs font-semibold text-overlay-1 uppercase tracking-wide">
                Connect to Server
              </div>
              <button
                onClick={handleNewConnection}
                className="p-1 rounded hover:bg-surface-0 text-overlay-0 hover:text-text transition-colors"
                title="New Connection"
              >
                <Plus size={14} weight="bold" />
              </button>
            </div>
          </div>

          {/* Connections list */}
          <div className="flex-1 overflow-auto px-3">
            <ConnectionList
              onEdit={handleEditConnection}
              onConnect={handleConnect}
            />
          </div>
        </>
      )}

      {/* Quick actions */}
      <div className="border-t border-border p-3 space-y-1">
        <button
          className={cn(
            'w-full flex items-center gap-2 px-3 py-2 text-sm rounded-md',
            'text-subtext-1 hover:text-text hover:bg-surface-0 transition-colors'
          )}
        >
          <Clock size={16} weight="duotone" />
          Recent Queries
        </button>
        <button
          className={cn(
            'w-full flex items-center gap-2 px-3 py-2 text-sm rounded-md',
            'text-subtext-1 hover:text-text hover:bg-surface-0 transition-colors'
          )}
        >
          <Star size={16} weight="duotone" />
          Saved Queries
        </button>
      </div>

      {/* Modals */}
      <ConnectionForm
        open={showConnectionForm}
        onOpenChange={setShowConnectionForm}
        editConnection={editingConnection}
      />
    </div>
  )
}
