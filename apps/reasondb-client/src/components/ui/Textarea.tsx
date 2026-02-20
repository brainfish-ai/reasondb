import * as React from 'react'
import { cn } from '@/lib/utils'

export interface TextareaProps
  extends React.TextareaHTMLAttributes<HTMLTextAreaElement> {
  error?: string
}

const Textarea = React.forwardRef<HTMLTextAreaElement, TextareaProps>(
  ({ className, error, ...props }, ref) => {
    return (
      <>
        <textarea
          className={cn(
            'flex min-h-[80px] w-full rounded-md border border-border bg-surface-0 px-3 py-2 text-sm text-text',
            'placeholder:text-overlay-0',
            'focus:outline-none focus:ring-1 focus:ring-inset focus:ring-primary focus:border-primary',
            'disabled:cursor-not-allowed disabled:opacity-50',
            'resize-y transition-colors',
            error && 'border-red focus:ring-red',
            className
          )}
          ref={ref}
          {...props}
        />
        {error && (
          <p className="mt-1 text-xs text-red">{error}</p>
        )}
      </>
    )
  }
)
Textarea.displayName = 'Textarea'

export { Textarea }
