import { useMutation } from '@tanstack/react-query'
import { ScanSearch } from 'lucide-react'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { api } from '@/lib/api'
import { useT } from '@/lib/i18n'

/** Asks the master to compare the file store with its records now (administrators). */
export function RescanButton({ variant = 'outline' }: { variant?: 'outline' | 'default' }) {
  const t = useT()
  const rescan = useMutation({
    mutationFn: () => api.post<{ started: boolean }>('/api/files/rescan'),
    onSuccess: (r) => toast.success(r.started ? t('Scanning the file store. New files show up as they are found.') : t('A scan is already running.')),
    onError: (e) => toast.error(e.message),
  })
  return (
    <Button variant={variant} disabled={rescan.isPending} onClick={() => rescan.mutate()}>
      <ScanSearch className="size-4" />{t('Scan now')}
    </Button>
  )
}
