import { describe, it, expect, beforeEach } from 'vitest'
import { useConnectionStore } from './connectionStore'

describe('connectionStore', () => {
  beforeEach(() => {
    // Reset store before each test
    useConnectionStore.getState().reset()
  })

  describe('addConnection', () => {
    it('should add a new connection with generated id and createdAt', () => {
      const { addConnection } = useConnectionStore.getState()
      
      addConnection({
        name: 'Test Connection',
        host: 'localhost',
        port: 8080,
        ssl: false,
      })

      const state = useConnectionStore.getState()
      expect(state.connections).toHaveLength(1)
      expect(state.connections[0].name).toBe('Test Connection')
      expect(state.connections[0].host).toBe('localhost')
      expect(state.connections[0].port).toBe(8080)
      expect(state.connections[0].id).toBeDefined()
      expect(state.connections[0].createdAt).toBeDefined()
    })

    it('should add multiple connections', () => {
      const { addConnection } = useConnectionStore.getState()

      addConnection({ name: 'Connection 1', host: 'host1', port: 8080, ssl: false })
      addConnection({ name: 'Connection 2', host: 'host2', port: 8081, ssl: true })

      const state = useConnectionStore.getState()
      expect(state.connections).toHaveLength(2)
      expect(state.connections[0].name).toBe('Connection 1')
      expect(state.connections[1].name).toBe('Connection 2')
    })

    it('should preserve optional fields', () => {
      const { addConnection } = useConnectionStore.getState()

      addConnection({
        name: 'Full Connection',
        host: 'example.com',
        port: 443,
        ssl: true,
        apiKey: 'secret-key',
        color: '#ff0000',
        group: 'Production',
      })

      const state = useConnectionStore.getState()
      expect(state.connections[0].apiKey).toBe('secret-key')
      expect(state.connections[0].color).toBe('#ff0000')
      expect(state.connections[0].group).toBe('Production')
    })
  })

  describe('updateConnection', () => {
    it('should update an existing connection', () => {
      const { addConnection } = useConnectionStore.getState()
      
      addConnection({ name: 'Original', host: 'localhost', port: 8080, ssl: false })
      
      const state = useConnectionStore.getState()
      const connectionId = state.connections[0].id

      useConnectionStore.getState().updateConnection(connectionId, {
        name: 'Updated',
        port: 9090,
      })

      const updatedState = useConnectionStore.getState()
      expect(updatedState.connections[0].name).toBe('Updated')
      expect(updatedState.connections[0].port).toBe(9090)
      expect(updatedState.connections[0].host).toBe('localhost') // Unchanged
    })

    it('should not affect other connections', () => {
      const { addConnection } = useConnectionStore.getState()

      addConnection({ name: 'Connection 1', host: 'host1', port: 8080, ssl: false })
      addConnection({ name: 'Connection 2', host: 'host2', port: 8081, ssl: false })

      const state = useConnectionStore.getState()
      const firstId = state.connections[0].id

      useConnectionStore.getState().updateConnection(firstId, { name: 'Updated 1' })

      const updatedState = useConnectionStore.getState()
      expect(updatedState.connections[0].name).toBe('Updated 1')
      expect(updatedState.connections[1].name).toBe('Connection 2')
    })
  })

  describe('deleteConnection', () => {
    it('should remove a connection', () => {
      const { addConnection } = useConnectionStore.getState()

      addConnection({ name: 'To Delete', host: 'localhost', port: 8080, ssl: false })
      
      const state = useConnectionStore.getState()
      expect(state.connections).toHaveLength(1)

      const connectionId = state.connections[0].id
      useConnectionStore.getState().deleteConnection(connectionId)

      const updatedState = useConnectionStore.getState()
      expect(updatedState.connections).toHaveLength(0)
    })

    it('should clear activeConnectionId if deleted connection was active', () => {
      const { addConnection } = useConnectionStore.getState()

      addConnection({ name: 'Active', host: 'localhost', port: 8080, ssl: false })
      
      const state = useConnectionStore.getState()
      const connectionId = state.connections[0].id

      useConnectionStore.getState().setActiveConnection(connectionId)
      expect(useConnectionStore.getState().activeConnectionId).toBe(connectionId)

      useConnectionStore.getState().deleteConnection(connectionId)
      expect(useConnectionStore.getState().activeConnectionId).toBeNull()
    })

    it('should not clear activeConnectionId if different connection is deleted', () => {
      const { addConnection } = useConnectionStore.getState()

      addConnection({ name: 'Active', host: 'localhost', port: 8080, ssl: false })
      addConnection({ name: 'Other', host: 'localhost', port: 8081, ssl: false })

      const state = useConnectionStore.getState()
      const activeId = state.connections[0].id
      const otherId = state.connections[1].id

      useConnectionStore.getState().setActiveConnection(activeId)
      useConnectionStore.getState().deleteConnection(otherId)

      expect(useConnectionStore.getState().activeConnectionId).toBe(activeId)
    })
  })

  describe('setActiveConnection', () => {
    it('should set the active connection', () => {
      const { addConnection } = useConnectionStore.getState()

      addConnection({ name: 'Test', host: 'localhost', port: 8080, ssl: false })
      
      const state = useConnectionStore.getState()
      const connectionId = state.connections[0].id

      useConnectionStore.getState().setActiveConnection(connectionId)
      expect(useConnectionStore.getState().activeConnectionId).toBe(connectionId)
    })

    it('should update lastUsedAt when setting active connection', () => {
      const { addConnection } = useConnectionStore.getState()

      addConnection({ name: 'Test', host: 'localhost', port: 8080, ssl: false })
      
      const state = useConnectionStore.getState()
      const connectionId = state.connections[0].id
      const originalLastUsed = state.connections[0].lastUsedAt

      useConnectionStore.getState().setActiveConnection(connectionId)
      
      const updatedState = useConnectionStore.getState()
      expect(updatedState.connections[0].lastUsedAt).toBeDefined()
      expect(updatedState.connections[0].lastUsedAt).not.toBe(originalLastUsed)
    })

    it('should allow clearing active connection', () => {
      const { addConnection } = useConnectionStore.getState()

      addConnection({ name: 'Test', host: 'localhost', port: 8080, ssl: false })
      
      const state = useConnectionStore.getState()
      const connectionId = state.connections[0].id

      useConnectionStore.getState().setActiveConnection(connectionId)
      expect(useConnectionStore.getState().activeConnectionId).toBe(connectionId)

      useConnectionStore.getState().setActiveConnection(null)
      expect(useConnectionStore.getState().activeConnectionId).toBeNull()
    })
  })

  describe('setConnecting', () => {
    it('should set isConnecting state', () => {
      expect(useConnectionStore.getState().isConnecting).toBe(false)

      useConnectionStore.getState().setConnecting(true)
      expect(useConnectionStore.getState().isConnecting).toBe(true)

      useConnectionStore.getState().setConnecting(false)
      expect(useConnectionStore.getState().isConnecting).toBe(false)
    })
  })

  describe('setConnectionError', () => {
    it('should set connection error', () => {
      expect(useConnectionStore.getState().connectionError).toBeNull()

      useConnectionStore.getState().setConnectionError('Connection refused')
      expect(useConnectionStore.getState().connectionError).toBe('Connection refused')

      useConnectionStore.getState().setConnectionError(null)
      expect(useConnectionStore.getState().connectionError).toBeNull()
    })
  })

  describe('getConnection', () => {
    it('should return connection by id', () => {
      const { addConnection } = useConnectionStore.getState()

      addConnection({ name: 'Find Me', host: 'localhost', port: 8080, ssl: false })
      
      const state = useConnectionStore.getState()
      const connectionId = state.connections[0].id

      const found = useConnectionStore.getState().getConnection(connectionId)
      expect(found).toBeDefined()
      expect(found?.name).toBe('Find Me')
    })

    it('should return undefined for non-existent id', () => {
      const found = useConnectionStore.getState().getConnection('non-existent-id')
      expect(found).toBeUndefined()
    })
  })

  describe('reset', () => {
    it('should reset store to initial state', () => {
      const { addConnection, setActiveConnection, setConnecting, setConnectionError } =
        useConnectionStore.getState()

      addConnection({ name: 'Test', host: 'localhost', port: 8080, ssl: false })
      const connectionId = useConnectionStore.getState().connections[0].id
      setActiveConnection(connectionId)
      setConnecting(true)
      setConnectionError('Error')

      useConnectionStore.getState().reset()

      const state = useConnectionStore.getState()
      expect(state.connections).toHaveLength(0)
      expect(state.activeConnectionId).toBeNull()
      expect(state.isConnecting).toBe(false)
      expect(state.connectionError).toBeNull()
    })
  })
})
