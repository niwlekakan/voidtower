import { useEffect, useState, useRef, useCallback } from 'react'
import { Bell, WifiOff, Wifi, LayoutGrid, CheckCheck, X } from 'lucide-react'
import { useMetricsStore } from '@/store/metrics'
import { useAiosStore } from '@/aios/store/aios'
import UiModeToggle from '@/components/ui/UiModeToggle'
import type { DeviceTier } from '@/aios/hooks/useDeviceTier'
import { PRESET_LIST } from '@/aios/AiosPresets'
import type { PresetName } from '@/aios/store/aios'

interface Props {
  tier: DeviceTier
}

// ── Backend alert type ────────────────────────────────────────────────────────

interface BackendAlert {
  id: string
  title: string
  message: string
  severity: string   // 'danger' | 'warning' | 'info' | 'critical'
  category: string
  node_id: string | null
  resource_type: string | null
  resource_id: string | null
  state: string
  acknowledged_by: string | null
  acknowledged_at: number | null
  resolved_at: number | null
  created_at: number
  updated_at: number
}

// ── Helpers ──────────────────────────────────────────────────────────────────

function fmtBytes(b: number): string {
  if (b < 1024) return `${b}B`
  if (b < 1024 * 1024) return `${(b / 1024).toFixed(0)}K`
  return `${(b / (1024 * 1024)).toFixed(1)}M`
}

function fmtUptime(secs: number): string {
  const d = Math.floor(secs / 86400)
  const h = Math.floor((secs % 86400) / 3600)
  const m = Math.floor((secs % 3600) / 60)
  if (d > 0) return `${d}d ${h}h`
  if (h > 0) return `${h}h ${m}m`
  return `${m}m`
}

