import { useState, useEffect } from 'react'
import { Bell, Wifi, WifiOff } from 'lucide-react'
import { useMetricsStore } from '@/store/metrics'
import { useAiosStore } from '@/aios/store/aios'
import type { DeviceTier } from '@/aios/hooks/useDeviceTier'
import UiModeToggle from '@/components/ui/UiModeToggle'

interface Props {
  tier: DeviceTier
}

function fmt(n: number) { return n.toFixed(0) }
function fmtBytes(b: number) {
  if (b < 1024) return `${b}B`
  if (b < 1024 * 1024) return `${(b / 1024).toFixed(0)}K`
  return `${(b / (1024 * 1024)).toFixed(1)}M`
}

function Clock() {
  const [time, setTime] = useState(() => new Date())
  useEffect(() => {
    const t = setInterval(() => setTime(new Date()), 1000)
    return () => clearInterval(t)
  }, [])
  return (
    <span style={{ fontVariantNumeric: 'tabular-nums' }}>
      {time.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}
    </span>
  )
}

export default function AiosStatusBar({ tier }: Props) {
  const { snapshot, connected } = useMetricsStore()
  const { activeWorkspace, setWorkspace, splitPair, uncoupleSplit } = useAiosStore()
  const [notifOpen, setNotifOpen] = useState(false)

  const isPhone = tier === 'phone'
  const isTv = tier === 'tv'
  const h = isPhone ? 28 : isTv ? 52 : 36

  const net = snapshot?.networks?.reduce(
    (acc, n) => ({ rx: acc.rx + n.rx_bytes_per_sec, tx: acc.tx + n.tx_bytes_per_sec }),
    { rx: 0, tx: 0 },
  ) ?? null

  const gpu = snapshot?.gpu?.[0] ?? null

  return (
    <div
      style={{
        position: 'fixed', top: 0, left: 0, right: 0, zIndex: 10000,
        height: h, display: 'flex', alignItems: 'center',
        background: 'var(--bg-panel)', borderBottom: '1px solid var(--border-subtle)',
        padding: '0 12px', gap: 12, fontSize: isTv ? 16 : 11,
        color: 'var(--text-muted)', userSelect: 'none',
      }}
    >
      {/* Left: hostname + uptime */}
      {!isPhone && snapshot && (
        <div style={{ display: 'flex', gap: 8, flexShrink: 0 }}>
          <span style={{ color: 'var(--text-secondary)', fontWeight: 600 }}>{snapshot.hostname}</span>
          {!isTv && (
            <span>up {Math.floor(snapshot.uptime_secs / 3600)}h {Math.floor((snapshot.uptime_secs % 3600) / 60)}m</span>
          )}
        </div>
      )}

      {/* Center: metrics */}
      <div style={{ flex: 1, display: 'flex', justifyContent: 'center', alignItems: 'center', gap: 14 }}>
        {snapshot && !isPhone ? (
          <>
            <MetricPill label="CPU" value={`${fmt(snapshot.cpu_usage)}%`} pct={snapshot.cpu_usage} />
            <MetricPill
              label="RAM"
              value={`${fmt((snapshot.ram_used / snapshot.ram_total) * 100)}%`}
              pct={(snapshot.ram_used / snapshot.ram_total) * 100}
            />
            {gpu && <MetricPill label="GPU" value={`${fmt(gpu.util_pct)}%`} pct={gpu.util_pct} />}
            {net && !isTv && (
              <span>↓{fmtBytes(net.rx)}/s ↑{fmtBytes(net.tx)}/s</span>
            )}
          </>
        ) : (
          /* Phone: just workspace dots */
          <WorkspaceDots active={activeWorkspace} setWorkspace={setWorkspace}  />
        )}
      </div>

      {/* Right */}
      <div style={{ display: 'flex', alignItems: 'center', gap: 10, flexShrink: 0 }}>
        {/* Split indicator */}
        {splitPair && !isPhone && (
          <button
            onClick={uncoupleSplit}
            style={{
              fontSize: 10, padding: '1px 6px', borderRadius: 4,
              background: 'var(--accent-primary-subtle)', color: 'var(--accent-primary)',
              border: 'none', cursor: 'pointer',
            }}
            title="Exit split view"
          >
            ⊞ split
          </button>
        )}

        {/* Workspace dots (desktop+) */}
        {!isPhone && (
          <WorkspaceDots active={activeWorkspace} setWorkspace={setWorkspace}  />
        )}

        {/* Connection indicator */}
        {!isPhone && (
          connected
            ? <Wifi size={11} style={{ color: 'var(--accent-success)', opacity: 0.7 }} />
            : <WifiOff size={11} style={{ color: 'var(--accent-warning)' }} />
        )}

        {/* Notification bell */}
        <button
          onClick={() => setNotifOpen((o) => !o)}
          style={{ background: 'none', border: 'none', cursor: 'pointer', color: 'var(--text-muted)', padding: 2, lineHeight: 1 }}
          aria-label="Notifications"
        >
          <Bell size={isPhone ? 13 : 12} />
        </button>

        {!isPhone && <UiModeToggle compact />}

        <Clock />
      </div>

      {/* Notification drawer */}
      {notifOpen && (
        <div
          style={{
            position: 'absolute', top: h, right: 8, width: 280, maxHeight: 320,
            background: 'var(--bg-panel)', border: '1px solid var(--border-subtle)',
            borderRadius: 8, boxShadow: '0 8px 32px rgba(0,0,0,0.4)',
            overflowY: 'auto', zIndex: 10001,
          }}
        >
          <div style={{ padding: '10px 12px', borderBottom: '1px solid var(--border-subtle)', fontSize: 12, fontWeight: 600, color: 'var(--text-primary)' }}>
            Notifications
          </div>
          <div style={{ padding: '8px 12px', fontSize: 12, color: 'var(--text-muted)' }}>
            No new notifications.
          </div>
        </div>
      )}
    </div>
  )
}

function MetricPill({ label, value, pct }: { label: string; value: string; pct: number }) {
  const color = pct > 85 ? 'var(--accent-danger)' : pct > 60 ? 'var(--accent-warning)' : 'var(--text-muted)'
  return (
    <span style={{ color }}>
      <span style={{ color: 'var(--text-disabled)', marginRight: 2 }}>{label}</span>
      {value}
    </span>
  )
}

function WorkspaceDots({
  active, setWorkspace,
}: {
  active: number
  setWorkspace: (i: 0 | 1 | 2 | 3) => void
}) {
  return (
    <div style={{ display: 'flex', gap: 5 }}>
      {([0, 1, 2, 3] as const).map((i) => (
        <button
          key={i}
          onClick={() => setWorkspace(i)}
          aria-label={`Workspace ${i + 1}`}
          style={{
            width: i === active ? 18 : 6, height: 6,
            borderRadius: 3, border: 'none', cursor: 'pointer',
            transition: 'width 0.2s, background 0.2s',
            background: i === active ? 'var(--accent-primary)' : 'var(--border-subtle)',
          }}
        />
      ))}
    </div>
  )
}
