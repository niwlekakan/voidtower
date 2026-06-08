import { useState, useEffect, useCallback } from 'react'
import { Play, Square, RotateCcw, PowerOff, RefreshCw, Terminal } from 'lucide-react'
import { api } from '@/api/client'
import type { LxcContainer } from '@/api/types'
import { notify } from '@/store/notifications'
import Button from '@/components/ui/Button'

function statusColor(status: string) {
  switch (status.toLowerCase()) {
    case 'running': return 'var(--accent-success)'
    case 'stopped': return 'var(--accent-danger)'
    case 'paused':  return 'var(--accent-warning)'
    default:        return 'var(--text-muted)'
  }
}

function StatusDot({ status }: { status: string }) {
  const color = statusColor(status)
  return (
    <span style={{ display: 'inline-flex', alignItems: 'center', gap: 6, fontSize: 12 }}>
      <span style={{ width: 7, height: 7, borderRadius: '50%', background: color, flexShrink: 0 }} />
      <span style={{ color }}>{status}</span>
    </span>
  )
}

function ActionBtn({ icon: Icon, label, onClick, disabled }: {
  icon: React.ElementType; label: string; onClick: () => void; disabled?: boolean
}) {
  return (
    <button
      onClick={onClick}
      disabled={disabled}
      title={label}
      style={{
        display: 'flex', alignItems: 'center', gap: 5, padding: '4px 10px',
        borderRadius: 5, border: '1px solid var(--border-subtle)',
        background: 'var(--bg-elevated)', color: 'var(--text-secondary)',
        cursor: disabled ? 'not-allowed' : 'pointer', fontSize: 12,
        opacity: disabled ? 0.4 : 1, transition: 'opacity 0.15s',
      }}
      onMouseEnter={e => !disabled && (e.currentTarget.style.borderColor = 'var(--accent-primary)')}
      onMouseLeave={e => (e.currentTarget.style.borderColor = 'var(--border-subtle)')}
    >
      <Icon size={12} />
      {label}
    </button>
  )
}

