import { useCallback, useEffect, useState } from 'react'
import { Trash2, Download } from 'lucide-react'
import NativePanelShell, { NativeRow, StatusDot, IconBtn, EmptyState, LoadingState } from './NativePanelShell'

interface Model {
  id: string; name: string; provider: string; enabled: boolean
  size?: string; family?: string; parameter_size?: string
  quantization_level?: string; context_length?: number
}

interface OllamaRaw {
  name: string; size?: number
  details?: { family?: string; parameter_size?: string; quantization_level?: string }
}

const TABS = [{ id: 'all', label: 'All' }, { id: 'ollama', label: 'Ollama' }]

export default function NativeModelsPanel() {
  const [models,   setModels]   = useState<Model[]>([])
  const [loading,  setLoading]  = useState(true)
  const [tab,      setTab]      = useState('all')
  const [pulling,  setPulling]  = useState(false)
  const [pullName, setPullName] = useState('')
  const [pullMsg,  setPullMsg]  = useState('')
  const [deleting, setDeleting] = useState<string | null>(null)

  const load = useCallback(async () => {
    setLoading(true)
    if (tab === 'all') {
      const r = await fetch('/api/models', { credentials: 'include' })
      if (r.ok) { const d = await r.json(); setModels(d.models ?? []) }
    } else {
      const r = await fetch('/api/models/ollama', { credentials: 'include' })
      if (r.ok) {
        const d = await r.json()
        setModels((d.models ?? []).map((m: OllamaRaw) => ({
          id: m.name, name: m.name, provider: 'ollama', enabled: true,
          size: m.size ? `${(m.size / 1e9).toFixed(1)}G` : undefined,
          family:             m.details?.family,
          parameter_size:     m.details?.parameter_size,
          quantization_level: m.details?.quantization_level,
        })))
      }
    }
    setLoading(false)
  }, [tab])

  useEffect(() => { load() }, [load])

  async function pull() {
    if (!pullName.trim()) return
    setPulling(true)
    setPullMsg('Pulling…')
    await fetch('/api/models/ollama/pull', {
      method: 'POST', credentials: 'include',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ name: pullName.trim() }),
    })
    setPullMsg('Done')
    setPulling(false)
    setPullName('')
    setTimeout(() => setPullMsg(''), 3000)
    if (tab === 'ollama') load()
  }

  async function del(name: string) {
    setDeleting(name)
    await fetch(`/api/models/ollama/${encodeURIComponent(name)}`, { method: 'DELETE', credentials: 'include' })
    setDeleting(null)
    load()
  }

  const actions = tab === 'ollama' ? (
    <div style={{ display: 'flex', alignItems: 'center', gap: 4, flex: 1 }}>
      <input
        value={pullName}
        onChange={e => setPullName(e.target.value)}
        onKeyDown={e => e.key === 'Enter' && pull()}
        placeholder="model:tag"
        style={{
          flex: 1, fontSize: 11, padding: '3px 6px', borderRadius: 4,
          border: '1px solid var(--border-subtle)', background: 'var(--bg-elevated)',
          color: 'var(--text-primary)', outline: 'none',
        }}
      />
      <IconBtn title={pulling ? 'Pulling…' : 'Pull model'} onClick={pull}>
        <Download size={12} style={{ opacity: pulling ? 0.4 : 1 }} />
      </IconBtn>
      {pullMsg && <span style={{ fontSize: 10, color: 'var(--text-muted)' }}>{pullMsg}</span>}
    </div>
  ) : undefined

  return (
    <NativePanelShell tabs={TABS} activeTab={tab} onTabChange={setTab} actions={actions}>
      {loading ? <LoadingState /> : models.length === 0 ? <EmptyState text="No models" /> :
        models.map(m => {
          const sub2parts: string[] = [m.provider]
          if (m.size) sub2parts.push(m.size)
          if (m.family) sub2parts.push(m.family)
          if (m.parameter_size) sub2parts.push(m.parameter_size)
          if (m.quantization_level) sub2parts.push(m.quantization_level)
          if (m.context_length) sub2parts.push(`ctx ${m.context_length.toLocaleString()}`)
          return (
            <NativeRow key={m.id}>
              <StatusDot color={m.enabled ? '#22c55e' : '#94a3b8'} />
              <div style={{ flex: 1, minWidth: 0 }}>
                <div style={{ fontSize: 11, color: 'var(--text-primary)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{m.name}</div>
                <div style={{ fontSize: 9, color: 'var(--text-muted)' }}>{sub2parts.join(' · ')}</div>
              </div>
              {tab === 'ollama' && (
                <IconBtn title={deleting === m.name ? 'Deleting…' : 'Delete model'} danger onClick={() => del(m.name)}>
                  <Trash2 size={11} style={{ opacity: deleting === m.name ? 0.4 : 1 }} />
                </IconBtn>
              )}
            </NativeRow>
          )
        })
      }
    </NativePanelShell>
  )
}
