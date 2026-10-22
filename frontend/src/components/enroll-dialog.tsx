import { Check, Copy } from 'lucide-react'
import { useState } from 'react'
import { Button } from './ui/button'
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from './ui/dialog'
import { Input } from './ui/input'
import { Label } from './ui/label'
import { useT } from '@/lib/i18n'

/** Where devices reach the master: the gRPC port on this host, or this host itself behind an https proxy. */
function guessServer(): string {
  const { protocol, hostname, host } = window.location
  return protocol === 'https:' ? `https://${host}` : `http://${hostname}:7443`
}

function CopyBox({ text }: { text: string }) {
  const t = useT()
  const [copied, setCopied] = useState(false)
  return (
    <div className="relative">
      <pre className="overflow-x-auto rounded-md border bg-muted p-3 pr-12 text-xs leading-relaxed whitespace-pre-wrap break-all">{text}</pre>
      <Button
        variant="ghost"
        size="icon"
        className="absolute top-1.5 right-1.5 size-8"
        aria-label={t('Copy')}
        onClick={() => {
          void navigator.clipboard.writeText(text).then(() => {
            setCopied(true)
            window.setTimeout(() => setCopied(false), 1500)
          })
        }}
      >
        {copied ? <Check className="size-4" /> : <Copy className="size-4" />}
      </Button>
    </div>
  )
}

/** Shown once after a device is created or its token rotated: the token and the command that uses it. */
export function EnrollDialog({ name, token, onClose }: { name: string; token: string | null; onClose: () => void }) {
  const t = useT()
  const [server, setServer] = useState(guessServer)
  return (
    <Dialog open={token !== null} onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="sm:max-w-xl">
        <DialogHeader>
          <DialogTitle>{t('Connect {name}', { name })}</DialogTitle>
          <DialogDescription>{t('The token is shown only now. Run the command on the device and paste the token when it asks; it is stored in the system keychain.')}</DialogDescription>
        </DialogHeader>
        <div className="space-y-4">
          <div className="space-y-2">
            <Label htmlFor="server">{t('Master address as the device reaches it')}</Label>
            <Input id="server" value={server} onChange={(e) => setServer(e.target.value)} />
            <p className="text-xs text-muted-foreground">
              {t('Plain http:// is fine on a home network or VPN. Over the internet use an https:// address (a Cloudflare tunnel or reverse proxy).')}
            </p>
          </div>
          <div className="space-y-2">
            <Label>{t('Command')}</Label>
            <CopyBox text={`fafnirx enroll --server ${server}`} />
          </div>
          <div className="space-y-2">
            <Label>{t('Token')}</Label>
            <CopyBox text={token ?? ''} />
          </div>
        </div>
        <DialogFooter>
          <Button onClick={onClose}>{t('Done')}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
