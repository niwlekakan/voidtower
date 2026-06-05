import { useEffect, useRef, useState } from 'react'
import {
  AlertTriangle, CheckCircle, Cpu, Database, FolderOpen,
  HardDrive, Plus, RefreshCw, Server, Trash2, XCircle,
  type LucideIcon,
} from 'lucide-react'
import { api } from '@/api/client'
import { useMetricsStore } from '@/store/metrics'
import type { BlockDevice, FstabEntry, MountInfo, RaidArray, SmartInfo, StoragePaths } from '@/api/types'

// ─── Utilities ────────────────────────────────────────────────────────────────

function fmt(bytes: number, d = 1) {
  if (!bytes) return '0 B'
  const k = 1024, s = ['B','KB','MB','GB','TB']
  const i = Math.floor(Math.log(bytes) / Math.log(k))
  return `${(bytes / Math.pow(k, i)).toFixed(d)} ${s[i]}`
}

const inputStyle = {
  background: 'var(--bg-elevated)',
  border: '1px solid var(--border-default)',
  color: 'var(--text-primary)',
  borderRadius: 4,
  padding: '4px 8px',
  fontSize: 12,
} as const

function UsageBar({ used, total }: { used: number; total: number }) {
  const pct = total > 0 ? (used / total) * 100 : 0
  const color = pct > 90 ? 'var(--accent-danger)' : pct > 75 ? 'var(--accent-warning)' : 'var(--accent-success)'
  return (
    <div className="flex items-center gap-2">
      <div className="flex-1 h-1.5 rounded-full overflow-hidden" style={{ background: 'var(--bg-elevated)', minWidth: 60 }}>
        <div className="h-full rounded-full" style={{ width: `${pct.toFixed(1)}%`, background: color }} />
      </div>
      <span className="w-10 text-right font-mono text-xs" style={{ color }}>{pct.toFixed(1)}%</span>
    </div>
  )
}

function HealthBadge({ health }: { health: string }) {
  if (health === 'healthy') return <span className="flex items-center gap-1 text-xs" style={{ color: 'var(--accent-success)' }}><CheckCircle size={12} /> Healthy</span>
  if (health === 'failing') return <span className="flex items-center gap-1 text-xs" style={{ color: 'var(--accent-danger)' }}><XCircle size={12} /> Failing</span>
  if (health === 'unavailable') return <span className="flex items-center gap-1 text-xs" style={{ color: 'var(--text-muted)' }}><AlertTriangle size={12} /> No smartctl</span>
  return <span className="text-xs" style={{ color: 'var(--text-muted)' }}>—</span>
}

function RaidStateBadge({ state }: { state: string }) {
  const s = state.toLowerCase()
  const color = s.includes('active') || s.includes('clean') ? 'var(--accent-success)'
    : s.includes('degraded') ? 'var(--accent-warning)' : 'var(--accent-danger)'
  return <span className="text-xs font-medium px-1.5 py-0.5 rounded" style={{ color, background: color + '22' }}>{state || 'unknown'}</span>
}

function Skeleton({ rows = 4 }: { rows?: number }) {
  return (
    <div className="space-y-2">
      {Array.from({ length: rows }).map((_, i) => (
        <div key={i} className="h-8 rounded animate-pulse" style={{ background: 'var(--bg-elevated)', opacity: 0.5 }} />
      ))}
    </div>
  )
}

function ErrBar({ msg, onDismiss }: { msg: string; onDismiss: () => void }) {
  return (
    <div className="text-xs px-3 py-2 rounded flex items-center gap-2"
      style={{ background: 'var(--accent-danger)22', color: 'var(--accent-danger)', border: '1px solid var(--accent-danger)44' }}>
      <AlertTriangle size={12} /> {msg}
      <button className="ml-auto" onClick={onDismiss}>✕</button>
    </div>
  )
}

function TabBtn({ active, onClick, icon: Icon, label }: { active: boolean; onClick: () => void; icon: LucideIcon; label: string }) {
  return (
    <button onClick={onClick} className="flex items-center gap-2 px-3 py-2 text-sm rounded transition-colors"
      style={{
        color: active ? 'var(--accent-primary)' : 'var(--text-muted)',
        background: active ? 'var(--accent-primary)1a' : 'transparent',
        borderBottom: active ? '2px solid var(--accent-primary)' : '2px solid transparent',
      }}>
      <Icon size={14} />{label}
    </button>
  )
}

// ─── Drive Picker ─────────────────────────────────────────────────────────────
// Shows a visual grid of discovered block devices for easy selection.

interface DrivePickerProps {
  devices: BlockDevice[]
  selected: string[]           // list of device paths already chosen
  onToggle: (path: string) => void
  single?: boolean             // if true, behave like a radio (replace selection)
  filter?: (d: BlockDevice) => boolean  // restrict which devices appear
  label?: string
}

function flattenDevices(devs: BlockDevice[], filter?: (d: BlockDevice) => boolean): BlockDevice[] {
  const out: BlockDevice[] = []
  const walk = (d: BlockDevice) => {
    if (!filter || filter(d)) out.push(d)
    d.children.forEach(walk)
  }
  devs.forEach(walk)
  return out
}

