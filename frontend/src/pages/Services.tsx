import { useState, useEffect, useCallback } from 'react'
import { RefreshCw, Play, Square, RotateCcw, Tag as TagIcon } from 'lucide-react'
import { api, ApiClientError } from '@/api/client'
import type { ServiceInfo, ServiceAction, Tag, TagMap } from '@/api/types'
import { useAuthStore } from '@/store/auth'
import { notify } from '@/store/notifications'
import { useFiltersStore } from '@/store/filters'
import StatusBadge from '@/components/ui/StatusBadge'
import Button from '@/components/ui/Button'
import LogViewer from '@/components/ui/LogViewer'
import ConfirmDialog from '@/components/ui/ConfirmDialog'
import { TagPill, TagPopover } from '@/components/ui/TagPill'
import SendToOdysseus from '@/components/ui/SendToOdysseus'

type ActionMeta = { service: string; action: ServiceAction }

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
  const globalTag = useFiltersStore((s) => s.globalTag)
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

  const activeTag = globalTag ?? filterTag
  const filtered = activeTag ? services.filter(s => (tagMap[s.name] || []).some(t => t.id === activeTag)) : services

  if (!systemdAvailable) return (
    <div className="p-6 text-sm" style={{ color: 'var(--text-muted)' }}>systemd is not available on this system.</div>
  )

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h1 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>Services</h1>
        <Button size="sm" onClick={load} loading={loading}><RefreshCw size={13} /> Refresh</Button>
      </div>

      {/* Tag filter bar — only shown when no global tag is active */}
      {allTags.length > 0 && !globalTag && (
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

      {/* Wide layout — table */}
      <div className="services-table panel overflow-hidden">
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
                      <TagPopover resourceType="service" resourceId={svc.name} allTags={allTags} assigned={tagMap[svc.name] || []} onClose={handlePopoverClose} />
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
                    <SendToOdysseus
                      context={`Service: ${svc.name}\n${svc.description ? `Description: ${svc.description}\n` : ''}State: ${svc.active_state} (${svc.sub_state})\nEnabled: ${svc.enabled ? 'yes' : 'no'}`}
                    />
                  </div>
                </td>
              </tr>
            ))}
            {!loading && filtered.length === 0 && (
              <tr><td colSpan={5} className="px-4 py-8 text-center text-sm" style={{ color: 'var(--text-muted)' }}>
                {activeTag ? 'No services with this tag.' : 'No services found.'}
              </td></tr>
            )}
          </tbody>
        </table>
      </div>

      {/* Narrow layout — compact cards (shown via container query) */}
      <div className="services-cards" style={{ display: 'none' }}>
        {filtered.map(svc => {
          const isActive = svc.active_state === 'active'
          const dotColor = isActive ? 'var(--accent-success)' : svc.active_state === 'failed' ? 'var(--accent-danger)' : 'var(--text-muted)'
          return (
            <div key={svc.name} style={{
              background: 'var(--bg-card)',
              border: '1px solid var(--border-subtle)',
              borderRadius: 8,
              padding: '10px 12px',
            }}>
              <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                <span style={{ width: 8, height: 8, borderRadius: '50%', background: dotColor, flexShrink: 0,
                  boxShadow: isActive ? '0 0 6px var(--accent-success)' : undefined }} />
                <span style={{ flex: 1, fontWeight: 600, fontSize: 13, color: 'var(--text-primary)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                  {svc.name}
                </span>
                <div style={{ display: 'flex', gap: 4, flexShrink: 0 }}>
                  {canAct && !isActive && (
                    <button title="Start" onClick={() => setConfirm({ service: svc.name, action: 'start' })} style={{
                      width: 28, height: 28, borderRadius: 6, border: '1px solid var(--border-subtle)',
                      background: 'var(--bg-elevated)', cursor: 'pointer', display: 'flex', alignItems: 'center', justifyContent: 'center', color: 'var(--accent-success)',
                    }}>
                      <Play size={12} />
                    </button>
                  )}
                  {canAct && isActive && (
                    <button title="Stop" onClick={() => setConfirm({ service: svc.name, action: 'stop' })} style={{
                      width: 28, height: 28, borderRadius: 6, border: '1px solid var(--border-subtle)',
                      background: 'var(--bg-elevated)', cursor: 'pointer', display: 'flex', alignItems: 'center', justifyContent: 'center', color: 'var(--accent-danger)',
                    }}>
                      <Square size={12} />
                    </button>
                  )}
                  {canAct && (
                    <button title="Restart" onClick={() => setConfirm({ service: svc.name, action: 'restart' })} style={{
                      width: 28, height: 28, borderRadius: 6, border: '1px solid var(--border-subtle)',
                      background: 'var(--bg-elevated)', cursor: 'pointer', display: 'flex', alignItems: 'center', justifyContent: 'center', color: 'var(--text-secondary)',
                    }}>
                      <RotateCcw size={12} />
                    </button>
                  )}
                  <button title="Logs" onClick={() => viewLogs(svc.name)} style={{
                    width: 28, height: 28, borderRadius: 6, border: '1px solid var(--border-subtle)',
                    background: 'var(--bg-elevated)', cursor: 'pointer', display: 'flex', alignItems: 'center', justifyContent: 'center', color: 'var(--text-secondary)', fontSize: 10, fontWeight: 600,
                  }}>
                    log
                  </button>
                </div>
              </div>
              {svc.description && (
                <div style={{ fontSize: 11, color: 'var(--text-muted)', marginTop: 4, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                  {svc.description}
                </div>
              )}
              <div style={{ display: 'flex', gap: 6, marginTop: 4, alignItems: 'center', flexWrap: 'wrap' }}>
                <span style={{ fontSize: 11, color: isActive ? 'var(--accent-success)' : 'var(--text-muted)' }}>{svc.active_state}</span>
                <span style={{ fontSize: 11, color: 'var(--text-disabled)' }}>·</span>
                <span style={{ fontSize: 11, color: 'var(--text-muted)' }}>{svc.sub_state}</span>
                <span style={{ fontSize: 11, color: 'var(--text-disabled)' }}>·</span>
                <span style={{ fontSize: 11, color: svc.enabled ? 'var(--accent-success)' : 'var(--text-muted)' }}>{svc.enabled ? 'enabled' : 'disabled'}</span>
              </div>
            </div>
          )
        })}
        {!loading && filtered.length === 0 && (
          <div style={{ padding: '32px 0', textAlign: 'center', fontSize: 13, color: 'var(--text-muted)' }}>
            {activeTag ? 'No services with this tag.' : 'No services found.'}
          </div>
        )}
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
