import { useState, useEffect, useCallback, useRef } from 'react'
import { RefreshCw, Play, Square, RotateCcw, Tag as TagIcon } from 'lucide-react'
import { api, ApiClientError } from '@/api/client'
import type { ServiceInfo, ServiceAction, Tag, TagMap } from '@/api/types'
import { useAuthStore } from '@/store/auth'
import { notify } from '@/store/notifications'
import StatusBadge from '@/components/ui/StatusBadge'
import Button from '@/components/ui/Button'
import LogViewer from '@/components/ui/LogViewer'
import ConfirmDialog from '@/components/ui/ConfirmDialog'
import { TagPill } from '@/components/ui/TagPill'

type ActionMeta = { service: string; action: ServiceAction }

function TagPopover({ resourceId, allTags, assigned, onClose }: {
  resourceId: string; allTags: Tag[]; assigned: Tag[]; onClose: () => void
}) {
  const ref = useRef<HTMLDivElement>(null)
  const assignedIds = new Set(assigned.map(t => t.id))

  useEffect(() => {
    const handler = (e: MouseEvent) => { if (ref.current && !ref.current.contains(e.target as Node)) onClose() }
    document.addEventListener('mousedown', handler)
    return () => document.removeEventListener('mousedown', handler)
  }, [onClose])

  const toggle = async (tag: Tag) => {
    try {
      if (assignedIds.has(tag.id)) await api.tags.unassign(tag.id, 'service', resourceId)
      else await api.tags.assign(tag.id, 'service', resourceId)
    } catch { notify.error('Failed to update tag') }
    onClose()
  }

  if (allTags.length === 0) return (
    <div ref={ref} style={{ position: 'absolute', zIndex: 50, top: '100%', left: 0, background: 'var(--bg-panel)', border: '1px solid var(--border-subtle)', borderRadius: 8, padding: 10, minWidth: 160, boxShadow: '0 4px 16px rgba(0,0,0,0.3)' }}>
      <p style={{ fontSize: 12, color: 'var(--text-muted)' }}>No tags yet. Create some on the Tags page.</p>
    </div>
  )

  return (
    <div ref={ref} style={{ position: 'absolute', zIndex: 50, top: '100%', left: 0, background: 'var(--bg-panel)', border: '1px solid var(--border-subtle)', borderRadius: 8, padding: 8, minWidth: 160, boxShadow: '0 4px 16px rgba(0,0,0,0.3)', display: 'flex', flexDirection: 'column', gap: 2 }}>
      {allTags.map(tag => (
        <button key={tag.id} onClick={() => toggle(tag)} style={{
          display: 'flex', alignItems: 'center', gap: 8, padding: '4px 8px', borderRadius: 5,
          background: assignedIds.has(tag.id) ? 'var(--accent-primary-subtle)' : 'transparent',
          border: 'none', cursor: 'pointer', width: '100%', textAlign: 'left',
        }}>
          <span style={{ width: 10, height: 10, borderRadius: '50%', background: tag.color, flexShrink: 0 }} />
          <span style={{ fontSize: 12, color: 'var(--text-primary)' }}>{tag.name}</span>
          {assignedIds.has(tag.id) && <span style={{ marginLeft: 'auto', fontSize: 11, color: 'var(--accent-primary)' }}>✓</span>}
        </button>
      ))}
    </div>
  )
}

