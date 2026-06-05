import { useState, useEffect, useRef, useCallback } from 'react'
import { useParams, useNavigate } from 'react-router-dom'
import { ArrowLeft, Terminal, FileText, Play, Square, RotateCcw, CheckCircle, XCircle, AlertTriangle } from 'lucide-react'
import { api, ApiClientError } from '@/api/client'
import { notify } from '@/store/notifications'
import type { ContainerInfo } from '@/api/types'
import Button from '@/components/ui/Button'

type Tab = 'overview' | 'compose' | 'logs' | 'terminal'

// ─── Live log stream via WebSocket ──────────────────────────────────
function LogStream({ containerId }: { containerId: string }) {
  const outputRef = useRef<HTMLDivElement>(null)
  const [lines, setLines] = useState<string[]>(['Connecting…'])
  const [connected, setConnected] = useState(false)

  useEffect(() => {
    const proto = location.protocol === 'https:' ? 'wss' : 'ws'
    const ws = new WebSocket(`${proto}://${location.host}/api/containers/${containerId}/logs/stream`)

    ws.onopen = () => { setConnected(true); setLines([]) }
    ws.onmessage = (e) => {
      try {
        const msg = JSON.parse(e.data)
        if (msg.type === 'log') {
          setLines((prev) => [...prev.slice(-2000), msg.line])
        }
      } catch { /* ignore */ }
    }
    ws.onclose = () => { setConnected(false); setLines((p) => [...p, '[stream closed]']) }
    ws.onerror = () => setLines((p) => [...p, '[connection error]'])

    return () => ws.close()
  }, [containerId])

  useEffect(() => {
    if (outputRef.current) outputRef.current.scrollTop = outputRef.current.scrollHeight
  }, [lines])

  return (
    <div className="flex flex-col rounded overflow-hidden" style={{ background: 'var(--terminal-bg)', maxHeight: '60vh' }}>
      <div className="flex items-center gap-2 px-3 py-1.5 border-b text-xs shrink-0" style={{ borderColor: 'rgba(255,255,255,0.06)', color: 'var(--text-muted)' }}>
        <span className="w-2 h-2 rounded-full" style={{ background: connected ? 'var(--accent-success)' : 'var(--accent-danger)' }} />
        {connected ? `logs: ${containerId.slice(0, 12)}` : 'disconnected'}
      </div>
      <div
        ref={outputRef}
        className="overflow-y-auto p-3 font-mono text-xs leading-relaxed whitespace-pre-wrap"
        style={{ color: 'var(--text-secondary)' }}
      >
        {lines.length ? lines.join('\n') : <span style={{ color: 'var(--text-muted)' }}>No logs.</span>}
      </div>
    </div>
  )
}

// ─── Exec terminal via WebSocket ────────────────────────────────────
function ExecTerminal({ containerId }: { containerId: string }) {
  const outputRef = useRef<HTMLDivElement>(null)
  const inputRef  = useRef<HTMLInputElement>(null)
  const wsRef     = useRef<WebSocket | null>(null)
  const [lines, setLines] = useState<string[]>(['Connecting…'])
  const [input, setInput] = useState('')
  const [connected, setConnected] = useState(false)

  useEffect(() => {
    const proto = location.protocol === 'https:' ? 'wss' : 'ws'
    const ws = new WebSocket(`${proto}://${location.host}/api/containers/${containerId}/exec`)
    wsRef.current = ws

    ws.onopen = () => { setConnected(true); setLines([]) }
    ws.onmessage = (e) => {
      try {
        const msg = JSON.parse(e.data)
        if (msg.type === 'output') {
          setLines((prev) => [...prev.slice(-500), ...msg.data.split('\n')])
        }
      } catch { /* raw text fallback */ }
    }
    ws.onclose = () => { setConnected(false); setLines((p) => [...p, '\r\n[connection closed]']) }
    ws.onerror = () => setLines((p) => [...p, '[connection error — is the container running?]'])

    return () => ws.close()
  }, [containerId])

  useEffect(() => {
    if (outputRef.current) outputRef.current.scrollTop = outputRef.current.scrollHeight
  }, [lines])

  const send = (data: string) => {
    wsRef.current?.send(JSON.stringify({ type: 'input', data }))
  }

  const handleKey = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter') { send(input + '\n'); setInput('') }
    else if (e.key === 'c' && e.ctrlKey) { send('\x03'); setInput('') }
    else if (e.key === 'Tab') { e.preventDefault(); send('\t') }
  }

  return (
    <div className="flex flex-col h-full rounded overflow-hidden" style={{ background: 'var(--terminal-bg)', minHeight: 400 }}>
      <div className="flex items-center gap-2 px-3 py-1.5 border-b text-xs" style={{ borderColor: 'rgba(255,255,255,0.06)', color: 'var(--text-muted)' }}>
        <span className="w-2 h-2 rounded-full" style={{ background: connected ? 'var(--accent-success)' : 'var(--accent-danger)' }} />
        {connected ? `exec: ${containerId.slice(0, 12)}` : 'disconnected'}
      </div>
      <div
        ref={outputRef}
        className="flex-1 overflow-y-auto p-3 font-mono text-xs leading-relaxed whitespace-pre-wrap"
        style={{ color: 'var(--terminal-green)' }}
        onClick={() => inputRef.current?.focus()}
      >
        {lines.join('\n')}
      </div>
      <div className="flex items-center gap-2 px-3 py-2 border-t" style={{ borderColor: 'rgba(255,255,255,0.06)' }}>
        <span className="font-mono text-xs" style={{ color: 'var(--terminal-green)' }}>$</span>
        <input
          ref={inputRef}
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={handleKey}
          disabled={!connected}
          className="flex-1 bg-transparent outline-none font-mono text-xs"
          style={{ color: 'var(--text-primary)', caretColor: 'var(--terminal-cursor)' }}
          placeholder={connected ? 'type a command…' : 'not connected'}
          autoFocus
        />
      </div>
    </div>
  )
}

