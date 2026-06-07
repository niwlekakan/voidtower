import { useState, useEffect, useCallback } from 'react'
import { Tag as TagIcon } from 'lucide-react'
import { api } from '@/api/client'
import type { LocalVm, ProxmoxVm, ProxmoxConfig, Tag, TagMap } from '@/api/types'
import { notify } from '@/store/notifications'
import { useFiltersStore } from '@/store/filters'
import { TagPill, TagPopover } from '@/components/ui/TagPill'
import Button from '@/components/ui/Button'

// ── helpers ───────────────────────────────────────────────────────────────────

function stateColor(state: string) {
  switch (state.toLowerCase()) {
    case 'running': return 'var(--accent-success)'
    case 'stopped':
    case 'shut off': return 'var(--accent-danger)'
    case 'paused':
    case 'suspended': return 'var(--accent-warning)'
    default: return 'var(--text-muted)'
  }
}

function StateDot({ state }: { state: string }) {
  const color = stateColor(state)
  return (
    <span style={{ color, display: 'inline-flex', alignItems: 'center', gap: 5, fontSize: 12 }}>
      <span style={{ width: 7, height: 7, borderRadius: '50%', background: color, display: 'inline-block' }} />
      {state}
    </span>
  )
}

function fmtMem(bytes: number) {
  const gb = bytes / 1e9
  if (gb >= 1) return `${gb.toFixed(1)} GB`
  return `${(bytes / 1e6).toFixed(0)} MB`
}

function fmtUptime(secs: number) {
  if (secs === 0) return '—'
  const d = Math.floor(secs / 86400)
  const h = Math.floor((secs % 86400) / 3600)
  const m = Math.floor((secs % 3600) / 60)
  if (d > 0) return `${d}d ${h}h`
  if (h > 0) return `${h}h ${m}m`
  return `${m}m`
}

// ── tab styles ────────────────────────────────────────────────────────────────

function Tab({ label, active, onClick }: { label: string; active: boolean; onClick: () => void }) {
  return (
    <button
      onClick={onClick}
      style={{
        padding: '6px 18px', borderRadius: 6, border: 'none', cursor: 'pointer', fontSize: 13, fontWeight: 500,
        background: active ? 'var(--accent-primary)' : 'transparent',
        color: active ? '#fff' : 'var(--text-secondary)',
        transition: 'background 0.15s',
      }}
    >
      {label}
    </button>
  )
}

// ── local KVM section ─────────────────────────────────────────────────────────

const LOCAL_ACTIONS: { label: string; action: string; states: string[] }[] = [
  { label: 'Start',    action: 'start',    states: ['shut off', 'stopped', 'crashed'] },
  { label: 'Shutdown', action: 'shutdown', states: ['running'] },
  { label: 'Reboot',   action: 'reboot',   states: ['running'] },
  { label: 'Suspend',  action: 'suspend',  states: ['running'] },
  { label: 'Resume',   action: 'resume',   states: ['paused', 'pmsuspended'] },
  { label: 'Force off',action: 'destroy',  states: ['running', 'paused'] },
]

