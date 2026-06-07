import { useEffect, useRef, useState, useCallback, useId } from 'react'
import { Terminal as XTerm } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { WebLinksAddon } from '@xterm/addon-web-links'
import '@xterm/xterm/css/xterm.css'
import {
  Plus, Trash2, Server, Monitor, Key, X, Clipboard,
  ChevronDown, ChevronUp, Pencil, Check, Wifi, WifiOff,
} from 'lucide-react'
import { api } from '@/api/client'
import type { SshSession } from '@/api/types'
import { useAuthStore } from '@/store/auth'
import { notify } from '@/store/notifications'
import Button from '@/components/ui/Button'

// ── xterm theme ───────────────────────────────────────────────────────────────

const TERM_THEME = {
  background:  '#0d0f0e',
  foreground:  '#d7ffe8',
  cursor:      '#8b5cf6',
  cursorAccent:'#0d0f0e',
  black:       '#1a1c1b',
  red:         '#ff5555',
  green:       '#00ff9c',
  yellow:      '#f1fa8c',
  blue:        '#6272a4',
  magenta:     '#bd93f9',
  cyan:        '#8be9fd',
  white:       '#f8f8f2',
  brightBlack: '#44475a',
  brightRed:   '#ff6e6e',
  brightGreen: '#69ff94',
  brightYellow:'#ffffa5',
  brightBlue:  '#d6acff',
  brightMagenta:'#ff92df',
  brightCyan:  '#a4ffff',
  brightWhite: '#ffffff',
}

// ── toolbar button style ──────────────────────────────────────────────────────

const toolBtn: React.CSSProperties = {
  background: 'none',
  border: '1px solid var(--border-subtle)',
  cursor: 'pointer',
  color: 'var(--text-secondary)',
  borderRadius: 4,
  padding: '2px 8px',
  fontSize: 11,
  display: 'flex',
  alignItems: 'center',
  gap: 3,
}

// ── PtyTerminal ───────────────────────────────────────────────────────────────

interface PtyTerminalProps {
  wsUrl: string
  label?: string
  onClose?: () => void
  reconnect?: boolean
}

