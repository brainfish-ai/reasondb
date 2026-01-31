import { useMemo, useEffect } from 'react'
import { useFilterStore } from '@/stores/filterStore'
import { detectColumnType, type ColumnInfo } from '@/lib/filter-types'
import type { Document } from '@/stores/tableStore'

/**
 * Extract nested keys from an object with dot notation
 */
function extractNestedKeys(
  obj: Record<string, unknown>,
  prefix: string,
  keys: Set<string>
) {
  Object.entries(obj).forEach(([key, value]) => {
    const path = prefix ? `${prefix}.${key}` : key
    keys.add(path)
    if (typeof value === 'object' && value !== null && !Array.isArray(value)) {
      extractNestedKeys(value as Record<string, unknown>, path, keys)
    }
  })
}

/**
 * Get a nested value from an object by dot-notation path
 */
function getNestedValue(obj: Record<string, unknown>, path: string): unknown {
  const parts = path.split('.')
  let current: unknown = obj
  for (const part of parts) {
    if (current && typeof current === 'object') {
      current = (current as Record<string, unknown>)[part]
    } else {
      return undefined
    }
  }
  return current
}

/**
 * Hook for detecting columns from documents
 */
export function useColumnDetection(documents: Document[]) {
  const { setDetectedColumns } = useFilterStore()

  const detectedColumns = useMemo<ColumnInfo[]>(() => {
    if (documents.length === 0) return []

    // Standard columns
    const cols: ColumnInfo[] = [
      { name: 'id', type: 'text', path: 'data.id' },
      { name: 'title', type: 'text', path: 'data.title' },
      { name: 'total_nodes', type: 'number', path: 'data.total_nodes' },
      { name: 'tags', type: 'array', path: 'data.tags' },
      { name: 'created_at', type: 'date', path: 'data.created_at' },
    ]

    // Extract metadata columns from all documents
    const metadataKeys = new Set<string>()
    documents.forEach(doc => {
      const metadata = doc.data.metadata as Record<string, unknown> | undefined
      if (metadata) {
        extractNestedKeys(metadata, 'metadata', metadataKeys)
      }
    })

    // Add metadata columns with detected types
    metadataKeys.forEach(key => {
      // Get sample value to detect type
      let sampleValue: unknown = undefined
      for (const doc of documents) {
        const metadata = doc.data.metadata as Record<string, unknown> | undefined
        if (metadata) {
          const keyWithoutPrefix = key.replace('metadata.', '')
          sampleValue = getNestedValue(metadata, keyWithoutPrefix)
          if (sampleValue !== undefined) break
        }
      }

      cols.push({
        name: key,
        type: detectColumnType(sampleValue),
        path: `data.${key}`,
      })
    })

    return cols
  }, [documents])

  // Update filter store with detected columns
  useEffect(() => {
    setDetectedColumns(detectedColumns)
  }, [detectedColumns, setDetectedColumns])

  return detectedColumns
}