// ─── Compose tab ─────────────────────────────────────────────────────
function ComposeTab({ containerId }: { containerId: string }) {
  const [data, setData] = useState<{ found: boolean; path?: string; content?: string; message?: string } | null>(null)
  const [edited, setEdited]     = useState('')
  const [dirty, setDirty]       = useState(false)
  const [proposing, setProposing] = useState(false)
  const [diff, setDiff] = useState<{ proposed_path: string; added: number; removed: number } | null>(null)
  const [applying, setApplying] = useState(false)

  useEffect(() => {
    fetch(`/api/containers/${containerId}/compose`, { credentials: 'include' })
      .then((r) => r.json())
      .then((r) => { setData(r); setEdited(r.content ?? '') })
      .catch(() => setData({ found: false, message: 'Failed to load' }))
  }, [containerId])

  const propose = async () => {
    if (!data?.path) return
    setProposing(true)
    try {
      const r = await fetch(`/api/containers/${containerId}/compose/propose`, {
        method: 'POST',
        credentials: 'include',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ path: data.path, content: edited }),
      }).then((r) => r.json())
      setDiff(r)
      notify.success(`Staged: +${r.added} -${r.removed} lines`)
    } catch {
      notify.error('Failed to stage changes')
    } finally {
      setProposing(false)
    }
  }

  const apply = async () => {
    if (!diff) return
    setApplying(true)
    try {
      const r = await fetch(`/api/containers/${containerId}/compose/apply`, {
        method: 'POST',
        credentials: 'include',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ proposed_path: diff.proposed_path }),
      }).then((r) => r.json())
      if (r.ok) {
        notify.success('Applied — stack restarting')
        setDiff(null); setDirty(false)
      } else {
        notify.error(r.stderr || 'Apply failed')
      }
    } finally {
      setApplying(false)
    }
  }

  if (!data) return <p className="text-xs p-4" style={{ color: 'var(--text-muted)' }}>Loading…</p>
  if (!data.found) return (
    <p className="text-xs p-4" style={{ color: 'var(--text-muted)' }}>{data.message ?? 'No compose file found.'}</p>
  )

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between">
        <p className="text-xs font-mono" style={{ color: 'var(--text-muted)' }}>{data.path}</p>
        <div className="flex gap-2">
          {dirty && !diff && (
            <Button size="sm" variant="ghost" loading={proposing} onClick={propose}>
              Stage changes
            </Button>
          )}
          {diff && (
            <>
              <span className="text-xs self-center" style={{ color: 'var(--accent-warning)' }}>
                +{diff.added} −{diff.removed} staged
              </span>
              <Button size="sm" variant="primary" loading={applying} onClick={apply}>
                Apply &amp; restart
              </Button>
              <Button size="sm" variant="ghost" onClick={() => setDiff(null)}>Discard</Button>
            </>
          )}
        </div>
      </div>

      {diff && (
        <div className="rounded p-3 text-xs" style={{ background: 'var(--accent-warning-subtle)', border: '1px solid var(--accent-warning)', color: 'var(--accent-warning)' }}>
          <AlertTriangle size={12} className="inline mr-1.5" />
          Staged changes will restart the container stack. Confirm before applying.
        </div>
      )}

      <textarea
        value={edited}
        onChange={(e) => { setEdited(e.target.value); setDirty(true); setDiff(null) }}
        className="w-full font-mono text-xs p-3 rounded resize-y outline-none"
        rows={24}
        style={{ background: 'var(--bg-card)', border: '1px solid var(--border-subtle)', color: 'var(--text-primary)' }}
        spellCheck={false}
      />
    </div>
  )
}

