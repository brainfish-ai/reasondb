import { create } from 'zustand'
import type { LlmTestResult } from '../lib/api'

export interface LlmHealthState {
  testResult: LlmTestResult | null
  testing: boolean
  hasError: boolean

  setTestResult: (result: LlmTestResult | null) => void
  setTesting: (testing: boolean) => void
}

export const useLlmHealthStore = create<LlmHealthState>((set) => ({
  testResult: null,
  testing: false,
  hasError: false,

  setTestResult: (result) =>
    set({
      testResult: result,
      hasError: result
        ? !result.ingestion.ok || !result.retrieval.ok
        : false,
    }),

  setTesting: (testing) => set({ testing }),
}))
