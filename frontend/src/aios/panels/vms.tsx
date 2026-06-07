import { useEffect, useState } from 'react'
import { Play, Square, RotateCcw, Pause, Power } from 'lucide-react'
import NativePanelShell, { NativeRow, StatusDot, IconBtn, EmptyState, LoadingState } from './NativePanelShell'

interface VM {
  id: string; name: string; state: string; source: 'local' | 'proxmox'
  vcpus?: number; memory_mb?: number
  cpu_usage?: number; mem_bytes?: number; maxmem_bytes?: number
  kind?: 'vm' | 'lxc'
  node?: string
}

const TABS = [{ id: 'all', label: 'All' }, { id: 'local', label: 'Local' }, { id: 'proxmox', label: 'Proxmox' }]

function vmColor(s: string) {
  return s === 'running' ? '#22c55e' : s === 'paused' ? '#f59e0b' : '#94a3b8'
}

function fmtMem(b: number) {
  const gb = b / 1e9
  return gb >= 1 ? `${gb.toFixed(1)}G` : `${(b / 1e6).toFixed(0)}M`
}

export default function NativeVMsPanel() {
  const [vms, setVms] = useState<VM[]>([])
  const [loading, setLoading] = useState(true)
  const [tab, setTab] = useState('all')

  async function load() {
    setLoading(true)
    const [lr, pr] = await Promise.all([
      fetch('/api/vms/local', { credentials: 'include' }),
      fetch('/api/vms/proxmox/vms', { credentials: 'include' }),
    ])
    const local: VM[] = lr.ok ? ((await lr.json()).vms ?? []).map((v: VM) => ({ ...v, source: 'local' as const })) : []
    const proxmox: VM[] = pr.ok ? ((await pr.json()).vms ?? []).map((v: VM) => ({ ...v, source: 'proxmox' as const })) : []
    setVms([...local, ...proxmox])
    setLoading(false)
  }

  async function actLocal(id: string, action: string) {
    await fetch('/api/vms/local/action', {
      method: 'POST', credentials: 'include',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ id, action }),
    })
    load()
  }

  async function actProxmox(vm: VM, action: string) {
    await fetch('/api/vms/proxmox/action', {
      method: 'POST', credentials: 'include',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ vmid: vm.id, node: vm.node, action, kind: vm.kind ?? 'vm' }),
    })
    load()
  }

  function act(vm: VM, action: string) {
    if (vm.source === 'proxmox') actProxmox(vm, action)
    else actLocal(vm.id, action)
  }

  useEffect(() => { load() }, [])

  const filtered = tab === 'all' ? vms : vms.filter(v => v.source === tab)

  return (
    <NativePanelShell tabs={TABS} activeTab={tab} onTabChange={setTab}>
      {loading ? <LoadingState /> : filtered.length === 0 ? <EmptyState text="No VMs" /> :
        filtered.map(v => {
          const st = v.state.toLowerCase()
          const running = st === 'running'
          const stopped = st === 'stopped' || st === 'shut off' || st === 'crashed'
          const paused  = st === 'paused' || st === 'suspended'
          return (
            <NativeRow key={`${v.source}-${v.id}`}>
              <StatusDot color={vmColor(v.state)} />
              <div style={{ flex: 1, minWidth: 0 }}>
                <div style={{ fontSize: 11, color: 'var(--text-primary)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', display: 'flex', alignItems: 'center', gap: 4 }}>
                  {v.name}
                  {v.kind && (
                    <span style={{ fontSize: 9, color: v.kind === 'lxc' ? '#f59e0b' : '#3b82f6', fontWeight: 600 }}>{v.kind.toUpperCase()}</span>
                  )}
                </div>
                <div style={{ fontSize: 9, color: 'var(--text-muted)' }}>
                  {v.state} · {v.source}{v.vcpus ? ` · ${v.vcpus}c` : ''}{v.memory_mb ? `/${Math.round(v.memory_mb / 1024)}G` : ''}
                </div>
                {running && (v.cpu_usage !== undefined || v.mem_bytes !== undefined) && (
                  <div style={{ fontSize: 9, color: 'var(--text-muted)' }}>
                    {v.cpu_usage !== undefined && `CPU ${v.cpu_usage.toFixed(0)}%`}
                    {v.cpu_usage !== undefined && v.mem_bytes !== undefined && ' · '}
                    {v.mem_bytes !== undefined && v.maxmem_bytes !== undefined && `${fmtMem(v.mem_bytes)}/${fmtMem(v.maxmem_bytes)}`}
                  </div>
                )}
              </div>
              {stopped && <IconBtn title="Start" onClick={() => act(v, 'start')}><Play size={11} /></IconBtn>}
              {running && <>
                <IconBtn title="Stop"     onClick={() => act(v, 'stop')}><Square size={11} /></IconBtn>
                <IconBtn title="Pause"    onClick={() => act(v, 'pause')}><Pause size={11} /></IconBtn>
                {v.source === 'local' && <IconBtn title="Shutdown" onClick={() => act(v, 'shutdown')}><Power size={11} /></IconBtn>}
                <IconBtn title="Restart"  onClick={() => act(v, 'restart')}><RotateCcw size={11} /></IconBtn>
              </>}
              {paused && <IconBtn title="Resume" onClick={() => act(v, 'resume')}><Play size={11} /></IconBtn>}
            </NativeRow>
          )
        })
      }
    </NativePanelShell>
  )
}
