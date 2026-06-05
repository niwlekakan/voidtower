import { useEffect, useRef, useState, useCallback } from 'react'
import { Terminal as XTerm } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import '@xterm/xterm/css/xterm.css'
import { Plus, Trash2, Server, Monitor, Key, X } from 'lucide-react'
import { api } from '@/api/client'
import type { SshSession } from '@/api/types'
import { useAuthStore } from '@/store/auth'
import { notify } from '@/store/notifications'
import Button from '@/components/ui/Button'

// ── shared xterm theme ────────────────────────────────────────────────────────

const TERM_THEME = {
  background: '#020403',
  foreground: '#d7ffe8',
  cursor: '#8b5cf6',
  green: '#00ff9c',
}

const TERM_OPTS = {
  fontFamily: 'JetBrains Mono, Fira Code, Cascadia Code, SF Mono, Consolas, monospace',
  fontSize: 13,
  theme: TERM_THEME,
  cursorBlink: true,
  scrollback: 5000,
}

// ── reusable PTY terminal component ──────────────────────────────────────────

function PtyTerminal({ wsUrl, onClose }: { wsUrl: string; onClose?: () => void }) {
  const containerRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    const container = containerRef.current
    if (!container) return

    const term = new XTerm(TERM_OPTS)
    const fit = new FitAddon()
    term.loadAddon(fit)
    term.open(container)
    fit.fit()

    const ws = new WebSocket(wsUrl)

    ws.onopen = () => ws.send(JSON.stringify({ type: 'Resize', cols: term.cols, rows: term.rows }))
    ws.onmessage = (e) => {
      try {
        const msg = JSON.parse(e.data)
        if (msg.type === 'Output') term.write(msg.data)
        if (msg.type === 'Closed') term.writeln('\r\n\x1b[31m[session closed]\x1b[0m')
      } catch { /* empty */ }
    }
    ws.onclose = () => term.writeln('\r\n\x1b[33m[disconnected]\x1b[0m')
    ws.onerror = () => term.writeln('\r\n\x1b[31m[connection error]\x1b[0m')
    term.onData((data) => { if (ws.readyState === WebSocket.OPEN) ws.send(JSON.stringify({ type: 'Input', data })) })

    const ro = new ResizeObserver(() => {
      fit.fit()
      if (ws.readyState === WebSocket.OPEN) ws.send(JSON.stringify({ type: 'Resize', cols: term.cols, rows: term.rows }))
    })
    ro.observe(container)

    return () => { ro.disconnect(); ws.close(); term.dispose() }
  }, [wsUrl])

  return (
    <div style={{ display: 'flex', flexDirection: 'column', flex: 1, minHeight: 0 }}>
      {onClose && (
        <div style={{ display: 'flex', justifyContent: 'flex-end', padding: '4px 8px', background: 'var(--bg-panel)', borderBottom: '1px solid var(--border-subtle)' }}>
          <button onClick={onClose} style={{ background: 'none', border: 'none', cursor: 'pointer', color: 'var(--text-muted)', display: 'flex', alignItems: 'center', gap: 4, fontSize: 12 }}>
            <X size={13} /> Close session
          </button>
        </div>
      )}
      <div ref={containerRef} style={{ flex: 1, background: '#020403', minHeight: 400 }} />
    </div>
  )
}

// ── SSH saved session card ────────────────────────────────────────────────────