function PtyTerminal({ wsUrl, label, onClose, reconnect = true }: PtyTerminalProps) {
  const containerRef = useRef<HTMLDivElement>(null)
  const termRef      = useRef<XTerm | null>(null)
  const wsRef        = useRef<WebSocket | null>(null)
  const fitRef       = useRef<FitAddon | null>(null)
  const deadRef      = useRef(false)
  const [connected, setConnected] = useState(false)
  const [fontSize, setFontSize]   = useState(() => Number(localStorage.getItem('vt-term-fontsize') ?? 13))

  const paste = useCallback(async () => {
    try {
      const text = await navigator.clipboard.readText()
      if (wsRef.current?.readyState === WebSocket.OPEN)
        wsRef.current.send(JSON.stringify({ type: 'input', data: text }))
    } catch { /* clipboard permission denied */ }
  }, [])

  // Font size: apply to live terminal and persist
  const changeFontSize = useCallback((delta: number) => {
    setFontSize(prev => {
      const next = Math.min(28, Math.max(9, prev + delta))
      if (termRef.current) (termRef.current.options as { fontSize: number }).fontSize = next
      // re-fit after resize
      setTimeout(() => {
        fitRef.current?.fit()
        if (wsRef.current?.readyState === WebSocket.OPEN && termRef.current)
          wsRef.current.send(JSON.stringify({ type: 'resize', cols: termRef.current.cols, rows: termRef.current.rows }))
      }, 50)
      localStorage.setItem('vt-term-fontsize', String(next))
      return next
    })
  }, [])

  useEffect(() => {
    const container = containerRef.current
    if (!container) return
    deadRef.current = false

    const term = new XTerm({
      fontFamily: 'JetBrains Mono, Fira Code, Cascadia Code, SF Mono, Consolas, monospace',
      fontSize,
      theme: TERM_THEME,
      cursorBlink: true,
      scrollback: 5000,
      allowProposedApi: true,
    })
    const fit   = new FitAddon()
    const links = new WebLinksAddon()
    term.loadAddon(fit)
    term.loadAddon(links)
    term.open(container)
    fit.fit()
    termRef.current = term
    fitRef.current  = fit

    // Auto-copy selection to clipboard
    term.onSelectionChange(() => {
      const sel = term.getSelection()
      if (sel) navigator.clipboard.writeText(sel).catch(() => {})
    })

    // Ctrl+Shift+V → paste
    term.attachCustomKeyEventHandler(e => {
      if (e.type === 'keydown' && e.ctrlKey && e.shiftKey && e.key === 'V') { paste(); return false }
      return true
    })

    let reconnectTimer: ReturnType<typeof setTimeout> | null = null

    const connect = () => {
      if (deadRef.current) return
      const ws = new WebSocket(wsUrl)
      wsRef.current = ws

      ws.onopen = () => {
        setConnected(true)
        ws.send(JSON.stringify({ type: 'resize', cols: term.cols, rows: term.rows }))
      }
      ws.onmessage = e => {
        try {
          const msg = JSON.parse(e.data as string)
          if (msg.type === 'output') term.write(msg.data as string)
          if (msg.type === 'closed') {
            term.writeln('\r\n\x1b[31m[session closed]\x1b[0m')
            if (!reconnect) deadRef.current = true
          }
        } catch { /* ignore */ }
      }
      ws.onclose = () => {
        setConnected(false)
        if (!deadRef.current && reconnect) {
          term.writeln('\r\n\x1b[33m[disconnected — reconnecting in 3s…]\x1b[0m')
          reconnectTimer = setTimeout(connect, 3000)
        }
      }
      ws.onerror = () => setConnected(false)
    }

    term.onData(data => {
      if (wsRef.current?.readyState === WebSocket.OPEN)
        wsRef.current.send(JSON.stringify({ type: 'input', data }))
    })

    connect()

    const ro = new ResizeObserver(() => {
      fit.fit()
      if (wsRef.current?.readyState === WebSocket.OPEN)
        wsRef.current.send(JSON.stringify({ type: 'resize', cols: term.cols, rows: term.rows }))
    })
    ro.observe(container)

    return () => {
      deadRef.current = true
      if (reconnectTimer) clearTimeout(reconnectTimer)
      ro.disconnect()
      wsRef.current?.close()
      term.dispose()
    }
    // wsUrl intentional dep only — don't recreate on fontSize change
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [wsUrl])

  return (
    <div style={{ display: 'flex', flexDirection: 'column', flex: 1, minHeight: 0 }}>
      {/* Toolbar */}
      <div style={{
        display: 'flex', alignItems: 'center', gap: 6, padding: '4px 10px',
        background: 'var(--bg-panel)', borderBottom: '1px solid var(--border-subtle)', flexShrink: 0,
      }}>
        {/* Connection indicator */}
        <span title={connected ? 'Connected' : 'Disconnected'}>
          {connected
            ? <Wifi size={13} style={{ color: 'var(--accent-success)' }} />
            : <WifiOff size={13} style={{ color: 'var(--accent-danger)' }} />}
        </span>

        {label && <span style={{ fontSize: 12, color: 'var(--text-muted)', flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{label}</span>}
        {!label && <span style={{ flex: 1 }} />}

        {/* Paste */}
        <button onClick={paste} title="Paste (Ctrl+Shift+V)" style={toolBtn}>
          <Clipboard size={11} /> Paste
        </button>

        {/* Font size */}
        <div style={{ display: 'flex', alignItems: 'center', gap: 2 }}>
          <button onClick={() => changeFontSize(-1)} style={toolBtn} title="Decrease font size">A-</button>
          <span style={{ fontSize: 11, color: 'var(--text-muted)', minWidth: 20, textAlign: 'center' }}>{fontSize}</span>
          <button onClick={() => changeFontSize(1)}  style={toolBtn} title="Increase font size">A+</button>
        </div>

        {/* Clear */}
        <button onClick={() => termRef.current?.clear()} style={toolBtn} title="Clear scrollback">
          Clear
        </button>

        {onClose && (
          <button onClick={onClose} style={{ ...toolBtn, color: 'var(--accent-danger)', border: 'none' }} title="Close">
            <X size={13} />
          </button>
        )}
      </div>

      <div ref={containerRef} style={{ flex: 1, background: TERM_THEME.background, minHeight: 0, overflow: 'hidden' }} />
    </div>
  )
}

// ── Multi-tab local terminal ──────────────────────────────────────────────────

interface LocalTab {
  id: string
  label: string
}

function LocalTerminalArea() {
  const [tabs, setTabs]       = useState<LocalTab[]>([{ id: crypto.randomUUID(), label: 'Shell 1' }])
  const [activeId, setActiveId] = useState<string>(() => tabs[0].id)

  const newTab = () => {
    const id    = crypto.randomUUID()
    const label = `Shell ${tabs.length + 1}`
    setTabs(t => [...t, { id, label }])
    setActiveId(id)
  }

  const closeTab = (id: string) => {
    setTabs(t => {
      const next = t.filter(x => x.id !== id)
      if (next.length === 0) return [{ id: crypto.randomUUID(), label: 'Shell 1' }]
      return next
    })
    setActiveId(cur => {
      if (cur !== id) return cur
      const idx = tabs.findIndex(t => t.id === id)
      return tabs[idx + 1]?.id ?? tabs[idx - 1]?.id ?? tabs[0].id
    })
  }

  return (
    <div style={{ display: 'flex', flexDirection: 'column', flex: 1, minHeight: 0 }}>
      {/* Tab bar */}
      <div style={{
        display: 'flex', alignItems: 'center', gap: 2,
        background: 'var(--bg-panel)', borderBottom: '1px solid var(--border-subtle)',
        padding: '0 8px', flexShrink: 0, overflowX: 'auto',
      }}>
        {tabs.map(tab => (
          <div key={tab.id} style={{ display: 'flex', alignItems: 'center' }}>
            <button
              onClick={() => setActiveId(tab.id)}
              style={{
                padding: '6px 12px', border: 'none', cursor: 'pointer', fontSize: 12, fontWeight: 500,
                background: activeId === tab.id ? 'var(--bg-root)' : 'transparent',
                color: activeId === tab.id ? 'var(--text-primary)' : 'var(--text-muted)',
                borderBottom: activeId === tab.id ? '2px solid var(--accent-primary)' : '2px solid transparent',
              }}
            >
              {tab.label}
            </button>
            {tabs.length > 1 && (
              <button onClick={() => closeTab(tab.id)}
                style={{ background: 'none', border: 'none', cursor: 'pointer', color: 'var(--text-disabled)', padding: '0 2px', display: 'flex' }}>
                <X size={11} />
              </button>
            )}
          </div>
        ))}
        <button onClick={newTab} title="New shell tab"
          style={{ background: 'none', border: 'none', cursor: 'pointer', color: 'var(--text-muted)', padding: '6px 8px', display: 'flex', flexShrink: 0 }}>
          <Plus size={13} />
        </button>
      </div>

      {/* Terminal panels — keep all mounted so they stay alive */}
      {tabs.map(tab => (
        <div key={tab.id} style={{ display: activeId === tab.id ? 'flex' : 'none', flex: 1, minHeight: 0, flexDirection: 'column' }}>
          <PtyTerminal wsUrl={api.terminal.wsUrl()} label={tab.label} />
        </div>
      ))}
    </div>
  )
}

// ── SSH session form ──────────────────────────────────────────────────────────

interface SessionFormProps {
  initial?: SshSession
  onSaved: (s: SshSession) => void
  onCancel: () => void
}

function SessionForm({ initial, onSaved, onCancel }: SessionFormProps) {
  const uid = useId()
  const [form, setForm] = useState({
    label:    initial?.label    ?? '',
    host:     initial?.host     ?? '',
    port:     String(initial?.port ?? 22),
    username: initial?.username ?? '',
    key_path: initial?.key_path ?? '',
    password: '',
  })
  const [saving, setSaving] = useState(false)
  const [showPass, setShowPass] = useState(false)

  const f = (k: keyof typeof form) => ({
    value: form[k],
    onChange: (e: React.ChangeEvent<HTMLInputElement>) => setForm(p => ({ ...p, [k]: e.target.value })),
  })

  const save = async () => {
    if (!form.label.trim() || !form.host.trim() || !form.username.trim()) {
      notify.error('Label, host and username are required')
      return
    }
    setSaving(true)
    try {
      const payload = {
        label:    form.label.trim(),
        host:     form.host.trim(),
        port:     Number(form.port) || 22,
        username: form.username.trim(),
        key_path: form.key_path.trim() || undefined,
        password: form.password || undefined,
      }
      const saved = initial
        ? await api.terminal.updateSshSession(initial.id, payload)
        : await api.terminal.createSshSession(payload)
      onSaved(saved)
    } catch { notify.error('Failed to save session') }
    finally { setSaving(false) }
  }

  const input = (label: string, key: keyof typeof form, placeholder: string, type = 'text') => (
    <div key={key} style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
      <label htmlFor={uid + key} style={{ fontSize: 12, color: 'var(--text-muted)', fontWeight: 500 }}>{label}</label>
      <input id={uid + key} type={type} placeholder={placeholder} {...f(key)}
        style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)', borderRadius: 6, color: 'var(--text-primary)', padding: '6px 10px', fontSize: 13, outline: 'none' }} />
    </div>
  )

  return (
    <div style={{ background: 'var(--bg-card)', border: '1px solid var(--border-subtle)', borderRadius: 8, padding: 16 }}>
      <h3 style={{ fontSize: 14, fontWeight: 600, color: 'var(--text-primary)', marginBottom: 12 }}>
        {initial ? 'Edit session' : 'New SSH session'}
      </h3>
      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(180px, 1fr))', gap: 10, marginBottom: 12 }}>
        {input('Label',              'label',    'Home server')}
        {input('Host / IP',          'host',     '192.168.1.10')}
        {input('Port',               'port',     '22', 'number')}
        {input('Username',           'username', 'root')}
        {input('SSH key path (opt)', 'key_path', '~/.ssh/id_rsa')}
        <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
          <label style={{ fontSize: 12, color: 'var(--text-muted)', fontWeight: 500 }}>
            Password (opt){initial?.password_set && !form.password && <span style={{ color: 'var(--accent-primary)', marginLeft: 6 }}>●&nbsp;saved</span>}
          </label>
          <div style={{ position: 'relative' }}>
            <input
              type={showPass ? 'text' : 'password'}
              placeholder={initial?.password_set ? '(keep existing)' : 'stored encrypted'}
              {...f('password')}
              style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)', borderRadius: 6, color: 'var(--text-primary)', padding: '6px 32px 6px 10px', fontSize: 13, outline: 'none', width: '100%', boxSizing: 'border-box' }}
            />
            <button type="button" onClick={() => setShowPass(v => !v)}
              style={{ position: 'absolute', right: 6, top: '50%', transform: 'translateY(-50%)', background: 'none', border: 'none', cursor: 'pointer', color: 'var(--text-muted)', padding: 2 }}>
              {showPass ? <ChevronUp size={12} /> : <ChevronDown size={12} />}
            </button>
          </div>
        </div>
      </div>
      <p style={{ fontSize: 12, color: 'var(--text-muted)', marginBottom: 12 }}>
        Passwords are stored encrypted. Leave blank to enter interactively. Key path auth is preferred when set.
        Auto-fill requires <code>sshpass</code> installed on the server.
      </p>
      <div style={{ display: 'flex', gap: 8 }}>
        <Button size="sm" onClick={save} loading={saving}><Check size={12} /> Save</Button>
        <Button size="sm" variant="ghost" onClick={onCancel}>Cancel</Button>
      </div>
    </div>
  )
}