function LocalVmsSection({ allTags, tagMap, globalTag, onTagsChanged }: {
  allTags: Tag[]; tagMap: TagMap; globalTag: string | null; onTagsChanged: () => void
}) {
  const [vms, setVms] = useState<LocalVm[]>([])
  const [available, setAvailable] = useState(true)
  const [loading, setLoading] = useState(true)
  const [busy, setBusy] = useState<string | null>(null)
  const [popover, setPopover] = useState<string | null>(null)

  const load = useCallback(async () => {
    try {
      const res = await api.vms.listLocal()
      setVms(res.vms)
      setAvailable(res.libvirt_available)
    } catch { notify.error('Failed to load local VMs') }
    finally { setLoading(false) }
  }, [])

  useEffect(() => { load(); const t = setInterval(load, 10_000); return () => clearInterval(t) }, [load])

  const doAction = async (vm: LocalVm, action: string) => {
    setBusy(`${vm.name}-${action}`)
    try {
      const res = await api.vms.localAction(vm.name, action)
      if (res.ok) { notify.success(`${action}: ${vm.name}`); load() }
      else notify.error(res.message || 'Action failed')
    } catch { notify.error('Action failed') }
    finally { setBusy(null) }
  }

  if (loading) return <p style={{ color: 'var(--text-muted)', fontSize: 13 }}>Loading…</p>

  if (!available) return (
    <div style={{ padding: '16px 0', color: 'var(--text-muted)', fontSize: 13 }}>
      <p>libvirt / virsh not found on this host.</p>
      <p style={{ marginTop: 4 }}>Install with: <code style={{ background: 'var(--bg-code)', padding: '1px 6px', borderRadius: 4 }}>sudo pacman -S libvirt virt-manager</code></p>
    </div>
  )

  const displayed = globalTag ? vms.filter(vm => (tagMap[vm.name] || []).some(t => t.id === globalTag)) : vms

  if (displayed.length === 0) return (
    <div style={{ color: 'var(--text-muted)', fontSize: 13, padding: '16px 0' }}>
      {globalTag ? 'No local VMs with this tag.' : 'No VMs found. Create one with virt-manager or virt-install.'}
    </div>
  )

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
      {displayed.map(vm => (
        <div key={vm.name} style={{
          background: 'var(--bg-card)', border: '1px solid var(--border-subtle)', borderRadius: 8,
          padding: '12px 16px', display: 'flex', alignItems: 'center', gap: 12, flexWrap: 'wrap',
        }}>
          <div style={{ flex: '1 1 160px', minWidth: 0 }}>
            <div style={{ fontWeight: 600, fontSize: 14, color: 'var(--text-primary)', marginBottom: 2 }}>{vm.name}</div>
            <StateDot state={vm.state} />
            {vm.id !== null && (
              <span style={{ marginLeft: 8, fontSize: 11, color: 'var(--text-muted)' }}>ID {vm.id}</span>
            )}
            <div style={{ display: 'flex', gap: 4, flexWrap: 'wrap', marginTop: 4, alignItems: 'center', position: 'relative' }}>
              {(tagMap[vm.name] || []).map(t => <TagPill key={t.id} tag={t} />)}
              <button onClick={() => setPopover(popover === vm.name ? null : vm.name)} style={{
                background: 'none', border: '1px dashed var(--border-subtle)', borderRadius: 10,
                cursor: 'pointer', color: 'var(--text-muted)', fontSize: 11, padding: '0px 6px', lineHeight: '18px',
              }}><TagIcon size={10} style={{ display: 'inline', verticalAlign: 'middle' }} /></button>
              {popover === vm.name && (
                <TagPopover resourceType="vm" resourceId={vm.name} allTags={allTags} assigned={tagMap[vm.name] || []} onClose={() => { setPopover(null); onTagsChanged() }} />
              )}
            </div>
          </div>
          <div style={{ display: 'flex', gap: 6, flexWrap: 'wrap' }}>
            {LOCAL_ACTIONS.filter(a => a.states.includes(vm.state.toLowerCase())).map(a => (
              <Button
                key={a.action}
                size="sm"
                variant={a.action === 'destroy' ? 'danger' : 'secondary'}
                loading={busy === `${vm.name}-${a.action}`}
                onClick={() => doAction(vm, a.action)}
              >
                {a.label}
              </Button>
            ))}
          </div>
        </div>
      ))}
    </div>
  )
}

// ── Proxmox config panel ──────────────────────────────────────────────────────