function SessionCard({ session, onConnect, onDelete }: {
  session: SshSession
  onConnect: () => void
  onDelete: () => void
}) {
  const lastUsed = session.last_used
    ? new Date(session.last_used * 1000).toLocaleDateString(undefined, { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' })
    : 'never'

  return (
    <div
      onClick={onConnect}
      style={{
        background: 'var(--bg-card)', border: '1px solid var(--border-subtle)', borderRadius: 8,
        padding: '12px 16px', cursor: 'pointer', transition: 'border-color 0.15s',
        display: 'flex', alignItems: 'center', gap: 12,
      }}
      onMouseEnter={e => (e.currentTarget.style.borderColor = 'var(--accent-primary)')}
      onMouseLeave={e => (e.currentTarget.style.borderColor = 'var(--border-subtle)')}
    >
      <Server size={20} style={{ color: 'var(--accent-primary)', flexShrink: 0 }} />
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ fontWeight: 600, fontSize: 14, color: 'var(--text-primary)', marginBottom: 2 }}>{session.label}</div>
        <div style={{ fontSize: 12, color: 'var(--text-muted)', fontFamily: 'monospace' }}>
          {session.username}@{session.host}:{session.port}
        </div>
        {session.key_path && (
          <div style={{ fontSize: 11, color: 'var(--text-disabled)', marginTop: 2, display: 'flex', alignItems: 'center', gap: 4 }}>
            <Key size={10} /> {session.key_path}
          </div>
        )}
      </div>
      <div style={{ fontSize: 11, color: 'var(--text-disabled)', textAlign: 'right', flexShrink: 0 }}>
        <div>last used</div>
        <div>{lastUsed}</div>
      </div>
      <button
        onClick={e => { e.stopPropagation(); onDelete() }}
        style={{ background: 'none', border: 'none', cursor: 'pointer', color: 'var(--accent-danger)', padding: 4, borderRadius: 4, opacity: 0.7 }}
        title="Delete session"
      >
        <Trash2 size={14} />
      </button>
    </div>
  )
}

// ── SSH new session form ──────────────────────────────────────────────────────

function NewSessionForm({ onSaved }: { onSaved: () => void }) {
  const [form, setForm] = useState({ label: '', host: '', port: '22', username: '', key_path: '' })
  const [saving, setSaving] = useState(false)

  const field = (key: keyof typeof form, label: string, placeholder: string, type = 'text') => (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
      <label style={{ fontSize: 12, color: 'var(--text-muted)', fontWeight: 500 }}>{label}</label>
      <input
        type={type}
        value={form[key]}
        placeholder={placeholder}
        onChange={e => setForm(p => ({ ...p, [key]: e.target.value }))}
        style={{ background: 'var(--bg-input)', border: '1px solid var(--border-subtle)', borderRadius: 6, color: 'var(--text-primary)', padding: '6px 10px', fontSize: 13, outline: 'none' }}
      />
    </div>
  )

  const save = async () => {
    if (!form.label.trim() || !form.host.trim() || !form.username.trim()) {
      notify.error('Label, host and username are required')
      return
    }
    setSaving(true)
    try {
      await api.terminal.createSshSession({
        label: form.label.trim(),
        host: form.host.trim(),
        port: Number(form.port) || 22,
        username: form.username.trim(),
        key_path: form.key_path.trim() || undefined,
      })
      setForm({ label: '', host: '', port: '22', username: '', key_path: '' })
      onSaved()
    } catch { notify.error('Failed to save session') }
    finally { setSaving(false) }
  }

  return (
    <div style={{ background: 'var(--bg-card)', border: '1px solid var(--border-subtle)', borderRadius: 8, padding: 16 }}>
      <h3 style={{ fontSize: 14, fontWeight: 600, color: 'var(--text-primary)', marginBottom: 12 }}>New saved session</h3>
      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(180px, 1fr))', gap: 10, marginBottom: 12 }}>
        {field('label',    'Label',    'Home server')}
        {field('host',     'Host / IP', '192.168.1.10')}
        {field('port',     'Port',      '22', 'number')}
        {field('username', 'Username',  'root')}
        {field('key_path', 'SSH key path (optional)', '~/.ssh/id_rsa')}
      </div>
      <p style={{ fontSize: 12, color: 'var(--text-muted)', marginBottom: 10 }}>
        Password auth is entered interactively in the terminal. Leave key path blank to use password or SSH agent.
      </p>
      <Button size="sm" onClick={save} loading={saving}>
        <Plus size={13} /> Save session
      </Button>
    </div>
  )
}

// ── page ──────────────────────────────────────────────────────────────────────