export default function LxcPage() {
  const [containers, setContainers] = useState<LxcContainer[]>([])
  const [available, setAvailable] = useState(true)
  const [loading, setLoading] = useState(true)
  const [busy, setBusy] = useState<Record<number, boolean>>({})

  const load = useCallback(() => {
    setLoading(true)
    api.lxc.list()
      .then(r => { setAvailable(r.available); setContainers(r.containers) })
      .catch(() => notify.error('Failed to load LXC containers'))
      .finally(() => setLoading(false))
  }, [])

  useEffect(() => { load() }, [load])

  const doAction = async (vmid: number, action: string) => {
    setBusy(b => ({ ...b, [vmid]: true }))
    try {
      const r = await api.lxc.action(vmid, action)
      if (r.ok) {
        notify.success(`${action} sent to CT ${vmid}`)
        setTimeout(load, 1200)
      } else {
        notify.error(r.message || `Failed to ${action} CT ${vmid}`)
      }
    } catch {
      notify.error(`Failed to ${action} CT ${vmid}`)
    } finally {
      setBusy(b => ({ ...b, [vmid]: false }))
    }
  }

  return (
    <div className="space-y-5 max-w-5xl">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>LXC Containers</h1>
          <p className="text-xs mt-0.5" style={{ color: 'var(--text-muted)' }}>
            Local Proxmox containers managed via <code style={{ fontFamily: 'var(--font-mono)' }}>pct</code>
          </p>
        </div>
        <Button size="sm" variant="ghost" onClick={load}>
          <RefreshCw size={13} className={`mr-1.5 ${loading ? 'animate-spin' : ''}`} />
          Refresh
        </Button>
      </div>

      {!available && !loading && (
        <div className="card" style={{ textAlign: 'center', padding: '40px 24px' }}>
          <p style={{ fontSize: 36, opacity: 0.2, marginBottom: 12 }}>📦</p>
          <p className="text-sm font-medium" style={{ color: 'var(--text-primary)', marginBottom: 6 }}>
            pct not found
          </p>
          <p className="text-xs" style={{ color: 'var(--text-muted)', maxWidth: 360, margin: '0 auto' }}>
            The <code style={{ fontFamily: 'var(--font-mono)' }}>pct</code> command is not installed.
            It is available on Proxmox VE hosts as part of the <code style={{ fontFamily: 'var(--font-mono)' }}>pve-container</code> package.
          </p>
        </div>
      )}

      {available && loading && (
        <div className="card" style={{ textAlign: 'center', padding: '32px' }}>
          <p className="text-sm" style={{ color: 'var(--text-muted)' }}>Loading containers…</p>
        </div>
      )}

      {available && !loading && containers.length === 0 && (
        <div className="card" style={{ textAlign: 'center', padding: '40px 24px' }}>
          <p style={{ fontSize: 36, opacity: 0.2, marginBottom: 12 }}>📦</p>
          <p className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>No LXC containers</p>
          <p className="text-xs mt-1" style={{ color: 'var(--text-muted)' }}>
            Create containers with <code style={{ fontFamily: 'var(--font-mono)' }}>pct create</code> or via the Proxmox web UI.
          </p>
        </div>
      )}

      {available && containers.length > 0 && (
        <div className="card" style={{ padding: 0, overflow: 'hidden' }}>
          <table style={{ width: '100%', borderCollapse: 'collapse', fontSize: 13 }}>
            <thead>
              <tr style={{ borderBottom: '1px solid var(--border-subtle)' }}>
                {['VMID', 'Name', 'Status', 'Actions'].map(h => (
                  <th key={h} style={{
                    padding: '10px 16px', textAlign: 'left', fontSize: 11,
                    fontWeight: 600, textTransform: 'uppercase', letterSpacing: '0.06em',
                    color: 'var(--text-muted)',
                  }}>{h}</th>
                ))}
              </tr>
            </thead>
            <tbody>
              {containers.map((ct, i) => {
                const isRunning = ct.status.toLowerCase() === 'running'
                const isBusy = !!busy[ct.vmid]
                return (
                  <tr
                    key={ct.vmid}
                    style={{
                      borderBottom: i < containers.length - 1 ? '1px solid var(--border-subtle)' : 'none',
                      background: 'transparent',
                    }}
                  >
                    <td style={{ padding: '12px 16px', fontFamily: 'var(--font-mono)', color: 'var(--text-muted)', fontSize: 12 }}>
                      {ct.vmid}
                    </td>
                    <td style={{ padding: '12px 16px', fontWeight: 500, color: 'var(--text-primary)' }}>
                      {ct.name || <span style={{ color: 'var(--text-disabled)' }}>—</span>}
                    </td>
                    <td style={{ padding: '12px 16px' }}>
                      <StatusDot status={ct.status} />
                    </td>
                    <td style={{ padding: '8px 16px' }}>
                      <div style={{ display: 'flex', gap: 6, flexWrap: 'wrap' }}>
                        {!isRunning && (
                          <ActionBtn icon={Play} label="Start" disabled={isBusy}
                            onClick={() => doAction(ct.vmid, 'start')} />
                        )}
                        {isRunning && (
                          <ActionBtn icon={RotateCcw} label="Restart" disabled={isBusy}
                            onClick={() => doAction(ct.vmid, 'restart')} />
                        )}
                        {isRunning && (
                          <ActionBtn icon={PowerOff} label="Shutdown" disabled={isBusy}
                            onClick={() => doAction(ct.vmid, 'shutdown')} />
                        )}
                        {isRunning && (
                          <ActionBtn icon={Square} label="Stop" disabled={isBusy}
                            onClick={() => doAction(ct.vmid, 'stop')} />
                        )}
                        <ActionBtn
                          icon={Terminal} label={`pct enter ${ct.vmid}`} disabled={false}
                          onClick={() => {
                            navigator.clipboard.writeText(`pct enter ${ct.vmid}`)
                              .then(() => notify.success('Copied to clipboard'))
                              .catch(() => {})
                          }}
                        />
                      </div>
                    </td>
                  </tr>
                )
              })}
            </tbody>
          </table>
        </div>
      )}

      <p className="text-xs" style={{ color: 'var(--text-disabled)' }}>
        Actions run as root via <code style={{ fontFamily: 'var(--font-mono)' }}>pct</code>.
        Use the Terminal page for interactive container shells (<code style={{ fontFamily: 'var(--font-mono)' }}>pct enter &lt;vmid&gt;</code>).
      </p>
    </div>
  )
}