function ProxmoxConfigPanel({ onSaved }: { onSaved: () => void }) {
  const [cfg, setCfg] = useState<ProxmoxConfig>({ host: '', port: 8006, token: '', node: 'pve', verify_ssl: false })
  const [saving, setSaving] = useState(false)
  const [testing, setTesting] = useState(false)
  const [testResult, setTestResult] = useState<{ ok: boolean; message: string } | null>(null)

  useEffect(() => {
    api.vms.getProxmoxConfig().then(c => { if (c) setCfg(c) }).catch(() => {})
  }, [])

  const save = async () => {
    setSaving(true)
    try {
      await api.vms.setProxmoxConfig(cfg)
      notify.success('Proxmox config saved')
      onSaved()
    } catch { notify.error('Failed to save config') }
    finally { setSaving(false) }
  }

  const test = async () => {
    setTesting(true)
    setTestResult(null)
    try {
      const res = await api.vms.testProxmox()
      if (res.ok) {
        setTestResult({ ok: true, message: `Connected — nodes: ${res.nodes?.join(', ') || '?'}` })
      } else {
        setTestResult({ ok: false, message: res.message || 'Connection failed' })
      }
    } catch (e: unknown) {
      setTestResult({ ok: false, message: String(e) })
    } finally { setTesting(false) }
  }

  const field = (label: string, node: React.ReactNode) => (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
      <label style={{ fontSize: 12, color: 'var(--text-muted)', fontWeight: 500 }}>{label}</label>
      {node}
    </div>
  )

  const input = (value: string | number, onChange: (v: string) => void, placeholder = '', type = 'text') => (
    <input
      type={type}
      value={value}
      placeholder={placeholder}
      onChange={e => onChange(e.target.value)}
      style={{
        background: 'var(--bg-input)', border: '1px solid var(--border-subtle)', borderRadius: 6,
        color: 'var(--text-primary)', padding: '6px 10px', fontSize: 13, outline: 'none',
      }}
    />
  )

  return (
    <div style={{ background: 'var(--bg-card)', border: '1px solid var(--border-subtle)', borderRadius: 8, padding: 16, marginBottom: 16 }}>
      <h3 style={{ fontSize: 14, fontWeight: 600, color: 'var(--text-primary)', marginBottom: 12 }}>Proxmox Connection</h3>
      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(200px, 1fr))', gap: 10, marginBottom: 12 }}>
        {field('Host / IP', input(cfg.host, v => setCfg(p => ({ ...p, host: v })), '192.168.1.100'))}
        {field('Port', input(cfg.port, v => setCfg(p => ({ ...p, port: Number(v) })), '8006', 'number'))}
        {field('Node', input(cfg.node, v => setCfg(p => ({ ...p, node: v })), 'pve — or "all" for all nodes'))}
        {field('API Token', input(cfg.token, v => setCfg(p => ({ ...p, token: v })), 'user@pam!tokenid=uuid', 'password'))}
      </div>
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 12 }}>
        <input type="checkbox" id="px-ssl" checked={cfg.verify_ssl} onChange={e => setCfg(p => ({ ...p, verify_ssl: e.target.checked }))} />
        <label htmlFor="px-ssl" style={{ fontSize: 13, color: 'var(--text-secondary)', cursor: 'pointer' }}>Verify SSL certificate</label>
      </div>
      <div style={{ display: 'flex', gap: 8 }}>
        <Button size="sm" onClick={save} loading={saving}>Save</Button>
        <Button size="sm" variant="secondary" onClick={test} loading={testing}>Test connection</Button>
      </div>
      {testResult && (
        <div style={{
          marginTop: 10, padding: '8px 12px', borderRadius: 6, fontSize: 13,
          background: testResult.ok ? 'var(--accent-success-subtle)' : 'var(--accent-danger-subtle)',
          color: testResult.ok ? 'var(--accent-success)' : 'var(--accent-danger)',
          border: `1px solid ${testResult.ok ? 'var(--accent-success)' : 'var(--accent-danger)'}`,
        }}>
          {testResult.message}
        </div>
      )}
      <p style={{ marginTop: 10, fontSize: 12, color: 'var(--text-muted)' }}>
        Create a token in Proxmox: Datacenter → API Tokens → Add. Grant it <code style={{ background: 'var(--bg-code)', padding: '0 4px', borderRadius: 3 }}>PVEVMUser</code> or higher.
      </p>
    </div>
  )
}

// ── Proxmox VMs section ───────────────────────────────────────────────────────

