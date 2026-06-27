import { useEffect, useState, useCallback, useRef } from 'react'
import { Play, Square, RotateCcw, Server, Database, ClipboardList, Camera, Monitor, RotateCw, Trash2 } from 'lucide-react'
import NativePanelShell, { NativeRow, StatusDot, IconBtn, EmptyState, LoadingState } from './NativePanelShell'
import { ConsoleModal } from '@/pages/ProxmoxPage'
import ChangePlanModal, { type ChangePlan } from '@/components/ui/ChangePlanModal'
import { api, ApiClientError } from '@/api/client'
import { notify } from '@/store/notifications'
import type { ProxmoxHost, PveVm, PveStorage, PveTask, PveSnapshot } from '@/api/types'

type Tab = 'vms' | 'storage' | 'tasks' | 'snapshots'

function vmColor(s: string) {
  return s === 'running' ? '#22c55e' : s === 'stopped' ? '#94a3b8' : '#f59e0b'
}

function taskColor(s: string) {
  if (!s || s === 'OK') return '#22c55e'
  if (s.startsWith('WARNINGS')) return '#f59e0b'
  if (s.startsWith('ERROR')) return '#ef4444'
  return '#f59e0b'
}

function fmtBytes(b: number) {
  if (b === 0) return '0 B'
  const u = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.floor(Math.log2(b) / 10)
  return `${(b / Math.pow(1024, i)).toFixed(1)} ${u[i]}`
}

function relTime(ts: number) {
  const s = Math.floor(Date.now() / 1000) - ts
  if (s < 60) return `${s}s ago`
  if (s < 3600) return `${Math.floor(s / 60)}m ago`
  if (s < 86400) return `${Math.floor(s / 3600)}h ago`
  return `${Math.floor(s / 86400)}d ago`
}

