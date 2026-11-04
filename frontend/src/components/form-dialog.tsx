import type { FormEvent, ReactNode } from 'react'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle, DialogTrigger } from '@/components/ui/dialog'
import { cn } from '@/lib/utils'

/**
 * A dialog with a form: a title and a few words, the fields (`children`), the reason an attempt failed, and one button that sends
 * it. Every dialog that asks for something to be typed is this; the caller keeps the fields and the request.
 * `trigger` is the button that opens it (for a dialog that opens itself, leave it out and control `open`).
 */
export function FormDialog({
  open,
  onOpenChange,
  trigger,
  title,
  description,
  onSubmit,
  submit,
  disabled = false,
  pending = false,
  error,
  footer,
  className,
  children,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  trigger?: ReactNode
  title: string
  description?: ReactNode
  onSubmit: () => void
  /** The words on the button that sends it. */
  submit: string
  /** Why it cannot be sent yet. */
  disabled?: boolean
  pending?: boolean
  /** What went wrong the last time, in words for the person. */
  error?: string | null
  /** Buttons at the start of the footer, away from the one that sends. */
  footer?: ReactNode
  className?: string
  children?: ReactNode
}) {
  const send = (e: FormEvent) => {
    e.preventDefault()
    if (!disabled && !pending) onSubmit()
  }
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      {trigger && <DialogTrigger asChild>{trigger}</DialogTrigger>}
      <DialogContent className={className}>
        <form onSubmit={send} className="space-y-4">
          <DialogHeader>
            <DialogTitle>{title}</DialogTitle>
            {description && <DialogDescription>{description}</DialogDescription>}
          </DialogHeader>
          {children}
          {error && <p role="alert" className="text-sm text-destructive">{error}</p>}
          <DialogFooter className={cn(footer && 'items-center gap-2 sm:justify-between')}>
            {footer}
            <Button type="submit" disabled={disabled || pending}>{submit}</Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
