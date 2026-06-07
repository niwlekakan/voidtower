import { useEffect, useState } from 'react'
import { Play, Square, RotateCcw, Terminal, Shield, ShieldOff } from 'lucide-react'
import NativePanelShell, { NativeRow, StatusDot, IconBtn, EmptyState, LoadingState } from './NativePanelShell'

interface Service {
  name: string
  display_name?: string
  description?: string
  active: string
  active_state?: string
  sub_state?: string
  enabled: boolean
}

function statusColor(active: string) {
  if (active === 'active') return '#22c55e'
  if (active === 'failed') return '#ef4444'
  return '#94a3b8'
}

export default function NativeServicesPanel() {
  const [services, setServices] = useState<Service[]>([])
  const [loading, setLoading] = useState(true)
  const [search, setSearch] = useState('')
  const [logsFor, setLogsFor] = useState<string | null>(null)
  const [logLines, setLogLines] = useState<string[]>([])

  async function load() {
    const r = await fetch('/api/services', { credentials: 'include' })
    if (r.ok) { const d = await r.json(); setServices(d.services ?? []) }
    setLoading(false)
  }

  async function act(name: string, action: string) {
    await fetch(`/api/services/${encodeURIComponent(name)}/action`, {
      method: 'POST', credentials: 'include',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ action }),
    })
    load()
  }

  async function viewLogs(name: string) {
    if (logsFor === name) { setLogsFor(null); return }
    setLogsFor(name)
    setLogLines([])
    const r = await fetch(`/api/services/${encodeURIComponent(name)}/logs?lines=20`, { credentials: 'include' })
    if (r.ok) { const d = await r.json(); setLogLines(d.lines ?? []) }
    else setLogLines(['Failed to load logs'])
  }

  useEffect(() => { load() }, [])

  const filtered = services.filter(s =>
    s.name.toLowerCase().includes(search.toLowerCase()) ||
    (s.display_name ?? '').toLowerCase().includes(search.toLowerCase())
  )

  return (
    <NativePanelShell search={search} onSearch={setSearch} searchPlaceholder="Filter services…">
      {loading ? <LoadingState /> : filtered.length === 0 ? <EmptyState text="No services" /> :
        filtered.map(s => {
          const activeStr = s.active_state ?? s.active
          const desc = s.description ?? s.sub_state
          const enabledLabel = s.enabled ? '' : ' · disabled'
          const subLine = (desc ? desc : activeStr) + enabledLabel
          return (
            <div key={s.name}>
              <NativeRow>
                <StatusDot color={statusColor(activeStr)} />
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div style={{ fontSize: 11, color: 'var(--text-primary)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{s.display_name ?? s.name}</div>
                  <div style={{ fontSize: 9, color: 'var(--text-muted)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{subLine}</div>
                </div>
                <IconBtn title="Logs" onClick={() => viewLogs(s.name)}><Terminal size={11} /></IconBtn>
                {s.enabled
                  ? <IconBtn title="Disable" onClick={() => act(s.name, 'disable')}><ShieldOff size={11} /></IconBtn>
                  : <IconBtn title="Enable"  onClick={() => act(s.name, 'enable')}><Shield size={11} /></IconBtn>
                }
                <IconBtn title="Start"   onClick={() => act(s.name, 'start')}><Play size={11} /></IconBtn>
                <IconBtn title="Stop"    onClick={() => act(s.name, 'stop')}><Square size={11} /></IconBtn>
                <IconBtn title="Restart" onClick={() => act(s.name, 'restart')}><RotateCcw size={11} /></IconBtn>
              </NativeRow>
              {logsFor === s.name && (
                <div style={{ background: 'var(--bg-elevated)', borderBottom: '1px solid var(--border-subtle)', padding: '4px 10px' }}>
                  {logLines.length === 0
                    ? <div style={{ fontSize: 10, color: 'var(--text-muted)' }}>Loading…</div>
                    : <pre style={{ margin: 0, fontSize: 9, color: 'var(--text-secondary)', fontFamily: 'monospace', maxHeight: 120, overflowY: 'auto', whiteSpace: 'pre-wrap', wordBreak: 'break-all' }}>{logLines.join('\n')}</pre>
                  }
                </div>
              )}
            </div>
          )
        })
      }
    </NativePanelShell>
  )
}
