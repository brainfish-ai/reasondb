import * as React from 'react'
import { cn } from '@/lib/utils'

export interface InputProps
  extends React.InputHTMLAttributes<HTMLInputElement> {
  error?: string
}

const Input = React.forwardRef<HTMLInputElement, InputProps>(
  ({ className, type, error, id, ...props }, ref) => {
    const errorId = error && id ? `${id}-error` : undefined

    return (
      <>
        <input
          type={type}
          id={id}
          className={cn(
            'flex h-9 w-full rounded-md border border-border bg-surface-0 px-3 py-2 text-sm text-text',
            'file:border-0 file:bg-transparent file:text-sm file:font-medium',
            'placeholder:text-overlay-0',
            'focus:outline-none focus:ring-1 focus:ring-inset focus:ring-primary focus:border-primary',
            'disabled:cursor-not-allowed disabled:opacity-50',
            'transition-colors',
            error && 'border-red focus:ring-red',
            className
          )}
          ref={ref}
          aria-invalid={error ? true : undefined}
          aria-describedby={errorId}
          {...props}
        />
        {error && (
          <p id={errorId} className="mt-1 text-xs text-red" role="alert">
            {error}
          </p>
        )}
      </>
    )
  }
)
Input.displayName = 'Input'

export { Input }
