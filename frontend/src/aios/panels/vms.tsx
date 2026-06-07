import { useEffect, useState } from 'react'
import { Play, Square, RotateCcw } from 'lucide-react'
import NativePanelShell, { NativeRow, StatusDot, IconBtn, EmptyState, LoadingState } from './NativePanelShell'

interface VM { id: string; name: string; state: string; source: 'local' | 'proxmox'; vcpus?: number; memory_mb?: number }

const TABS = [{ id: 'all', label: 'All' }, { id: 'local', label: 'Local' }, { id: 'proxmox', label: 'Proxmox' }]

function vmColor(s: string) {
  return s === 'running' ? '#22c55e' : s === 'paused' ? '#f59e0b' : '#94a3b8'
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

  async function act(id: string, action: string) {
    await fetch('/api/vms/local/action', {
      method: 'POST', credentials: 'include',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ id, action }),
    })
    load()
  }

  useEffect(() => { load() }, [])

  const filtered = tab === 'all' ? vms : vms.filter(v => v.source === tab)

  return (
    <NativePanelShell tabs={TABS} activeTab={tab} onTabChange={setTab}>
      {loading ? <LoadingState /> : filtered.length === 0 ? <EmptyState text="No VMs" /> :
        filtered.map(v => (
          <NativeRow key={`${v.source}-${v.id}`}>
            <StatusDot color={vmColor(v.state)} />
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ fontSize: 11, color: 'var(--text-primary)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{v.name}</div>
              <div style={{ fontSize: 9, color: 'var(--text-muted)' }}>{v.state} · {v.source}{v.vcpus ? ` · ${v.vcpus}c` : ''}{v.memory_mb ? `/${Math.round(v.memory_mb / 1024)}G` : ''}</div>
            </div>
            {v.source === 'local' && <>
              <IconBtn title="Start"   onClick={() => act(v.id, 'start')}><Play size={11} /></IconBtn>
              <IconBtn title="Stop"    onClick={() => act(v.id, 'stop')}><Square size={11} /></IconBtn>
              <IconBtn title="Restart" onClick={() => act(v.id, 'restart')}><RotateCcw size={11} /></IconBtn>
            </>}
          </NativeRow>
        ))
      }
    </NativePanelShell>
  )
}
