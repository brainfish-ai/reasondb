import {
  useRef,
  useEffect,
  useCallback,
  useState,
  type ReactNode,
  type RefObject,
} from 'react'
import { loader } from '@monaco-editor/react'
import type * as MonacoTypes from 'monaco-editor'
import { registerRqlLanguage, RQL_LANGUAGE_ID } from '@/lib/rql-language'
import { ensureTheme, THEME_NAME } from '@/lib/monaco-theme'
import { trackMonacoMount } from '@/hooks/useMemoryDiagnostics'
import { MonacoContext, type MonacoContextValue } from './MonacoContext'

type Monaco = typeof MonacoTypes

interface ModelEntry {
  model: MonacoTypes.editor.ITextModel
  viewState: MonacoTypes.editor.ICodeEditorViewState | null
  language: string
}

const EDITOR_OPTIONS: MonacoTypes.editor.IStandaloneEditorConstructionOptions = {
  minimap: { enabled: false },
  fontSize: 14,
  fontFamily: "'JetBrains Mono', 'Fira Code', 'SF Mono', Consolas, monospace",
  lineNumbers: 'on',
  renderLineHighlight: 'all',
  scrollBeyondLastLine: false,
  wordWrap: 'on',
  automaticLayout: true,
  tabSize: 2,
  padding: { top: 12, bottom: 12 },
  suggestOnTriggerCharacters: true,
  quickSuggestions: true,
  folding: true,
  bracketPairColorization: { enabled: true },
  guides: { bracketPairs: true, indentation: true },
  scrollbar: { verticalScrollbarSize: 8, horizontalScrollbarSize: 8 },
}

export function MonacoProvider({ children }: { children: ReactNode }) {
  const [monacoInstance, setMonacoInstance] = useState<Monaco | null>(null)
  const [editorInstance, setEditorInstance] = useState<MonacoTypes.editor.IStandaloneCodeEditor | null>(null)
  const [isReady, setIsReady] = useState(false)

  const modelsRef = useRef<Map<string, ModelEntry>>(new Map())
  const activeModelIdRef = useRef<string | null>(null)
  // The wrapper div is what Monaco is created inside. We move this entire
  // div between containers so that automaticLayout's ResizeObserver stays
  // attached to the correct element.
  const editorWrapperRef = useRef<HTMLDivElement | null>(null)

  useEffect(() => {
    let disposed = false

    loader.config({
      paths: { vs: 'https://cdn.jsdelivr.net/npm/monaco-editor@0.55.1/min/vs' },
    })

    // Create a wrapper div that will house the Monaco editor.
    // It fills whatever container it's placed into.
    const wrapper = document.createElement('div')
    wrapper.style.width = '100%'
    wrapper.style.height = '100%'
    wrapper.style.position = 'relative'
    editorWrapperRef.current = wrapper

    loader.init().then((monaco) => {
      if (disposed) return

      ensureTheme(monaco)
      registerRqlLanguage(monaco)

      const editor = monaco.editor.create(wrapper, {
        ...EDITOR_OPTIONS,
        theme: 'rql-catppuccin',
      })

      trackMonacoMount('singleton')

      setMonacoInstance(monaco)
      setEditorInstance(editor)
      setIsReady(true)
    })

    return () => {
      disposed = true
    }
  }, [])

  useEffect(() => {
    if (!editorInstance) return
    const models = modelsRef.current
    return () => {
      editorInstance.dispose()
      for (const entry of models.values()) {
        entry.model.dispose()
      }
      models.clear()
    }
  }, [editorInstance])

  const getOrCreateModel = useCallback(
    (modelId: string, content: string, language = RQL_LANGUAGE_ID) => {
      if (!monacoInstance) return null

      const existing = modelsRef.current.get(modelId)
      if (existing) return existing.model

      const model = monacoInstance.editor.createModel(content, language)
      modelsRef.current.set(modelId, { model, viewState: null, language })
      return model
    },
    [monacoInstance],
  )

  const updateModelContent = useCallback((modelId: string, content: string) => {
    const entry = modelsRef.current.get(modelId)
    if (entry && entry.model.getValue() !== content) {
      entry.model.setValue(content)
    }
  }, [])

  const disposeModel = useCallback((modelId: string) => {
    const entry = modelsRef.current.get(modelId)
    if (entry) {
      entry.model.dispose()
      modelsRef.current.delete(modelId)
    }
  }, [])

  const attachTo = useCallback(
    (containerRef: RefObject<HTMLDivElement | null>, modelId: string) => {
      if (!editorInstance) return
      const container = containerRef.current
      const wrapper = editorWrapperRef.current
      if (!container || !wrapper) return

      const entry = modelsRef.current.get(modelId)
      if (!entry) return

      // Save view state of the model we're leaving
      if (activeModelIdRef.current && activeModelIdRef.current !== modelId) {
        const prevEntry = modelsRef.current.get(activeModelIdRef.current)
        if (prevEntry) {
          prevEntry.viewState = editorInstance.saveViewState()
        }
      }

      // Move the wrapper (with Monaco inside) into the target container
      if (wrapper.parentElement !== container) {
        container.appendChild(wrapper)
      }

      editorInstance.setModel(entry.model)

      if (entry.viewState) {
        editorInstance.restoreViewState(entry.viewState)
      }

      if (monacoInstance) {
        const theme = entry.language === RQL_LANGUAGE_ID ? 'rql-catppuccin' : THEME_NAME
        monacoInstance.editor.setTheme(theme)
      }

      activeModelIdRef.current = modelId

      editorInstance.layout()
      editorInstance.focus()
    },
    [editorInstance, monacoInstance],
  )

  const detach = useCallback((modelId: string) => {
    if (!editorInstance) return

    if (activeModelIdRef.current === modelId) {
      const entry = modelsRef.current.get(modelId)
      if (entry) {
        entry.viewState = editorInstance.saveViewState()
      }

      // Remove wrapper from the container (it becomes detached from the DOM
      // until the next attachTo call)
      const wrapper = editorWrapperRef.current
      if (wrapper?.parentElement) {
        wrapper.parentElement.removeChild(wrapper)
      }
      activeModelIdRef.current = null
    }
  }, [editorInstance])

  const setEditorOptions = useCallback(
    (options: MonacoTypes.editor.IEditorOptions) => {
      editorInstance?.updateOptions(options)
    },
    [editorInstance],
  )

  const value: MonacoContextValue = {
    monaco: monacoInstance,
    editor: editorInstance,
    isReady,
    attachTo,
    detach,
    getOrCreateModel,
    updateModelContent,
    disposeModel,
    setEditorOptions,
  }

  return (
    <MonacoContext.Provider value={value}>
      {children}
    </MonacoContext.Provider>
  )
}