export default function NativeProxmoxPanel() {
  const [hosts, setHosts] = useState<ProxmoxHost[]>([])
  const [activeHost, setActiveHost] = useState<string>('')
  const [tab, setTab] = useState<Tab>('vms')
  const [vms, setVms] = useState<PveVm[]>([])
  const [storage, setStorage] = useState<PveStorage[]>([])
  const [tasks, setTasks] = useState<PveTask[]>([])
  const [loading, setLoading] = useState(true)
  const [acting, setActing] = useState<string | null>(null)
  const [consoleVm, setConsoleVm] = useState<PveVm | null>(null)
  const [snapshots, setSnapshots] = useState<Record<number, PveSnapshot[]>>({})
  const [snapLoading, setSnapLoading] = useState(false)
  const [snapPlan, setSnapPlan] = useState<{ vmid: number; action: 'rollback' | 'delete'; snapname: string; plan: ChangePlan } | null>(null)
  const [snapConfirming, setSnapConfirming] = useState(false)

  useEffect(() => {
    fetch('/api/proxmox/hosts', { credentials: 'include' })
      .then(r => r.ok ? r.json() : { hosts: [] })
      .then(d => {
        const h: ProxmoxHost[] = d.hosts ?? []
        setHosts(h)
        if (h.length > 0) setActiveHost(h[0].id)
      })
  }, [])

  const hasLoadedRef = useRef(false)

  const fetchAll = useCallback(async () => {
    if (!activeHost) return
    if (!hasLoadedRef.current) setLoading(true)
    try {
      const [rv, rs, rt] = await Promise.all([
        fetch(`/api/proxmox/${activeHost}/vms`, { credentials: 'include' }),
        fetch(`/api/proxmox/${activeHost}/storage`, { credentials: 'include' }),
        fetch(`/api/proxmox/${activeHost}/tasks`, { credentials: 'include' }),
      ])
      if (rv.ok) setVms(await rv.json())
      if (rs.ok) setStorage(await rs.json())
      if (rt.ok) setTasks(await rt.json())
      hasLoadedRef.current = true
    } finally {
      setLoading(false)
    }
  }, [activeHost])

  useEffect(() => {
    if (activeHost) {
      fetchAll()
      const t = setInterval(fetchAll, 10_000)
      return () => clearInterval(t)
    }
  }, [activeHost, fetchAll])

  async function vmAction(vmid: number, action: 'start' | 'stop' | 'reboot') {
    const key = `${vmid}-${action}`
    setActing(key)
    try {
      await fetch(`/api/proxmox/${activeHost}/vms/${vmid}/${action}`, {
        method: 'POST', credentials: 'include',
      })
      await fetchAll()
    } finally {
      setActing(null)
    }
  }

  const vmsRef = useRef<PveVm[]>([])
  vmsRef.current = vms

  const fetchSnapshots = useCallback(async () => {
    if (!activeHost || vmsRef.current.length === 0) return
    setSnapLoading(true)
    try {
      const entries = await Promise.all(vmsRef.current.map(async vm => {
        try {
          const list = await api.proxmox.getSnapshots(activeHost, vm.vmid, (vm.type ?? 'qemu') as 'qemu' | 'lxc')
          return [vm.vmid, list.filter(s => s.name !== 'current')] as const
        } catch { return [vm.vmid, []] as const }
      }))
      setSnapshots(Object.fromEntries(entries))
    } finally {
      setSnapLoading(false)
    }
  }, [activeHost])

  useEffect(() => {
    if (tab === 'snapshots') fetchSnapshots()
  }, [tab, fetchSnapshots])

  async function requestRollback(vmid: number, snapname: string) {
    try {
      const res = await api.proxmox.rollbackSnapshotPlan(activeHost, vmid, snapname)
      setSnapPlan({ vmid, action: 'rollback', snapname, plan: res.plan })
    } catch (err) { notify.error(err instanceof ApiClientError ? err.message : 'Failed to fetch plan') }
  }

  async function requestDelete(vmid: number, snapname: string) {
    try {
      const res = await api.proxmox.deleteSnapshotPlan(activeHost, vmid, snapname)
      setSnapPlan({ vmid, action: 'delete', snapname, plan: res.plan })
    } catch (err) { notify.error(err instanceof ApiClientError ? err.message : 'Failed to fetch plan') }
  }

  async function confirmSnapPlan() {
    if (!snapPlan) return
    setSnapConfirming(true)
    try {
      if (snapPlan.action === 'rollback') {
        await api.proxmox.rollbackSnapshot(activeHost, snapPlan.vmid, snapPlan.snapname)
        notify.success(`Rollback to "${snapPlan.snapname}" queued`)
      } else {
        await api.proxmox.deleteSnapshot(activeHost, snapPlan.vmid, snapPlan.snapname)
        notify.success(`Snapshot "${snapPlan.snapname}" deleted`)
      }
      setSnapPlan(null)
      await fetchSnapshots()
    } catch (err) {
      notify.error(err instanceof ApiClientError ? err.message : `${snapPlan.action === 'rollback' ? 'Rollback' : 'Delete'} failed`)
    } finally { setSnapConfirming(false) }
  }

  if (hosts.length === 0) {
    return (
      <NativePanelShell>
        <EmptyState text="No Proxmox hosts configured" />
      </NativePanelShell>
    )
  }

  const tabStyle = (t: Tab): React.CSSProperties => ({
    fontSize: 10,
    padding: '3px 8px',
    borderRadius: 4,
    cursor: 'pointer',
    background: tab === t ? 'var(--accent-primary)' : 'transparent',
    color: tab === t ? '#fff' : 'var(--text-muted)',
    border: 'none',
  })

  return (
    <NativePanelShell>
      {/* host selector */}
      {hosts.length > 1 && (
        <div style={{ padding: '4px 8px', borderBottom: '1px solid var(--border-subtle)' }}>
          <select
            value={activeHost}
            onChange={e => setActiveHost(e.target.value)}
            style={{ fontSize: 10, background: 'var(--bg-surface)', color: 'var(--text-primary)', border: '1px solid var(--border-subtle)', borderRadius: 4, padding: '2px 4px', width: '100%' }}
          >
            {hosts.map(h => <option key={h.id} value={h.id}>{h.name}</option>)}
          </select>
        </div>
      )}

      {/* tabs */}
      <div style={{ display: 'flex', gap: 2, padding: '4px 8px', borderBottom: '1px solid var(--border-subtle)' }}>
        <button style={tabStyle('vms')}    onClick={() => setTab('vms')}>    <Server size={9} style={{ display: 'inline', marginRight: 3 }} />VMs</button>
        <button style={tabStyle('storage')} onClick={() => setTab('storage')}><Database size={9} style={{ display: 'inline', marginRight: 3 }} />Storage</button>
        <button style={tabStyle('tasks')}  onClick={() => setTab('tasks')}>  <ClipboardList size={9} style={{ display: 'inline', marginRight: 3 }} />Tasks</button>
        <button style={tabStyle('snapshots')} onClick={() => setTab('snapshots')}><Camera size={9} style={{ display: 'inline', marginRight: 3 }} />Snapshots</button>
      </div>

      {loading ? <LoadingState /> : (
        <>
          {tab === 'vms' && (
            vms.length === 0 ? <EmptyState text="No VMs or containers" /> :
            vms.map(vm => (
              <NativeRow key={vm.vmid}>
                <StatusDot color={vmColor(vm.status)} />
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div style={{ fontSize: 11, color: 'var(--text-primary)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                    {vm.name} <span style={{ color: 'var(--text-muted)', fontSize: 9 }}>({vm.vmid})</span>
                  </div>
                  <div style={{ fontSize: 9, color: 'var(--text-muted)' }}>
                    {vm.type.toUpperCase()} · {(vm.cpu * 100).toFixed(0)}% CPU · {fmtBytes(vm.mem)}/{fmtBytes(vm.maxmem)} RAM
                  </div>
                </div>
                <IconBtn title="Start"  onClick={() => { if (!acting) vmAction(vm.vmid, 'start') }}><Play size={11} /></IconBtn>
                <IconBtn title="Stop"   onClick={() => { if (!acting) vmAction(vm.vmid, 'stop') }}><Square size={11} /></IconBtn>
                <IconBtn title="Reboot" onClick={() => { if (!acting) vmAction(vm.vmid, 'reboot') }}><RotateCcw size={11} /></IconBtn>
                {vm.status === 'running' && (
                  <IconBtn title="Console" onClick={() => setConsoleVm(vm)}><Monitor size={11} /></IconBtn>
                )}
              </NativeRow>
            ))
          )}

          {tab === 'storage' && (
            storage.length === 0 ? <EmptyState text="No storage pools" /> :
            storage.map(s => {
              const pct = s.total > 0 ? (s.used / s.total) * 100 : 0
              return (
                <NativeRow key={s.storage}>
                  <StatusDot color={s.active ? '#22c55e' : '#94a3b8'} />
                  <div style={{ flex: 1, minWidth: 0 }}>
                    <div style={{ fontSize: 11, color: 'var(--text-primary)' }}>{s.storage} <span style={{ fontSize: 9, color: 'var(--text-muted)' }}>{s.type}</span></div>
                    <div style={{ display: 'flex', alignItems: 'center', gap: 4, marginTop: 2 }}>
                      <div style={{ flex: 1, height: 3, background: 'var(--bg-surface)', borderRadius: 2 }}>
                        <div style={{ width: `${pct.toFixed(0)}%`, height: '100%', borderRadius: 2, background: pct > 85 ? '#ef4444' : 'var(--accent-primary)' }} />
                      </div>
                      <span style={{ fontSize: 9, color: 'var(--text-muted)', whiteSpace: 'nowrap' }}>{fmtBytes(s.avail)} free</span>
                    </div>
                  </div>
                </NativeRow>
              )
            })
          )}

          {tab === 'tasks' && (
            tasks.length === 0 ? <EmptyState text="No recent tasks" /> :
            tasks.slice(0, 20).map(t => (
              <NativeRow key={t.upid}>
                <StatusDot color={taskColor(t.status)} />
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div style={{ fontSize: 11, color: 'var(--text-primary)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                    {t.type}{t.id ? ` ${t.id}` : ''}
                  </div>
                  <div style={{ fontSize: 9, color: 'var(--text-muted)' }}>{t.node} · {relTime(t.starttime)}</div>
                </div>
                <span style={{ fontSize: 9, color: 'var(--text-muted)', whiteSpace: 'nowrap' }}>{t.status || 'running'}</span>
              </NativeRow>
            ))
          )}

          {tab === 'snapshots' && (
            snapLoading ? <LoadingState /> :
            Object.values(snapshots).every(s => s.length === 0) ? <EmptyState text="No snapshots" /> :
            vms.filter(vm => (snapshots[vm.vmid] ?? []).length > 0).map(vm => (
              <div key={vm.vmid}>
                <div style={{ fontSize: 9, color: 'var(--text-muted)', padding: '4px 8px 0', textTransform: 'uppercase', letterSpacing: '0.04em' }}>
                  {vm.name ?? vm.vmid}
                </div>
                {(snapshots[vm.vmid] ?? []).map(s => (
                  <NativeRow key={`${vm.vmid}-${s.name}`}>
                    <Camera size={11} style={{ color: 'var(--text-muted)', flexShrink: 0 }} />
                    <div style={{ flex: 1, minWidth: 0 }}>
                      <div style={{ fontSize: 11, color: 'var(--text-primary)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{s.name}</div>
                      {s.description && <div style={{ fontSize: 9, color: 'var(--text-muted)' }}>{s.description}</div>}
                    </div>
                    <IconBtn title="Rollback" onClick={() => requestRollback(vm.vmid, s.name)}><RotateCw size={11} /></IconBtn>
                    <IconBtn title="Delete" onClick={() => requestDelete(vm.vmid, s.name)}><Trash2 size={11} /></IconBtn>
                  </NativeRow>
                ))}
              </div>
            ))
          )}
        </>
      )}

      {consoleVm && (
        <ConsoleModal hostId={activeHost} vm={consoleVm} onClose={() => setConsoleVm(null)} />
      )}
      {snapPlan && (
        <ChangePlanModal plan={snapPlan.plan} confirming={snapConfirming} onConfirm={confirmSnapPlan} onCancel={() => setSnapPlan(null)} />
      )}
    </NativePanelShell>
  )
}
