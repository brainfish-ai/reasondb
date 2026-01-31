import { describe, it, expect, beforeEach } from 'vitest'
import { detectContext, getSchema, updateTableMetadataFields } from './sql-completion'
import { useSchemaStore } from '@/stores/schemaStore'

describe('SQL Completion Engine', () => {
  beforeEach(() => {
    // Reset and set up test schema using store
    useSchemaStore.getState().reset()
    useSchemaStore.getState().setTables([
      {
        id: 'tbl_users',
        name: 'users',
        columns: [
          { name: 'id', type: 'uuid' },
          { name: 'name', type: 'text' },
          { name: 'email', type: 'text' },
          { name: 'created_at', type: 'timestamp' },
        ],
      },
      {
        id: 'tbl_posts',
        name: 'posts',
        columns: [
          { name: 'id', type: 'uuid' },
          { name: 'title', type: 'text' },
          { name: 'content', type: 'text' },
          { name: 'user_id', type: 'uuid' },
        ],
      },
    ])
  })

  describe('detectContext', () => {
    describe('keyword context', () => {
      it('should detect keyword at start', () => {
        const result = detectContext('', 0)
        expect(result.context).toBe('keyword')
      })

      it('should detect keyword after SELECT columns (needs FROM)', () => {
        const result = detectContext('SELECT * ', 9)
        expect(result.context).toBe('keyword')
      })

      it('should detect keyword after FROM table', () => {
        const result = detectContext('SELECT * FROM users ', 20)
        expect(result.context).toBe('keyword')
      })
    })

    describe('table context', () => {
      it('should detect table after FROM', () => {
        const result = detectContext('SELECT * FROM ', 14)
        expect(result.context).toBe('table')
      })

      it('should detect table after JOIN', () => {
        const result = detectContext('SELECT * FROM users JOIN ', 25)
        expect(result.context).toBe('table')
      })

      it('should detect table after UPDATE', () => {
        const result = detectContext('UPDATE ', 7)
        expect(result.context).toBe('table')
      })

      it('should detect table after INSERT INTO', () => {
        const result = detectContext('INSERT INTO ', 12)
        expect(result.context).toBe('table')
      })

      it('should detect table when typing partial table name after FROM', () => {
        // User is typing "use" after FROM - should suggest tables starting with "use"
        const result = detectContext('SELECT * FROM use', 17)
        expect(result.context).toBe('table')
        expect(result.prefix).toBe('use')
      })

      it('should detect table when typing partial table name after JOIN', () => {
        const result = detectContext('SELECT * FROM users JOIN pos', 28)
        expect(result.context).toBe('table')
        expect(result.prefix).toBe('pos')
      })
    })

    describe('column context', () => {
      it('should detect column right after SELECT', () => {
        const result = detectContext('SELECT ', 7)
        expect(result.context).toBe('column')
      })

      it('should detect column after comma in SELECT', () => {
        const result = detectContext('SELECT id, ', 11)
        expect(result.context).toBe('column')
      })

      it('should detect column after WHERE', () => {
        const result = detectContext('SELECT * FROM users WHERE ', 26)
        expect(result.context).toBe('column')
      })

      it('should detect column after AND', () => {
        const result = detectContext('SELECT * FROM users WHERE id = 1 AND ', 37)
        expect(result.context).toBe('column')
      })

      it('should detect column after ORDER BY', () => {
        const result = detectContext('SELECT * FROM users ORDER BY ', 29)
        expect(result.context).toBe('column')
      })

      it('should detect column after GROUP BY', () => {
        const result = detectContext('SELECT * FROM users GROUP BY ', 29)
        expect(result.context).toBe('column')
      })

      it('should detect column when typing partial column name after WHERE', () => {
        // User is typing "kno" after WHERE - should suggest columns starting with "kno"
        const result = detectContext('SELECT * FROM knowledge WHERE kno', 33)
        expect(result.context).toBe('column')
        expect(result.prefix).toBe('kno')
      })

      it('should detect column when typing partial column name after AND', () => {
        const result = detectContext('SELECT * FROM users WHERE id = 1 AND nam', 40)
        expect(result.context).toBe('column')
        expect(result.prefix).toBe('nam')
      })

      it('should detect column when typing partial column name after ORDER BY', () => {
        const result = detectContext('SELECT * FROM users ORDER BY cre', 32)
        expect(result.context).toBe('column')
        expect(result.prefix).toBe('cre')
      })
    })

    describe('operator context', () => {
      it('should detect operator after column in WHERE', () => {
        const result = detectContext('SELECT * FROM users WHERE id ', 29)
        expect(result.context).toBe('operator')
      })

      it('should detect operator after table.column', () => {
        const result = detectContext('SELECT * FROM users WHERE users.id ', 35)
        expect(result.context).toBe('operator')
      })

      it('should detect operator after AND column', () => {
        const result = detectContext('SELECT * FROM users WHERE id = 1 AND name ', 42)
        expect(result.context).toBe('operator')
      })
    })

    describe('alias support', () => {
      it('should detect column after alias dot', () => {
        const result = detectContext('SELECT * FROM users u WHERE u.', 30)
        expect(result.context).toBe('column')
        expect(result.tableName).toBe('users')
      })

      it('should detect column after table dot', () => {
        const result = detectContext('SELECT * FROM users WHERE users.', 32)
        expect(result.context).toBe('column')
        expect(result.tableName).toBe('users')
      })
    })

    describe('case insensitivity', () => {
      it('should handle lowercase', () => {
        const result = detectContext('select * from ', 14)
        expect(result.context).toBe('table')
      })

      it('should handle mixed case', () => {
        const result = detectContext('Select * From ', 14)
        expect(result.context).toBe('table')
      })
    })
  })

  describe('schema management', () => {
    it('should store and retrieve schema', () => {
      const schema = getSchema()
      expect(schema.tables).toHaveLength(2)
      expect(schema.tables[0].name).toBe('users')
      expect(schema.tables[1].name).toBe('posts')
    })
  })

  describe('metadata field extraction', () => {
    it('should extract top-level metadata fields', () => {
      // Reset schema with a table using store
      useSchemaStore.getState().reset()
      useSchemaStore.getState().setTables([{
        id: 'tbl_documents',
        name: 'documents',
        columns: [
          { name: 'id', type: 'uuid' },
          { name: 'title', type: 'text' },
          { name: 'metadata', type: 'jsonb' },
        ],
      }])
      
      // Add metadata fields from documents
      updateTableMetadataFields('documents', [
        { metadata: { author: 'John', category: 'tech' } },
        { metadata: { author: 'Jane', priority: 'high' } },
      ])
      
      const schema = getSchema()
      const docTable = schema.tables.find(t => t.name === 'documents')
      expect(docTable).toBeDefined()
      const columnNames = docTable!.columns.map(c => c.name)
      
      expect(columnNames).toContain('metadata.author')
      expect(columnNames).toContain('metadata.category')
      expect(columnNames).toContain('metadata.priority')
    })

    it('should extract nested metadata fields', () => {
      useSchemaStore.getState().reset()
      useSchemaStore.getState().setTables([{
        id: 'tbl_articles',
        name: 'articles',
        columns: [
          { name: 'id', type: 'uuid' },
          { name: 'metadata', type: 'jsonb' },
        ],
      }])
      
      updateTableMetadataFields('articles', [
        { 
          metadata: { 
            author: { name: 'John', email: 'john@example.com' },
            source: { url: 'https://example.com', type: 'web' },
          } 
        },
      ])
      
      const schema = getSchema()
      const articleTable = schema.tables.find(t => t.name === 'articles')
      expect(articleTable).toBeDefined()
      const columnNames = articleTable!.columns.map(c => c.name)
      
      expect(columnNames).toContain('metadata.author')
      expect(columnNames).toContain('metadata.author.name')
      expect(columnNames).toContain('metadata.author.email')
      expect(columnNames).toContain('metadata.source')
      expect(columnNames).toContain('metadata.source.url')
      expect(columnNames).toContain('metadata.source.type')
    })
  })
})
