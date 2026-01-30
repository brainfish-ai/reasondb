import { useState } from 'react'
import {
  Database,
  FileText,
  MagnifyingGlass,
  Clock,
  Star,
  Plus,
  CaretDown,
  CaretRight,
  Folder,
  FolderOpen,
  PlugsConnected,
} from '@phosphor-icons/react'
import { cn } from '@/lib/utils'
import { useConnectionStore, type Connection } from '@/stores/connectionStore'
import { ConnectionList } from '@/components/connection/ConnectionList'
import { ConnectionForm } from '@/components/connection/ConnectionForm'

interface TreeItem {
  id: string
  name: string
  type: 'table' | 'document' | 'folder'
  children?: TreeItem[]
}

// Mock data - will be replaced with actual API data
const mockTables: TreeItem[] = [
  {
    id: '1',
    name: 'legal_contracts',
    type: 'table',
    children: [
      { id: '1-1', name: 'nda_agreement.md', type: 'document' },
      { id: '1-2', name: 'msa_contract.md', type: 'document' },
      { id: '1-3', name: 'sla_agreement.md', type: 'document' },
    ],
  },
  {
    id: '2',
    name: 'research_papers',
    type: 'table',
    children: [
      { id: '2-1', name: 'ml_foundations.pdf', type: 'document' },
      { id: '2-2', name: 'transformer_arch.pdf', type: 'document' },
    ],
  },
  {
    id: '3',
    name: 'knowledge_base',
    type: 'table',
    children: [],
  },
]

function TreeNode({
  item,
  level = 0,
}: {
  item: TreeItem
  level?: number
}) {
  const [isExpanded, setIsExpanded] = useState(false)
  const hasChildren = item.children && item.children.length > 0

  const getIcon = () => {
    if (item.type === 'table') {
      return isExpanded ? (
        <FolderOpen size={16} weight="duotone" className="text-blue" />
      ) : (
        <Folder size={16} weight="duotone" className="text-blue" />
      )
    }
    return <FileText size={16} weight="duotone" className="text-subtext-0" />
  }

  return (
    <div>
      <button
        onClick={() => hasChildren && setIsExpanded(!isExpanded)}
        className={cn(
          'w-full flex items-center gap-2 px-2 py-1.5 text-sm text-left',
          'hover:bg-surface-0 rounded-md transition-colors',
          'text-subtext-1 hover:text-text'
        )}
        style={{ paddingLeft: `${level * 12 + 8}px` }}
      >
        {hasChildren ? (
          isExpanded ? (
            <CaretDown size={12} weight="bold" className="text-overlay-0" />
          ) : (
            <CaretRight size={12} weight="bold" className="text-overlay-0" />
          )
        ) : (
          <span className="w-3" />
        )}
        {getIcon()}
        <span className="truncate">{item.name}</span>
        {item.type === 'table' && item.children && (
          <span className="ml-auto text-xs text-overlay-0">
            {item.children.length}
          </span>
        )}
      </button>
      {isExpanded && item.children && (
        <div>
          {item.children.map((child) => (
            <TreeNode key={child.id} item={child} level={level + 1} />
          ))}
        </div>
      )}
    </div>
  )
}

export function Sidebar() {
  const { activeConnectionId, setActiveConnection, setConnecting } = useConnectionStore()
  const [searchQuery, setSearchQuery] = useState('')
  const [showConnectionForm, setShowConnectionForm] = useState(false)
  const [editingConnection, setEditingConnection] = useState<Connection | undefined>()
  const [activeSection, setActiveSection] = useState<'connections' | 'tables'>('connections')

  // Auto-switch to tables when connected
  const effectiveSection = activeConnectionId ? activeSection : 'connections'

  const handleConnect = async (connection: Connection) => {
    setConnecting(true)
    // Simulate connection delay
    await new Promise((resolve) => setTimeout(resolve, 500))
    setActiveConnection(connection.id)
    setConnecting(false)
    setActiveSection('tables')
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
      {/* Search - only show when connected */}
      {activeConnectionId && (
        <div className="p-3">
          <div className="relative">
            <MagnifyingGlass
              size={16}
              weight="bold"
              className="absolute left-3 top-1/2 -translate-y-1/2 text-overlay-0"
            />
            <input
              type="text"
              placeholder="Search tables..."
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              className={cn(
                'w-full pl-9 pr-3 py-2 text-sm rounded-md',
                'bg-surface-0 border border-border',
                'text-text placeholder-overlay-0',
                'focus:outline-none focus:ring-2 focus:ring-primary focus:border-transparent',
                'transition-all'
              )}
            />
          </div>
        </div>
      )}

      {/* Section tabs - only show when connected */}
      {activeConnectionId && (
        <div className="px-3 pb-2 flex gap-1">
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
          <div className="px-3 py-2">
            <div className="flex items-center justify-between mb-2">
              <div className="flex items-center gap-2 text-xs font-semibold text-overlay-1 uppercase tracking-wide">
                <Database size={14} weight="bold" />
                Tables
              </div>
              <button
                className="p-1 rounded hover:bg-surface-0 text-overlay-0 hover:text-text transition-colors"
                title="New Table"
              >
                <Plus size={14} weight="bold" />
              </button>
            </div>

            <div className="space-y-0.5">
              {mockTables.map((table) => (
                <TreeNode key={table.id} item={table} />
              ))}
            </div>
          </div>
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

      {/* Connection Form Modal */}
      <ConnectionForm
        open={showConnectionForm}
        onOpenChange={setShowConnectionForm}
        editConnection={editingConnection}
      />
    </div>
  )
}
