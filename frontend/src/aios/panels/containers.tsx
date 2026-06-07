import { useEffect, useState } from 'react'
import { Play, Square, RotateCcw, Trash2, Terminal } from 'lucide-react'
import NativePanelShell, { NativeRow, StatusDot, IconBtn, EmptyState, LoadingState } from './NativePanelShell'

interface Port { host_port: number; container_port: number; protocol: string }
interface Container {
  id: string
  name: string
  image: string
  state: string
  ports?: Port[]
}

function stateColor(s: string) {
  return s === 'running' ? '#22c55e' : s === 'paused' ? '#f59e0b' : '#94a3b8'
}

function fmtPorts(ports: Port[]): string {
  return ports.filter(p => p.host_port).map(p => `${p.host_port}→${p.container_port}`).join(' ')
}

export default function NativeContainersPanel() {
  const [containers, setContainers] = useState<Container[]>([])
  const [loading, setLoading] = useState(true)
  const [search, setSearch] = useState('')
  const [logsFor, setLogsFor] = useState<string | null>(null)
  const [logLines, setLogLines] = useState<string[]>([])

  async function load() {
    const r = await fetch('/api/containers', { credentials: 'include' })
    if (r.ok) { const d = await r.json(); setContainers(d.containers ?? []) }
    setLoading(false)
  }

  async function act(id: string, action: string) {
    await fetch(`/api/containers/${encodeURIComponent(id)}/action`, {
      method: 'POST', credentials: 'include',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ action }),
    })
    load()
  }

  async function viewLogs(id: string) {
    if (logsFor === id) { setLogsFor(null); return }
    setLogsFor(id)
    setLogLines([])
    const r = await fetch(`/api/containers/${encodeURIComponent(id)}/logs?lines=30`, { credentials: 'include' })
    if (r.ok) { const d = await r.json(); setLogLines(d.lines ?? []) }
    else setLogLines(['Failed to load logs'])
  }

  useEffect(() => { load() }, [])

  const filtered = containers.filter(c =>
    c.name.toLowerCase().includes(search.toLowerCase()) ||
    c.image.toLowerCase().includes(search.toLowerCase())
  )

  return (
    <NativePanelShell search={search} onSearch={setSearch} searchPlaceholder="Filter containers…">
      {loading ? <LoadingState /> : filtered.length === 0 ? <EmptyState text="No containers" /> :
        filtered.map(c => {
          const portStr = c.ports && c.ports.length > 0 ? fmtPorts(c.ports) : ''
          const subLine = [c.image, portStr].filter(Boolean).join(' · ')
          return (
            <div key={c.id}>
              <NativeRow>
                <StatusDot color={stateColor(c.state)} />
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div style={{ fontSize: 11, color: 'var(--text-primary)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{c.name}</div>
                  <div style={{ fontSize: 9, color: 'var(--text-muted)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{subLine}</div>
                </div>
                <IconBtn title="Logs"    onClick={() => viewLogs(c.id)}><Terminal size={11} /></IconBtn>
                <IconBtn title="Start"   onClick={() => act(c.id, 'start')}><Play size={11} /></IconBtn>
                <IconBtn title="Stop"    onClick={() => act(c.id, 'stop')}><Square size={11} /></IconBtn>
                <IconBtn title="Restart" onClick={() => act(c.id, 'restart')}><RotateCcw size={11} /></IconBtn>
                <IconBtn title="Remove"  onClick={() => act(c.id, 'remove')} danger><Trash2 size={11} /></IconBtn>
              </NativeRow>
              {logsFor === c.id && (
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