function DrivePicker({ devices, selected, onToggle, single, filter, label }: DrivePickerProps) {
  const flat = flattenDevices(devices, filter)
  if (flat.length === 0) return (
    <p className="text-xs py-2" style={{ color: 'var(--text-muted)' }}>No block devices found. Click Refresh on the Devices tab.</p>
  )
  return (
    <div>
      {label && <p className="text-xs mb-2" style={{ color: 'var(--text-muted)' }}>{label}</p>}
      <div className="grid gap-1.5" style={{ gridTemplateColumns: 'repeat(auto-fill, minmax(200px, 1fr))' }}>
        {flat.map(d => {
          const isSelected = selected.includes(d.path)
          const isMounted  = !!d.mountpoint
          return (
            <button
              key={d.path}
              onClick={() => onToggle(d.path)}
              className="text-left rounded p-2.5 transition-colors"
              style={{
                background: isSelected ? 'var(--accent-primary)22' : 'var(--bg-elevated)',
                border: `1px solid ${isSelected ? 'var(--accent-primary)' : 'var(--border-default)'}`,
                opacity: isMounted && !isSelected ? 0.65 : 1,
              }}
            >
              <div className="flex items-center gap-2 mb-1">
                {single
                  ? <input type="radio" readOnly checked={isSelected} className="w-3 h-3 shrink-0" />
                  : <input type="checkbox" readOnly checked={isSelected} className="w-3 h-3 shrink-0" />
                }
                <HardDrive size={12} style={{ color: isSelected ? 'var(--accent-primary)' : 'var(--text-muted)', flexShrink: 0 }} />
                <span className="font-mono text-xs font-medium truncate" style={{ color: isSelected ? 'var(--accent-primary)' : 'var(--text-primary)' }}>
                  {d.name}
                </span>
                <span className="ml-auto text-xs shrink-0" style={{ color: 'var(--text-muted)' }}>
                  {fmt(d.size_bytes)}
                </span>
              </div>
              <div className="text-xs pl-5 space-y-0.5" style={{ color: 'var(--text-muted)' }}>
                {(d.model || d.vendor) && <div className="truncate">{d.model || d.vendor}</div>}
                <div className="flex items-center gap-2">
                  <span className="px-1 rounded" style={{ background: 'var(--bg-base)' }}>{d.device_type}</span>
                  {d.fstype && <span>{d.fstype}</span>}
                  {isMounted && <span style={{ color: 'var(--accent-warning)' }}>mounted</span>}
                </div>
                <div className="font-mono truncate">{d.path}</div>
              </div>
            </button>
          )
        })}
      </div>
    </div>
  )
}

// ─── Overview tab ─────────────────────────────────────────────────────────────