// ─── Main page ──────────────────────────────────────────────────────
export default function ContainerDetailPage() {
  const { id } = useParams<{ id: string }>()
  const navigate = useNavigate()
  const [tab, setTab] = useState<Tab>('overview')
  const [container, setContainer] = useState<ContainerInfo | null>(null)
  const [actioning, setActioning] = useState(false)

  const refresh = useCallback(() => {
    api.containers.list().then((r) => {
      const c = r.containers.find((c) => c.id === id || c.short_id === id)
      setContainer(c ?? null)
    }).catch(() => {})
  }, [id])

  useEffect(() => { refresh() }, [refresh])

  const doAction = async (action: 'start' | 'stop' | 'restart') => {
    if (!id) return
    setActioning(true)
    try {
      await api.containers.action(id, action)
      notify.success(`${action} sent`)
      setTimeout(refresh, 1500)
    } catch (err) {
      notify.error(err instanceof ApiClientError ? err.message : 'Action failed')
    } finally {
      setActioning(false)
    }
  }

  if (!container) return (
    <div className="flex flex-col items-center justify-center h-40 gap-3">
      <p className="text-sm" style={{ color: 'var(--text-muted)' }}>Container not found.</p>
      <Button size="sm" variant="ghost" onClick={() => navigate('/containers')}>
        <ArrowLeft size={13} className="mr-1" /> Back
      </Button>
    </div>
  )

  const isRunning = container.state === 'running'
  const TABS: { id: Tab; label: string; icon: React.ElementType }[] = [
    { id: 'overview', label: 'Overview', icon: FileText },
    { id: 'compose',  label: 'Compose',  icon: FileText },
    { id: 'logs',     label: 'Logs',     icon: FileText },
    { id: 'terminal', label: 'Terminal', icon: Terminal },
  ]

  return (
    <div className="space-y-4 max-w-4xl">
      {/* Header */}
      <div className="flex items-center gap-3">
        <button onClick={() => navigate('/containers')} className="hover:opacity-70" style={{ color: 'var(--text-muted)' }}>
          <ArrowLeft size={18} />
        </button>
        <div className="flex-1 min-w-0">
          <h1 className="text-base font-semibold font-mono truncate" style={{ color: 'var(--text-primary)' }}>
            {container.name}
          </h1>
          <p className="text-xs font-mono" style={{ color: 'var(--text-muted)' }}>
            {container.short_id} · {container.image}
          </p>
        </div>
        <div className="flex items-center gap-2">
          <span className="flex items-center gap-1.5 text-xs" style={{ color: isRunning ? 'var(--accent-success)' : 'var(--accent-danger)' }}>
            {isRunning ? <CheckCircle size={13} /> : <XCircle size={13} />}
            {container.state}
          </span>
          <Button size="sm" variant="ghost" disabled={actioning || !isRunning} onClick={() => doAction('stop')}>
            <Square size={12} className="mr-1" /> Stop
          </Button>
          <Button size="sm" variant="ghost" disabled={actioning} onClick={() => doAction(isRunning ? 'restart' : 'start')}>
            {isRunning ? <RotateCcw size={12} className="mr-1" /> : <Play size={12} className="mr-1" />}
            {isRunning ? 'Restart' : 'Start'}
          </Button>
        </div>
      </div>

      {/* Tabs */}
      <div className="flex gap-0 border-b" style={{ borderColor: 'var(--border-subtle)' }}>
        {TABS.map(({ id: tid, label }) => (
          <button
            key={tid}
            onClick={() => setTab(tid)}
            className="px-4 py-2 text-xs transition-colors"
            style={{
              borderBottom: `2px solid ${tab === tid ? 'var(--accent-primary)' : 'transparent'}`,
              color: tab === tid ? 'var(--accent-primary)' : 'var(--text-muted)',
              marginBottom: -1,
            }}
          >
            {label}
          </button>
        ))}
      </div>

      {/* Tab content */}
      {tab === 'overview' && (
        <div className="grid grid-cols-2 gap-3 text-xs">
          {[
            ['ID',      container.id],
            ['Image',   container.image],
            ['Status',  container.status],
            ['State',   container.state],
            ['Created', new Date(container.created * 1000).toLocaleString()],
            ['Ports',   container.ports.map((p) => `${p.host_port ?? ''}→${p.container_port}/${p.protocol}`).join(', ') || '—'],
          ].map(([k, v]) => (
            <div key={k} className="panel p-3">
              <p className="mb-1" style={{ color: 'var(--text-muted)' }}>{k}</p>
              <p className="font-mono break-all" style={{ color: 'var(--text-primary)' }}>{v}</p>
            </div>
          ))}
        </div>
      )}

      {tab === 'compose' && id && <ComposeTab containerId={id} />}

      {tab === 'logs' && id && <LogStream containerId={id} />}

      {tab === 'terminal' && id && (
        isRunning
          ? <ExecTerminal containerId={id} />
          : <p className="text-xs p-4" style={{ color: 'var(--text-muted)' }}>Container must be running to exec a terminal.</p>
      )}
    </div>
  )
}
