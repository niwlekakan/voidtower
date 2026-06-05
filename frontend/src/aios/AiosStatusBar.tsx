import { useEffect, useState } from 'react'
import { Bell, WifiOff, Wifi } from 'lucide-react'
import { useMetricsStore } from '@/store/metrics'
import { useAiosStore } from '@/aios/store/aios'
import UiModeToggle from '@/components/ui/UiModeToggle'
import type { DeviceTier } from '@/aios/hooks/useDeviceTier'

interface Props {
  tier: DeviceTier
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
  const { activeWorkspace, setWorkspace } = useAiosStore()
  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 5 }}>
      {([0, 1, 2, 3] as const).map((i) => (
        <button
          key={i}
          onClick={() => setWorkspace(i)}
          aria-label={`Workspace ${i + 1}`}
          title={`Workspace ${i + 1}`}
          style={{
            width: i === activeWorkspace ? 18 : 6,
            height: 6,
            borderRadius: 3,
            border: 'none',
            cursor: 'pointer',
            padding: 0,
            transition: 'width 0.2s, background 0.2s',
            background: i === activeWorkspace ? 'var(--accent-primary)' : 'rgba(255,255,255,0.25)',
            flexShrink: 0,
          }}
        />
      ))}
    </div>
  )
}

function NotifBell() {
  const [open, setOpen] = useState(false)
  // No global alerts reactive store yet — static 0
  const count = 0
  const BAR_H = 28

  return (
    <div style={{ position: 'relative' }}>
      <button
        onClick={() => setOpen((o) => !o)}
        aria-label="Notifications"
        style={{
          background: 'none', border: 'none', cursor: 'pointer',
          color: 'var(--text-muted)', padding: 3, lineHeight: 1,
          display: 'flex', alignItems: 'center', justifyContent: 'center',
        }}
      >
        <Bell size={13} />
        {count > 0 && (
          <span style={{
            position: 'absolute', top: 0, right: 0,
            width: 14, height: 14, borderRadius: '50%',
            background: 'var(--accent-danger)', color: '#fff',
            fontSize: 9, fontWeight: 700,
            display: 'flex', alignItems: 'center', justifyContent: 'center',
          }}>
            {count > 9 ? '9+' : count}
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
            position: 'absolute', top: BAR_H - 4, right: 0,
            width: 280, maxHeight: 320,
            background: 'rgba(0,0,0,0.75)',
            backdropFilter: 'blur(20px)',
            WebkitBackdropFilter: 'blur(20px)',
            border: '1px solid rgba(255,255,255,0.12)',
            borderRadius: 10,
            boxShadow: '0 8px 32px rgba(0,0,0,0.5)',
            zIndex: 10002, overflowY: 'auto',
          }}>
            <div style={{ padding: '10px 14px', borderBottom: '1px solid rgba(255,255,255,0.08)', fontSize: 12, fontWeight: 600, color: 'var(--text-primary)' }}>
              Notifications
            </div>
            <div style={{ padding: '10px 14px', fontSize: 12, color: 'var(--text-muted)' }}>
              No new notifications.
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

        <NotifBell />

        <Clock />

        {/* ⊞ Tower Mode pill → switches to tower UI */}
        <UiModeToggle compact />
      </div>
    </div>
  )
}
