import { useEffect, useState } from 'react'
import { Eye, EyeOff } from 'lucide-react'
import NativePanelShell, { NativeRow, IconBtn, EmptyState, LoadingState } from './NativePanelShell'

interface Secret { id: string; name: string; scope?: string }

export default function NativeSecretsPanel() {
  const [secrets, setSecrets] = useState<Secret[]>([])
  const [revealed, setRevealed] = useState<Record<string, string>>({})
  const [loading, setLoading] = useState(true)
  const [search, setSearch] = useState('')

  async function load() {
    const r = await fetch('/api/secrets', { credentials: 'include' })
    if (r.ok) { const d = await r.json(); setSecrets(d.secrets ?? []) }
    setLoading(false)
  }

  async function reveal(id: string) {
    if (revealed[id]) { setRevealed(prev => { const n = { ...prev }; delete n[id]; return n }); return }
    const r = await fetch(`/api/secrets/${id}/reveal`, { credentials: 'include' })
    if (r.ok) { const d = await r.json(); setRevealed(prev => ({ ...prev, [id]: d.value ?? '•••' })) }
  }

  useEffect(() => { load() }, [])

  const filtered = secrets.filter(s => s.name.toLowerCase().includes(search.toLowerCase()))

  return (
    <NativePanelShell search={search} onSearch={setSearch} searchPlaceholder="Filter secrets…">
      {loading ? <LoadingState /> : filtered.length === 0 ? <EmptyState text="No secrets" /> :
        filtered.map(s => (
          <NativeRow key={s.id}>
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ fontSize: 11, color: 'var(--text-primary)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{s.name}</div>
              {revealed[s.id] && <div style={{ fontSize: 9, color: 'var(--text-muted)', fontFamily: 'monospace', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{revealed[s.id]}</div>}
              {!revealed[s.id] && s.scope && <div style={{ fontSize: 9, color: 'var(--text-muted)' }}>{s.scope}</div>}
            </div>
            <IconBtn title={revealed[s.id] ? 'Hide' : 'Reveal'} onClick={() => reveal(s.id)}>
              {revealed[s.id] ? <EyeOff size={11} /> : <Eye size={11} />}
            </IconBtn>
          </NativeRow>
        ))
      }
    </NativePanelShell>
  )
}