// ── Session card ──────────────────────────────────────────────────────────────

function SessionCard({ session, active, onConnect, onEdit, onDelete }: {
  session: SshSession
  active: boolean
  onConnect: () => void
  onEdit: () => void
  onDelete: () => void
}) {
  const lastUsed = session.last_used
    ? new Date(session.last_used * 1000).toLocaleString(undefined, { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' })
    : 'never'

  return (
    <div
      onClick={onConnect}
      style={{
        background: active ? 'var(--accent-primary-subtle)' : 'var(--bg-card)',
        border: `1px solid ${active ? 'var(--accent-primary)' : 'var(--border-subtle)'}`,
        borderRadius: 8, padding: '10px 14px', cursor: 'pointer',
        display: 'flex', alignItems: 'center', gap: 10,
        transition: 'border-color 0.15s',
      }}
    >
      <Server size={18} style={{ color: active ? 'var(--accent-primary)' : 'var(--text-muted)', flexShrink: 0 }} />
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ fontWeight: 600, fontSize: 13, color: 'var(--text-primary)' }}>{session.label}</div>
        <div style={{ fontSize: 11, color: 'var(--text-muted)', fontFamily: 'monospace' }}>
          {session.username}@{session.host}:{session.port}
          {session.key_path && <span style={{ marginLeft: 8, opacity: 0.7 }}><Key size={9} style={{ display: 'inline', marginRight: 2 }} />{session.key_path}</span>}
          {session.password_set && !session.key_path && <span style={{ marginLeft: 8, color: 'var(--accent-primary)', opacity: 0.8 }}>🔑 pw</span>}
        </div>
      </div>
      <div style={{ fontSize: 10, color: 'var(--text-disabled)', textAlign: 'right', flexShrink: 0 }}>
        {lastUsed}
      </div>
      <button onClick={e => { e.stopPropagation(); onEdit() }}
        style={{ background: 'none', border: 'none', cursor: 'pointer', color: 'var(--text-muted)', padding: 4 }} title="Edit">
        <Pencil size={12} />
      </button>
      <button onClick={e => { e.stopPropagation(); onDelete() }}
        style={{ background: 'none', border: 'none', cursor: 'pointer', color: 'var(--accent-danger)', padding: 4 }} title="Delete">
        <Trash2 size={12} />
      </button>
    </div>
  )
}

