import { useEffect, useState } from 'react'
import { Trash2 } from 'lucide-react'
import NativePanelShell, { NativeRow, StatusDot, IconBtn, EmptyState, LoadingState } from './NativePanelShell'

interface Token { id: string; name: string; scope?: string; last_used?: string; expires_at?: string }

export default function NativeIntegrationsPanel() {
  const [tokens, setTokens] = useState<Token[]>([])
  const [loading, setLoading] = useState(true)

  async function load() {
    const r = await fetch('/api/integrations/tokens', { credentials: 'include' })
    if (r.ok) { const d = await r.json(); setTokens(d.tokens ?? []) }
    setLoading(false)
  }

  async function revoke(id: string) {
    await fetch(`/api/integrations/tokens/${id}`, { method: 'DELETE', credentials: 'include' })
    load()
  }

  useEffect(() => { load() }, [])

  function isExpired(t: Token) {
    return t.expires_at ? new Date(t.expires_at) < new Date() : false
  }

  return (
    <NativePanelShell>
      {loading ? <LoadingState /> : tokens.length === 0 ? <EmptyState text="No API tokens" /> :
        tokens.map(t => (
          <NativeRow key={t.id}>
            <StatusDot color={isExpired(t) ? '#ef4444' : '#22c55e'} />
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ fontSize: 11, color: 'var(--text-primary)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{t.name}</div>
              <div style={{ fontSize: 9, color: 'var(--text-muted)' }}>
                {t.scope ?? 'full'}{t.last_used ? ` · used ${t.last_used}` : ''}
                {isExpired(t) ? ' · expired' : ''}
              </div>
            </div>
            <IconBtn title="Revoke token" onClick={() => revoke(t.id)} danger><Trash2 size={11} /></IconBtn>
          </NativeRow>
        ))
      }
    </NativePanelShell>
  )
}
