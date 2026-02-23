import { useCallback, useSyncExternalStore } from 'react'

export type ToastVariant = 'default' | 'success' | 'error' | 'loading'

export interface Toast {
  id: string
  title: string
  description?: string
  variant: ToastVariant
  duration?: number
  action?: {
    label: string
    onClick: () => void
  }
}

type ToastInput = Omit<Toast, 'id'>

const TOAST_LIMIT = 5
const DEFAULT_DURATION = 5000

let toasts: Toast[] = []
let listeners: Array<() => void> = []

function emitChange() {
  for (const listener of listeners) {
    listener()
  }
}

function subscribe(listener: () => void) {
  listeners.push(listener)
  return () => {
    listeners = listeners.filter(l => l !== listener)
  }
}

function getSnapshot() {
  return toasts
}

function addToast(input: ToastInput): string {
  const id = crypto.randomUUID()
  const newToast: Toast = { ...input, id }

  toasts = [newToast, ...toasts].slice(0, TOAST_LIMIT)
  emitChange()

  if (input.variant !== 'loading') {
    const duration = input.duration ?? DEFAULT_DURATION
    setTimeout(() => dismissToast(id), duration)
  }

  return id
}

function updateToast(id: string, updates: Partial<ToastInput>) {
  toasts = toasts.map(t => (t.id === id ? { ...t, ...updates } : t))
  emitChange()

  if (updates.variant && updates.variant !== 'loading') {
    const duration = updates.duration ?? DEFAULT_DURATION
    setTimeout(() => dismissToast(id), duration)
  }
}

function dismissToast(id: string) {
  toasts = toasts.filter(t => t.id !== id)
  emitChange()
}

export function toast(input: ToastInput): string {
  return addToast(input)
}

toast.success = (title: string, description?: string) =>
  addToast({ title, description, variant: 'success' })

toast.error = (title: string, description?: string) =>
  addToast({ title, description, variant: 'error', duration: 8000 })

toast.loading = (title: string, description?: string) =>
  addToast({ title, description, variant: 'loading' })

toast.update = updateToast
toast.dismiss = dismissToast

export function useToast() {
  const currentToasts = useSyncExternalStore(subscribe, getSnapshot, getSnapshot)

  const dismiss = useCallback((id: string) => dismissToast(id), [])

  return { toasts: currentToasts, dismiss, toast }
}