export default function TerminalPage() {
  const user = useAuthStore((s) => s.user)
  const [tab, setTab] = useState<'local' | 'ssh'>('local')
  const [sessions, setSessions] = useState<SshSession[]>([])
  const [activeSession, setActiveSession] = useState<SshSession | null>(null)
  const [showNewForm, setShowNewForm] = useState(false)

  const loadSessions = useCallback(async () => {
    try { setSessions(await api.terminal.listSshSessions()) } catch { /* empty */ }
  }, [])

  useEffect(() => { loadSessions() }, [loadSessions])

  const deleteSession = async (id: string) => {
    if (!confirm('Delete this saved SSH session?')) return
    try { await api.terminal.deleteSshSession(id); loadSessions() }
    catch { notify.error('Failed to delete session') }
  }

  if (user?.role === 'viewer') {
    return (
      <div className="flex items-center justify-center h-40 text-sm" style={{ color: 'var(--text-muted)' }}>
        Terminal access requires operator role or higher.
      </div>
    )
  }

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: 'calc(100vh - 88px)', gap: 12 }}>
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', flexShrink: 0 }}>
        <h1 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>Terminal</h1>
        {tab === 'ssh' && (
          <Button size="sm" variant="secondary" onClick={() => setShowNewForm(v => !v)}>
            <Plus size={13} /> {showNewForm ? 'Cancel' : 'New session'}
          </Button>
        )}
      </div>

      {/* Tab bar */}
      <div style={{ display: 'flex', gap: 4, background: 'var(--bg-panel)', borderRadius: 8, padding: 4, width: 'fit-content', flexShrink: 0 }}>
        {([['local', Monitor, 'Local shell'], ['ssh', Server, 'SSH']] as const).map(([id, Icon, label]) => (
          <button
            key={id}
            onClick={() => { setTab(id); setActiveSession(null) }}
            style={{
              display: 'flex', alignItems: 'center', gap: 6, padding: '6px 16px', borderRadius: 6,
              border: 'none', cursor: 'pointer', fontSize: 13, fontWeight: 500,
              background: tab === id ? 'var(--accent-primary)' : 'transparent',
              color: tab === id ? '#fff' : 'var(--text-secondary)',
            }}
          >
            <Icon size={14} /> {label}
          </button>
        ))}
      </div>

      {/* Local terminal */}
      {tab === 'local' && (
        <div style={{ flex: 1, border: '1px solid var(--border-subtle)', borderRadius: 8, overflow: 'hidden', display: 'flex', flexDirection: 'column' }}>
          <PtyTerminal wsUrl={api.terminal.wsUrl()} />
        </div>
      )}

      {/* SSH tab */}
      {tab === 'ssh' && (
        <div style={{ flex: 1, minHeight: 0, display: 'flex', flexDirection: 'column', gap: 12, overflowY: 'auto' }}>

          {/* Active SSH terminal */}
          {activeSession && (
            <div style={{ border: '1px solid var(--accent-primary)', borderRadius: 8, overflow: 'hidden', display: 'flex', flexDirection: 'column', minHeight: 400 }}>
              <div style={{ padding: '6px 12px', background: 'var(--bg-panel)', borderBottom: '1px solid var(--border-subtle)', display: 'flex', alignItems: 'center', gap: 8 }}>
                <Server size={13} style={{ color: 'var(--accent-primary)' }} />
                <span style={{ fontSize: 12, color: 'var(--text-primary)', fontFamily: 'monospace' }}>
                  {activeSession.username}@{activeSession.host}:{activeSession.port}
                </span>
                <span style={{ fontSize: 11, color: 'var(--text-muted)', marginLeft: 4 }}>— {activeSession.label}</span>
                <button onClick={() => setActiveSession(null)} style={{ marginLeft: 'auto', background: 'none', border: 'none', cursor: 'pointer', color: 'var(--text-muted)', display: 'flex', alignItems: 'center', gap: 4, fontSize: 12 }}>
                  <X size={13} /> Disconnect
                </button>
              </div>
              <PtyTerminal
                key={activeSession.id + Date.now()}
                wsUrl={api.terminal.sshWsUrl(activeSession.id)}
              />
            </div>
          )}

          {/* New session form */}
          {showNewForm && (
            <NewSessionForm onSaved={() => { setShowNewForm(false); loadSessions() }} />
          )}

          {/* Saved sessions list */}
          {sessions.length === 0 && !showNewForm ? (
            <div style={{ color: 'var(--text-muted)', fontSize: 13, padding: '24px 0', textAlign: 'center' }}>
              No saved sessions yet. Click <strong>New session</strong> to add one.
            </div>
          ) : (
            <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
              {sessions.map(s => (
                <SessionCard
                  key={s.id}
                  session={s}
                  onConnect={() => setActiveSession(s)}
                  onDelete={() => deleteSession(s.id)}
                />
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  )
}