const PX_ACTIONS: { label: string; action: string; statuses: string[] }[] = [
  { label: 'Start',    action: 'start',    statuses: ['stopped'] },
  { label: 'Shutdown', action: 'shutdown', statuses: ['running'] },
  { label: 'Reboot',   action: 'reboot',   statuses: ['running'] },
  { label: 'Suspend',  action: 'suspend',  statuses: ['running'] },
  { label: 'Resume',   action: 'resume',   statuses: ['paused', 'suspended'] },
  { label: 'Stop',     action: 'stop',     statuses: ['running', 'paused'] },
]

function ProxmoxVmsSection({ reload, allTags, tagMap, globalTag, onTagsChanged }: {
  reload: number; allTags: Tag[]; tagMap: TagMap; globalTag: string | null; onTagsChanged: () => void
}) {
  const [vms, setVms] = useState<ProxmoxVm[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState<string | null>(null)
  const [configured, setConfigured] = useState(true)
  const [popover, setPopover] = useState<string | null>(null)

  const load = useCallback(async () => {
    try {
      const res = await api.vms.listProxmox()
      setVms(res.vms)
      setError(null)
    } catch (e: unknown) {
      const msg = String(e)
      if (msg.includes('not configured') || msg.includes('400')) {
        setConfigured(false)
      } else {
        setError(msg)
      }
    } finally { setLoading(false) }
  }, [])

  useEffect(() => { setLoading(true); load(); const t = setInterval(load, 15_000); return () => clearInterval(t) }, [load, reload])

  const doAction = async (vm: ProxmoxVm, action: string) => {
    const key = `${vm.vmid}-${action}`
    setBusy(key)
    try {
      const res = await api.vms.proxmoxAction(vm.vmid, vm.kind, vm.node, action)
      if (res.ok) { notify.success(`${action}: ${vm.name}`); setTimeout(load, 2000) }
      else notify.error(res.message || 'Action failed')
    } catch { notify.error('Action failed') }
    finally { setBusy(null) }
  }

  if (!configured) return (
    <p style={{ color: 'var(--text-muted)', fontSize: 13 }}>Configure Proxmox connection above to see VMs and containers.</p>
  )
  if (loading) return <p style={{ color: 'var(--text-muted)', fontSize: 13 }}>Loading…</p>
  if (error) return <p style={{ color: 'var(--accent-danger)', fontSize: 13 }}>{error}</p>

  const displayed = globalTag ? vms.filter(vm => (tagMap[String(vm.vmid)] || []).some(t => t.id === globalTag)) : vms

  if (displayed.length === 0) return <p style={{ color: 'var(--text-muted)', fontSize: 13 }}>{globalTag ? 'No Proxmox VMs with this tag.' : 'No VMs or containers found on the configured node.'}</p>

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
      {displayed.map(vm => {
        const vmKey = String(vm.vmid)
        const popKey = `${vm.node}-${vm.vmid}`
        return (
          <div key={popKey} style={{
            background: 'var(--bg-card)', border: '1px solid var(--border-subtle)', borderRadius: 8,
            padding: '12px 16px', display: 'flex', alignItems: 'center', gap: 12, flexWrap: 'wrap',
          }}>
            <div style={{ flex: '1 1 200px', minWidth: 0 }}>
              <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 2 }}>
                <span style={{ fontWeight: 600, fontSize: 14, color: 'var(--text-primary)' }}>{vm.name || `VM ${vm.vmid}`}</span>
                <span style={{
                  fontSize: 10, padding: '1px 6px', borderRadius: 4, fontWeight: 600, letterSpacing: '0.05em',
                  background: vm.kind === 'lxc' ? 'var(--accent-primary-subtle)' : 'var(--bg-code)',
                  color: vm.kind === 'lxc' ? 'var(--accent-primary)' : 'var(--text-muted)',
                  textTransform: 'uppercase',
                }}>
                  {vm.kind === 'lxc' ? 'LXC' : 'VM'}
                </span>
              </div>
              <div style={{ display: 'flex', gap: 12, flexWrap: 'wrap', alignItems: 'center' }}>
                <StateDot state={vm.status} />
                <span style={{ fontSize: 11, color: 'var(--text-muted)' }}>ID {vm.vmid} · {vm.node}</span>
                {vm.status === 'running' && (
                  <>
                    <span style={{ fontSize: 11, color: 'var(--text-muted)' }}>CPU {(vm.cpu * 100).toFixed(1)}%</span>
                    <span style={{ fontSize: 11, color: 'var(--text-muted)' }}>{fmtMem(vm.mem)} / {fmtMem(vm.maxmem)}</span>
                    <span style={{ fontSize: 11, color: 'var(--text-muted)' }}>up {fmtUptime(vm.uptime)}</span>
                  </>
                )}
              </div>
              <div style={{ display: 'flex', gap: 4, flexWrap: 'wrap', marginTop: 4, alignItems: 'center', position: 'relative' }}>
                {(tagMap[vmKey] || []).map(t => <TagPill key={t.id} tag={t} />)}
                <button onClick={() => setPopover(popover === popKey ? null : popKey)} style={{
                  background: 'none', border: '1px dashed var(--border-subtle)', borderRadius: 10,
                  cursor: 'pointer', color: 'var(--text-muted)', fontSize: 11, padding: '0px 6px', lineHeight: '18px',
                }}><TagIcon size={10} style={{ display: 'inline', verticalAlign: 'middle' }} /></button>
                {popover === popKey && (
                  <TagPopover resourceType="vm" resourceId={vmKey} allTags={allTags} assigned={tagMap[vmKey] || []} onClose={() => { setPopover(null); onTagsChanged() }} />
                )}
              </div>
            </div>
            <div style={{ display: 'flex', gap: 6, flexWrap: 'wrap' }}>
              {PX_ACTIONS.filter(a => a.statuses.includes(vm.status)).map(a => (
                <Button
                  key={a.action}
                  size="sm"
                  variant={a.action === 'stop' ? 'danger' : 'secondary'}
                  loading={busy === `${vm.vmid}-${a.action}`}
                  onClick={() => doAction(vm, a.action)}
                >
                  {a.label}
                </Button>
              ))}
            </div>
          </div>
        )
      })}
    </div>
  )
}

