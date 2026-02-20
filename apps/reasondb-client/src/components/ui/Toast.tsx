import * as React from 'react'
import * as ToastPrimitive from '@radix-ui/react-toast'
import { X, CheckCircle, WarningCircle, SpinnerGap } from '@phosphor-icons/react'
import { cn } from '@/lib/utils'
import { useToast, type ToastVariant } from '@/hooks/useToast'

const variantIcon: Record<ToastVariant, React.ReactNode> = {
  default: null,
  success: <CheckCircle size={18} weight="fill" className="text-green shrink-0" />,
  error: <WarningCircle size={18} weight="fill" className="text-red shrink-0" />,
  loading: <SpinnerGap size={18} weight="bold" className="text-mauve shrink-0 animate-spin" />,
}

export function ToastProvider({ children }: { children: React.ReactNode }) {
  const { toasts, dismiss } = useToast()

  return (
    <ToastPrimitive.Provider swipeDirection="right" duration={Infinity}>
      {children}

      {toasts.map((t) => (
        <ToastPrimitive.Root
          key={t.id}
          open
          onOpenChange={(open) => { if (!open) dismiss(t.id) }}
          className={cn(
            'group pointer-events-auto relative flex items-start gap-3 overflow-hidden',
            'rounded-lg border border-border bg-mantle p-3 pr-8 shadow-lg',
            'data-[state=open]:animate-in data-[state=closed]:animate-out',
            'data-[state=closed]:fade-out-80 data-[state=open]:fade-in-0',
            'data-[state=closed]:slide-out-to-right-full data-[state=open]:slide-in-from-right-full',
            'data-[swipe=move]:translate-x-(--radix-toast-swipe-move-x)',
            'data-[swipe=cancel]:translate-x-0 data-[swipe=cancel]:transition-transform',
            'data-[swipe=end]:translate-x-(--radix-toast-swipe-end-x)',
          )}
        >
          {variantIcon[t.variant]}
          <div className="flex-1 min-w-0">
            <ToastPrimitive.Title className="text-sm font-medium text-text leading-tight">
              {t.title}
            </ToastPrimitive.Title>
            {t.description && (
              <ToastPrimitive.Description className="mt-0.5 text-xs text-subtext-0 leading-relaxed">
                {t.description}
              </ToastPrimitive.Description>
            )}
            {t.action && (
              <ToastPrimitive.Action altText={t.action.label} asChild>
                <button
                  onClick={t.action.onClick}
                  className="mt-1.5 text-xs font-medium text-mauve hover:text-mauve/80 underline underline-offset-2"
                >
                  {t.action.label}
                </button>
              </ToastPrimitive.Action>
            )}
          </div>
          <ToastPrimitive.Close className="absolute right-2 top-2 rounded-sm p-0.5 text-overlay-0 opacity-0 transition-opacity group-hover:opacity-100 hover:text-text">
            <X size={14} />
          </ToastPrimitive.Close>
        </ToastPrimitive.Root>
      ))}

      <ToastPrimitive.Viewport className="fixed bottom-4 right-4 z-100 flex flex-col gap-2 w-80 max-h-screen outline-none" />
    </ToastPrimitive.Provider>
  )
}
