import { useState, useEffect, useCallback, useRef } from 'react'
import { Server, Plus, Trash2, Play, Square, RotateCcw, ChevronRight } from 'lucide-react'
import { api, ApiClientError } from '@/api/client'
import { notify } from '@/store/notifications'
import type { ProxmoxHost, PveVm, PveNode, PveStorage, PveTask, AddHostRequest } from '@/api/types'
import Button from '@/components/ui/Button'

// ── helpers ───────────────────────────────────────────────────────────────────

function fmtBytes(b: number) {
  if (b === 0) return '0 B'
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
  const diff = Math.floor(Date.now() / 1000) - ts
  if (diff < 60) return `${diff}s ago`
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`
  return `${Math.floor(diff / 86400)}d ago`
}

function StatusDot({ status }: { status: string }) {
  const s = status.toLowerCase()
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

function UsageBar({ used, total, label }: { used: number; total: number; label: string }) {
  const pct = total > 0 ? Math.min(100, (used / total) * 100) : 0
  const color = pct > 85 ? 'var(--accent-danger)' : pct > 60 ? 'var(--accent-warning)' : 'var(--accent-success)'
  return (
    <div>
      <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 3 }}>
        <span style={{ fontSize: 11, color: 'var(--text-muted)' }}>{label}</span>
        <span style={{ fontSize: 11, color: 'var(--text-secondary)' }}>{fmtBytes(used)} / {fmtBytes(total)}</span>
      </div>
      <div style={{ height: 4, borderRadius: 2, background: 'var(--bg-base)' }}>
        <div style={{ height: '100%', width: `${pct}%`, borderRadius: 2, background: color, transition: 'width 0.3s' }} />
      </div>
    </div>
  )
}

// ── sub-tabs ──────────────────────────────────────────────────────────────────

type HostTab = 'vms' | 'storage' | 'tasks'

// ── Add Host Modal ────────────────────────────────────────────────────────────

interface AddHostModalProps {
  onClose: () => void
  onAdded: () => void
}

function AddHostModal({ onClose, onAdded }: AddHostModalProps) {
  const [form, setForm] = useState<AddHostRequest>({
    name: '',
    url: '',
    node: 'pve',
    token_id: '',
    token_secret: '',
    fingerprint: '',
  })
  const [saving, setSaving] = useState(false)

  const set = (k: keyof AddHostRequest, v: string) =>
    setForm(f => ({ ...f, [k]: v }))

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    setSaving(true)
    try {
      const payload: AddHostRequest = { ...form }
      if (!payload.fingerprint) delete payload.fingerprint
      if (!payload.node) payload.node = 'pve'
      await api.proxmox.addHost(payload)
      notify.success('Proxmox host added')
      onAdded()
      onClose()
    } catch (err) {
      notify.error(err instanceof ApiClientError ? err.message : 'Failed to add host')
    } finally {
      setSaving(false)
    }
  }

  return (
    <div
      style={{
        position: 'fixed', inset: 0, zIndex: 50,
        background: 'rgba(0,0,0,0.55)',
        display: 'flex', alignItems: 'center', justifyContent: 'center',
      }}
      onClick={e => { if (e.target === e.currentTarget) onClose() }}
    >
      <div
        style={{
          background: 'var(--bg-panel)', border: '1px solid var(--border-subtle)',
          borderRadius: 10, padding: 24, width: '100%', maxWidth: 440,
        }}
      >
        <h2 style={{ fontSize: 15, fontWeight: 600, color: 'var(--text-primary)', marginBottom: 20 }}>
          Add Proxmox Host
        </h2>
        <form onSubmit={handleSubmit} style={{ display: 'flex', flexDirection: 'column', gap: 14 }}>
          {([
            { label: 'Name', key: 'name', placeholder: 'My Proxmox', required: true },
            { label: 'URL', key: 'url', placeholder: 'https://192.168.1.100:8006', required: true },
            { label: 'Node', key: 'node', placeholder: 'pve', required: false },
            { label: 'Token ID', key: 'token_id', placeholder: 'user@realm!tokenname', required: true },
            { label: 'Token Secret', key: 'token_secret', placeholder: 'xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx', required: true },
            { label: 'Fingerprint (optional)', key: 'fingerprint', placeholder: 'AA:BB:CC:…', required: false },
          ] as { label: string; key: keyof AddHostRequest; placeholder: string; required: boolean }[]).map(f => (
            <div key={f.key}>
              <label
                htmlFor={`addhost-${f.key}`}
                style={{ display: 'block', fontSize: 12, color: 'var(--text-secondary)', marginBottom: 5 }}
              >
                {f.label}
              </label>
              <input
                id={`addhost-${f.key}`}
                type={f.key === 'token_secret' ? 'password' : 'text'}
                value={form[f.key] ?? ''}
                onChange={e => set(f.key, e.target.value)}
                placeholder={f.placeholder}
                required={f.required}
                style={{
                  width: '100%', padding: '7px 10px', borderRadius: 6, fontSize: 13,
                  background: 'var(--bg-elevated)', border: '1px solid var(--border-default)',
                  color: 'var(--text-primary)', outline: 'none', boxSizing: 'border-box',
                }}
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

// ── Node Overview ─────────────────────────────────────────────────────────────

function NodeOverview({ nodes }: { nodes: PveNode[] }) {
  if (nodes.length === 0) return null
  return (
    <div style={{ display: 'grid', gap: 12, gridTemplateColumns: 'repeat(auto-fill, minmax(220px, 1fr))' }}>
      {nodes.map(n => (
        <div key={n.node} style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)', borderRadius: 8, padding: 14 }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 12 }}>
            <Server size={14} style={{ color: 'var(--accent-primary)', flexShrink: 0 }} />
            <span style={{ fontSize: 13, fontWeight: 600, color: 'var(--text-primary)' }}>{n.node}</span>
            <span style={{
              marginLeft: 'auto', fontSize: 11, padding: '1px 7px', borderRadius: 4,
              background: n.status === 'online' ? 'var(--accent-success-subtle)' : 'var(--bg-base)',
              color: n.status === 'online' ? 'var(--accent-success)' : 'var(--text-muted)',
            }}>
              {n.status}
            </span>
          </div>
          <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
            <UsageBar used={n.cpu * n.maxcpu} total={n.maxcpu} label={`CPU (${n.maxcpu} cores)`} />
            <UsageBar used={n.mem} total={n.maxmem} label="RAM" />
            <UsageBar used={n.disk} total={n.maxdisk} label="Disk" />
          </div>
          {n.uptime > 0 && (
            <div style={{ fontSize: 11, color: 'var(--text-muted)', marginTop: 8 }}>
              up {fmtUptime(n.uptime)}
            </div>
          )}
        </div>
      ))}
    </div>
  )
}

// ── VMs Table sort header ────────────────────────────────────────────────────

interface SortHdProps { k: SortKey; label: string; sortKey: SortKey; sortAsc: boolean; onSort: (k: SortKey) => void }

function SortHd({ k, label, sortKey, sortAsc, onSort }: SortHdProps) {
  return (
    <th
      onClick={() => onSort(k)}
      style={{ padding: '8px 12px', textAlign: 'left', fontSize: 11, textTransform: 'uppercase', letterSpacing: '0.06em', color: 'var(--text-muted)', cursor: 'pointer', userSelect: 'none', whiteSpace: 'nowrap' }}
    >
      {label}{sortKey === k ? (sortAsc ? ' ↑' : ' ↓') : ''}
    </th>
  )
}

// ── VMs Table ─────────────────────────────────────────────────────────────────

type SortKey = keyof Pick<PveVm, 'name' | 'vmid' | 'status' | 'cpu' | 'mem' | 'node'>

function VmsTable({ hostId, vms, onRefresh }: { hostId: string; vms: PveVm[]; onRefresh: () => void }) {
  const [sortKey, setSortKey] = useState<SortKey>('vmid')
  const [sortAsc, setSortAsc] = useState(true)
  const [busy, setBusy] = useState<string | null>(null)

  const handleSort = (k: SortKey) => {
    if (k === sortKey) setSortAsc(a => !a)
    else { setSortKey(k); setSortAsc(true) }
  }

  const sorted = [...vms].sort((a, b) => {
    let cmp = 0
    if (sortKey === 'name' || sortKey === 'status' || sortKey === 'node') {
      cmp = a[sortKey].localeCompare(b[sortKey])
    } else {
      cmp = a[sortKey] - b[sortKey]
    }
    return sortAsc ? cmp : -cmp
  })

  const handleAction = async (vm: PveVm, action: 'start' | 'stop' | 'reboot') => {
    const key = `${vm.vmid}-${action}`
    setBusy(key)
    try {
      await api.proxmox.vmAction(hostId, vm.vmid, action)
      notify.success(`${action} sent to ${vm.name}`)
      onRefresh()
    } catch (err) {
      notify.error(err instanceof ApiClientError ? err.message : `Failed to ${action} ${vm.name}`)
    } finally {
      setBusy(null)
    }
  }


  return (
    <div style={{ overflowX: 'auto' }}>
      <table style={{ width: '100%', borderCollapse: 'collapse', fontSize: 13 }}>
        <thead>
          <tr style={{ borderBottom: '1px solid var(--border-subtle)' }}>
            <SortHd k="name" label="Name" sortKey={sortKey} sortAsc={sortAsc} onSort={handleSort} />
            <SortHd k="vmid" label="VMID" sortKey={sortKey} sortAsc={sortAsc} onSort={handleSort} />
            <th style={{ padding: '8px 12px', fontSize: 11, textTransform: 'uppercase', letterSpacing: '0.06em', color: 'var(--text-muted)', textAlign: 'left' }}>Type</th>
            <SortHd k="status" label="Status" sortKey={sortKey} sortAsc={sortAsc} onSort={handleSort} />
            <SortHd k="cpu" label="CPU" sortKey={sortKey} sortAsc={sortAsc} onSort={handleSort} />
            <SortHd k="mem" label="RAM" sortKey={sortKey} sortAsc={sortAsc} onSort={handleSort} />
            <th style={{ padding: '8px 12px', fontSize: 11, textTransform: 'uppercase', letterSpacing: '0.06em', color: 'var(--text-muted)', textAlign: 'left' }}>Disk</th>
            <SortHd k="node" label="Node" sortKey={sortKey} sortAsc={sortAsc} onSort={handleSort} />
            <th style={{ padding: '8px 12px', fontSize: 11, textTransform: 'uppercase', letterSpacing: '0.06em', color: 'var(--text-muted)', textAlign: 'left' }}>Actions</th>
          </tr>
        </thead>
        <tbody>
          {sorted.map(vm => (
            <tr key={`${vm.node}-${vm.vmid}`} style={{ borderBottom: '1px solid var(--border-subtle)' }}>
              <td style={{ padding: '10px 12px', color: 'var(--text-primary)', fontWeight: 500 }}>{vm.name}</td>
              <td style={{ padding: '10px 12px', color: 'var(--text-secondary)', fontFamily: 'monospace' }}>{vm.vmid}</td>
              <td style={{ padding: '10px 12px' }}>
                <span style={{ fontSize: 11, padding: '2px 7px', borderRadius: 4, background: 'var(--bg-elevated)', color: 'var(--text-muted)', border: '1px solid var(--border-subtle)' }}>
                  {vm.type.toUpperCase()}
                </span>
              </td>
              <td style={{ padding: '10px 12px' }}><StatusDot status={vm.status} /></td>
              <td style={{ padding: '10px 12px', color: 'var(--text-secondary)' }}>
                {vm.status === 'running' ? `${(vm.cpu * 100).toFixed(1)}%` : '—'}
              </td>
              <td style={{ padding: '10px 12px', color: 'var(--text-secondary)' }}>
                {vm.status === 'running'
                  ? `${(vm.mem / 1073741824).toFixed(1)} / ${(vm.maxmem / 1073741824).toFixed(1)} GB`
                  : `— / ${(vm.maxmem / 1073741824).toFixed(1)} GB`}
              </td>
              <td style={{ padding: '10px 12px', color: 'var(--text-secondary)' }}>
                {fmtBytes(vm.disk)} / {fmtBytes(vm.maxdisk)}
              </td>
              <td style={{ padding: '10px 12px', color: 'var(--text-muted)', fontFamily: 'monospace', fontSize: 12 }}>{vm.node}</td>
              <td style={{ padding: '10px 12px' }}>
                <div style={{ display: 'flex', gap: 4 }}>
                  {vm.status !== 'running' && (
                    <button
                      onClick={() => handleAction(vm, 'start')}
                      disabled={busy !== null}
                      title="Start"
                      aria-label={`Start ${vm.name}`}
                      style={{ padding: '4px 6px', borderRadius: 5, border: '1px solid var(--border-subtle)', background: 'var(--bg-elevated)', color: 'var(--accent-success)', cursor: busy !== null ? 'not-allowed' : 'pointer', opacity: busy !== null ? 0.4 : 1 }}
                    >
                      <Play size={12} />
                    </button>
                  )}
                  {vm.status === 'running' && (
                    <>
                      <button
                        onClick={() => handleAction(vm, 'stop')}
                        disabled={busy !== null}
                        title="Stop"
                        aria-label={`Stop ${vm.name}`}
                        style={{ padding: '4px 6px', borderRadius: 5, border: '1px solid var(--border-subtle)', background: 'var(--bg-elevated)', color: 'var(--accent-danger)', cursor: busy !== null ? 'not-allowed' : 'pointer', opacity: busy !== null ? 0.4 : 1 }}
                      >
                        <Square size={12} />
                      </button>
                      <button
                        onClick={() => handleAction(vm, 'reboot')}
                        disabled={busy !== null}
                        title="Reboot"
                        aria-label={`Reboot ${vm.name}`}
                        style={{ padding: '4px 6px', borderRadius: 5, border: '1px solid var(--border-subtle)', background: 'var(--bg-elevated)', color: 'var(--accent-warning)', cursor: busy !== null ? 'not-allowed' : 'pointer', opacity: busy !== null ? 0.4 : 1 }}
                      >
                        <RotateCcw size={12} />
                      </button>
                    </>
                  )}
                </div>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      {sorted.length === 0 && (
        <div style={{ padding: '32px 12px', textAlign: 'center', color: 'var(--text-muted)', fontSize: 13 }}>
          No VMs or containers found.
        </div>
      )}
    </div>
  )
}

// ── Storage Table ─────────────────────────────────────────────────────────────

function StorageTable({ pools }: { pools: PveStorage[] }) {
  return (
    <div style={{ overflowX: 'auto' }}>
      <table style={{ width: '100%', borderCollapse: 'collapse', fontSize: 13 }}>
        <thead>
          <tr style={{ borderBottom: '1px solid var(--border-subtle)' }}>
            {['Storage', 'Type', 'Usage', 'Active'].map(h => (
              <th key={h} style={{ padding: '8px 12px', textAlign: 'left', fontSize: 11, textTransform: 'uppercase', letterSpacing: '0.06em', color: 'var(--text-muted)' }}>
                {h}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {pools.map(p => (
            <tr key={p.storage} style={{ borderBottom: '1px solid var(--border-subtle)' }}>
              <td style={{ padding: '10px 12px', color: 'var(--text-primary)', fontWeight: 500, fontFamily: 'monospace' }}>{p.storage}</td>
              <td style={{ padding: '10px 12px', color: 'var(--text-muted)', fontSize: 12 }}>{p.type}</td>
              <td style={{ padding: '10px 12px', minWidth: 180 }}>
                <UsageBar used={p.used} total={p.total} label="" />
              </td>
              <td style={{ padding: '10px 12px' }}>
                <span style={{
                  fontSize: 11, padding: '2px 7px', borderRadius: 4,
                  background: p.active ? 'var(--accent-success-subtle)' : 'var(--bg-elevated)',
                  color: p.active ? 'var(--accent-success)' : 'var(--text-muted)',
                }}>
                  {p.active ? 'active' : 'inactive'}
                </span>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      {pools.length === 0 && (
        <div style={{ padding: '32px 12px', textAlign: 'center', color: 'var(--text-muted)', fontSize: 13 }}>
          No storage pools found.
        </div>
      )}
    </div>
  )
}

// ── Tasks Table ───────────────────────────────────────────────────────────────

function TasksTable({ tasks }: { tasks: PveTask[] }) {
  const shown = tasks.slice(0, 20)
  return (
    <div style={{ overflowX: 'auto' }}>
      <table style={{ width: '100%', borderCollapse: 'collapse', fontSize: 13 }}>
        <thead>
          <tr style={{ borderBottom: '1px solid var(--border-subtle)' }}>
            {['Type', 'ID', 'Node', 'Status', 'Started'].map(h => (
              <th key={h} style={{ padding: '8px 12px', textAlign: 'left', fontSize: 11, textTransform: 'uppercase', letterSpacing: '0.06em', color: 'var(--text-muted)' }}>
                {h}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {shown.map(t => {
            const s = t.status.toLowerCase()
            const dotColor = s === 'ok' ? 'var(--accent-success)'
              : s.startsWith('err') ? 'var(--accent-danger)'
              : 'var(--accent-warning)'
            return (
              <tr key={t.upid} style={{ borderBottom: '1px solid var(--border-subtle)' }}>
                <td style={{ padding: '10px 12px', color: 'var(--text-primary)' }}>{t.type}</td>
                <td style={{ padding: '10px 12px', color: 'var(--text-muted)', fontFamily: 'monospace', fontSize: 11 }}>
                  {t.id ?? '—'}
                </td>
                <td style={{ padding: '10px 12px', color: 'var(--text-muted)', fontFamily: 'monospace', fontSize: 12 }}>{t.node}</td>
                <td style={{ padding: '10px 12px' }}>
                  <span style={{ display: 'inline-flex', alignItems: 'center', gap: 5 }}>
                    <span style={{ width: 7, height: 7, borderRadius: '50%', background: dotColor, flexShrink: 0, display: 'inline-block' }} />
                    <span style={{ color: 'var(--text-secondary)', fontSize: 12 }}>{t.status}</span>
                  </span>
                </td>
                <td style={{ padding: '10px 12px', color: 'var(--text-muted)', fontSize: 12 }}>
                  {fmtRelTime(t.starttime)}
                </td>
              </tr>
            )
          })}
        </tbody>
      </table>
      {shown.length === 0 && (
        <div style={{ padding: '32px 12px', textAlign: 'center', color: 'var(--text-muted)', fontSize: 13 }}>
          No recent tasks.
        </div>
      )}
    </div>
  )
}

// ── Per-host content panel ────────────────────────────────────────────────────

function HostPanel({ host }: { host: ProxmoxHost }) {
  const [tab, setTab] = useState<HostTab>('vms')
  const [nodes, setNodes] = useState<PveNode[]>([])
  const [vms, setVms] = useState<PveVm[]>([])
  const [storage, setStorage] = useState<PveStorage[]>([])
  const [tasks, setTasks] = useState<PveTask[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  const fetchAll = useCallback(async () => {
    setLoading(true)
    setError(null)
    try {
      const [n, v, s, t] = await Promise.all([
        api.proxmox.getNodes(host.id),
        api.proxmox.getVms(host.id),
        api.proxmox.getStorage(host.id),
        api.proxmox.getTasks(host.id),
      ])
      setNodes(n)
      setVms(v)
      setStorage(s)
      setTasks(t)
    } catch (err) {
      setError(err instanceof ApiClientError ? err.message : 'Failed to load data')
    } finally {
      setLoading(false)
    }
  }, [host.id])

  useEffect(() => {
    fetchAll()
    const id = setInterval(fetchAll, 15_000)
    return () => clearInterval(id)
  }, [fetchAll])

  if (loading) {
    return <div style={{ padding: '40px 0', textAlign: 'center', color: 'var(--text-muted)', fontSize: 13 }}>Loading…</div>
  }

  if (error) {
    return (
      <div style={{ padding: 16, borderRadius: 8, background: 'var(--bg-elevated)', border: '1px solid var(--accent-danger)', color: 'var(--accent-danger)', fontSize: 13 }}>
        {error}
      </div>
    )
  }

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 20 }}>
      {/* Node overview */}
      {nodes.length > 0 && (
        <div>
          <p style={{ fontSize: 11, textTransform: 'uppercase', letterSpacing: '0.08em', color: 'var(--text-muted)', fontWeight: 500, marginBottom: 10 }}>
            Nodes
          </p>
          <NodeOverview nodes={nodes} />
        </div>
      )}

      {/* Tabs */}
      <div>
        <div style={{ display: 'flex', gap: 4, marginBottom: 14, borderBottom: '1px solid var(--border-subtle)', paddingBottom: 10 }}>
          {(['vms', 'storage', 'tasks'] as HostTab[]).map(t => (
            <button
              key={t}
              onClick={() => setTab(t)}
              style={{
                padding: '5px 16px', borderRadius: 6, border: 'none', cursor: 'pointer', fontSize: 13, fontWeight: 500,
                background: tab === t ? 'var(--accent-primary)' : 'transparent',
                color: tab === t ? '#fff' : 'var(--text-secondary)',
                transition: 'background 0.15s',
              }}
            >
              {t === 'vms' ? 'VMs & LXCs' : t.charAt(0).toUpperCase() + t.slice(1)}
            </button>
          ))}
          <button
            onClick={fetchAll}
            style={{ marginLeft: 'auto', padding: '5px 12px', borderRadius: 6, border: '1px solid var(--border-subtle)', background: 'transparent', color: 'var(--text-muted)', cursor: 'pointer', fontSize: 12 }}
          >
            Refresh
          </button>
        </div>

        <div style={{ background: 'var(--bg-panel)', border: '1px solid var(--border-subtle)', borderRadius: 8, overflow: 'hidden' }}>
          {tab === 'vms' && <VmsTable hostId={host.id} vms={vms} onRefresh={fetchAll} />}
          {tab === 'storage' && <StorageTable pools={storage} />}
          {tab === 'tasks' && <TasksTable tasks={tasks} />}
        </div>
      </div>
    </div>
  )
}

// ── Main Page ─────────────────────────────────────────────────────────────────

export default function ProxmoxPage() {
  const [hosts, setHosts] = useState<ProxmoxHost[]>([])
  const [loading, setLoading] = useState(true)
  const [showAddModal, setShowAddModal] = useState(false)
  const [activeHostId, setActiveHostId] = useState<string | null>(null)
  const [deleting, setDeleting] = useState<string | null>(null)

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
    } catch {
      notify.error('Failed to load Proxmox hosts')
    } finally {
      setLoading(false)
    }
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
      if (activeHostId === host.id) {
        setActiveHostId(remaining.length > 0 ? remaining[0].id : null)
      }
    } catch (err) {
      notify.error(err instanceof ApiClientError ? err.message : 'Failed to remove host')
    } finally {
      setDeleting(null)
    }
  }

  const activeHost = hosts.find(h => h.id === activeHostId) ?? null

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 20 }}>
      {/* Header */}
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
        <h1 style={{ fontSize: 18, fontWeight: 600, color: 'var(--text-primary)' }}>Proxmox</h1>
        <Button size="sm" variant="primary" onClick={() => setShowAddModal(true)}>
          <Plus size={13} style={{ marginRight: 5 }} /> Add Host
        </Button>
      </div>

      {loading && (
        <div style={{ padding: '40px 0', textAlign: 'center', color: 'var(--text-muted)', fontSize: 13 }}>
          Loading hosts…
        </div>
      )}

      {!loading && hosts.length === 0 && (
        <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 14, padding: '60px 0' }}>
          <Server size={40} style={{ color: 'var(--text-muted)', opacity: 0.35 }} />
          <p style={{ color: 'var(--text-muted)', fontSize: 14 }}>No Proxmox hosts configured.</p>
          <Button size="sm" variant="ghost" onClick={() => setShowAddModal(true)}>
            <Plus size={12} style={{ marginRight: 5 }} /> Add your first host
          </Button>
        </div>
      )}

      {!loading && hosts.length > 0 && (
        <>
          {/* Host tabs */}
          <div style={{ display: 'flex', gap: 6, flexWrap: 'wrap', alignItems: 'center' }}>
            {hosts.map(h => (
              <div key={h.id} style={{ display: 'flex', alignItems: 'center' }}>
                <button
                  onClick={() => setActiveHostId(h.id)}
                  style={{
                    padding: '6px 14px', borderRadius: 6, border: '1px solid var(--border-subtle)',
                    background: activeHostId === h.id ? 'var(--accent-primary-subtle)' : 'var(--bg-elevated)',
                    color: activeHostId === h.id ? 'var(--accent-primary)' : 'var(--text-secondary)',
                    cursor: 'pointer', fontSize: 13, fontWeight: activeHostId === h.id ? 600 : 400,
                    display: 'flex', alignItems: 'center', gap: 6,
                  }}
                >
                  <ChevronRight size={12} style={{ opacity: activeHostId === h.id ? 1 : 0 }} />
                  {h.name}
                  <span style={{ fontSize: 11, color: 'var(--text-muted)', fontFamily: 'monospace' }}>
                    {h.node}
                  </span>
                </button>
                <button
                  onClick={() => handleDelete(h)}
                  disabled={deleting === h.id}
                  title={`Remove host ${h.name}`}
                  aria-label={`Remove host ${h.name}`}
                  style={{ marginLeft: 4, padding: '4px 5px', borderRadius: 5, border: '1px solid var(--border-subtle)', background: 'transparent', color: 'var(--accent-danger)', cursor: deleting === h.id ? 'not-allowed' : 'pointer', opacity: deleting === h.id ? 0.4 : 1 }}
                >
                  <Trash2 size={12} />
                </button>
              </div>
            ))}
          </div>

          {/* Active host content */}
          {activeHost && <HostPanel key={activeHost.id} host={activeHost} />}
        </>
      )}

      {showAddModal && (
        <AddHostModal
          onClose={() => setShowAddModal(false)}
          onAdded={fetchHosts}
        />
      )}
    </div>
  )
}
