import { useState, useEffect, useCallback, useRef } from 'react'
import {
  Server, Plus, Trash2, Play, Square, RotateCcw, Camera, ChevronDown, ChevronRight,
  RefreshCw, AlertCircle, Database, Cpu, MemoryStick, HardDrive, Monitor, X,
} from 'lucide-react'
import { api, ApiClientError } from '@/api/client'
import { notify } from '@/store/notifications'
import type { ProxmoxHost, PveVm, PveNode, PveStorage, PveTask, PveSnapshot, AddHostRequest, Tag, TagMap } from '@/api/types'
import Button from '@/components/ui/Button'

// ── helpers ───────────────────────────────────────────────────────────────────

function fmtBytes(b: number) {
  if (!b || b === 0) return '0 B'
  const gb = b / 1073741824
  if (gb >= 1) return `${gb.toFixed(1)} GB`
  const mb = b / 1048576
  if (mb >= 1) return `${mb.toFixed(0)} MB`
  return `${(b / 1024).toFixed(0)} KB`
}

function fmtUptime(secs: number) {
  if (!secs) return '—'
  const d = Math.floor(secs / 86400)
  const h = Math.floor((secs % 86400) / 3600)
  const m = Math.floor((secs % 3600) / 60)
  if (d > 0) return `${d}d ${h}h`
  if (h > 0) return `${h}h ${m}m`
  return `${m}m`
}