function fmtTs(unix: number): string {
  const d = new Date(unix * 1000)
  const now = Date.now()
  const diff = Math.floor((now - unix * 1000) / 1000)
  if (diff < 60) return 'just now'
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`
  return d.toLocaleDateString([], { month: 'short', day: 'numeric' })
}

function useClock(): string {
  const [time, setTime] = useState(() => {
    const now = new Date()
    return now.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', hour12: false })
  })
  useEffect(() => {
    const id = setInterval(() => {
      setTime(new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', hour12: false }))
    }, 10_000)
    return () => clearInterval(id)
  }, [])
  return time
}

// ── Sub-components ────────────────────────────────────────────────────────────

function MetricPill({
  label, value, pct,
}: { label: string; value: string; pct?: number }) {
  const warningColor = pct !== undefined && pct > 85
    ? 'var(--accent-danger)'
    : pct !== undefined && pct > 60
      ? 'var(--accent-warning)'
      : 'var(--text-muted)'

  return (
    <span style={{ display: 'flex', alignItems: 'center', gap: 3 }}>
      <span style={{ color: 'rgba(255,255,255,0.35)', fontSize: 10 }}>{label}</span>
      <span style={{ color: pct !== undefined ? warningColor : 'var(--text-muted)', fontVariantNumeric: 'tabular-nums', fontSize: 11 }}>
        {value}
      </span>
    </span>
  )
}

function Clock() {
  const time = useClock()
  return (
    <span style={{ fontVariantNumeric: 'tabular-nums', fontSize: 12, color: 'var(--text-primary)' }}>
      {time}
    </span>
  )
}

function WorkspaceDots() {
  const { activeWorkspace, workspaceNames, setWorkspace, renameWorkspace } = useAiosStore()
  const [editing, setEditing] = useState<number | null>(null)
  const [draft, setDraft] = useState('')
  const inputRef = useRef<HTMLInputElement>(null)

  const startEdit = (i: number) => {
    setEditing(i)
    setDraft(workspaceNames[i])
    setTimeout(() => inputRef.current?.select(), 0)
  }

  const commitEdit = () => {
    if (editing !== null) renameWorkspace(editing as 0|1|2|3, draft)
    setEditing(null)
  }

  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 5 }}>
      {([0, 1, 2, 3] as const).map((i) => {
        const isActive = i === activeWorkspace
        const name = workspaceNames[i]

        if (editing === i) {
          return (
            <input
              key={i}
              ref={inputRef}
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
              onBlur={commitEdit}
              onKeyDown={(e) => {
                if (e.key === 'Enter') commitEdit()
                if (e.key === 'Escape') setEditing(null)
              }}
              style={{
                width: 64, height: 18, fontSize: 10, padding: '0 4px',
                borderRadius: 4, border: '1px solid var(--accent-primary)',
                background: 'var(--bg-elevated)', color: 'var(--text-primary)',
                outline: 'none',
              }}
            />
          )
        }

        return (
          <button
            key={i}
            onClick={() => setWorkspace(i)}
            onDoubleClick={() => startEdit(i)}
            aria-label={name}
            title={`${name} (double-click to rename)`}
            style={{
              height: 18,
              padding: isActive ? '0 6px' : '0',
              width: isActive ? 'auto' : 6,
              minWidth: isActive ? 24 : 6,
              borderRadius: 9,
              border: 'none',
              cursor: 'pointer',
              transition: 'width 0.2s, background 0.2s, padding 0.2s',
              background: isActive ? 'var(--accent-primary)' : 'rgba(255,255,255,0.25)',
              color: '#fff',
              fontSize: 9,
              fontWeight: 600,
              letterSpacing: '0.03em',
              whiteSpace: 'nowrap',
              overflow: 'hidden',
              flexShrink: 0,
            }}
          >
            {isActive ? name : ''}
          </button>
        )
      })}
    </div>
  )
}

// ── Notification Center ───────────────────────────────────────────────────────

const SEV_COLOR: Record<string, string> = {
  critical: '#ef4444',
  danger:   '#ef4444',
  warning:  '#f59e0b',
  info:     '#3b82f6',
}
const SEV_BG: Record<string, string> = {
  critical: 'rgba(239,68,68,0.10)',
  danger:   'rgba(239,68,68,0.10)',
  warning:  'rgba(245,158,11,0.10)',
  info:     'rgba(59,130,246,0.10)',
}
const SEV_ORDER: Record<string, number> = {
  critical: 0, danger: 1, warning: 2, info: 3,
}
const SEV_LABEL: Record<string, string> = {
  critical: 'Critical', danger: 'Danger', warning: 'Warning', info: 'Info',
}

function NotifBell({ tier }: { tier: DeviceTier }) {
  const [open, setOpen] = useState(false)
  const [alerts, setAlerts] = useState<BackendAlert[]>([])
  const [dismissed, setDismissed] = useState<Set<string>>(new Set())
  const [acking, setAcking] = useState<Set<string>>(new Set())
  const isPhone = tier === 'phone'
  const BAR_H = 28
  const { openOdysseus } = useAiosStore()

  const fetchAlerts = useCallback(async () => {
    try {
      const res = await fetch('/api/alerts?state=active&limit=50', { credentials: 'include' })
      if (!res.ok) return
      const data = await res.json() as { alerts: BackendAlert[] }
      setAlerts(data.alerts ?? [])
    } catch {
      // swallow — backend may not be running in dev
    }
  }, [])

  // Initial fetch + polling every 30s
  useEffect(() => {
    fetchAlerts()
    const id = setInterval(fetchAlerts, 30_000)
    return () => clearInterval(id)
  }, [fetchAlerts])

  const visible = alerts.filter((a) => !dismissed.has(a.id))
  const count = visible.length

  const dismiss = useCallback((id: string) => {
    setDismissed((d) => new Set([...d, id]))
  }, [])

  const acknowledge = useCallback(async (id: string) => {
    setAcking((s) => new Set([...s, id]))
    try {
      await fetch(`/api/alerts/${id}/acknowledge`, { method: 'POST', credentials: 'include' })
      setDismissed((d) => new Set([...d, id]))
      setAlerts((prev) => prev.filter((a) => a.id !== id))
    } catch {
      // swallow
    } finally {
      setAcking((s) => { const n = new Set(s); n.delete(id); return n })
    }
  }, [])

  const clearAll = useCallback(async () => {
    const ids = visible.map((a) => a.id)
    ids.forEach((id) => dismiss(id))
    // Best-effort ack all
    for (const id of ids) {
      try {
        await fetch(`/api/alerts/${id}/acknowledge`, { method: 'POST', credentials: 'include' })
      } catch {
        // swallow
      }
    }
    setAlerts([])
  }, [visible, dismiss])

  // Group by severity
  const grouped = visible.reduce<Record<string, BackendAlert[]>>((acc, a) => {
    const sev = a.severity || 'info'
    if (!acc[sev]) acc[sev] = []
    acc[sev].push(a)
    return acc
  }, {})
  const severities = Object.keys(grouped).sort(
    (a, b) => (SEV_ORDER[a] ?? 99) - (SEV_ORDER[b] ?? 99),
  )

  const dropdownW = isPhone ? Math.min(window.innerWidth - 16, 360) : 380

  return (
    <div style={{ position: 'relative' }}>
      <button
        onClick={() => setOpen((o) => !o)}
        aria-label="Notifications"
        style={{
          background: 'none', border: 'none', cursor: 'pointer',
          color: count > 0 ? 'var(--accent-warning)' : 'var(--text-muted)',
          padding: 3, lineHeight: 1,
          display: 'flex', alignItems: 'center', justifyContent: 'center',
          position: 'relative',
          transition: 'color 0.2s',
        }}
      >
        <Bell size={13} />
        {count > 0 && (
          <span style={{
            position: 'absolute', top: -2, right: -2,
            minWidth: 14, height: 14, borderRadius: 7,
            background: count > 0
              ? (grouped['critical'] || grouped['danger'] ? 'var(--accent-danger)' : 'var(--accent-warning)')
              : 'var(--accent-danger)',
            color: '#fff',
            fontSize: 9, fontWeight: 700,
            display: 'flex', alignItems: 'center', justifyContent: 'center',
            padding: '0 3px',
            lineHeight: 1,
          }}>
            {count > 99 ? '99+' : count}
          </span>
        )}
      </button>

      {open && (
        <>
          <div
            style={{ position: 'fixed', inset: 0, zIndex: 10001 }}
            onClick={() => setOpen(false)}
          />
          <div style={{
            position: 'absolute',
            top: BAR_H - 4,
            right: isPhone ? undefined : 0,
            left: isPhone ? '50%' : undefined,
            transform: isPhone ? 'translateX(-50%)' : undefined,
            width: dropdownW,
            maxHeight: 480,
            background: 'rgba(8,8,12,0.88)',
            backdropFilter: 'blur(28px)',
            WebkitBackdropFilter: 'blur(28px)',
            border: '1px solid rgba(255,255,255,0.12)',
            borderRadius: 12,
            boxShadow: '0 12px 48px rgba(0,0,0,0.6), 0 2px 8px rgba(0,0,0,0.4)',
            zIndex: 10002,
            display: 'flex',
            flexDirection: 'column',
            overflow: 'hidden',
          }}>
            {/* Header */}
            <div style={{
              padding: '10px 14px',
              borderBottom: '1px solid rgba(255,255,255,0.08)',
              display: 'flex', alignItems: 'center', justifyContent: 'space-between',
              flexShrink: 0,
            }}>
              <span style={{ fontSize: 12, fontWeight: 700, color: 'var(--text-primary)', letterSpacing: '0.02em' }}>
                Notifications
              </span>
              <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                {count > 0 && (
                  <button
                    onClick={clearAll}
                    title="Clear all"
                    style={{
                      background: 'none', border: 'none', cursor: 'pointer',
                      color: 'rgba(255,255,255,0.45)', fontSize: 11,
                      display: 'flex', alignItems: 'center', gap: 4,
                      padding: '2px 6px', borderRadius: 4,
                      transition: 'color 0.15s, background 0.15s',
                    }}
                    onMouseEnter={(e) => { (e.currentTarget as HTMLButtonElement).style.color = 'var(--text-primary)'; (e.currentTarget as HTMLButtonElement).style.background = 'rgba(255,255,255,0.08)' }}
                    onMouseLeave={(e) => { (e.currentTarget as HTMLButtonElement).style.color = 'rgba(255,255,255,0.45)'; (e.currentTarget as HTMLButtonElement).style.background = 'none' }}
                  >
                    <CheckCheck size={11} />
                    Clear all
                  </button>
                )}
                <button
                  onClick={() => openOdysseus()}
                  title="Send to Odysseus"
                  style={{
                    background: 'rgba(139,92,246,0.15)', border: '1px solid rgba(139,92,246,0.3)',
                    borderRadius: 5, cursor: 'pointer',
                    color: '#a78bfa', fontSize: 10, fontWeight: 600,
                    padding: '2px 8px', letterSpacing: '0.02em',
                  }}
                >
                  Odysseus
                </button>
              </div>
            </div>

            {/* Alert list */}
            <div style={{ overflowY: 'auto', flex: 1 }}>
              {count === 0 && (
                <div style={{
                  padding: '32px 20px', textAlign: 'center',
                  color: 'rgba(255,255,255,0.3)', fontSize: 12,
                }}>
                  <Bell size={24} style={{ opacity: 0.2, marginBottom: 8, display: 'block', margin: '0 auto 8px' }} />
                  No active alerts
                </div>
              )}

              {severities.map((sev) => (
                <div key={sev}>
                  {/* Severity group header */}
                  <div style={{
                    padding: '6px 14px 4px',
                    fontSize: 10, fontWeight: 700,
                    color: SEV_COLOR[sev] ?? 'var(--text-muted)',
                    letterSpacing: '0.08em',
                    textTransform: 'uppercase',
                    background: 'rgba(0,0,0,0.2)',
                    borderBottom: `1px solid ${SEV_COLOR[sev] ?? 'rgba(255,255,255,0.08)'}22`,
                    display: 'flex', alignItems: 'center', gap: 6,
                  }}>
                    <span style={{
                      display: 'inline-block', width: 6, height: 6, borderRadius: '50%',
                      background: SEV_COLOR[sev] ?? 'var(--text-muted)',
                      boxShadow: `0 0 6px ${SEV_COLOR[sev] ?? 'transparent'}`,
                      flexShrink: 0,
                    }} />
                    {SEV_LABEL[sev] ?? sev}
                    <span style={{ marginLeft: 'auto', opacity: 0.6, fontWeight: 400 }}>
                      {grouped[sev].length}
                    </span>
                  </div>

                  {/* Alert items */}
                  {grouped[sev].map((alert) => (
                    <div
                      key={alert.id}
                      style={{
                        padding: '10px 14px',
                        borderBottom: '1px solid rgba(255,255,255,0.05)',
                        background: SEV_BG[sev] ?? 'transparent',
                        borderLeft: `3px solid ${SEV_COLOR[sev] ?? 'transparent'}`,
                        display: 'flex', flexDirection: 'column', gap: 4,
                      }}
                    >
                      <div style={{ display: 'flex', alignItems: 'flex-start', justifyContent: 'space-between', gap: 8 }}>
                        <span style={{ fontSize: 12, fontWeight: 600, color: 'var(--text-primary)', lineHeight: 1.3 }}>
                          {alert.title}
                        </span>
                        <div style={{ display: 'flex', gap: 4, flexShrink: 0 }}>
                          <button
                            onClick={() => acknowledge(alert.id)}
                            disabled={acking.has(alert.id)}
                            title="Acknowledge"
                            style={{
                              background: 'rgba(255,255,255,0.08)',
                              border: '1px solid rgba(255,255,255,0.12)',
                              borderRadius: 4, cursor: 'pointer',
                              color: 'rgba(255,255,255,0.6)', fontSize: 10,
                              padding: '2px 7px', lineHeight: 1.4,
                              opacity: acking.has(alert.id) ? 0.4 : 1,
                              transition: 'opacity 0.15s',
                            }}
                          >
                            {acking.has(alert.id) ? '…' : 'Ack'}
                          </button>
                          <button
                            onClick={() => dismiss(alert.id)}
                            title="Dismiss"
                            style={{
                              background: 'none', border: 'none', cursor: 'pointer',
                              color: 'rgba(255,255,255,0.3)', padding: 2, lineHeight: 1,
                              display: 'flex', alignItems: 'center',
                            }}
                          >
                            <X size={11} />
                          </button>
                        </div>
                      </div>
                      <span style={{ fontSize: 11, color: 'rgba(255,255,255,0.55)', lineHeight: 1.4 }}>
                        {alert.message}
                      </span>
                      <span style={{ fontSize: 10, color: 'rgba(255,255,255,0.25)' }}>
                        {alert.category}
                        {alert.resource_type ? ` · ${alert.resource_type}` : ''}
                        {alert.resource_id ? ` / ${alert.resource_id}` : ''}
                        {'  '}
                        {fmtTs(alert.created_at)}
                      </span>
                    </div>
                  ))}
                </div>
              ))}
            </div>

            {/* Footer */}
            <div style={{
              padding: '7px 14px',
              borderTop: '1px solid rgba(255,255,255,0.07)',
              fontSize: 10, color: 'rgba(255,255,255,0.25)',
              display: 'flex', justifyContent: 'space-between', alignItems: 'center',
              flexShrink: 0,
            }}>
              <span>Polls every 30s</span>
              <button
                onClick={fetchAlerts}
                style={{
                  background: 'none', border: 'none', cursor: 'pointer',
                  color: 'rgba(255,255,255,0.35)', fontSize: 10, padding: 0,
                }}
              >
                Refresh
              </button>
            </div>
          </div>
        </>
      )}
    </div>
  )
}

// ── Presets button ─────────────────────────────────────────────────────────────

function PresetsButton() {
  const [open, setOpen] = useState(false)
  const { applyPreset } = useAiosStore()
  const ref = useRef<HTMLDivElement>(null)
  const BAR_H = 28

  const handleApply = useCallback((name: PresetName) => {
    applyPreset(name)
    setOpen(false)
  }, [applyPreset])

  return (
    <div ref={ref} style={{ position: 'relative' }}>
      <button
        onClick={() => setOpen((o) => !o)}
        aria-label="Layout presets"
        title="Layout presets"
        style={{
          background: open ? 'rgba(255,255,255,0.10)' : 'none',
          border: open ? '1px solid rgba(255,255,255,0.15)' : '1px solid transparent',
          borderRadius: 5, cursor: 'pointer',
          color: open ? 'var(--text-primary)' : 'var(--text-muted)',
          padding: '2px 4px', lineHeight: 1,
          display: 'flex', alignItems: 'center', justifyContent: 'center',
          transition: 'color 0.15s, background 0.15s, border-color 0.15s',
        }}
      >
        <LayoutGrid size={12} />
      </button>

      {open && (
        <>
          <div
            style={{ position: 'fixed', inset: 0, zIndex: 10001 }}
            onClick={() => setOpen(false)}
          />
          <div style={{
            position: 'absolute', top: BAR_H - 4, right: 0,
            width: 340,
            background: 'rgba(8,8,12,0.90)',
            backdropFilter: 'blur(28px)',
            WebkitBackdropFilter: 'blur(28px)',
            border: '1px solid rgba(255,255,255,0.12)',
            borderRadius: 12,
            boxShadow: '0 12px 48px rgba(0,0,0,0.6)',
            zIndex: 10002, overflow: 'hidden',
          }}>
            <div style={{
              padding: '10px 14px',
              borderBottom: '1px solid rgba(255,255,255,0.08)',
              fontSize: 12, fontWeight: 700,
              color: 'var(--text-primary)', letterSpacing: '0.02em',
            }}>
              Layout Presets
            </div>

            <div style={{ padding: '8px 10px', display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 8 }}>
              {PRESET_LIST.map((preset) => (
                <button
                  key={preset.name}
                  onClick={() => handleApply(preset.name)}
                  style={{
                    background: 'rgba(255,255,255,0.04)',
                    border: '1px solid rgba(255,255,255,0.10)',
                    borderRadius: 8, cursor: 'pointer',
                    padding: '10px 10px 8px',
                    textAlign: 'left',
                    transition: 'background 0.15s, border-color 0.15s',
                    display: 'flex', flexDirection: 'column', gap: 6,
                  }}
                  onMouseEnter={(e) => {
                    ;(e.currentTarget as HTMLButtonElement).style.background = 'rgba(255,255,255,0.09)'
                    ;(e.currentTarget as HTMLButtonElement).style.borderColor = 'rgba(255,255,255,0.20)'
                  }}
                  onMouseLeave={(e) => {
                    ;(e.currentTarget as HTMLButtonElement).style.background = 'rgba(255,255,255,0.04)'
                    ;(e.currentTarget as HTMLButtonElement).style.borderColor = 'rgba(255,255,255,0.10)'
                  }}
                >
                  {/* ASCII thumbnail */}
                  <pre style={{
                    fontFamily: 'monospace', fontSize: 8, lineHeight: 1.35,
                    color: 'rgba(255,255,255,0.40)', margin: 0,
                    pointerEvents: 'none',
                    whiteSpace: 'pre',
                  }}>
                    {preset.thumbnail}
                  </pre>
                  <div>
                    <div style={{ fontSize: 11, fontWeight: 600, color: 'var(--text-primary)' }}>
                      {preset.label}
                    </div>
                    <div style={{ fontSize: 10, color: 'rgba(255,255,255,0.35)', marginTop: 2, lineHeight: 1.3 }}>
                      {preset.description}
                    </div>
                  </div>
                </button>
              ))}
            </div>

            <div style={{
              padding: '7px 14px',
              borderTop: '1px solid rgba(255,255,255,0.07)',
              fontSize: 10, color: 'rgba(255,255,255,0.25)',
            }}>
              Clears current workspace, then opens the layout
            </div>
          </div>
        </>
      )}
    </div>
  )
}

// ── MetricsStrip ──────────────────────────────────────────────────────────────

function MetricsStrip({ tier }: { tier: DeviceTier }) {
  const { snapshot, connected } = useMetricsStore()

  // Phone: hide metrics, show nothing here (clock shown on right)
  if (tier === 'phone') return null

  const compact = tier === 'tablet'

  if (!snapshot) {
    return (
      <span style={{ fontSize: 10, color: 'rgba(255,255,255,0.3)' }}>
        {connected ? 'loading…' : 'offline'}
      </span>
    )
  }

  const cpuPct = snapshot.cpu_usage
  const ramPct = (snapshot.ram_used / snapshot.ram_total) * 100
  const net = snapshot.networks.reduce(
    (acc, n) => ({ rx: acc.rx + n.rx_bytes_per_sec, tx: acc.tx + n.tx_bytes_per_sec }),
    { rx: 0, tx: 0 },
  )
  const gpu = snapshot.gpu?.length > 0 ? snapshot.gpu[0] : null

  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
      <MetricPill label="CPU" value={`${cpuPct.toFixed(0)}%`} pct={cpuPct} />
      <MetricPill label="RAM" value={`${ramPct.toFixed(0)}%`} pct={ramPct} />
      {!compact && (
        <MetricPill
          label="NET"
          value={`↓${fmtBytes(net.rx)}/s ↑${fmtBytes(net.tx)}/s`}
        />
      )}
      {gpu && !compact && (
        <MetricPill label="GPU" value={`${gpu.util_pct.toFixed(0)}%`} pct={gpu.util_pct} />
      )}
      {!compact && (
        <MetricPill label="UP" value={fmtUptime(snapshot.uptime_secs)} />
      )}
    </div>
  )
}

// ── AiosStatusBar ─────────────────────────────────────────────────────────────

export const STATUS_BAR_H = 28

export default function AiosStatusBar({ tier }: Props) {
  const { connected } = useMetricsStore()
  const { splitPair, uncoupleSplit } = useAiosStore()
  const isPhone = tier === 'phone'
  const isTv = tier === 'tv'

  return (
    <div
      style={{
        position: 'fixed', top: 0, left: 0, right: 0,
        height: STATUS_BAR_H, zIndex: 10000,
        display: 'flex', alignItems: 'center', gap: 10,
        padding: '0 12px',
        // Glass effect
        background: 'rgba(0,0,0,0.40)',
        backdropFilter: 'blur(12px)',
        WebkitBackdropFilter: 'blur(12px)',
        borderBottom: '1px solid rgba(255,255,255,0.08)',
        userSelect: 'none',
        fontSize: isTv ? 15 : 11,
      }}
    >
      {/* Left: VoidTower logo/name */}
      <div style={{ display: 'flex', alignItems: 'center', gap: 5, flexShrink: 0 }}>
        <span style={{ fontSize: isTv ? 22 : 14 }}>⬡</span>
        {!isPhone && (
          <span style={{ fontSize: isTv ? 14 : 11, fontWeight: 700, letterSpacing: '0.05em', color: 'var(--text-primary)' }}>
            VoidTower
          </span>
        )}
      </div>

      {/* Center: live metrics */}
      <div style={{ flex: 1, display: 'flex', justifyContent: 'center', alignItems: 'center' }}>
        {isPhone ? (
          // Phone: workspace dots in center
          <WorkspaceDots />
        ) : (
          <MetricsStrip tier={tier} />
        )}
      </div>

      {/* Right: workspaces + badge + clock + mode toggle */}
      <div style={{ display: 'flex', alignItems: 'center', gap: 10, flexShrink: 0 }}>
        {/* Split exit button */}
        {splitPair && !isPhone && (
          <button
            onClick={uncoupleSplit}
            title="Exit split view"
            style={{
              fontSize: 10, padding: '1px 6px', borderRadius: 4,
              background: 'var(--accent-primary-subtle)', color: 'var(--accent-primary)',
              border: 'none', cursor: 'pointer',
            }}
          >
            ⊞ split
          </button>
        )}

        {/* Workspace dots (desktop only — phone shows them center) */}
        {!isPhone && <WorkspaceDots />}

        {/* WS connection indicator */}
        {!isPhone && (
          connected
            ? <Wifi size={11} style={{ color: 'var(--accent-success)', opacity: 0.6 }} />
            : <WifiOff size={11} style={{ color: 'var(--accent-warning)' }} />
        )}

        {/* Layout presets */}
        {!isPhone && <PresetsButton />}

        <NotifBell tier={tier} />

        <Clock />

        {/* ⊞ Tower Mode pill → switches to tower UI */}
        <UiModeToggle compact />
      </div>
    </div>
  )
}