function OverviewTab({ raids }: { raids: RaidArray[] }) {
  const snapshot = useMetricsStore(s => s.snapshot)
  return (
    <div className="space-y-4">
      <div className="card space-y-3">
        <h2 className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>Disk Usage</h2>
        {!snapshot ? <Skeleton rows={3} /> : (
          <div className="overflow-x-auto">
            <table className="w-full text-xs">
              <thead>
                <tr style={{ borderBottom: '1px solid var(--border-subtle)' }}>
                  {['Mount','Device','Filesystem','Used','Total','Free','Usage'].map(h => (
                    <th key={h} className="px-3 py-2 text-left uppercase tracking-wider" style={{ color: 'var(--text-muted)' }}>{h}</th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {snapshot.disks.map(d => (
                  <tr key={d.mount_point} style={{ borderBottom: '1px solid var(--border-subtle)' }}>
                    <td className="px-3 py-2.5 font-mono font-medium" style={{ color: 'var(--text-primary)' }}>{d.mount_point}</td>
                    <td className="px-3 py-2.5 font-mono" style={{ color: 'var(--text-muted)' }}>{d.name}</td>
                    <td className="px-3 py-2.5" style={{ color: 'var(--text-secondary)' }}>{d.fs_type}</td>
                    <td className="px-3 py-2.5" style={{ color: 'var(--text-secondary)' }}>{fmt(d.used)}</td>
                    <td className="px-3 py-2.5" style={{ color: 'var(--text-secondary)' }}>{fmt(d.total)}</td>
                    <td className="px-3 py-2.5" style={{ color: 'var(--text-muted)' }}>{fmt(d.available)}</td>
                    <td className="px-3 py-2.5 w-36"><UsageBar used={d.used} total={d.total} /></td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>
      {raids.length > 0 && (
        <div className="card space-y-3">
          <h2 className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>RAID Arrays</h2>
          <div className="space-y-2">
            {raids.map(r => (
              <div key={r.path} className="flex items-center justify-between px-3 py-2 rounded text-xs"
                style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-default)' }}>
                <div className="flex items-center gap-3">
                  <Database size={14} style={{ color: 'var(--text-muted)' }} />
                  <span className="font-mono font-medium" style={{ color: 'var(--text-primary)' }}>{r.path}</span>
                  <span style={{ color: 'var(--text-muted)' }}>{r.level}</span>
                </div>
                <div className="flex items-center gap-3">
                  <span style={{ color: 'var(--text-muted)' }}>{fmt(r.size_bytes)}</span>
                  {r.failed_devices > 0 && <span className="flex items-center gap-1" style={{ color: 'var(--accent-danger)' }}><AlertTriangle size={12} /> {r.failed_devices} failed</span>}
                  <RaidStateBadge state={r.state} />
                </div>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  )
}

// ─── Devices tab ──────────────────────────────────────────────────────────────

function DeviceRow({ dev, depth, onSmartClick, smartData }: {
  dev: BlockDevice; depth: number
  onSmartClick: (name: string) => void; smartData: Record<string, SmartInfo>
}) {
  const [expanded, setExpanded] = useState(depth === 0)
  const smart = smartData[dev.name]
  return (
    <>
      <tr style={{ borderBottom: '1px solid var(--border-subtle)' }}>
        <td className="px-3 py-2.5 font-mono text-xs" style={{ color: 'var(--text-primary)', paddingLeft: `${12 + depth * 20}px` }}>
          <div className="flex items-center gap-2">
            {dev.children.length > 0
              ? <button onClick={() => setExpanded(!expanded)} className="text-xs w-3" style={{ color: 'var(--text-muted)' }}>{expanded ? '▾' : '▸'}</button>
              : <span className="w-3" />}
            <HardDrive size={12} style={{ color: 'var(--text-muted)', flexShrink: 0 }} />
            {dev.name}
          </div>
        </td>
        <td className="px-3 py-2.5 text-xs" style={{ color: 'var(--text-muted)' }}>{dev.model || dev.vendor || '—'}</td>
        <td className="px-3 py-2.5 text-xs font-mono" style={{ color: 'var(--text-secondary)' }}>{fmt(dev.size_bytes)}</td>
        <td className="px-3 py-2.5 text-xs">
          <span className="px-1.5 py-0.5 rounded text-xs" style={{ background: 'var(--bg-elevated)', color: 'var(--text-muted)' }}>{dev.device_type}</span>
        </td>
        <td className="px-3 py-2.5 text-xs" style={{ color: 'var(--text-muted)' }}>{dev.fstype || '—'}</td>
        <td className="px-3 py-2.5 text-xs font-mono" style={{ color: 'var(--text-muted)' }}>{dev.mountpoint || '—'}</td>
        <td className="px-3 py-2.5 text-xs">
          {smart ? <HealthBadge health={smart.health} />
            : dev.device_type === 'disk'
              ? <button onClick={() => onSmartClick(dev.name)} className="text-xs underline" style={{ color: 'var(--accent-primary)' }}>Check SMART</button>
              : <span style={{ color: 'var(--text-muted)' }}>—</span>}
        </td>
      </tr>
      {expanded && dev.children.map(child => (
        <DeviceRow key={child.name} dev={child} depth={depth + 1} onSmartClick={onSmartClick} smartData={smartData} />
      ))}
    </>
  )
}

function DevicesTab() {
  const [devices, setDevices] = useState<BlockDevice[]>([])
  const [loading, setLoading] = useState(true)
  const [smartData, setSmartData] = useState<Record<string, SmartInfo>>({})

  const load = async () => {
    setLoading(true)
    try { const r = await api.storage.devices(); setDevices(r.devices) } catch { /* empty */ }
    setLoading(false)
  }
  useEffect(() => { load() }, [])

  const fetchSmart = async (name: string) => {
    try { const info = await api.storage.smart(name); setSmartData(p => ({ ...p, [name]: info })) } catch { /* empty */ }
  }

  return (
    <div className="card space-y-3">
      <div className="flex items-center justify-between">
        <h2 className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>Block Devices</h2>
        <button onClick={load} className="flex items-center gap-1.5 text-xs px-2 py-1 rounded"
          style={{ color: 'var(--text-muted)', background: 'var(--bg-elevated)' }}>
          <RefreshCw size={12} /> Refresh
        </button>
      </div>
      {loading ? <Skeleton rows={5} /> : devices.length === 0
        ? <p className="text-xs py-4 text-center" style={{ color: 'var(--text-muted)' }}>No block devices found (lsblk may not be available)</p>
        : (
          <div className="overflow-x-auto">
            <table className="w-full text-xs">
              <thead>
                <tr style={{ borderBottom: '1px solid var(--border-subtle)' }}>
                  {['Device','Model','Size','Type','Filesystem','Mountpoint','SMART'].map(h => (
                    <th key={h} className="px-3 py-2 text-left uppercase tracking-wider" style={{ color: 'var(--text-muted)' }}>{h}</th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {devices.map(dev => <DeviceRow key={dev.name} dev={dev} depth={0} onSmartClick={fetchSmart} smartData={smartData} />)}
              </tbody>
            </table>
          </div>
        )}
    </div>
  )
}

// ─── Mounts tab ───────────────────────────────────────────────────────────────

function MountsTab() {
  const [mounts, setMounts]   = useState<MountInfo[]>([])
  const [fstab, setFstab]     = useState<FstabEntry[]>([])
  const [devices, setDevices] = useState<BlockDevice[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError]     = useState('')
  const [confirm, setConfirm] = useState<string | null>(null)

  const [mDevice, setMDevice]       = useState('')
  const [mMountpoint, setMMountpoint] = useState('')
  const [mFstype, setMFstype]       = useState('ext4')
  const [mOptions, setMOptions]     = useState('defaults')
  const [mBusy, setMBusy]           = useState(false)
  const [showPicker, setShowPicker] = useState(false)

  const [fDevice, setFDevice]         = useState('')
  const [fMountpoint, setFMountpoint] = useState('')
  const [fFstype, setFFstype]         = useState('ext4')
  const [fOptions, setFOptions]       = useState('defaults')
  const [fBusy, setFBusy]             = useState(false)

  const fstypes = ['ext4','ext3','xfs','btrfs','vfat','ntfs','auto']

  const loadAll = async () => {
    setLoading(true)
    try {
      const [m, f, d] = await Promise.all([api.storage.mounts(), api.storage.fstab(), api.storage.devices()])
      setMounts(m.mounts); setFstab(f.entries); setDevices(d.devices)
    } catch (e: any) { setError(e.message ?? 'Load failed') }
    setLoading(false)
  }
  useEffect(() => { loadAll() }, [])

  const doUmount = async (mp: string) => {
    try { await api.storage.umount(mp); await loadAll() } catch (e: any) { setError(e.message ?? 'Unmount failed') }
    setConfirm(null)
  }
  const doMount = async () => {
    setMBusy(true); setError('')
    try { await api.storage.mount(mDevice, mMountpoint, mFstype, mOptions); setMDevice(''); setMMountpoint(''); await loadAll() }
    catch (e: any) { setError(e.message ?? 'Mount failed') }
    setMBusy(false)
  }
  const doAddFstab = async () => {
    setFBusy(true); setError('')
    try { await api.storage.addFstab({ device: fDevice, mountpoint: fMountpoint, fstype: fFstype, options: fOptions }); setFDevice(''); setFMountpoint(''); await loadAll() }
    catch (e: any) { setError(e.message ?? 'Failed to add fstab entry') }
    setFBusy(false)
  }
  const doRemoveFstab = async (idx: number) => {
    try { await api.storage.removeFstab(idx); await loadAll() }
    catch (e: any) { setError(e.message ?? 'Failed to remove entry') }
    setConfirm(null)
  }

  // When a device is picked in the picker, fill the device field and close
  const handlePickDevice = (path: string) => {
    setMDevice(path)
    setShowPicker(false)
  }

  return (
    <div className="space-y-4">
      {error && <ErrBar msg={error} onDismiss={() => setError('')} />}

      {/* Current mounts */}
      <div className="card space-y-3">
        <div className="flex items-center justify-between">
          <h2 className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>Current Mounts</h2>
          <button onClick={loadAll} className="flex items-center gap-1.5 text-xs px-2 py-1 rounded"
            style={{ color: 'var(--text-muted)', background: 'var(--bg-elevated)' }}>
            <RefreshCw size={12} /> Refresh
          </button>
        </div>
        {loading ? <Skeleton rows={4} /> : mounts.length === 0
          ? <p className="text-xs py-4 text-center" style={{ color: 'var(--text-muted)' }}>No user mounts found</p>
          : (
            <div className="overflow-x-auto">
              <table className="w-full text-xs">
                <thead>
                  <tr style={{ borderBottom: '1px solid var(--border-subtle)' }}>
                    {['Device','Mountpoint','FS','Usage',''].map(h => (
                      <th key={h} className="px-3 py-2 text-left uppercase tracking-wider" style={{ color: 'var(--text-muted)' }}>{h}</th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {mounts.map(m => (
                    <tr key={m.mountpoint} style={{ borderBottom: '1px solid var(--border-subtle)' }}>
                      <td className="px-3 py-2.5 font-mono" style={{ color: 'var(--text-muted)' }}>{m.device}</td>
                      <td className="px-3 py-2.5 font-mono font-medium" style={{ color: 'var(--text-primary)' }}>{m.mountpoint}</td>
                      <td className="px-3 py-2.5" style={{ color: 'var(--text-secondary)' }}>{m.fstype}</td>
                      <td className="px-3 py-2.5 w-48">
                        <UsageBar used={m.used_bytes} total={m.size_bytes} />
                        <div className="mt-0.5" style={{ color: 'var(--text-muted)', fontSize: 10 }}>{fmt(m.used_bytes)} / {fmt(m.size_bytes)}</div>
                      </td>
                      <td className="px-3 py-2.5 text-right">
                        {confirm === m.mountpoint ? (
                          <span className="flex items-center gap-1 justify-end">
                            <span style={{ color: 'var(--text-muted)' }}>Confirm?</span>
                            <button onClick={() => doUmount(m.mountpoint)} className="text-xs px-2 py-0.5 rounded" style={{ background: 'var(--accent-danger)22', color: 'var(--accent-danger)' }}>Yes</button>
                            <button onClick={() => setConfirm(null)} className="text-xs px-2 py-0.5 rounded" style={{ background: 'var(--bg-elevated)', color: 'var(--text-muted)' }}>No</button>
                          </span>
                        ) : (
                          <button onClick={() => setConfirm(m.mountpoint)} className="flex items-center gap-1 text-xs px-2 py-1 rounded ml-auto"
                            style={{ color: 'var(--accent-danger)', background: 'var(--accent-danger)11' }}>
                            <Trash2 size={11} /> Unmount
                          </button>
                        )}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
      </div>

      {/* Mount new */}
      <div className="card space-y-3">
        <h2 className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>Mount Device</h2>

        {/* Drive picker toggle */}
        <div>
          <button onClick={() => setShowPicker(v => !v)}
            className="flex items-center gap-1.5 text-xs px-2 py-1.5 rounded mb-3"
            style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-default)', color: 'var(--text-secondary)' }}>
            <HardDrive size={12} /> {showPicker ? 'Hide drive picker' : 'Pick from discovered drives'}
          </button>
          {showPicker && (
            <div className="mb-3 p-3 rounded" style={{ background: 'var(--bg-base)', border: '1px solid var(--border-subtle)' }}>
              <DrivePicker
                devices={devices}
                selected={mDevice ? [mDevice] : []}
                onToggle={handlePickDevice}
                single
                label="Select device to mount — click to fill the form below"
              />
            </div>
          )}
        </div>

        <div className="grid grid-cols-2 gap-2 md:grid-cols-4">
          <div className="space-y-1">
            <label className="text-xs" style={{ color: 'var(--text-muted)' }}>Device</label>
            <input value={mDevice} onChange={e => setMDevice(e.target.value)} placeholder="/dev/sdb1" style={{ ...inputStyle, width: '100%' }} />
          </div>
          <div className="space-y-1">
            <label className="text-xs" style={{ color: 'var(--text-muted)' }}>Mountpoint</label>
            <input value={mMountpoint} onChange={e => setMMountpoint(e.target.value)} placeholder="/mnt/data" style={{ ...inputStyle, width: '100%' }} />
          </div>
          <div className="space-y-1">
            <label className="text-xs" style={{ color: 'var(--text-muted)' }}>Filesystem</label>
            <select value={mFstype} onChange={e => setMFstype(e.target.value)} style={{ ...inputStyle, width: '100%' }}>
              {fstypes.map(f => <option key={f}>{f}</option>)}
            </select>
          </div>
          <div className="space-y-1">
            <label className="text-xs" style={{ color: 'var(--text-muted)' }}>Options</label>
            <input value={mOptions} onChange={e => setMOptions(e.target.value)} style={{ ...inputStyle, width: '100%' }} />
          </div>
        </div>
        <button onClick={doMount} disabled={mBusy || !mDevice || !mMountpoint}
          className="flex items-center gap-1.5 text-xs px-3 py-1.5 rounded disabled:opacity-50"
          style={{ background: 'var(--accent-primary)', color: '#fff' }}>
          <Plus size={12} /> {mBusy ? 'Mounting…' : 'Mount'}
        </button>
      </div>

      {/* Fstab */}
      <div className="card space-y-3">
        <h2 className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>/etc/fstab Entries</h2>
        {loading ? <Skeleton rows={3} /> : fstab.length === 0
          ? <p className="text-xs py-2" style={{ color: 'var(--text-muted)' }}>No fstab entries found</p>
          : (
            <div className="overflow-x-auto">
              <table className="w-full text-xs">
                <thead>
                  <tr style={{ borderBottom: '1px solid var(--border-subtle)' }}>
                    {['Device','Mountpoint','FS','Options',''].map(h => (
                      <th key={h} className="px-3 py-2 text-left uppercase tracking-wider" style={{ color: 'var(--text-muted)' }}>{h}</th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {fstab.map(e => (
                    <tr key={e.line_idx} style={{ borderBottom: '1px solid var(--border-subtle)' }}>
                      <td className="px-3 py-2.5 font-mono" style={{ color: 'var(--text-muted)' }}>{e.device}</td>
                      <td className="px-3 py-2.5 font-mono" style={{ color: 'var(--text-primary)' }}>{e.mountpoint}</td>
                      <td className="px-3 py-2.5" style={{ color: 'var(--text-secondary)' }}>{e.fstype}</td>
                      <td className="px-3 py-2.5 font-mono" style={{ color: 'var(--text-muted)' }}>{e.options}</td>
                      <td className="px-3 py-2.5 text-right">
                        {confirm === `fstab-${e.line_idx}` ? (
                          <span className="flex items-center gap-1 justify-end">
                            <span style={{ color: 'var(--text-muted)' }}>Remove?</span>
                            <button onClick={() => doRemoveFstab(e.line_idx)} className="text-xs px-2 py-0.5 rounded" style={{ background: 'var(--accent-danger)22', color: 'var(--accent-danger)' }}>Yes</button>
                            <button onClick={() => setConfirm(null)} className="text-xs px-2 py-0.5 rounded" style={{ background: 'var(--bg-elevated)', color: 'var(--text-muted)' }}>No</button>
                          </span>
                        ) : (
                          <button onClick={() => setConfirm(`fstab-${e.line_idx}`)} className="flex items-center gap-1 text-xs px-2 py-1 rounded ml-auto"
                            style={{ color: 'var(--accent-danger)', background: 'var(--accent-danger)11' }}>
                            <Trash2 size={11} /> Remove
                          </button>
                        )}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        <div className="pt-2" style={{ borderTop: '1px solid var(--border-subtle)' }}>
          <p className="text-xs font-medium mb-2" style={{ color: 'var(--text-secondary)' }}>Add entry</p>
          <div className="grid grid-cols-2 gap-2 md:grid-cols-4">
            <input value={fDevice} onChange={e => setFDevice(e.target.value)} placeholder="Device / UUID" style={{ ...inputStyle, width: '100%' }} />
            <input value={fMountpoint} onChange={e => setFMountpoint(e.target.value)} placeholder="Mountpoint" style={{ ...inputStyle, width: '100%' }} />
            <select value={fFstype} onChange={e => setFFstype(e.target.value)} style={{ ...inputStyle, width: '100%' }}>
              {fstypes.map(f => <option key={f}>{f}</option>)}
            </select>
            <input value={fOptions} onChange={e => setFOptions(e.target.value)} placeholder="defaults" style={{ ...inputStyle, width: '100%' }} />
          </div>
          <button onClick={doAddFstab} disabled={fBusy || !fDevice || !fMountpoint}
            className="mt-2 flex items-center gap-1.5 text-xs px-3 py-1.5 rounded disabled:opacity-50"
            style={{ background: 'var(--accent-primary)', color: '#fff' }}>
            <Plus size={12} /> {fBusy ? 'Adding…' : 'Add to fstab'}
          </button>
        </div>
      </div>
    </div>
  )
}

// ─── RAID tab ─────────────────────────────────────────────────────────────────

function RaidTab() {
  const [available, setAvailable] = useState(true)
  const [arrays, setArrays]       = useState<RaidArray[]>([])
  const [devices, setDevices]     = useState<BlockDevice[]>([])
  const [loading, setLoading]     = useState(true)
  const [error, setError]         = useState('')
  const [confirm, setConfirm]     = useState<string | null>(null)
  const [showCreate, setShowCreate] = useState(false)

  const [cName, setCName]         = useState('md0')
  const [cLevel, setCLevel]       = useState('1')
  const [cSelected, setCSelected] = useState<string[]>([])
  const [cBusy, setCBusy]         = useState(false)

  const loadAll = async () => {
    setLoading(true)
    try {
      const [r, d] = await Promise.all([api.storage.raid(), api.storage.devices()])
      setAvailable(r.available); setArrays(r.arrays); setDevices(d.devices)
    } catch (e: any) { setError(e.message ?? 'Load failed') }
    setLoading(false)
  }
  useEffect(() => { loadAll() }, [])

  const toggleDev = (path: string) =>
    setCSelected(prev => prev.includes(path) ? prev.filter(p => p !== path) : [...prev, path])

  const doCreate = async () => {
    setCBusy(true); setError('')
    try { await api.storage.createRaid(cName, cLevel, cSelected); setShowCreate(false); setCSelected([]); await loadAll() }
    catch (e: any) { setError(e.message ?? 'Create failed') }
    setCBusy(false)
  }
  const doStop = async (path: string) => {
    try { await api.storage.stopRaid(path); await loadAll() }
    catch (e: any) { setError(e.message ?? 'Stop failed') }
    setConfirm(null)
  }

  return (
    <div className="space-y-4">
      {error && <ErrBar msg={error} onDismiss={() => setError('')} />}
      {!available && !loading && (
        <div className="text-xs px-3 py-2 rounded flex items-center gap-2"
          style={{ background: 'var(--bg-elevated)', color: 'var(--text-muted)', border: '1px solid var(--border-default)' }}>
          <AlertTriangle size={12} /> mdadm is not installed — RAID management unavailable
        </div>
      )}

      <div className="card space-y-3">
        <div className="flex items-center justify-between">
          <h2 className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>RAID Arrays</h2>
          <div className="flex items-center gap-2">
            <button onClick={loadAll} className="flex items-center gap-1.5 text-xs px-2 py-1 rounded"
              style={{ color: 'var(--text-muted)', background: 'var(--bg-elevated)' }}>
              <RefreshCw size={12} /> Refresh
            </button>
            {available && (
              <button onClick={() => setShowCreate(v => !v)} className="flex items-center gap-1.5 text-xs px-2 py-1 rounded"
                style={{ background: 'var(--accent-primary)', color: '#fff' }}>
                <Plus size={12} /> Create Array
              </button>
            )}
          </div>
        </div>

        {loading ? <Skeleton rows={3} /> : arrays.length === 0
          ? <p className="text-xs py-4 text-center" style={{ color: 'var(--text-muted)' }}>No RAID arrays found</p>
          : (
            <div className="space-y-2">
              {arrays.map(arr => (
                <div key={arr.path} className="rounded p-3 space-y-2"
                  style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-default)' }}>
                  <div className="flex items-center justify-between">
                    <div className="flex items-center gap-2">
                      <Database size={14} style={{ color: 'var(--text-muted)' }} />
                      <span className="font-mono text-sm font-medium" style={{ color: 'var(--text-primary)' }}>{arr.path}</span>
                      <span className="text-xs px-1.5 py-0.5 rounded" style={{ background: 'var(--bg-base)', color: 'var(--text-muted)' }}>{arr.level}</span>
                      <RaidStateBadge state={arr.state} />
                    </div>
                    {confirm === arr.path ? (
                      <span className="flex items-center gap-1">
                        <span className="text-xs" style={{ color: 'var(--text-muted)' }}>Stop array?</span>
                        <button onClick={() => doStop(arr.path)} className="text-xs px-2 py-0.5 rounded" style={{ background: 'var(--accent-danger)22', color: 'var(--accent-danger)' }}>Yes</button>
                        <button onClick={() => setConfirm(null)} className="text-xs px-2 py-0.5 rounded" style={{ background: 'var(--bg-elevated)', color: 'var(--text-muted)' }}>No</button>
                      </span>
                    ) : (
                      <button onClick={() => setConfirm(arr.path)} className="flex items-center gap-1 text-xs px-2 py-1 rounded"
                        style={{ color: 'var(--accent-danger)', background: 'var(--accent-danger)11' }}>
                        <XCircle size={11} /> Stop
                      </button>
                    )}
                  </div>
                  <div className="grid grid-cols-3 gap-2 text-xs" style={{ color: 'var(--text-muted)' }}>
                    <span>Size: {fmt(arr.size_bytes)}</span>
                    <span>Active: {arr.active_devices}</span>
                    {arr.failed_devices > 0
                      ? <span style={{ color: 'var(--accent-danger)' }}>Failed: {arr.failed_devices}</span>
                      : <span>Failed: 0</span>}
                  </div>
                  <div className="flex flex-wrap gap-1">
                    {arr.devices.map(dev => (
                      <span key={dev} className="text-xs font-mono px-1.5 py-0.5 rounded"
                        style={{ background: 'var(--bg-base)', color: 'var(--text-muted)' }}>{dev}</span>
                    ))}
                  </div>
                </div>
              ))}
            </div>
          )}

        {showCreate && (
          <div className="rounded p-4 space-y-4 mt-2"
            style={{ border: '1px solid var(--border-default)', background: 'var(--bg-base)' }}>
            <p className="text-xs font-semibold" style={{ color: 'var(--text-primary)' }}>Create RAID Array</p>
            <div className="grid grid-cols-2 gap-3">
              <div className="space-y-1">
                <label className="text-xs" style={{ color: 'var(--text-muted)' }}>Array name (md*)</label>
                <input value={cName} onChange={e => setCName(e.target.value)} style={{ ...inputStyle, width: '100%' }} placeholder="md0" />
              </div>
              <div className="space-y-1">
                <label className="text-xs" style={{ color: 'var(--text-muted)' }}>RAID level</label>
                <select value={cLevel} onChange={e => setCLevel(e.target.value)} style={{ ...inputStyle, width: '100%' }}>
                  {['0','1','5','6','10'].map(l => <option key={l} value={l}>RAID {l}</option>)}
                </select>
              </div>
            </div>

            {/* Drive picker for RAID — multi-select, disks and partitions only */}
            <DrivePicker
              devices={devices}
              selected={cSelected}
              onToggle={toggleDev}
              filter={d => d.device_type === 'disk' || d.device_type === 'part'}
              label={`Select 2+ devices for the array — ${cSelected.length} selected`}
            />

            <div className="flex gap-2">
              <button onClick={doCreate} disabled={cBusy || cSelected.length < 2 || !cName}
                className="flex items-center gap-1.5 text-xs px-3 py-1.5 rounded disabled:opacity-50"
                style={{ background: 'var(--accent-primary)', color: '#fff' }}>
                <Server size={12} /> {cBusy ? 'Creating…' : 'Create Array'}
              </button>
              <button onClick={() => setShowCreate(false)} className="text-xs px-3 py-1.5 rounded"
                style={{ background: 'var(--bg-elevated)', color: 'var(--text-muted)' }}>
                Cancel
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  )
}

// ─── Locations tab ────────────────────────────────────────────────────────────
// Configure where containers, app vault, VMs, and backups are stored.

interface LocationSlot {
  key: keyof StoragePaths
  label: string
  description: string
  icon: LucideIcon
  defaultPath: string
}

const LOCATION_SLOTS: LocationSlot[] = [
  { key: 'containers', label: 'Containers & Compose',  description: 'Docker Compose projects and container data', icon: Database,   defaultPath: '/var/lib/voidtower/apps' },
  { key: 'appvault',   label: 'App Vault deployments', description: 'App Vault template files and volumes',       icon: Server,     defaultPath: '/var/lib/voidtower/apps' },
  { key: 'vms',        label: 'Virtual Machines',      description: 'KVM/libvirt disk images and VM data',       icon: Cpu,        defaultPath: '/var/lib/libvirt/images' },
  { key: 'backups',    label: 'Backups',               description: 'Restic repositories and backup archives',    icon: HardDrive,  defaultPath: '/var/lib/voidtower/backups' },
]

function LocationCard({
  slot, currentPath, mounts, devices, onSave,
}: {
  slot: LocationSlot
  currentPath: string | null
  mounts: MountInfo[]
  devices: BlockDevice[]
  onSave: (key: keyof StoragePaths, path: string) => Promise<void>
}) {
  const [editing, setEditing]       = useState(false)
  const [showPicker, setShowPicker] = useState(false)
  const [draft, setDraft]           = useState(currentPath ?? slot.defaultPath)
  const [busy, setBusy]             = useState(false)

  const Icon = slot.icon

  // Find mount info for the current path (best-prefix match)
  const mountForPath = (p: string) =>
    mounts.filter(m => p.startsWith(m.mountpoint)).sort((a, b) => b.mountpoint.length - a.mountpoint.length)[0]

  const mount = currentPath ? mountForPath(currentPath) : null
  const effectivePath = currentPath ?? slot.defaultPath

  const handleSave = async () => {
    setBusy(true)
    await onSave(slot.key, draft)
    setBusy(false)
    setEditing(false)
    setShowPicker(false)
  }

  // When user picks a device/mountpoint, prefill the draft with that mountpoint + a subdirectory
  const handlePickMount = (mountpoint: string) => {
    const sub = slot.defaultPath.split('/').pop() ?? slot.key
    setDraft(`${mountpoint.replace(/\/$/, '')}/${sub}`)
    setShowPicker(false)
  }

  return (
    <div className="rounded p-4 space-y-3" style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-default)' }}>
      <div className="flex items-start justify-between gap-3">
        <div className="flex items-center gap-3">
          <div className="p-2 rounded" style={{ background: 'var(--accent-primary)1a' }}>
            <Icon size={16} style={{ color: 'var(--accent-primary)' }} />
          </div>
          <div>
            <p className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>{slot.label}</p>
            <p className="text-xs" style={{ color: 'var(--text-muted)' }}>{slot.description}</p>
          </div>
        </div>
        {!editing && (
          <button onClick={() => { setDraft(effectivePath); setEditing(true) }}
            className="text-xs px-2 py-1 rounded shrink-0"
            style={{ background: 'var(--bg-base)', border: '1px solid var(--border-default)', color: 'var(--text-muted)' }}>
            Change
          </button>
        )}
      </div>

      {/* Current path display */}
      {!editing && (
        <div className="flex items-center gap-2">
          <span className="font-mono text-xs px-2 py-1 rounded flex-1"
            style={{ background: 'var(--bg-base)', color: currentPath ? 'var(--text-primary)' : 'var(--text-muted)', border: '1px solid var(--border-subtle)' }}>
            {effectivePath}
            {!currentPath && <span className="ml-2" style={{ color: 'var(--text-muted)' }}>(default)</span>}
          </span>
          {mount && (
            <div className="text-xs shrink-0" style={{ color: 'var(--text-muted)' }}>
              on <span className="font-mono" style={{ color: 'var(--text-secondary)' }}>{mount.mountpoint}</span>
              {' · '}<UsageBar used={mount.used_bytes} total={mount.size_bytes} />
            </div>
          )}
        </div>
      )}

      {/* Edit form */}
      {editing && (
        <div className="space-y-3">
          {/* Pick from mounted volumes */}
          <button onClick={() => setShowPicker(v => !v)}
            className="flex items-center gap-1.5 text-xs px-2 py-1.5 rounded"
            style={{ background: 'var(--bg-base)', border: '1px solid var(--border-default)', color: 'var(--text-secondary)' }}>
            <FolderOpen size={12} /> {showPicker ? 'Hide volume picker' : 'Pick from mounted volumes'}
          </button>

          {showPicker && (
            <div className="rounded p-3" style={{ background: 'var(--bg-base)', border: '1px solid var(--border-subtle)' }}>
              {mounts.length === 0
                ? <p className="text-xs" style={{ color: 'var(--text-muted)' }}>No mounted volumes found. Mount a drive on the Mounts tab first.</p>
                : (
                  <div className="grid gap-1.5" style={{ gridTemplateColumns: 'repeat(auto-fill, minmax(220px, 1fr))' }}>
                    {mounts.map(m => (
                      <button key={m.mountpoint} onClick={() => handlePickMount(m.mountpoint)}
                        className="text-left rounded p-2.5 transition-colors"
                        style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-default)' }}>
                        <div className="flex items-center gap-2 mb-1">
                          <HardDrive size={12} style={{ color: 'var(--text-muted)' }} />
                          <span className="font-mono text-xs font-medium" style={{ color: 'var(--text-primary)' }}>{m.mountpoint}</span>
                        </div>
                        <div className="pl-5 space-y-1">
                          <div className="text-xs" style={{ color: 'var(--text-muted)' }}>{m.device} · {m.fstype}</div>
                          <UsageBar used={m.used_bytes} total={m.size_bytes} />
                          <div className="text-xs" style={{ color: 'var(--text-muted)' }}>
                            {fmt(m.size_bytes - m.used_bytes)} free of {fmt(m.size_bytes)}
                          </div>
                        </div>
                      </button>
                    ))}
                  </div>
                )}

              {/* Also offer unmounted disks to remind user to mount first */}
              {devices.filter(d => d.device_type === 'disk' && !d.mountpoint).length > 0 && (
                <div className="mt-3 pt-3" style={{ borderTop: '1px solid var(--border-subtle)' }}>
                  <p className="text-xs mb-2" style={{ color: 'var(--text-muted)' }}>Unmounted disks — mount them first on the Mounts tab:</p>
                  <div className="flex flex-wrap gap-1">
                    {devices.filter(d => d.device_type === 'disk' && !d.mountpoint).map(d => (
                      <span key={d.path} className="text-xs font-mono px-2 py-0.5 rounded"
                        style={{ background: 'var(--bg-elevated)', color: 'var(--text-muted)', border: '1px solid var(--border-subtle)' }}>
                        {d.path} ({fmt(d.size_bytes)})
                      </span>
                    ))}
                  </div>
                </div>
              )}
            </div>
          )}

          {/* Manual path input */}
          <div className="space-y-1">
            <label className="text-xs" style={{ color: 'var(--text-muted)' }}>Path (absolute)</label>
            <input value={draft} onChange={e => setDraft(e.target.value)} style={{ ...inputStyle, width: '100%' }} placeholder={slot.defaultPath} />
            {draft && !draft.startsWith('/') && (
              <p className="text-xs" style={{ color: 'var(--accent-warning)' }}>Path must be absolute (start with /)</p>
            )}
          </div>

          <div className="flex gap-2">
            <button onClick={handleSave} disabled={busy || !draft.startsWith('/')}
              className="flex items-center gap-1.5 text-xs px-3 py-1.5 rounded disabled:opacity-50"
              style={{ background: 'var(--accent-primary)', color: '#fff' }}>
              {busy ? 'Saving…' : 'Save'}
            </button>
            <button onClick={() => { setEditing(false); setShowPicker(false) }}
              className="text-xs px-3 py-1.5 rounded"
              style={{ background: 'var(--bg-base)', color: 'var(--text-muted)' }}>
              Cancel
            </button>
          </div>
        </div>
      )}
    </div>
  )
}

function LocationsTab() {
  const [paths, setPaths]     = useState<StoragePaths>({ containers: null, appvault: null, vms: null, backups: null })
  const [mounts, setMounts]   = useState<MountInfo[]>([])
  const [devices, setDevices] = useState<BlockDevice[]>([])
  const [loading, setLoading] = useState(true)
  const [saved, setSaved]     = useState(false)

  useEffect(() => {
    Promise.all([api.storage.getPaths(), api.storage.mounts(), api.storage.devices()]).then(([p, m, d]) => {
      setPaths(p); setMounts(m.mounts); setDevices(d.devices)
    }).finally(() => setLoading(false))
  }, [])

  const handleSave = async (key: keyof StoragePaths, path: string) => {
    await api.storage.setPaths({ [key]: path })
    setPaths(prev => ({ ...prev, [key]: path }))
    setSaved(true)
    setTimeout(() => setSaved(false), 2500)
  }

  return (
    <div className="space-y-4">
      <div className="card space-y-1">
        <h2 className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>Storage Locations</h2>
        <p className="text-xs" style={{ color: 'var(--text-muted)' }}>
          Configure where VoidTower stores data for each subsystem. Pick from a mounted volume or type an absolute path.
          Changes take effect for new deployments — existing data is not moved automatically.
        </p>
        {saved && (
          <p className="text-xs pt-1" style={{ color: 'var(--accent-success)' }}>
            <CheckCircle size={11} style={{ display: 'inline', marginRight: 4 }} />Location saved.
          </p>
        )}
      </div>

      {loading ? <Skeleton rows={4} /> : (
        <div className="space-y-3">
          {LOCATION_SLOTS.map(slot => (
            <LocationCard
              key={slot.key}
              slot={slot}
              currentPath={paths[slot.key]}
              mounts={mounts}
              devices={devices}
              onSave={handleSave}
            />
          ))}
        </div>
      )}
    </div>
  )
}

// ─── Root page ────────────────────────────────────────────────────────────────

type TabId = 'overview' | 'devices' | 'mounts' | 'raid' | 'locations'

export default function StoragePage() {
  const [tab, setTab]     = useState<TabId>('overview')
  const [raids, setRaids] = useState<RaidArray[]>([])
  const intervalRef       = useRef<ReturnType<typeof setInterval> | null>(null)

  useEffect(() => {
    const loadRaid = async () => {
      try { const r = await api.storage.raid(); setRaids(r.arrays) } catch { /* empty */ }
    }
    loadRaid()
    if (tab === 'overview' || tab === 'mounts') {
      intervalRef.current = setInterval(loadRaid, 15_000)
    }
    return () => { if (intervalRef.current) clearInterval(intervalRef.current) }
  }, [tab])

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between flex-wrap gap-2">
        <h1 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>Storage</h1>
        <div className="flex items-center gap-1 flex-wrap">
          <TabBtn active={tab === 'overview'}   onClick={() => setTab('overview')}   icon={Cpu}       label="Overview" />
          <TabBtn active={tab === 'devices'}    onClick={() => setTab('devices')}    icon={HardDrive} label="Devices" />
          <TabBtn active={tab === 'mounts'}     onClick={() => setTab('mounts')}     icon={Server}    label="Mounts" />
          <TabBtn active={tab === 'raid'}       onClick={() => setTab('raid')}       icon={Database}  label="RAID / Mirror" />
          <TabBtn active={tab === 'locations'}  onClick={() => setTab('locations')}  icon={FolderOpen} label="Locations" />
        </div>
      </div>

      {tab === 'overview'  && <OverviewTab raids={raids} />}
      {tab === 'devices'   && <DevicesTab />}
      {tab === 'mounts'    && <MountsTab />}
      {tab === 'raid'      && <RaidTab />}
      {tab === 'locations' && <LocationsTab />}
    </div>
  )
}
