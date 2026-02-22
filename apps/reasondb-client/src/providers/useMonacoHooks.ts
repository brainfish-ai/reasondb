import { useContext, useRef, useEffect } from 'react'
import type * as MonacoTypes from 'monaco-editor'
import { RQL_LANGUAGE_ID } from '@/lib/rql-language'
import { MonacoContext } from './MonacoContext'

export function useMonaco() {
  const ctx = useContext(MonacoContext)
  if (!ctx) throw new Error('useMonaco must be used within MonacoProvider')
  return ctx
}

export interface UseMonacoEditorOptions {
  modelId: string
  initialContent: string
  language?: string
  onContentChange?: (value: string) => void
}

export function useMonacoEditor({
  modelId,
  initialContent,
  language = RQL_LANGUAGE_ID,
  onContentChange,
}: UseMonacoEditorOptions) {
  const { editor, monaco, isReady, attachTo, detach, getOrCreateModel } = useMonaco()
  const containerRef = useRef<HTMLDivElement | null>(null)
  const onChangeRef = useRef(onContentChange)
  const disposableRef = useRef<MonacoTypes.IDisposable | null>(null)

  useEffect(() => {
    onChangeRef.current = onContentChange
  })

  useEffect(() => {
    if (!isReady) return

    const model = getOrCreateModel(modelId, initialContent, language)
    if (!model) return

    attachTo(containerRef, modelId)

    disposableRef.current?.dispose()
    disposableRef.current = model.onDidChangeContent(() => {
      onChangeRef.current?.(model.getValue())
    })

    return () => {
      disposableRef.current?.dispose()
      disposableRef.current = null
      detach(modelId)
    }
  }, [isReady, modelId, language, attachTo, detach, getOrCreateModel, initialContent])

  return { containerRef, editor, monaco, isReady }
}
