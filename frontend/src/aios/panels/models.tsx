import { useEffect, useState } from 'react'
import NativePanelShell, { NativeRow, StatusDot, EmptyState, LoadingState } from './NativePanelShell'

interface Model { id: string; name: string; provider: string; enabled: boolean; size?: string }

const TABS = [{ id: 'all', label: 'All' }, { id: 'ollama', label: 'Ollama' }]

export default function NativeModelsPanel() {
  const [models, setModels] = useState<Model[]>([])
  const [loading, setLoading] = useState(true)
  const [tab, setTab] = useState('all')

  async function load() {
    setLoading(true)
    if (tab === 'all') {
      const r = await fetch('/api/models', { credentials: 'include' })
      if (r.ok) { const d = await r.json(); setModels(d.models ?? []) }
    } else {
      const r = await fetch('/api/models/ollama', { credentials: 'include' })
      if (r.ok) {
        const d = await r.json()
        setModels((d.models ?? []).map((m: { name: string; size?: number }) => ({
          id: m.name, name: m.name, provider: 'ollama', enabled: true,
          size: m.size ? `${(m.size / 1e9).toFixed(1)}G` : undefined,
        })))
      }
    }
    setLoading(false)
  }

  useEffect(() => { load() }, [tab])

  return (
    <NativePanelShell tabs={TABS} activeTab={tab} onTabChange={setTab}>
      {loading ? <LoadingState /> : models.length === 0 ? <EmptyState text="No models" /> :
        models.map(m => (
          <NativeRow key={m.id}>
            <StatusDot color={m.enabled ? '#22c55e' : '#94a3b8'} />
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ fontSize: 11, color: 'var(--text-primary)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{m.name}</div>
              <div style={{ fontSize: 9, color: 'var(--text-muted)' }}>{m.provider}{m.size ? ` · ${m.size}` : ''}</div>
            </div>
          </NativeRow>
        ))
      }
    </NativePanelShell>
  )
}
