import { useMutation, useQueryClient } from '@tanstack/react-query'
import { toast } from 'sonner'

/**
 * A request made when somebody clicks: it says that it worked (`success`), or why it did not (a toast; a dialog that shows the error
 * itself, from the result, sets `inline`), and has the pages that show what it changed ask again (`invalidate`, the query keys).
 */
export function useApiMutation<V = void, R = unknown>(
  request: (v: V) => Promise<R>,
  options: {
    invalidate?: string[][]
    success?: string | ((result: R, v: V) => string)
    onSuccess?: (result: R, v: V) => void
    /** The caller shows the error itself (a form), so no toast. */
    inline?: boolean
    /** Instead of the toast, when something else must happen on a failure. */
    onError?: (error: Error) => void
  } = {},
) {
  const client = useQueryClient()
  const { invalidate = [], success, onSuccess, inline = false, onError } = options
  return useMutation<R, Error, V>({
    mutationFn: request,
    onSuccess: (result, v) => {
      const message = typeof success === 'function' ? success(result, v) : success
      if (message) toast.success(message)
      for (const queryKey of invalidate) void client.invalidateQueries({ queryKey })
      onSuccess?.(result, v)
    },
    onError: onError ?? (inline ? undefined : (e) => toast.error(e.message)),
  })
}