// ── page ──────────────────────────────────────────────────────────────────────

export default function VMsPage() {
  const [tab, setTab] = useState<'local' | 'proxmox'>('local')
  const [pxReload, setPxReload] = useState(0)
  const [allTags, setAllTags] = useState<Tag[]>([])
  const [tagMap, setTagMap] = useState<TagMap>({})
  const globalTag = useFiltersStore((s) => s.globalTag)

  const loadTags = useCallback(async () => {
    try {
      const [tags, map] = await Promise.all([api.tags.list(), api.tags.map('vm')])
      setAllTags(tags)
      setTagMap(map)
    } catch { /* empty */ }
  }, [])

  useEffect(() => { loadTags() }, [loadTags])

  return (
    <div className="space-y-4">
      <div>
        <h1 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>Virtual Machines</h1>
        <p className="text-xs mt-0.5" style={{ color: 'var(--text-muted)' }}>
          Manage local KVM/QEMU VMs and Proxmox nodes from one place.
        </p>
      </div>

      <div className="flex gap-1 w-fit rounded-lg p-1" style={{ background: 'var(--bg-panel)' }}>
        <Tab label="Local (virsh)" active={tab === 'local'} onClick={() => setTab('local')} />
        <Tab label="Proxmox" active={tab === 'proxmox'} onClick={() => setTab('proxmox')} />
      </div>

      {tab === 'local' && (
        <LocalVmsSection allTags={allTags} tagMap={tagMap} globalTag={globalTag} onTagsChanged={loadTags} />
      )}

      {tab === 'proxmox' && (
        <>
          <ProxmoxConfigPanel onSaved={() => setPxReload(r => r + 1)} />
          <ProxmoxVmsSection reload={pxReload} allTags={allTags} tagMap={tagMap} globalTag={globalTag} onTagsChanged={loadTags} />
        </>
      )}
    </div>
  )
}