// ── SSH area ──────────────────────────────────────────────────────────────────

function SshArea() {
  const [sessions, setSessions]       = useState<SshSession[]>([])
  const [activeSession, setActive]    = useState<SshSession | null>(null)
  const [showNewForm, setShowNewForm] = useState(false)
  const [editSession, setEditSession] = useState<SshSession | null>(null)

  const load = useCallback(async () => {
    try { setSessions(await api.terminal.listSshSessions()) } catch { /* empty */ }
  }, [])

  useEffect(() => { load() }, [load])

  const deleteSession = async (id: string) => {
    if (!confirm('Delete this saved SSH session?')) return
    try { await api.terminal.deleteSshSession(id); load() }
    catch { notify.error('Failed to delete session') }
  }

  return (
    <div style={{ display: 'flex', gap: 16, flex: 1, minHeight: 0, overflow: 'hidden' }}>

      {/* Left sidebar — sessions list */}
      <div style={{
        width: 260, flexShrink: 0, display: 'flex', flexDirection: 'column', gap: 10,
        overflowY: 'auto', paddingRight: 4,
      }}>
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
          <span style={{ fontSize: 12, fontWeight: 600, color: 'var(--text-secondary)', textTransform: 'uppercase', letterSpacing: '0.06em' }}>Saved Sessions</span>
          <button onClick={() => { setShowNewForm(true); setEditSession(null) }}
            style={{ background: 'none', border: 'none', cursor: 'pointer', color: 'var(--accent-primary)', display: 'flex', alignItems: 'center', gap: 3, fontSize: 12 }}>
            <Plus size={13} /> New
          </button>
        </div>

        {(showNewForm || editSession) && (
          <SessionForm
            initial={editSession ?? undefined}
            onSaved={s => {
              load()
              setShowNewForm(false)
              setEditSession(null)
              setActive(s)
            }}
            onCancel={() => { setShowNewForm(false); setEditSession(null) }}
          />
        )}

        {sessions.length === 0 && !showNewForm ? (
          <div style={{ color: 'var(--text-muted)', fontSize: 12, padding: '16px 0', textAlign: 'center' }}>
            No sessions saved yet.
          </div>
        ) : (
          sessions.map(s => (
            <SessionCard key={s.id} session={s} active={activeSession?.id === s.id}
              onConnect={() => setActive(s)}
              onEdit={() => { setEditSession(s); setShowNewForm(false) }}
              onDelete={() => { deleteSession(s.id); if (activeSession?.id === s.id) setActive(null) }}
            />
          ))
        )}
      </div>

      {/* Right — terminal or prompt */}
      <div style={{ flex: 1, display: 'flex', flexDirection: 'column', minWidth: 0, border: '1px solid var(--border-subtle)', borderRadius: 8, overflow: 'hidden' }}>
        {activeSession ? (
          <PtyTerminal
            key={activeSession.id}
            wsUrl={api.terminal.sshWsUrl(activeSession.id)}
            label={`${activeSession.username}@${activeSession.host}:${activeSession.port} — ${activeSession.label}`}
            onClose={() => setActive(null)}
            reconnect={false}
          />
        ) : (
          <div style={{ flex: 1, display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center', gap: 10, color: 'var(--text-muted)' }}>
            <Server size={36} style={{ opacity: 0.25 }} />
            <span style={{ fontSize: 13 }}>Select a session to connect</span>
            <Button size="sm" variant="ghost" onClick={() => setShowNewForm(true)}>
              <Plus size={12} /> Add session
            </Button>
          </div>
        )}
      </div>
    </div>
  )
}

// ── Page ──────────────────────────────────────────────────────────────────────

export default function TerminalPage() {
  const user = useAuthStore(s => s.user)
  const [tab, setTab] = useState<'local' | 'ssh'>('local')

  if (user?.role === 'viewer') {
    return (
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'center', height: 160, color: 'var(--text-muted)', fontSize: 14 }}>
        Terminal access requires operator role or higher.
      </div>
    )
  }

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: 'calc(100vh - 88px)', gap: 12 }}>
      {/* Header + tabs */}
      <div style={{ display: 'flex', alignItems: 'center', gap: 12, flexShrink: 0 }}>
        <h1 style={{ fontSize: 18, fontWeight: 600, color: 'var(--text-primary)', margin: 0 }}>Terminal</h1>
        <div style={{ display: 'flex', gap: 4, background: 'var(--bg-panel)', borderRadius: 8, padding: 4 }}>
          {([['local', Monitor, 'Local Shell'], ['ssh', Server, 'SSH']] as const).map(([id, Icon, label]) => (
            <button key={id} onClick={() => setTab(id)} style={{
              display: 'flex', alignItems: 'center', gap: 6, padding: '5px 14px', borderRadius: 6,
              border: 'none', cursor: 'pointer', fontSize: 13, fontWeight: 500,
              background: tab === id ? 'var(--accent-primary)' : 'transparent',
              color: tab === id ? '#fff' : 'var(--text-secondary)',
            }}>
              <Icon size={14} /> {label}
            </button>
          ))}
        </div>
      </div>

      {/* Content */}
      {tab === 'local' && <LocalTerminalArea />}
      {tab === 'ssh'   && <SshArea />}
    </div>
  )
}
