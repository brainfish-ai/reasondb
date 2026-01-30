import { useState } from 'react'
import {
  Database,
  Clock,
  Star,
  Plus,
  PlugsConnected,
} from '@phosphor-icons/react'
import { cn } from '@/lib/utils'
import { useConnectionStore, type Connection } from '@/stores/connectionStore'
import { ConnectionList } from '@/components/connection/ConnectionList'
import { ConnectionForm } from '@/components/connection/ConnectionForm'
import { TableBrowser } from '@/components/table/TableBrowser'
import { CreateTableDialog } from '@/components/table/CreateTableDialog'
import { createClient, setClient } from '@/lib/api'

export function Sidebar() {
  const { activeConnectionId, setActiveConnection, setConnecting, setConnectionError } = useConnectionStore()
  const [showConnectionForm, setShowConnectionForm] = useState(false)
  const [showCreateTable, setShowCreateTable] = useState(false)
  const [editingConnection, setEditingConnection] = useState<Connection | undefined>()
  const [activeSection, setActiveSection] = useState<'connections' | 'tables'>('connections')

  // Auto-switch to tables when connected
  const effectiveSection = activeConnectionId ? activeSection : 'connections'

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
        setActiveSection('tables')
      } else {
        setConnectionError(result.error || 'Connection failed')
      }
    } catch (error) {
      setConnectionError(error instanceof Error ? error.message : 'Connection failed')
    } finally {
      setConnecting(false)
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
      {/* Section tabs - only show when connected */}
      {activeConnectionId && (
        <div className="px-3 pt-3 pb-2 flex gap-1">
          <button
            onClick={() => setActiveSection('connections')}
            className={cn(
              'flex-1 flex items-center justify-center gap-1.5 px-2 py-1.5 text-xs font-medium rounded-md transition-colors',
              effectiveSection === 'connections'
                ? 'bg-surface-0 text-text'
                : 'text-overlay-1 hover:text-text hover:bg-surface-0/50'
            )}
          >
            <PlugsConnected size={14} weight={effectiveSection === 'connections' ? 'fill' : 'bold'} />
            Connections
          </button>
          <button
            onClick={() => setActiveSection('tables')}
            className={cn(
              'flex-1 flex items-center justify-center gap-1.5 px-2 py-1.5 text-xs font-medium rounded-md transition-colors',
              effectiveSection === 'tables'
                ? 'bg-surface-0 text-text'
                : 'text-overlay-1 hover:text-text hover:bg-surface-0/50'
            )}
          >
            <Database size={14} weight={effectiveSection === 'tables' ? 'fill' : 'bold'} />
            Tables
          </button>
        </div>
      )}

      {/* Content */}
      <div className="flex-1 overflow-auto">
        {effectiveSection === 'connections' ? (
          <div className="px-3 py-2">
            <div className="flex items-center justify-between mb-2">
              <div className="text-xs font-semibold text-overlay-1 uppercase tracking-wide">
                {activeConnectionId ? 'Servers' : 'Connect to Server'}
              </div>
              <button
                onClick={handleNewConnection}
                className="p-1 rounded hover:bg-surface-0 text-overlay-0 hover:text-text transition-colors"
                title="New Connection"
              >
                <Plus size={14} weight="bold" />
              </button>
            </div>
            <ConnectionList
              onEdit={handleEditConnection}
              onConnect={handleConnect}
            />
          </div>
        ) : (
          <TableBrowser />
        )}
      </div>

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
      <CreateTableDialog
        open={showCreateTable}
        onOpenChange={setShowCreateTable}
      />
    </div>
  )
}
