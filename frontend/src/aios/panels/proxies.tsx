import { useEffect, useState } from 'react'
import { ToggleLeft, ToggleRight, Trash2 } from 'lucide-react'
import NativePanelShell, { NativeRow, StatusDot, IconBtn, EmptyState, LoadingState } from './NativePanelShell'

interface Proxy { id: string; name: string; domain: string; enabled: boolean }

export default function NativeProxiesPanel() {
  const [proxies, setProxies] = useState<Proxy[]>([])
  const [loading, setLoading] = useState(true)

  async function load() {
    const r = await fetch('/api/proxy', { credentials: 'include' })
    if (r.ok) { const d = await r.json(); setProxies(d.proxies ?? d ?? []) }
    setLoading(false)
  }

  async function toggle(id: string) {
    await fetch(`/api/proxy/${id}/toggle`, { method: 'POST', credentials: 'include' })
    load()
  }

  async function remove(id: string) {
    await fetch(`/api/proxy/${id}`, { method: 'DELETE', credentials: 'include' })
    load()
  }

  useEffect(() => { load() }, [])

  return (
    <NativePanelShell>
      {loading ? <LoadingState /> : proxies.length === 0 ? <EmptyState text="No proxies" /> :
        proxies.map(p => (
          <NativeRow key={p.id}>
            <StatusDot color={p.enabled ? '#22c55e' : '#94a3b8'} />
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ fontSize: 11, color: 'var(--text-primary)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{p.name}</div>
              <div style={{ fontSize: 9, color: 'var(--text-muted)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{p.domain}</div>
            </div>
            <IconBtn title={p.enabled ? 'Disable' : 'Enable'} onClick={() => toggle(p.id)}>
              {p.enabled ? <ToggleRight size={13} /> : <ToggleLeft size={13} />}
            </IconBtn>
            <IconBtn title="Delete" onClick={() => remove(p.id)} danger><Trash2 size={11} /></IconBtn>
          </NativeRow>
        ))
      }
    </NativePanelShell>
  )
}
