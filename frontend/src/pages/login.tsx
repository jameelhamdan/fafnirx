import { useQueryClient } from '@tanstack/react-query'
import { useState, type FormEvent } from 'react'
import { LanguageMenu } from '@/components/language-menu'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { api } from '@/lib/api'
import { useT } from '@/lib/i18n'
import type { Session } from '@/lib/types'

export default function Login() {
  const t = useT()
  const client = useQueryClient()
  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')
  const [error, setError] = useState('')
  const [busy, setBusy] = useState(false)

  async function submit(e: FormEvent) {
    e.preventDefault()
    setBusy(true)
    setError('')
    try {
      await api.post('/api/login', { username, password })
      client.setQueryData(['session'], await api.get<Session>('/api/session'))
    } catch (err) {
      setError(err instanceof Error ? err.message : t('Could not log in'))
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="relative grid min-h-svh place-items-center p-4">
      <div className="absolute top-3 end-3"><LanguageMenu /></div>
      <Card className="w-full max-w-sm">
        <CardHeader>
          <img src="/favicon.svg" alt="" className="mb-2 size-10" />
          <CardTitle>fafnirx</CardTitle>
          <CardDescription>{t('Log in to manage your files and devices.')}</CardDescription>
        </CardHeader>
        <CardContent>
          <form onSubmit={submit} className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="username">{t('User name')}</Label>
              <Input id="username" autoFocus autoComplete="username" autoCapitalize="none" spellCheck={false} value={username} onChange={(e) => setUsername(e.target.value)} />
            </div>
            <div className="space-y-2">
              <Label htmlFor="password">{t('Password')}</Label>
              <Input id="password" type="password" autoComplete="current-password" value={password} onChange={(e) => setPassword(e.target.value)} />
            </div>
            {error && (
              <p role="alert" className="text-sm text-destructive">
                {error}
              </p>
            )}
            <Button type="submit" className="w-full" disabled={busy || !username.trim() || !password}>
              {busy ? t('Logging in…') : t('Log in')}
            </Button>
          </form>
        </CardContent>
      </Card>
    </div>
  )
}
