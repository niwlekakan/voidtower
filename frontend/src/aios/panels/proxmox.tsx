import { useEffect, useState, useCallback } from 'react'
import { Play, Square, RotateCcw, Server, Database, ClipboardList } from 'lucide-react'
import NativePanelShell, { NativeRow, StatusDot, IconBtn, EmptyState, LoadingState } from './NativePanelShell'
import type { ProxmoxHost, PveVm, PveStorage, PveTask } from '@/api/types'

type Tab = 'vms' | 'storage' | 'tasks'

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

  useEffect(() => {
    fetch('/api/proxmox/hosts', { credentials: 'include' })
      .then(r => r.ok ? r.json() : { hosts: [] })
      .then(d => {
        const h: ProxmoxHost[] = d.hosts ?? []
        setHosts(h)
        if (h.length > 0) setActiveHost(h[0].id)
      })
  }, [])

  const fetchAll = useCallback(async () => {
    if (!activeHost) return
    setLoading(true)
    try {
      const [rv, rs, rt] = await Promise.all([
        fetch(`/api/proxmox/${activeHost}/vms`, { credentials: 'include' }),
        fetch(`/api/proxmox/${activeHost}/storage`, { credentials: 'include' }),
        fetch(`/api/proxmox/${activeHost}/tasks`, { credentials: 'include' }),
      ])
      if (rv.ok) setVms(await rv.json())
      if (rs.ok) setStorage(await rs.json())
      if (rt.ok) setTasks(await rt.json())
    } finally {
      setLoading(false)
    }
  }, [activeHost])

  useEffect(() => {
    if (activeHost) {
      fetchAll()
      const t = setInterval(fetchAll, 15_000)
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
        </>
      )}
    </NativePanelShell>
  )
}
