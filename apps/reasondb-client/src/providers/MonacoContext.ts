import { createContext, type RefObject } from 'react'
import type * as MonacoTypes from 'monaco-editor'

type Monaco = typeof MonacoTypes

export interface MonacoContextValue {
  monaco: Monaco | null
  editor: MonacoTypes.editor.IStandaloneCodeEditor | null
  isReady: boolean
  attachTo: (containerRef: RefObject<HTMLDivElement | null>, modelId: string) => void
  detach: (modelId: string) => void
  getOrCreateModel: (
    modelId: string,
    content: string,
    language?: string,
  ) => MonacoTypes.editor.ITextModel | null
  updateModelContent: (modelId: string, content: string) => void
  disposeModel: (modelId: string) => void
  setEditorOptions: (options: MonacoTypes.editor.IEditorOptions) => void
}

export const MonacoContext = createContext<MonacoContextValue | null>(null)