export default function ServicesPage() {
  const [services, setServices] = useState<ServiceInfo[]>([])
  const [loading, setLoading] = useState(true)
  const [systemdAvailable, setSystemdAvailable] = useState(true)
  const [logs, setLogs] = useState<{ name: string; lines: string[] } | null>(null)
  const [confirm, setConfirm] = useState<ActionMeta | null>(null)
  const [acting, setActing] = useState<string | null>(null)
  const [allTags, setAllTags] = useState<Tag[]>([])
  const [tagMap, setTagMap] = useState<TagMap>({})
  const [filterTag, setFilterTag] = useState<string | null>(null)
  const [popover, setPopover] = useState<string | null>(null)
  const user = useAuthStore((s) => s.user)
  const canAct = user?.role !== 'viewer'

  const load = useCallback(async () => {
    setLoading(true)
    try {
      const data = await api.services.list()
      setServices(data.services)
      setSystemdAvailable(data.systemd_available)
    } catch { notify.error('Failed to load services') }
    finally { setLoading(false) }
  }, [])

  const loadTags = useCallback(async () => {
    try {
      const [tags, map] = await Promise.all([api.tags.list(), api.tags.map('service')])
      setAllTags(tags)
      setTagMap(map)
    } catch { /* empty */ }
  }, [])

  useEffect(() => { load(); loadTags() }, [load, loadTags])

  const handlePopoverClose = useCallback(() => { setPopover(null); loadTags() }, [loadTags])

  const runAction = async (name: string, action: ServiceAction) => {
    setActing(name); setConfirm(null)
    try { await api.services.action(name, action); notify.success(`${action} ${name}`); setTimeout(load, 800) }
    catch (err) { notify.error(`Failed: ${err instanceof ApiClientError ? err.message : action}`) }
    finally { setActing(null) }
  }

  const viewLogs = async (name: string) => {
    try { const { lines } = await api.services.logs(name); setLogs({ name, lines }) }
    catch { notify.error('Failed to fetch logs') }
  }

  const filtered = filterTag ? services.filter(s => (tagMap[s.name] || []).some(t => t.id === filterTag)) : services

  if (!systemdAvailable) return (
    <div className="p-6 text-sm" style={{ color: 'var(--text-muted)' }}>systemd is not available on this system.</div>
  )

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h1 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>Services</h1>
        <Button size="sm" onClick={load} loading={loading}><RefreshCw size={13} /> Refresh</Button>
      </div>

      {/* Tag filter bar */}
      {allTags.length > 0 && (
        <div style={{ display: 'flex', gap: 6, flexWrap: 'wrap', alignItems: 'center' }}>
          <span style={{ fontSize: 12, color: 'var(--text-muted)' }}>Filter:</span>
          <button onClick={() => setFilterTag(null)} style={{
            padding: '2px 10px', borderRadius: 12, fontSize: 11, fontWeight: 600, cursor: 'pointer', border: '1px solid var(--border-subtle)',
            background: filterTag === null ? 'var(--accent-primary)' : 'transparent', color: filterTag === null ? '#fff' : 'var(--text-muted)',
          }}>All</button>
          {allTags.map(t => (
            <button key={t.id} onClick={() => setFilterTag(filterTag === t.id ? null : t.id)} style={{
              padding: '2px 10px', borderRadius: 12, fontSize: 11, fontWeight: 600, cursor: 'pointer',
              background: filterTag === t.id ? t.color + '33' : 'transparent',
              color: t.color, border: `1px solid ${filterTag === t.id ? t.color : t.color + '55'}`,
            }}>{t.name}</button>
          ))}
        </div>
      )}

      <div className="panel overflow-hidden">
        <table className="w-full text-sm">
          <thead>
            <tr style={{ borderBottom: '1px solid var(--border-subtle)' }}>
              {['Service', 'State', 'Sub-state', 'Enabled', 'Actions'].map(h => (
                <th key={h} className="px-4 py-2.5 text-left text-xs uppercase tracking-wider" style={{ color: 'var(--text-muted)' }}>{h}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {filtered.map(svc => (
              <tr key={svc.name} style={{ borderBottom: '1px solid var(--border-subtle)' }}>
                <td className="px-4 py-2.5">
                  <div style={{ color: 'var(--text-primary)' }}>{svc.name}</div>
                  {svc.description && <div className="text-xs truncate max-w-48" style={{ color: 'var(--text-muted)' }}>{svc.description}</div>}
                  <div style={{ display: 'flex', gap: 4, flexWrap: 'wrap', marginTop: 4, alignItems: 'center', position: 'relative' }}>
                    {(tagMap[svc.name] || []).map(t => <TagPill key={t.id} tag={t} />)}
                    {canAct && (
                      <button onClick={() => setPopover(popover === svc.name ? null : svc.name)} style={{
                        background: 'none', border: '1px dashed var(--border-subtle)', borderRadius: 10,
                        cursor: 'pointer', color: 'var(--text-muted)', fontSize: 11, padding: '0px 6px', lineHeight: '18px',
                      }}><TagIcon size={10} style={{ display: 'inline', verticalAlign: 'middle' }} /></button>
                    )}
                    {popover === svc.name && (
                      <TagPopover resourceId={svc.name} allTags={allTags} assigned={tagMap[svc.name] || []} onClose={handlePopoverClose} />
                    )}
                  </div>
                </td>
                <td className="px-4 py-2.5"><StatusBadge state={svc.active_state} /></td>
                <td className="px-4 py-2.5 text-xs" style={{ color: 'var(--text-secondary)' }}>{svc.sub_state}</td>
                <td className="px-4 py-2.5 text-xs" style={{ color: svc.enabled ? 'var(--accent-success)' : 'var(--text-muted)' }}>
                  {svc.enabled ? 'enabled' : 'disabled'}
                </td>
                <td className="px-4 py-2.5">
                  <div className="flex items-center gap-1">
                    {canAct && svc.active_state !== 'active' && (
                      <Button size="sm" variant="ghost" loading={acting === svc.name} onClick={() => setConfirm({ service: svc.name, action: 'start' })} title="Start"><Play size={12} /></Button>
                    )}
                    {canAct && svc.active_state === 'active' && (
                      <Button size="sm" variant="ghost" loading={acting === svc.name} onClick={() => setConfirm({ service: svc.name, action: 'stop' })} title="Stop"><Square size={12} /></Button>
                    )}
                    {canAct && (
                      <Button size="sm" variant="ghost" loading={acting === svc.name} onClick={() => setConfirm({ service: svc.name, action: 'restart' })} title="Restart"><RotateCcw size={12} /></Button>
                    )}
                    <Button size="sm" variant="ghost" onClick={() => viewLogs(svc.name)}>Logs</Button>
                  </div>
                </td>
              </tr>
            ))}
            {!loading && filtered.length === 0 && (
              <tr><td colSpan={5} className="px-4 py-8 text-center text-sm" style={{ color: 'var(--text-muted)' }}>
                {filterTag ? 'No services with this tag.' : 'No services found.'}
              </td></tr>
            )}
          </tbody>
        </table>
      </div>

      {logs && (
        <div className="panel p-4">
          <div className="flex items-center justify-between mb-3">
            <span className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>{logs.name} — logs</span>
            <Button size="sm" variant="ghost" onClick={() => setLogs(null)}>Close</Button>
          </div>
          <LogViewer lines={logs.lines} />
        </div>
      )}

      {confirm && (
        <ConfirmDialog
          title={`${confirm.action.charAt(0).toUpperCase() + confirm.action.slice(1)} service`}
          message={`Are you sure you want to ${confirm.action} ${confirm.service}?`}
          confirmLabel={confirm.action}
          danger={confirm.action === 'stop'}
          onConfirm={() => runAction(confirm.service, confirm.action)}
          onCancel={() => setConfirm(null)}
        />
      )}
    </div>
  )
}