function fmtRelTime(ts: number) {
  if (!ts) return '—'
  const diff = Math.floor(Date.now() / 1000) - ts
  if (diff < 60) return `${diff}s ago`
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`
  return `${Math.floor(diff / 86400)}d ago`
}

function pct(used: number, total: number) {
  return total > 0 ? Math.min(100, (used / total) * 100) : 0
}

function StatusDot({ status }: { status: string }) {
  const s = (status ?? '').toLowerCase()
  const color = s === 'running' ? 'var(--accent-success)'
    : s === 'stopped' ? 'var(--text-muted)'
    : s === 'paused' ? 'var(--accent-warning)'
    : 'var(--text-muted)'
  return (
    <span style={{ display: 'inline-flex', alignItems: 'center', gap: 5 }}>
      <span style={{ width: 7, height: 7, borderRadius: '50%', background: color, flexShrink: 0, display: 'inline-block' }} />
      <span style={{ color: 'var(--text-secondary)', fontSize: 12 }}>{status}</span>
    </span>
  )
}

function MiniBar({ value, total, color }: { value: number; total: number; color: string }) {
  const p = pct(value, total)
  return (
    <div style={{ height: 4, borderRadius: 2, background: 'var(--bg-base)', width: 60, flexShrink: 0 }}>
      <div style={{ height: '100%', width: `${p}%`, borderRadius: 2, background: color, transition: 'width 0.3s' }} />
    </div>
  )
}

function UsageBar({ used, total, label }: { used: number; total: number; label: string }) {
  const p = pct(used, total)
  const color = p > 85 ? 'var(--accent-danger)' : p > 60 ? 'var(--accent-warning)' : 'var(--accent-success)'
  return (
    <div>
      <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 3 }}>
        <span style={{ fontSize: 11, color: 'var(--text-muted)' }}>{label}</span>
        <span style={{ fontSize: 11, color: 'var(--text-secondary)' }}>{fmtBytes(used)} / {fmtBytes(total)} ({p.toFixed(0)}%)</span>
      </div>
      <div style={{ height: 5, borderRadius: 3, background: 'var(--bg-base)' }}>
        <div style={{ height: '100%', width: `${p}%`, borderRadius: 3, background: color, transition: 'width 0.3s' }} />
      </div>
    </div>
  )
}

// ── sub-tabs ──────────────────────────────────────────────────────────────────

type HostTab = 'vms' | 'storage' | 'tasks' | 'backups'

// ── Add Host Modal ────────────────────────────────────────────────────────────

function AddHostModal({ onClose, onAdded }: { onClose: () => void; onAdded: () => void }) {
  const [form, setForm] = useState<AddHostRequest>({ name: '', url: '', node: 'pve', token_id: '', token_secret: '', fingerprint: '' })
  const [saving, setSaving] = useState(false)
  const set = (k: keyof AddHostRequest, v: string) => setForm(f => ({ ...f, [k]: v }))

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    setSaving(true)
    try {
      const payload: AddHostRequest = { ...form }
      if (!payload.fingerprint) delete payload.fingerprint
      if (!payload.node) payload.node = 'pve'
      await api.proxmox.addHost(payload)
      notify.success('Proxmox host added')
      onAdded(); onClose()
    } catch (err) {
      notify.error(err instanceof ApiClientError ? err.message : 'Failed to add host')
    } finally { setSaving(false) }
  }

  const fields: { label: string; key: keyof AddHostRequest; placeholder: string; required: boolean; type?: string }[] = [
    { label: 'Name', key: 'name', placeholder: 'My Proxmox', required: true },
    { label: 'URL', key: 'url', placeholder: 'https://192.168.1.100:8006', required: true },
    { label: 'Node', key: 'node', placeholder: 'pve', required: false },
    { label: 'Token ID', key: 'token_id', placeholder: 'user@pam!tokenname', required: true },
    { label: 'Token Secret', key: 'token_secret', placeholder: 'xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx', required: true, type: 'password' },
    { label: 'Fingerprint (optional)', key: 'fingerprint', placeholder: 'AA:BB:CC:…', required: false },
  ]

  return (
    <div style={{ position: 'fixed', inset: 0, zIndex: 50, background: 'rgba(0,0,0,0.55)', display: 'flex', alignItems: 'center', justifyContent: 'center' }}
      onClick={e => { if (e.target === e.currentTarget) onClose() }}>
      <div style={{ background: 'var(--bg-panel)', border: '1px solid var(--border-subtle)', borderRadius: 10, padding: 24, width: '100%', maxWidth: 440 }}>
        <h2 style={{ fontSize: 15, fontWeight: 600, color: 'var(--text-primary)', marginBottom: 20 }}>Add Proxmox Host</h2>
        <form onSubmit={handleSubmit} style={{ display: 'flex', flexDirection: 'column', gap: 14 }}>
          {fields.map(f => (
            <div key={f.key}>
              <label htmlFor={`addhost-${f.key}`} style={{ display: 'block', fontSize: 12, color: 'var(--text-secondary)', marginBottom: 5 }}>{f.label}</label>
              <input
                id={`addhost-${f.key}`}
                type={f.type ?? 'text'}
                value={form[f.key] ?? ''}
                onChange={e => set(f.key, e.target.value)}
                placeholder={f.placeholder}
                required={f.required}
                style={{ width: '100%', padding: '7px 10px', borderRadius: 6, fontSize: 13, background: 'var(--bg-elevated)', border: '1px solid var(--border-default)', color: 'var(--text-primary)', outline: 'none', boxSizing: 'border-box' }}
              />
            </div>
          ))}
          <div style={{ display: 'flex', gap: 8, marginTop: 4 }}>
            <Button type="submit" size="sm" variant="primary" loading={saving}>Add Host</Button>
            <Button type="button" size="sm" variant="ghost" onClick={onClose}>Cancel</Button>
          </div>
        </form>
      </div>
    </div>
  )
}

// ── Snapshot modal ────────────────────────────────────────────────────────────

function CreateSnapshotModal({ hostId, vm, onClose, onDone }: { hostId: string; vm: PveVm; onClose: () => void; onDone: () => void }) {
  const [name, setName] = useState('')
  const [desc, setDesc] = useState('')
  const [saving, setSaving] = useState(false)

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!name.trim()) return
    setSaving(true)
    try {
      await api.proxmox.createSnapshot(hostId, vm.vmid, name.trim(), desc.trim())
      notify.success(`Snapshot "${name}" created`)
      onDone(); onClose()
    } catch (err) {
      notify.error(err instanceof ApiClientError ? err.message : 'Failed to create snapshot')
    } finally { setSaving(false) }
  }

  return (
    <div style={{ position: 'fixed', inset: 0, zIndex: 60, background: 'rgba(0,0,0,0.55)', display: 'flex', alignItems: 'center', justifyContent: 'center' }}
      onClick={e => { if (e.target === e.currentTarget) onClose() }}>
      <div style={{ background: 'var(--bg-panel)', border: '1px solid var(--border-subtle)', borderRadius: 10, padding: 24, width: '100%', maxWidth: 360 }}>
        <h2 style={{ fontSize: 14, fontWeight: 600, color: 'var(--text-primary)', marginBottom: 16 }}>
          Snapshot — {vm.name ?? `VM ${vm.vmid}`}
        </h2>
        <form onSubmit={handleSubmit} style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
          <div>
            <label style={{ display: 'block', fontSize: 12, color: 'var(--text-secondary)', marginBottom: 4 }}>Name</label>
            <input autoFocus value={name} onChange={e => setName(e.target.value)} placeholder="snap-1" required
              style={{ width: '100%', padding: '7px 10px', borderRadius: 6, fontSize: 13, background: 'var(--bg-elevated)', border: '1px solid var(--border-default)', color: 'var(--text-primary)', outline: 'none', boxSizing: 'border-box' }} />
          </div>
          <div>
            <label style={{ display: 'block', fontSize: 12, color: 'var(--text-secondary)', marginBottom: 4 }}>Description (optional)</label>
            <input value={desc} onChange={e => setDesc(e.target.value)} placeholder="before upgrade"
              style={{ width: '100%', padding: '7px 10px', borderRadius: 6, fontSize: 13, background: 'var(--bg-elevated)', border: '1px solid var(--border-default)', color: 'var(--text-primary)', outline: 'none', boxSizing: 'border-box' }} />
          </div>
          <div style={{ display: 'flex', gap: 8, marginTop: 4 }}>
            <Button type="submit" size="sm" variant="primary" loading={saving}>Create</Button>
            <Button type="button" size="sm" variant="ghost" onClick={onClose}>Cancel</Button>
          </div>
        </form>
      </div>
    </div>
  )
}

// ── Node overview ─────────────────────────────────────────────────────────────

function NodeCards({ nodes }: { nodes: PveNode[] }) {
  if (nodes.length === 0) return null
  return (
    <div style={{ display: 'grid', gap: 12, gridTemplateColumns: 'repeat(auto-fill, minmax(240px, 1fr))' }}>
      {nodes.map(n => {
        const cpuPct   = (n.cpu ?? 0) * 100
        const hasStats = (n.maxmem ?? 0) > 0
        return (
          <div key={n.node} style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)', borderRadius: 8, padding: 16 }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 14 }}>
              <Server size={14} style={{ color: 'var(--accent-primary)', flexShrink: 0 }} />
              <span style={{ fontSize: 13, fontWeight: 600, color: 'var(--text-primary)', flex: 1 }}>{n.node}</span>
              <span style={{ fontSize: 11, padding: '1px 7px', borderRadius: 4, background: n.status === 'online' ? 'var(--accent-success-subtle)' : 'var(--bg-base)', color: n.status === 'online' ? 'var(--accent-success)' : 'var(--text-muted)' }}>
                {n.status ?? 'unknown'}
              </span>
            </div>
            {hasStats ? (
              <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
                <UsageBar used={(n.cpu ?? 0) * (n.maxcpu ?? 1)} total={n.maxcpu ?? 1} label={`CPU — ${cpuPct.toFixed(1)}% of ${n.maxcpu ?? '?'} cores`} />
                <UsageBar used={n.mem ?? 0} total={n.maxmem ?? 0} label="RAM" />
                <UsageBar used={n.disk ?? 0} total={n.maxdisk ?? 0} label="Root disk" />
              </div>
            ) : (
              <p style={{ fontSize: 12, color: 'var(--text-muted)' }}>Metrics unavailable — token may need Sys.Audit permission</p>
            )}
            {(n.uptime ?? 0) > 0 && <div style={{ fontSize: 11, color: 'var(--text-muted)', marginTop: 10 }}>up {fmtUptime(n.uptime)}</div>}
          </div>
        )
      })}
    </div>
  )
}

// ── Summary bar ───────────────────────────────────────────────────────────────

function SummaryBar({ nodes, vms, storage }: { nodes: PveNode[]; vms: PveVm[]; storage: PveStorage[] }) {
  const running   = vms.filter(v => v.status === 'running').length
  const totalMem  = vms.reduce((s, v) => s + (v.maxmem ?? 0), 0)
  const usedMem   = vms.filter(v => v.status === 'running').reduce((s, v) => s + (v.mem ?? 0), 0)
  const totalDisk = storage.reduce((s, p) => s + (p.total ?? 0), 0)
  const usedDisk  = storage.reduce((s, p) => s + (p.used ?? 0), 0)

  const stat = (icon: React.ReactNode, label: string, value: string) => (
    <div style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '10px 16px', background: 'var(--bg-elevated)', borderRadius: 8, border: '1px solid var(--border-subtle)', flex: '1 1 140px' }}>
      {icon}
      <div>
        <div style={{ fontSize: 11, color: 'var(--text-muted)' }}>{label}</div>
        <div style={{ fontSize: 14, fontWeight: 600, color: 'var(--text-primary)' }}>{value}</div>
      </div>
    </div>
  )

  return (
    <div style={{ display: 'flex', gap: 10, flexWrap: 'wrap' }}>
      {stat(<Server size={16} style={{ color: 'var(--accent-primary)', flexShrink: 0 }} />, 'Nodes', String(nodes.length))}
      {stat(<Cpu size={16} style={{ color: 'var(--accent-success)', flexShrink: 0 }} />, 'VMs running', `${running} / ${vms.length}`)}
      {stat(<MemoryStick size={16} style={{ color: 'var(--accent-warning)', flexShrink: 0 }} />, 'RAM (running)', totalMem > 0 ? `${fmtBytes(usedMem)} / ${fmtBytes(totalMem)}` : '—')}
      {stat(<Database size={16} style={{ color: 'var(--text-secondary)', flexShrink: 0 }} />, 'Storage used', totalDisk > 0 ? `${fmtBytes(usedDisk)} / ${fmtBytes(totalDisk)}` : '—')}
    </div>
  )
}

// ── Snapshot row ──────────────────────────────────────────────────────────────

function SnapshotRow({ hostId, vm, onRefresh }: { hostId: string; vm: PveVm; onRefresh: () => void }) {
  const [open, setOpen] = useState(false)
  const [snaps, setSnaps] = useState<PveSnapshot[]>([])
  const [loading, setLoading] = useState(false)
  const [showCreate, setShowCreate] = useState(false)
  const [busy, setBusy] = useState<string | null>(null)

  const load = useCallback(async () => {
    setLoading(true)
    try {
      const list = await api.proxmox.getSnapshots(hostId, vm.vmid, vm.type as 'qemu' | 'lxc')
      setSnaps(list.filter(s => s.name !== 'current'))
    } catch { setSnaps([]) }
    finally { setLoading(false) }
  }, [hostId, vm.vmid, vm.type])

  const toggle = () => {
    if (!open) load()
    setOpen(o => !o)
  }

  const rollback = async (snapname: string) => {
    if (!confirm(`Roll back ${vm.name ?? vm.vmid} to "${snapname}"? The VM will be reset to this state.`)) return
    setBusy(snapname)
    try {
      await api.proxmox.rollbackSnapshot(hostId, vm.vmid, snapname)
      notify.success(`Rollback to "${snapname}" queued`)
      onRefresh()
    } catch (err) { notify.error(err instanceof ApiClientError ? err.message : 'Rollback failed') }
    finally { setBusy(null) }
  }

  const del = async (snapname: string) => {
    if (!confirm(`Delete snapshot "${snapname}"?`)) return
    setBusy(snapname)
    try {
      await api.proxmox.deleteSnapshot(hostId, vm.vmid, snapname)
      notify.success(`Snapshot "${snapname}" deleted`)
      await load()
    } catch (err) { notify.error(err instanceof ApiClientError ? err.message : 'Delete failed') }
    finally { setBusy(null) }
  }

  return (
    <>
      <tr>
        <td colSpan={9} style={{ padding: 0, borderBottom: '1px solid var(--border-subtle)' }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 6, padding: '6px 12px', background: 'var(--bg-base)', cursor: 'pointer' }} onClick={toggle}>
            {open ? <ChevronDown size={12} style={{ color: 'var(--text-muted)' }} /> : <ChevronRight size={12} style={{ color: 'var(--text-muted)' }} />}
            <Camera size={11} style={{ color: 'var(--text-muted)' }} />
            <span style={{ fontSize: 11, color: 'var(--text-muted)' }}>Snapshots</span>
            <button
              onClick={e => { e.stopPropagation(); setShowCreate(true) }}
              style={{ marginLeft: 'auto', fontSize: 11, padding: '2px 8px', borderRadius: 4, border: '1px solid var(--border-subtle)', background: 'transparent', color: 'var(--accent-primary)', cursor: 'pointer' }}>
              + New
            </button>
          </div>
          {open && (
            <div style={{ padding: '8px 12px 10px 30px', background: 'var(--bg-base)' }}>
              {loading && <span style={{ fontSize: 12, color: 'var(--text-muted)' }}>Loading…</span>}
              {!loading && snaps.length === 0 && <span style={{ fontSize: 12, color: 'var(--text-muted)' }}>No snapshots.</span>}
              {!loading && snaps.length > 0 && (
                <table style={{ width: '100%', borderCollapse: 'collapse', fontSize: 12 }}>
                  <thead>
                    <tr>
                      {['Name', 'Description', 'Created', ''].map(h => (
                        <th key={h} style={{ textAlign: 'left', padding: '3px 8px', color: 'var(--text-muted)', fontWeight: 500, fontSize: 11 }}>{h}</th>
                      ))}
                    </tr>
                  </thead>
                  <tbody>
                    {snaps.map(s => (
                      <tr key={s.name}>
                        <td style={{ padding: '4px 8px', color: 'var(--text-primary)', fontFamily: 'monospace' }}>{s.name}</td>
                        <td style={{ padding: '4px 8px', color: 'var(--text-muted)' }}>{s.description || '—'}</td>
                        <td style={{ padding: '4px 8px', color: 'var(--text-muted)', whiteSpace: 'nowrap' }}>{s.snaptime ? fmtRelTime(s.snaptime) : '—'}</td>
                        <td style={{ padding: '4px 8px' }}>
                          <div style={{ display: 'flex', gap: 4 }}>
                            <button disabled={busy !== null} onClick={() => rollback(s.name)}
                              style={{ fontSize: 11, padding: '2px 7px', borderRadius: 4, border: '1px solid var(--border-subtle)', background: 'transparent', color: 'var(--accent-warning)', cursor: busy ? 'not-allowed' : 'pointer', opacity: busy ? 0.5 : 1 }}>
                              Rollback
                            </button>
                            <button disabled={busy !== null} onClick={() => del(s.name)}
                              style={{ fontSize: 11, padding: '2px 7px', borderRadius: 4, border: '1px solid var(--border-subtle)', background: 'transparent', color: 'var(--accent-danger)', cursor: busy ? 'not-allowed' : 'pointer', opacity: busy ? 0.5 : 1 }}>
                              Delete
                            </button>
                          </div>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              )}
            </div>
          )}
        </td>
      </tr>
      {showCreate && <CreateSnapshotModal hostId={hostId} vm={vm} onClose={() => setShowCreate(false)} onDone={load} />}
    </>
  )
}

// ── Console modal (noVNC via CDN) ─────────────────────────────────────────────

interface ConsoleModalProps {
  hostId: string
  vm: PveVm
  onClose: () => void
}

function ConsoleModal({ hostId, vm, onClose }: ConsoleModalProps) {
  const containerRef = useRef<HTMLDivElement>(null)
  const rfbRef       = useRef<{ disconnect: () => void } | null>(null)
  const [status, setStatus] = useState<'connecting' | 'connected' | 'error'>('connecting')
  const [errMsg, setErrMsg]  = useState('')

  useEffect(() => {
    let cancelled = false
    const init = async () => {
      try {
        setStatus('connecting')
        const data = await api.proxmox.vncProxy(hostId, vm.vmid)
        if (cancelled) return

        const wsUrl = `wss://${data.proxmox_host}/api2/json/nodes/${data.node}/${data.kind}/${data.vmid}/vncwebsocket?port=${data.port}&vncticket=${encodeURIComponent(data.ticket)}`

        // eslint-disable-next-line @typescript-eslint/ban-ts-comment
        // @ts-ignore — CDN import, no types
        const { default: RFB } = await import(/* @vite-ignore */ 'https://cdn.jsdelivr.net/npm/@novnc/novnc@1.4.0/core/rfb.js')
        if (cancelled || !containerRef.current) return

        const rfb = new RFB(containerRef.current, wsUrl) as { disconnect: () => void }
        rfbRef.current = rfb
        ;(rfb as unknown as EventTarget).addEventListener('connect',    () => { if (!cancelled) setStatus('connected') })
        ;(rfb as unknown as EventTarget).addEventListener('disconnect', (e: unknown) => {
          if (!cancelled) {
            const ev = e as CustomEvent<{ clean: boolean }>
            if (!ev.detail?.clean) setErrMsg('Connection closed unexpectedly')
          }
        })
      } catch (e) {
        if (!cancelled) { setStatus('error'); setErrMsg(String(e)) }
      }
    }
    init()
    return () => {
      cancelled = true
      rfbRef.current?.disconnect()
    }
  }, [hostId, vm.vmid])

  return (
    <div
      style={{ position: 'fixed', inset: 0, zIndex: 100, background: 'rgba(0,0,0,0.92)', display: 'flex', flexDirection: 'column' }}
      onKeyDown={e => e.key === 'Escape' && onClose()}
      tabIndex={-1}
    >
      {/* Title bar */}
      <div style={{ display: 'flex', alignItems: 'center', gap: 10, padding: '8px 16px', background: 'var(--bg-panel)', borderBottom: '1px solid var(--border-subtle)', flexShrink: 0 }}>
        <Monitor size={15} style={{ color: 'var(--accent-primary)' }} />
        <span style={{ fontSize: 14, fontWeight: 600, color: 'var(--text-primary)' }}>
          Console — {vm.name ?? `VM ${vm.vmid}`}
          <span style={{ fontSize: 11, fontWeight: 400, color: 'var(--text-muted)', marginLeft: 8 }}>
            {vm.type?.toUpperCase()} · {vm.node}
          </span>
        </span>
        {status === 'connecting' && (
          <span style={{ fontSize: 12, color: 'var(--text-muted)', marginLeft: 8 }}>Connecting…</span>
        )}
        {status === 'connected' && (
          <span style={{ fontSize: 12, color: 'var(--accent-success)', marginLeft: 8 }}>Connected</span>
        )}
        <button onClick={onClose} title="Close"
          style={{ marginLeft: 'auto', background: 'none', border: 'none', cursor: 'pointer', color: 'var(--text-muted)', display: 'flex', padding: 4 }}>
          <X size={16} />
        </button>
      </div>

      {/* Canvas area */}
      <div ref={containerRef} style={{ flex: 1, background: '#111', overflow: 'hidden', position: 'relative' }}>
        {status === 'error' && (
          <div style={{ position: 'absolute', inset: 0, display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center', gap: 12, color: 'var(--accent-danger)' }}>
            <AlertCircle size={28} />
            <span style={{ fontSize: 13 }}>{errMsg || 'Failed to open console'}</span>
            <span style={{ fontSize: 12, color: 'var(--text-muted)', maxWidth: 420, textAlign: 'center' }}>
              If you see a certificate error, open <code style={{ fontSize: 11 }}>https://{/* host is runtime only */}…</code> in a tab, accept the cert, then retry.
            </span>
          </div>
        )}
      </div>
    </div>
  )
}

// ── VMs table ─────────────────────────────────────────────────────────────────

type SortKey = 'name' | 'vmid' | 'status' | 'cpu' | 'mem' | 'node'
type StatusFilter = 'all' | 'running' | 'stopped'
type TypeFilter   = 'all' | 'qemu' | 'lxc'

function VmsTable({ hostId, vms, tagsMap, allTags, onRefresh, onTagsChange }: {
  hostId: string; vms: PveVm[]; tagsMap: TagMap; allTags: Tag[]
  onRefresh: () => void; onTagsChange: () => void
}) {
  const [sortKey, setSortKey]     = useState<SortKey>('vmid')
  const [sortAsc, setSortAsc]     = useState(true)
  const [statusF, setStatusF]     = useState<StatusFilter>('all')
  const [typeF, setTypeF]         = useState<TypeFilter>('all')
  const [search, setSearch]       = useState('')
  const [busy, setBusy]           = useState<string | null>(null)
  const [consoleVm, setConsoleVm]   = useState<PveVm | null>(null)
  const [tagPopover, setTagPopover] = useState<number | null>(null) // vmid

  const toggleTag = async (vmid: number, tag: Tag, assigned: boolean) => {
    const rid = String(vmid)
    try {
      if (assigned) await api.tags.unassign(tag.id, 'proxmox_vm', rid)
      else          await api.tags.assign(tag.id, 'proxmox_vm', rid)
      onTagsChange()
    } catch { /* ignore */ }
  }

  const onSort = (k: SortKey) => { if (k === sortKey) setSortAsc(a => !a); else { setSortKey(k); setSortAsc(true) } }

  const filtered = vms.filter(v => {
    if (statusF !== 'all' && v.status !== statusF) return false
    if (typeF   !== 'all' && (v.type ?? '') !== typeF) return false
    if (search && !(v.name ?? '').toLowerCase().includes(search.toLowerCase()) && !String(v.vmid).includes(search)) return false
    return true
  })

  const sorted = [...filtered].sort((a, b) => {
    let cmp = 0
    if (sortKey === 'name' || sortKey === 'status' || sortKey === 'node') {
      cmp = (a[sortKey] ?? '').localeCompare(b[sortKey] ?? '')
    } else { cmp = (a[sortKey] ?? 0) - (b[sortKey] ?? 0) }
    return sortAsc ? cmp : -cmp
  })

  const handleAction = async (vm: PveVm, action: 'start' | 'stop' | 'reboot') => {
    const key = `${vm.vmid}-${action}`
    setBusy(key)
    try {
      await api.proxmox.vmAction(hostId, vm.vmid, action)
      notify.success(`${action} sent to ${vm.name ?? vm.vmid}`)
      onRefresh()
    } catch (err) { notify.error(err instanceof ApiClientError ? err.message : `Failed to ${action}`) }
    finally { setBusy(null) }
  }

  const thStyle: React.CSSProperties = { padding: '8px 12px', textAlign: 'left', fontSize: 11, textTransform: 'uppercase', letterSpacing: '0.06em', color: 'var(--text-muted)', cursor: 'pointer', userSelect: 'none', whiteSpace: 'nowrap' }
  const SortTh = ({ k, label }: { k: SortKey; label: string }) => (
    <th style={thStyle} onClick={() => onSort(k)}>{label}{sortKey === k ? (sortAsc ? ' ↑' : ' ↓') : ''}</th>
  )

  const filterBtn = (active: boolean, onClick: () => void, label: string) => (
    <button onClick={onClick} style={{ padding: '3px 10px', borderRadius: 5, border: '1px solid var(--border-subtle)', fontSize: 12, cursor: 'pointer', background: active ? 'var(--accent-primary)' : 'transparent', color: active ? '#fff' : 'var(--text-secondary)' }}>
      {label}
    </button>
  )

  return (
    <div>
      {/* Filter bar */}
      <div style={{ display: 'flex', gap: 6, padding: '10px 12px', borderBottom: '1px solid var(--border-subtle)', flexWrap: 'wrap', alignItems: 'center' }}>
        <div style={{ display: 'flex', gap: 4 }}>
          {filterBtn(statusF === 'all',     () => setStatusF('all'),     'All')}
          {filterBtn(statusF === 'running', () => setStatusF('running'), 'Running')}
          {filterBtn(statusF === 'stopped', () => setStatusF('stopped'), 'Stopped')}
        </div>
        <div style={{ width: 1, height: 18, background: 'var(--border-subtle)' }} />
        <div style={{ display: 'flex', gap: 4 }}>
          {filterBtn(typeF === 'all',  () => setTypeF('all'),  'All types')}
          {filterBtn(typeF === 'qemu', () => setTypeF('qemu'), 'QEMU')}
          {filterBtn(typeF === 'lxc',  () => setTypeF('lxc'),  'LXC')}
        </div>
        <input value={search} onChange={e => setSearch(e.target.value)} placeholder="Search name / VMID…"
          style={{ marginLeft: 'auto', padding: '4px 10px', borderRadius: 5, border: '1px solid var(--border-subtle)', background: 'var(--bg-elevated)', color: 'var(--text-primary)', fontSize: 12, outline: 'none', width: 180 }} />
      </div>

      <div style={{ overflowX: 'auto' }}>
        <table style={{ width: '100%', borderCollapse: 'collapse', fontSize: 13 }}>
          <thead>
            <tr style={{ borderBottom: '1px solid var(--border-subtle)' }}>
              <SortTh k="name"   label="Name" />
              <SortTh k="vmid"   label="ID" />
              <th style={thStyle}>Type</th>
              <SortTh k="status" label="Status" />
              <SortTh k="cpu"    label="CPU" />
              <SortTh k="mem"    label="RAM" />
              <th style={thStyle}>Disk</th>
              <SortTh k="node"   label="Node" />
              <th style={thStyle}>Tags</th>
              <th style={thStyle}>Actions</th>
            </tr>
          </thead>
          <tbody>
            {sorted.map(vm => {
              const vmType = (vm.type ?? 'qemu') as 'qemu' | 'lxc'
              const cpuPct = vm.status === 'running' ? pct(vm.cpu, 1) : 0
              const memPct = vm.status === 'running' ? pct(vm.mem, vm.maxmem) : 0
              const diskPct = pct(vm.disk, vm.maxdisk)
              return (
                <>
                  <tr key={`vm-${vm.node}-${vm.vmid}`} style={{ borderBottom: '1px solid var(--border-subtle)' }}>
                    <td style={{ padding: '10px 12px', color: 'var(--text-primary)', fontWeight: 500 }}>
                      {vm.name ?? `(${vm.vmid})`}
                    </td>
                    <td style={{ padding: '10px 12px', color: 'var(--text-secondary)', fontFamily: 'monospace' }}>{vm.vmid}</td>
                    <td style={{ padding: '10px 12px' }}>
                      <span style={{ fontSize: 11, padding: '2px 7px', borderRadius: 4, background: vmType === 'qemu' ? 'var(--accent-primary-subtle)' : 'var(--bg-elevated)', color: vmType === 'qemu' ? 'var(--accent-primary)' : 'var(--text-muted)', border: '1px solid var(--border-subtle)' }}>
                        {vmType.toUpperCase()}
                      </span>
                    </td>
                    <td style={{ padding: '10px 12px' }}><StatusDot status={vm.status} /></td>
                    <td style={{ padding: '10px 12px' }}>
                      {vm.status === 'running' ? (
                        <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
                          <span style={{ color: 'var(--text-secondary)', fontSize: 12, width: 36 }}>{cpuPct.toFixed(0)}%</span>
                          <MiniBar value={vm.cpu} total={1} color={cpuPct > 80 ? 'var(--accent-danger)' : 'var(--accent-success)'} />
                        </div>
                      ) : <span style={{ color: 'var(--text-muted)' }}>—</span>}
                    </td>
                    <td style={{ padding: '10px 12px' }}>
                      {vm.maxmem > 0 ? (
                        <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
                          <span style={{ color: 'var(--text-secondary)', fontSize: 12, whiteSpace: 'nowrap', minWidth: 60 }}>
                            {vm.status === 'running' ? fmtBytes(vm.mem) : '—'} / {fmtBytes(vm.maxmem)}
                          </span>
                          <MiniBar value={vm.mem} total={vm.maxmem} color={memPct > 85 ? 'var(--accent-danger)' : memPct > 60 ? 'var(--accent-warning)' : 'var(--accent-primary)'} />
                        </div>
                      ) : <span style={{ color: 'var(--text-muted)' }}>—</span>}
                    </td>
                    <td style={{ padding: '10px 12px' }}>
                      {vm.maxdisk > 0 ? (
                        <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
                          <span style={{ color: 'var(--text-secondary)', fontSize: 12, whiteSpace: 'nowrap', minWidth: 50 }}>{fmtBytes(vm.disk)}</span>
                          <MiniBar value={vm.disk} total={vm.maxdisk} color={diskPct > 85 ? 'var(--accent-danger)' : 'var(--text-muted)'} />
                        </div>
                      ) : <span style={{ color: 'var(--text-muted)' }}>—</span>}
                    </td>
                    <td style={{ padding: '10px 12px', color: 'var(--text-muted)', fontFamily: 'monospace', fontSize: 12 }}>{vm.node}</td>
                    <td style={{ padding: '8px 12px', position: 'relative' }}>
                      <div style={{ display: 'flex', flexWrap: 'wrap', gap: 3, alignItems: 'center' }}>
                        {(tagsMap[String(vm.vmid)] ?? []).map(t => (
                          <span key={t.id} style={{ fontSize: 10, padding: '1px 6px', borderRadius: 4, background: t.color + '33', color: t.color, border: `1px solid ${t.color}55`, cursor: 'pointer' }}
                            title="Click to remove" onClick={() => toggleTag(vm.vmid, t, true)}>
                            {t.name}
                          </span>
                        ))}
                        {allTags.length > 0 && (
                          <button onClick={e => { e.stopPropagation(); setTagPopover(tagPopover === vm.vmid ? null : vm.vmid) }}
                            style={{ fontSize: 10, padding: '1px 5px', borderRadius: 4, border: '1px dashed var(--border-subtle)', background: 'transparent', color: 'var(--text-muted)', cursor: 'pointer' }}>
                            +
                          </button>
                        )}
                        {tagPopover === vm.vmid && (
                          <div style={{ position: 'absolute', top: '100%', left: 0, zIndex: 20, background: 'var(--bg-panel)', border: '1px solid var(--border-subtle)', borderRadius: 7, padding: 8, display: 'flex', flexDirection: 'column', gap: 4, minWidth: 130, boxShadow: '0 4px 16px rgba(0,0,0,0.3)' }}>
                            {allTags.map(t => {
                              const assigned = (tagsMap[String(vm.vmid)] ?? []).some(a => a.id === t.id)
                              return (
                                <button key={t.id} onClick={() => { toggleTag(vm.vmid, t, assigned); setTagPopover(null) }}
                                  style={{ display: 'flex', alignItems: 'center', gap: 6, fontSize: 12, padding: '3px 6px', borderRadius: 5, border: 'none', background: assigned ? 'var(--accent-primary-subtle)' : 'transparent', color: 'var(--text-primary)', cursor: 'pointer', textAlign: 'left' }}>
                                  <span style={{ width: 8, height: 8, borderRadius: '50%', background: t.color, flexShrink: 0 }} />
                                  {t.name}
                                  {assigned && <span style={{ marginLeft: 'auto', fontSize: 10, color: 'var(--text-muted)' }}>✓</span>}
                                </button>
                              )
                            })}
                          </div>
                        )}
                      </div>
                    </td>
                    <td style={{ padding: '10px 12px' }}>
                      <div style={{ display: 'flex', gap: 4 }}>
                        {vm.status !== 'running' && (
                          <button onClick={() => handleAction(vm, 'start')} disabled={busy !== null} title="Start"
                            style={{ padding: '4px 6px', borderRadius: 5, border: '1px solid var(--border-subtle)', background: 'var(--bg-elevated)', color: 'var(--accent-success)', cursor: busy ? 'not-allowed' : 'pointer', opacity: busy ? 0.4 : 1 }}>
                            <Play size={12} />
                          </button>
                        )}
                        {vm.status === 'running' && (<>
                          <button onClick={() => handleAction(vm, 'stop')} disabled={busy !== null} title="Stop"
                            style={{ padding: '4px 6px', borderRadius: 5, border: '1px solid var(--border-subtle)', background: 'var(--bg-elevated)', color: 'var(--accent-danger)', cursor: busy ? 'not-allowed' : 'pointer', opacity: busy ? 0.4 : 1 }}>
                            <Square size={12} />
                          </button>
                          <button onClick={() => handleAction(vm, 'reboot')} disabled={busy !== null} title="Reboot"
                            style={{ padding: '4px 6px', borderRadius: 5, border: '1px solid var(--border-subtle)', background: 'var(--bg-elevated)', color: 'var(--accent-warning)', cursor: busy ? 'not-allowed' : 'pointer', opacity: busy ? 0.4 : 1 }}>
                            <RotateCcw size={12} />
                          </button>
                        </>)}
                        <button onClick={() => {}} title="Snapshots"
                          style={{ padding: '4px 6px', borderRadius: 5, border: '1px solid var(--border-subtle)', background: 'var(--bg-elevated)', color: 'var(--text-muted)', cursor: 'pointer' }}>
                          <Camera size={12} />
                        </button>
                        {vm.status === 'running' && (
                          <button onClick={() => setConsoleVm(vm)} title="Open console"
                            style={{ padding: '4px 6px', borderRadius: 5, border: '1px solid var(--border-subtle)', background: 'var(--bg-elevated)', color: 'var(--accent-primary)', cursor: 'pointer' }}>
                            <Monitor size={12} />
                          </button>
                        )}
                      </div>
                    </td>
                  </tr>
                  <SnapshotRow key={`snap-${vm.node}-${vm.vmid}`} hostId={hostId} vm={vm} onRefresh={onRefresh} />
                </>
              )
            })}
          </tbody>
        </table>
        {sorted.length === 0 && (
          <div style={{ padding: '32px 12px', textAlign: 'center', color: 'var(--text-muted)', fontSize: 13 }}>
            {vms.length === 0 ? 'No VMs or containers found.' : 'No results match the current filter.'}
          </div>
        )}
      </div>
      {consoleVm && (
        <ConsoleModal hostId={hostId} vm={consoleVm} onClose={() => setConsoleVm(null)} />
      )}
    </div>
  )
}

// ── Storage table ─────────────────────────────────────────────────────────────

function StorageTable({ pools }: { pools: PveStorage[] }) {
  return (
    <div style={{ overflowX: 'auto' }}>
      <table style={{ width: '100%', borderCollapse: 'collapse', fontSize: 13 }}>
        <thead>
          <tr style={{ borderBottom: '1px solid var(--border-subtle)' }}>
            {['Storage', 'Type', 'Used', 'Free', 'Total', 'Usage', 'Status'].map(h => (
              <th key={h} style={{ padding: '8px 12px', textAlign: 'left', fontSize: 11, textTransform: 'uppercase', letterSpacing: '0.06em', color: 'var(--text-muted)' }}>{h}</th>
            ))}
          </tr>
        </thead>
        <tbody>
          {pools.map(p => {
            const p_used = pct(p.used, p.total)
            const barColor = p_used > 85 ? 'var(--accent-danger)' : p_used > 60 ? 'var(--accent-warning)' : 'var(--accent-success)'
            const active = p.active || (p as unknown as Record<string, unknown>)['active'] === 1
            return (
              <tr key={p.storage} style={{ borderBottom: '1px solid var(--border-subtle)' }}>
                <td style={{ padding: '10px 12px', color: 'var(--text-primary)', fontWeight: 500, fontFamily: 'monospace' }}>{p.storage}</td>
                <td style={{ padding: '10px 12px', color: 'var(--text-muted)', fontSize: 12 }}>{p.type}</td>
                <td style={{ padding: '10px 12px', color: 'var(--text-secondary)', fontSize: 12 }}>{fmtBytes(p.used)}</td>
                <td style={{ padding: '10px 12px', color: 'var(--text-secondary)', fontSize: 12 }}>{fmtBytes(p.avail)}</td>
                <td style={{ padding: '10px 12px', color: 'var(--text-secondary)', fontSize: 12 }}>{fmtBytes(p.total)}</td>
                <td style={{ padding: '10px 12px', minWidth: 120 }}>
                  <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
                    <div style={{ flex: 1, height: 5, borderRadius: 3, background: 'var(--bg-base)' }}>
                      <div style={{ height: '100%', width: `${p_used}%`, borderRadius: 3, background: barColor }} />
                    </div>
                    <span style={{ fontSize: 11, color: 'var(--text-muted)', width: 32, textAlign: 'right' }}>{p_used.toFixed(0)}%</span>
                  </div>
                </td>
                <td style={{ padding: '10px 12px' }}>
                  <span style={{ fontSize: 11, padding: '2px 7px', borderRadius: 4, background: active ? 'var(--accent-success-subtle)' : 'var(--bg-elevated)', color: active ? 'var(--accent-success)' : 'var(--text-muted)' }}>
                    {active ? 'active' : 'inactive'}
                  </span>
                </td>
              </tr>
            )
          })}
        </tbody>
      </table>
      {pools.length === 0 && (
        <div style={{ padding: '32px 12px', textAlign: 'center', color: 'var(--text-muted)', fontSize: 13 }}>No storage pools found.</div>
      )}
    </div>
  )
}

// ── Tasks table ───────────────────────────────────────────────────────────────

function TasksTable({ tasks }: { tasks: PveTask[] }) {
  const shown = tasks.slice(0, 30)
  return (
    <div style={{ overflowX: 'auto' }}>
      <table style={{ width: '100%', borderCollapse: 'collapse', fontSize: 13 }}>
        <thead>
          <tr style={{ borderBottom: '1px solid var(--border-subtle)' }}>
            {['Type', 'ID', 'Node', 'Status', 'Started', 'Duration'].map(h => (
              <th key={h} style={{ padding: '8px 12px', textAlign: 'left', fontSize: 11, textTransform: 'uppercase', letterSpacing: '0.06em', color: 'var(--text-muted)' }}>{h}</th>
            ))}
          </tr>
        </thead>
        <tbody>
          {shown.map(t => {
            const s = (t.status ?? '').toLowerCase()
            const dotColor = s === 'ok' ? 'var(--accent-success)' : s.startsWith('err') ? 'var(--accent-danger)' : 'var(--accent-warning)'
            const duration = t.endtime && t.starttime ? `${t.endtime - t.starttime}s` : t.starttime ? 'running' : '—'
            return (
              <tr key={t.upid} style={{ borderBottom: '1px solid var(--border-subtle)' }}>
                <td style={{ padding: '10px 12px', color: 'var(--text-primary)' }}>{t.type}</td>
                <td style={{ padding: '10px 12px', color: 'var(--text-muted)', fontFamily: 'monospace', fontSize: 11 }}>{t.id ?? '—'}</td>
                <td style={{ padding: '10px 12px', color: 'var(--text-muted)', fontFamily: 'monospace', fontSize: 12 }}>{t.node}</td>
                <td style={{ padding: '10px 12px' }}>
                  <span style={{ display: 'inline-flex', alignItems: 'center', gap: 5 }}>
                    <span style={{ width: 7, height: 7, borderRadius: '50%', background: dotColor, flexShrink: 0 }} />
                    <span style={{ color: 'var(--text-secondary)', fontSize: 12 }}>{t.status}</span>
                  </span>
                </td>
                <td style={{ padding: '10px 12px', color: 'var(--text-muted)', fontSize: 12 }}>{fmtRelTime(t.starttime)}</td>
                <td style={{ padding: '10px 12px', color: 'var(--text-muted)', fontSize: 12 }}>{duration}</td>
              </tr>
            )
          })}
        </tbody>
      </table>
      {shown.length === 0 && (
        <div style={{ padding: '32px 12px', textAlign: 'center', color: 'var(--text-muted)', fontSize: 13 }}>No recent tasks.</div>
      )}
    </div>
  )
}

// ── Backups tab ───────────────────────────────────────────────────────────────

function BackupsPanel({ hostId }: { hostId: string }) {
  const [jobs, setJobs] = useState<import('@/api/types').PveBackupJob[]>([])
  const [archives, setArchives] = useState<import('@/api/types').PveBackupArchive[]>([])
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    setLoading(true)
    api.proxmox.getBackupJobs(hostId)
      .then(r => { setJobs(r.jobs ?? []); setArchives(r.archives ?? []) })
      .catch(() => {})
      .finally(() => setLoading(false))
  }, [hostId])

  if (loading) return <div style={{ padding: '32px 12px', textAlign: 'center', color: 'var(--text-muted)', fontSize: 13 }}>Loading…</div>

  return (
    <div style={{ padding: 16, display: 'flex', flexDirection: 'column', gap: 20 }}>
      {/* Scheduled backup jobs */}
      <div>
        <p style={{ fontSize: 11, textTransform: 'uppercase', letterSpacing: '0.08em', color: 'var(--text-muted)', fontWeight: 500, marginBottom: 10 }}>
          Scheduled Jobs ({jobs.length})
        </p>
        {jobs.length === 0
          ? <div style={{ color: 'var(--text-muted)', fontSize: 13 }}>No backup jobs configured.</div>
          : (
            <div style={{ overflowX: 'auto' }}>
              <table style={{ width: '100%', borderCollapse: 'collapse', fontSize: 13 }}>
                <thead>
                  <tr style={{ borderBottom: '1px solid var(--border-subtle)' }}>
                    {['Job ID', 'Schedule', 'Storage', 'VMs', 'Mode', 'Enabled'].map(h => (
                      <th key={h} style={{ padding: '8px 12px', textAlign: 'left', fontSize: 11, textTransform: 'uppercase', letterSpacing: '0.06em', color: 'var(--text-muted)' }}>{h}</th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {jobs.map(j => (
                    <tr key={j.id} style={{ borderBottom: '1px solid var(--border-subtle)' }}>
                      <td style={{ padding: '10px 12px', color: 'var(--text-primary)', fontFamily: 'monospace', fontSize: 12 }}>{j.id}</td>
                      <td style={{ padding: '10px 12px', color: 'var(--text-muted)', fontFamily: 'monospace', fontSize: 12 }}>{j.schedule ?? j.starttime ?? '—'}</td>
                      <td style={{ padding: '10px 12px', color: 'var(--text-secondary)', fontSize: 12 }}>{j.storage ?? '—'}</td>
                      <td style={{ padding: '10px 12px', color: 'var(--text-muted)', fontSize: 12, maxWidth: '16rem', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{j.vmid ?? 'all'}</td>
                      <td style={{ padding: '10px 12px', color: 'var(--text-muted)', fontSize: 12 }}>{j.mode ?? '—'}</td>
                      <td style={{ padding: '10px 12px', fontSize: 12 }}>
                        <span style={{ color: j.enabled ? 'var(--accent-success)' : 'var(--text-disabled)' }}>{j.enabled ? 'yes' : 'no'}</span>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )
        }
      </div>

      {/* Recent backup archives */}
      <div>
        <p style={{ fontSize: 11, textTransform: 'uppercase', letterSpacing: '0.08em', color: 'var(--text-muted)', fontWeight: 500, marginBottom: 10 }}>
          Recent Archives ({archives.length})
        </p>
        {archives.length === 0
          ? <div style={{ color: 'var(--text-muted)', fontSize: 13 }}>No backup archives found.</div>
          : (
            <div style={{ overflowX: 'auto' }}>
              <table style={{ width: '100%', borderCollapse: 'collapse', fontSize: 13 }}>
                <thead>
                  <tr style={{ borderBottom: '1px solid var(--border-subtle)' }}>
                    {['VM ID', 'Volume', 'Storage', 'Node', 'Created', 'Size'].map(h => (
                      <th key={h} style={{ padding: '8px 12px', textAlign: 'left', fontSize: 11, textTransform: 'uppercase', letterSpacing: '0.06em', color: 'var(--text-muted)' }}>{h}</th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {archives.slice(0, 50).map((a, i) => (
                    <tr key={i} style={{ borderBottom: '1px solid var(--border-subtle)' }}>
                      <td style={{ padding: '10px 12px', color: 'var(--text-primary)', fontFamily: 'monospace', fontSize: 12 }}>{a.vmid}</td>
                      <td style={{ padding: '10px 12px', color: 'var(--text-muted)', fontFamily: 'monospace', fontSize: 11, maxWidth: '20rem', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{a.volid}</td>
                      <td style={{ padding: '10px 12px', color: 'var(--text-secondary)', fontSize: 12 }}>{a.storage}</td>
                      <td style={{ padding: '10px 12px', color: 'var(--text-muted)', fontSize: 12 }}>{a.node}</td>
                      <td style={{ padding: '10px 12px', color: 'var(--text-muted)', fontSize: 12 }}>{fmtRelTime(a.ctime)}</td>
                      <td style={{ padding: '10px 12px', color: 'var(--text-muted)', fontSize: 12 }}>{fmtBytes(a.size)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )
        }
      </div>
    </div>
  )
}

// ── Per-host panel ────────────────────────────────────────────────────────────

function HostPanel({ host }: { host: ProxmoxHost }) {
  const [tab, setTab]         = useState<HostTab>('vms')
  const [nodes, setNodes]     = useState<PveNode[]>([])
  const [vms, setVms]         = useState<PveVm[]>([])
  const [storage, setStorage] = useState<PveStorage[]>([])
  const [tasks, setTasks]     = useState<PveTask[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError]     = useState<string | null>(null)
  const [tagsMap, setTagsMap] = useState<TagMap>({})
  const [allTags, setAllTags] = useState<Tag[]>([])

  const fetchTags = useCallback(async () => {
    try {
      const [m, all] = await Promise.all([api.tags.map('proxmox_vm'), api.tags.list()])
      setTagsMap(m); setAllTags(all)
    } catch { /* non-critical */ }
  }, [])

  const fetchAll = useCallback(async () => {
    setLoading(true); setError(null)
    try {
      const [n, v, s, t] = await Promise.all([
        api.proxmox.getNodes(host.id),
        api.proxmox.getVms(host.id),
        api.proxmox.getStorage(host.id),
        api.proxmox.getTasks(host.id),
      ])
      setNodes(Array.isArray(n) ? n : [])
      setVms(Array.isArray(v) ? v : [])
      setStorage(Array.isArray(s) ? s : [])
      setTasks(Array.isArray(t) ? t : [])
    } catch (err) {
      setError(err instanceof ApiClientError ? err.message : 'Failed to load data')
    } finally { setLoading(false) }
  }, [host.id])

  useEffect(() => {
    fetchAll()
    fetchTags()
    const id = setInterval(fetchAll, 15_000)
    return () => clearInterval(id)
  }, [fetchAll, fetchTags])

  if (loading) return (
    <div style={{ padding: '40px 0', textAlign: 'center', color: 'var(--text-muted)', fontSize: 13 }}>Loading…</div>
  )

  if (error) return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 10, padding: 16, borderRadius: 8, background: 'var(--bg-elevated)', border: '1px solid var(--accent-danger)', color: 'var(--accent-danger)', fontSize: 13 }}>
      <AlertCircle size={16} style={{ flexShrink: 0 }} />
      <span>{error}</span>
      <button onClick={fetchAll} style={{ marginLeft: 'auto', padding: '4px 10px', borderRadius: 5, border: '1px solid var(--accent-danger)', background: 'transparent', color: 'var(--accent-danger)', cursor: 'pointer', fontSize: 12 }}>Retry</button>
    </div>
  )

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 20 }}>
      <SummaryBar nodes={nodes} vms={vms} storage={storage} />

      {nodes.length > 0 && (
        <div>
          <p style={{ fontSize: 11, textTransform: 'uppercase', letterSpacing: '0.08em', color: 'var(--text-muted)', fontWeight: 500, marginBottom: 10 }}>
            Nodes
          </p>
          <NodeCards nodes={nodes} />
        </div>
      )}

      <div>
        <div style={{ display: 'flex', gap: 4, marginBottom: 0, borderBottom: '1px solid var(--border-subtle)', paddingBottom: 10 }}>
          {(['vms', 'storage', 'tasks', 'backups'] as HostTab[]).map(t => (
            <button key={t} onClick={() => setTab(t)} style={{ padding: '5px 16px', borderRadius: 6, border: 'none', cursor: 'pointer', fontSize: 13, fontWeight: 500, background: tab === t ? 'var(--accent-primary)' : 'transparent', color: tab === t ? '#fff' : 'var(--text-secondary)', transition: 'background 0.15s' }}>
              {t === 'vms' ? `VMs & LXCs (${vms.length})` : t === 'storage' ? `Storage (${storage.length})` : t === 'tasks' ? `Tasks (${tasks.length})` : 'Backups'}
            </button>
          ))}
          <button onClick={fetchAll} style={{ marginLeft: 'auto', display: 'flex', alignItems: 'center', gap: 5, padding: '5px 12px', borderRadius: 6, border: '1px solid var(--border-subtle)', background: 'transparent', color: 'var(--text-muted)', cursor: 'pointer', fontSize: 12 }}>
            <RefreshCw size={12} /> Refresh
          </button>
        </div>

        <div style={{ background: 'var(--bg-panel)', border: '1px solid var(--border-subtle)', borderTop: 'none', borderRadius: '0 0 8px 8px', overflow: 'hidden' }}>
          {tab === 'vms'     && <VmsTable hostId={host.id} vms={vms} tagsMap={tagsMap} allTags={allTags} onRefresh={fetchAll} onTagsChange={fetchTags} />}
          {tab === 'storage' && <StorageTable pools={storage} />}
          {tab === 'tasks'   && <TasksTable tasks={tasks} />}
          {tab === 'backups' && <BackupsPanel hostId={host.id} />}
        </div>
      </div>
    </div>
  )
}

// ── Main page ─────────────────────────────────────────────────────────────────

export default function ProxmoxPage() {
  const [hosts, setHosts]           = useState<ProxmoxHost[]>([])
  const [loading, setLoading]       = useState(true)
  const [showAddModal, setShowAddModal] = useState(false)
  const [activeHostId, setActiveHostId] = useState<string | null>(null)
  const [deleting, setDeleting]     = useState<string | null>(null)

  const initialisedRef = useRef(false)
  const fetchHosts = useCallback(async () => {
    setLoading(true)
    try {
      const list = await api.proxmox.listHosts()
      setHosts(list)
      if (list.length > 0 && !initialisedRef.current) {
        initialisedRef.current = true
        setActiveHostId(list[0].id)
      }
    } catch { notify.error('Failed to load Proxmox hosts') }
    finally { setLoading(false) }
  }, [])

  useEffect(() => { fetchHosts() }, [fetchHosts])

  const handleDelete = async (host: ProxmoxHost) => {
    if (!confirm(`Remove host "${host.name}"? This will not affect Proxmox itself.`)) return
    setDeleting(host.id)
    try {
      await api.proxmox.deleteHost(host.id)
      notify.success(`Host "${host.name}" removed`)
      const remaining = hosts.filter(h => h.id !== host.id)
      setHosts(remaining)
      if (activeHostId === host.id) setActiveHostId(remaining.length > 0 ? remaining[0].id : null)
    } catch (err) { notify.error(err instanceof ApiClientError ? err.message : 'Failed to remove host') }
    finally { setDeleting(null) }
  }

  const activeHost = hosts.find(h => h.id === activeHostId) ?? null

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: 'calc(100vh - 88px)', gap: 12 }}>
      {/* Header */}
      <div style={{ display: 'flex', alignItems: 'center', gap: 10, flexShrink: 0 }}>
        <Server size={18} style={{ color: 'var(--accent-primary)' }} />
        <h1 style={{ fontSize: 18, fontWeight: 600, color: 'var(--text-primary)', margin: 0 }}>Proxmox</h1>
      </div>

      {/* Sidebar + content */}
      <div style={{ display: 'flex', gap: 16, flex: 1, minHeight: 0, overflow: 'hidden' }}>

        {/* Left sidebar */}
        <div style={{
          width: 260, flexShrink: 0, display: 'flex', flexDirection: 'column', gap: 10,
          overflowY: 'auto', paddingRight: 4,
        }}>
          <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
            <span style={{ fontSize: 12, fontWeight: 600, color: 'var(--text-secondary)', textTransform: 'uppercase', letterSpacing: '0.06em' }}>Hosts</span>
            <button onClick={() => setShowAddModal(true)}
              style={{ background: 'none', border: 'none', cursor: 'pointer', color: 'var(--accent-primary)', display: 'flex', alignItems: 'center', gap: 3, fontSize: 12 }}>
              <Plus size={13} /> Add
            </button>
          </div>

          {loading && (
            <div style={{ color: 'var(--text-muted)', fontSize: 12, padding: '16px 0', textAlign: 'center' }}>Loading…</div>
          )}

          {!loading && hosts.length === 0 && (
            <div style={{ color: 'var(--text-muted)', fontSize: 12, padding: '16px 0', textAlign: 'center' }}>
              No hosts configured.
            </div>
          )}

          {hosts.map(h => (
            <div
              key={h.id}
              onClick={() => setActiveHostId(h.id)}
              style={{
                background: activeHostId === h.id ? 'var(--accent-primary-subtle)' : 'var(--bg-card)',
                border: `1px solid ${activeHostId === h.id ? 'var(--accent-primary)' : 'var(--border-subtle)'}`,
                borderRadius: 8, padding: '10px 14px', cursor: 'pointer',
                display: 'flex', alignItems: 'center', gap: 10,
                transition: 'border-color 0.15s',
              }}
            >
              <HardDrive size={18} style={{ color: activeHostId === h.id ? 'var(--accent-primary)' : 'var(--text-muted)', flexShrink: 0 }} />
              <div style={{ flex: 1, minWidth: 0 }}>
                <div style={{ fontWeight: 600, fontSize: 13, color: 'var(--text-primary)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{h.name}</div>
                <div style={{ fontSize: 11, color: 'var(--text-muted)', fontFamily: 'monospace', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                  {h.node} · {h.url.replace(/^https?:\/\//, '')}
                </div>
              </div>
              <button
                onClick={e => { e.stopPropagation(); handleDelete(h) }}
                disabled={deleting === h.id}
                title={`Remove ${h.name}`}
                style={{ background: 'none', border: 'none', cursor: deleting === h.id ? 'not-allowed' : 'pointer', color: 'var(--accent-danger)', padding: 4, flexShrink: 0, opacity: deleting === h.id ? 0.4 : 1 }}
              >
                <Trash2 size={12} />
              </button>
            </div>
          ))}
        </div>

        {/* Right content */}
        <div style={{ flex: 1, overflowY: 'auto', minWidth: 0 }}>
          {activeHost ? (
            <HostPanel key={activeHost.id} host={activeHost} />
          ) : (
            <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center', height: '100%', gap: 14, color: 'var(--text-muted)' }}>
              <Server size={40} style={{ opacity: 0.25 }} />
              <span style={{ fontSize: 13 }}>{loading ? 'Loading…' : 'Select a host or add one to get started'}</span>
              {!loading && hosts.length === 0 && (
                <Button size="sm" variant="ghost" onClick={() => setShowAddModal(true)}>
                  <Plus size={12} /> Add your first host
                </Button>
              )}
            </div>
          )}
        </div>
      </div>

      {showAddModal && <AddHostModal onClose={() => setShowAddModal(false)} onAdded={fetchHosts} />}
    </div>
  )
}
