import { useState, useEffect, useCallback } from 'react'
import { useNavigate } from 'react-router-dom'
import { Tag as TagIcon, Play, Square, RotateCcw } from 'lucide-react'
import { api } from '@/api/client'
import type { ContainerInfo, ContainerAction, Tag, TagMap } from '@/api/types'
import { notify } from '@/store/notifications'
import { useFiltersStore } from '@/store/filters'
import Button from '@/components/ui/Button'
import LogViewer from '@/components/ui/LogViewer'
import { TagPill, TagPopover } from '@/components/ui/TagPill'
import SendToOdysseus from '@/components/ui/SendToOdysseus'
import ChangePlanModal, { type ChangePlan } from '@/components/ui/ChangePlanModal'

function stateColor(state: string): string {
  switch (state.toLowerCase()) {
    case 'running': return 'var(--accent-success)'
    case 'exited':  return 'var(--accent-danger)'
    case 'paused':  return 'var(--accent-warning)'
    default:        return 'var(--text-muted)'
  }
}

function StateDot({ state }: { state: string }) {
  const color = stateColor(state)
  return (
    <span className="inline-flex items-center gap-1.5 text-xs" style={{ color }}>
      <span className="inline-block w-1.5 h-1.5 rounded-full" style={{ background: color }} />
      {state}
    </span>
  )
}

function fmtSize(bytes: number): string {
  const gb = bytes / 1e9
  if (gb >= 1) return `${gb.toFixed(1)} GB`
  const mb = bytes / 1e6
  if (mb >= 1) return `${mb.toFixed(0)} MB`
  return `${(bytes / 1e3).toFixed(0)} KB`
}

export default function ContainersPage() {
  const navigate = useNavigate()
  const [containers, setContainers] = useState<ContainerInfo[]>([])
  const [loading, setLoading] = useState(true)
  const [dockerAvailable, setDockerAvailable] = useState(true)
  const [actionLoading, setActionLoading] = useState<string | null>(null)
  const [logsFor, setLogsFor] = useState<ContainerInfo | null>(null)
  const [logLines, setLogLines] = useState<string[]>([])
  const [logsLoading, setLogsLoading] = useState(false)
  const [tab, setTab] = useState<'containers' | 'images'>('containers')
  const [images, setImages] = useState<import('@/api/types').ImageInfo[]>([])
  const [allTags, setAllTags] = useState<Tag[]>([])
  const [tagMap, setTagMap] = useState<TagMap>({})
  const [filterTag, setFilterTag] = useState<string | null>(null)
  const [popover, setPopover] = useState<string | null>(null)
  const globalTag = useFiltersStore((s) => s.globalTag)

  const load = useCallback(async () => {
    setLoading(true)
    try {
      const data = await api.containers.list()
      setContainers(data.containers)
      setDockerAvailable(data.docker_available)
    } catch {
      notify.error('Failed to load containers')
    } finally {
      setLoading(false)
    }
  }, [])

  const loadImages = useCallback(async () => {
    try {
      const data = await api.containers.images()
      setImages(data.images)
    } catch { /* empty */ }
  }, [])

  const loadTags = useCallback(async () => {
    try {
      const [tags, map] = await Promise.all([api.tags.list(), api.tags.map('container')])
      setAllTags(tags); setTagMap(map)
    } catch { /* empty */ }
  }, [])

  useEffect(() => {
    load(); loadTags()
    const t = setInterval(load, 10_000)
    return () => clearInterval(t)
  }, [load, loadTags])

  useEffect(() => {
    if (tab === 'images') loadImages()
  }, [tab, loadImages])

  const [removePlan, setRemovePlan] = useState<{ plan: ChangePlan; containerId: string } | null>(null)
  const [removeConfirming, setRemoveConfirming] = useState(false)

  const doAction = async (container: ContainerInfo, action: ContainerAction) => {
    if (action === 'remove') {
      try {
        const res = await api.containers.actionPlan(container.id, action)
        setRemovePlan({ plan: res.plan, containerId: container.id })
      } catch (e: unknown) {
        notify.error(e instanceof Error ? e.message : 'Failed to fetch plan')
      }
      return
    }
    setActionLoading(`${container.id}-${action}`)
    try {
      await api.containers.action(container.id, action)
      notify.success(`${action} sent to ${container.name}`)
      await load()
    } catch (e: unknown) {
      notify.error(e instanceof Error ? e.message : 'Action failed')
    } finally {
      setActionLoading(null)
    }
  }

  const confirmRemove = async () => {
    if (!removePlan) return
    setRemoveConfirming(true)
    try {
      await api.containers.action(removePlan.containerId, 'remove')
      notify.success('Container removed')
      setRemovePlan(null)
      await load()
    } catch (e: unknown) {
      notify.error(e instanceof Error ? e.message : 'Remove failed')
    } finally {
      setRemoveConfirming(false)
    }
  }

  const openLogs = async (container: ContainerInfo) => {
    setLogsFor(container)
    setLogsLoading(true)
    try {
      const data = await api.containers.logs(container.id, 300)
      setLogLines(data.lines)
    } catch {
      setLogLines(['Failed to load logs'])
    } finally {
      setLogsLoading(false)
    }
  }

  if (!dockerAvailable && !loading) {
    return (
      <div className="space-y-4">
        <h1 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>Containers</h1>
        <div className="panel p-8 text-center" style={{ color: 'var(--text-muted)' }}>
          <div className="text-sm mb-2">Docker is not available on this host.</div>
          <div className="text-xs">Install Docker and ensure the socket is accessible at /var/run/docker.sock</div>
        </div>
      </div>
    )
  }

  return (
    <div className="space-y-4">
      {removePlan && (
        <ChangePlanModal
          plan={removePlan.plan}
          confirming={removeConfirming}
          onConfirm={confirmRemove}
          onCancel={() => setRemovePlan(null)}
        />
      )}
      <div className="flex items-center justify-between">
        <h1 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>Containers</h1>
        <Button size="sm" onClick={load} loading={loading}>Refresh</Button>
      </div>

      {/* Tabs */}
      <div className="flex gap-1 border-b" style={{ borderColor: 'var(--border-subtle)' }}>
        {(['containers', 'images'] as const).map((t) => (
          <button
            key={t}
            onClick={() => setTab(t)}
            className="px-4 py-2 text-xs capitalize transition-colors border-b-2 -mb-px"
            style={{
              color: tab === t ? 'var(--accent-primary)' : 'var(--text-muted)',
              borderColor: tab === t ? 'var(--accent-primary)' : 'transparent',
            }}
          >
            {t}
          </button>
        ))}
      </div>

      {tab === 'containers' && (
        <>
          {allTags.length > 0 && !globalTag && (
            <div style={{ display: 'flex', gap: 6, flexWrap: 'wrap', alignItems: 'center' }}>
              <span style={{ fontSize: 12, color: 'var(--text-muted)' }}>Filter:</span>
              <button onClick={() => setFilterTag(null)} style={{ padding: '2px 10px', borderRadius: 12, fontSize: 11, fontWeight: 600, cursor: 'pointer', border: '1px solid var(--border-subtle)', background: filterTag === null ? 'var(--accent-primary)' : 'transparent', color: filterTag === null ? '#fff' : 'var(--text-muted)' }}>All</button>
              {allTags.map(t => (
                <button key={t.id} onClick={() => setFilterTag(filterTag === t.id ? null : t.id)} style={{ padding: '2px 10px', borderRadius: 12, fontSize: 11, fontWeight: 600, cursor: 'pointer', background: filterTag === t.id ? t.color + '33' : 'transparent', color: t.color, border: `1px solid ${filterTag === t.id ? t.color : t.color + '55'}` }}>{t.name}</button>
              ))}
            </div>
          )}
        {/* Wide layout — table */}
        <div className="containers-table panel overflow-hidden">
          <table className="w-full text-xs">
            <thead>
              <tr style={{ borderBottom: '1px solid var(--border-subtle)' }}>
                {['Name', 'Image', 'State', 'Status', 'Ports', 'Actions'].map((h) => (
                  <th key={h} className="px-4 py-2.5 text-left uppercase tracking-wider" style={{ color: 'var(--text-muted)' }}>{h}</th>
                ))}
              </tr>
            </thead>
            <tbody>
              {((globalTag ?? filterTag) ? containers.filter(c => (tagMap[c.name] || []).some(t => t.id === (globalTag ?? filterTag))) : containers).map((c) => {
                const isRunning = c.state === 'running'
                const actionKey = `${c.id}-`
                return (
                  <tr key={c.id} className="cursor-pointer hover:opacity-90 transition-opacity" onClick={() => navigate(`/containers/${c.id}`)} style={{ borderBottom: '1px solid var(--border-subtle)' }}>
                    <td className="px-4 py-2.5 font-mono font-medium" style={{ color: 'var(--text-primary)' }}>
                      {c.name}
                      <span className="ml-2 font-normal" style={{ color: 'var(--text-muted)' }}>{c.short_id}</span>
                      <div style={{ display: 'flex', gap: 4, flexWrap: 'wrap', marginTop: 3, alignItems: 'center', position: 'relative' }} onClick={e => e.stopPropagation()}>
                        {(tagMap[c.name] || []).map(t => <TagPill key={t.id} tag={t} />)}
                        <button onClick={() => setPopover(popover === c.name ? null : c.name)} style={{ background: 'none', border: '1px dashed var(--border-subtle)', borderRadius: 10, cursor: 'pointer', color: 'var(--text-muted)', fontSize: 11, padding: '0px 6px', lineHeight: '18px' }}><TagIcon size={10} style={{ display: 'inline', verticalAlign: 'middle' }} /></button>
                        {popover === c.name && <TagPopover resourceType="container" resourceId={c.name} allTags={allTags} assigned={tagMap[c.name] || []} onClose={() => { setPopover(null); loadTags() }} />}
                      </div>
                    </td>
                    <td className="px-4 py-2.5 max-w-48 truncate" style={{ color: 'var(--text-secondary)' }}>{c.image}</td>
                    <td className="px-4 py-2.5">
                      <StateDot state={c.state} />
                    </td>
                    <td className="px-4 py-2.5" style={{ color: 'var(--text-muted)' }}>{c.status}</td>
                    <td className="px-4 py-2.5 font-mono" style={{ color: 'var(--text-muted)' }}>
                      {c.ports.filter((p) => p.host_port).map((p) => (
                        <span key={`${p.host_port}:${p.container_port}`} className="mr-2">
                          {p.host_port}:{p.container_port}/{p.protocol}
                        </span>
                      ))}
                    </td>
                    <td className="px-4 py-2.5">
                      <div className="flex items-center gap-1 flex-wrap">
                        {isRunning ? (
                          <Button size="sm" variant="ghost" loading={actionLoading === `${actionKey}stop`}
                            onClick={() => doAction(c, 'stop')}>Stop</Button>
                        ) : (
                          <Button size="sm" variant="ghost" loading={actionLoading === `${actionKey}start`}
                            onClick={() => doAction(c, 'start')}>Start</Button>
                        )}
                        <Button size="sm" variant="ghost" loading={actionLoading === `${actionKey}restart`}
                          onClick={() => doAction(c, 'restart')}>Restart</Button>
                        <Button size="sm" variant="ghost" onClick={() => openLogs(c)}>Logs</Button>
                        <SendToOdysseus
                          context={`Container: ${c.name} (${c.short_id})\nImage: ${c.image}\nState: ${c.state} — ${c.status}${c.ports.filter(p => p.host_port).length ? `\nPorts: ${c.ports.filter(p => p.host_port).map(p => `${p.host_port}:${p.container_port}/${p.protocol}`).join(', ')}` : ''}`}
                        />
                      </div>
                    </td>
                  </tr>
                )
              })}
              {!loading && containers.length === 0 && (
                <tr>
                  <td colSpan={6} className="px-4 py-8 text-center" style={{ color: 'var(--text-muted)' }}>
                    No containers found.
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>

        {/* Narrow layout — compact cards (shown via container query) */}
        <div className="containers-cards" style={{ display: 'none' }}>
          {((globalTag ?? filterTag) ? containers.filter(c => (tagMap[c.name] || []).some(t => t.id === (globalTag ?? filterTag))) : containers).map((c) => {
            const isRunning = c.state === 'running'
            const dotColor = stateColor(c.state)
            const actionKey = `${c.id}-`
            const hostPorts = c.ports.filter(p => p.host_port)
            return (
              <div key={c.id} style={{
                background: 'var(--bg-card)',
                border: '1px solid var(--border-subtle)',
                borderRadius: 8,
                padding: '10px 12px',
              }}>
                <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                  <span style={{ width: 8, height: 8, borderRadius: '50%', background: dotColor, flexShrink: 0 }} />
                  <span
                    className="font-mono"
                    onClick={() => navigate(`/containers/${c.id}`)}
                    style={{ flex: 1, fontWeight: 600, fontSize: 13, color: 'var(--text-primary)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', cursor: 'pointer' }}
                  >
                    {c.name}
                  </span>
                  <div style={{ display: 'flex', gap: 4, flexShrink: 0 }}>
                    {isRunning ? (
                      <button title="Stop" onClick={() => doAction(c, 'stop')} style={{
                        width: 28, height: 28, borderRadius: 6, border: '1px solid var(--border-subtle)',
                        background: 'var(--bg-elevated)', cursor: 'pointer', display: 'flex', alignItems: 'center', justifyContent: 'center', color: 'var(--accent-danger)',
                      }}>
                        {actionLoading === `${actionKey}stop` ? '…' : <Square size={12} />}
                      </button>
                    ) : (
                      <button title="Start" onClick={() => doAction(c, 'start')} style={{
                        width: 28, height: 28, borderRadius: 6, border: '1px solid var(--border-subtle)',
                        background: 'var(--bg-elevated)', cursor: 'pointer', display: 'flex', alignItems: 'center', justifyContent: 'center', color: 'var(--accent-success)',
                      }}>
                        {actionLoading === `${actionKey}start` ? '…' : <Play size={12} />}
                      </button>
                    )}
                    <button title="Restart" onClick={() => doAction(c, 'restart')} style={{
                      width: 28, height: 28, borderRadius: 6, border: '1px solid var(--border-subtle)',
                      background: 'var(--bg-elevated)', cursor: 'pointer', display: 'flex', alignItems: 'center', justifyContent: 'center', color: 'var(--text-secondary)',
                    }}>
                      {actionLoading === `${actionKey}restart` ? '…' : <RotateCcw size={12} />}
                    </button>
                    <button title="Logs" onClick={() => openLogs(c)} style={{
                      width: 28, height: 28, borderRadius: 6, border: '1px solid var(--border-subtle)',
                      background: 'var(--bg-elevated)', cursor: 'pointer', display: 'flex', alignItems: 'center', justifyContent: 'center', color: 'var(--text-secondary)', fontSize: 10, fontWeight: 600,
                    }}>
                      log
                    </button>
                  </div>
                </div>
                <div style={{ fontSize: 11, color: 'var(--text-muted)', marginTop: 4, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                  {c.image}
                </div>
                <div style={{ display: 'flex', gap: 6, marginTop: 4, alignItems: 'center', flexWrap: 'wrap' }}>
                  <span style={{ fontSize: 11, color: dotColor }}>{c.state}</span>
                  {hostPorts.length > 0 && (
                    <>
                      <span style={{ fontSize: 11, color: 'var(--text-disabled)' }}>·</span>
                      <span className="font-mono" style={{ fontSize: 11, color: 'var(--accent-secondary)' }}>
                        {hostPorts.map(p => `${p.host_port}:${p.container_port}`).join(', ')}
                      </span>
                    </>
                  )}
                </div>
              </div>
            )
          })}
          {!loading && containers.length === 0 && (
            <div style={{ padding: '32px 0', textAlign: 'center', fontSize: 13, color: 'var(--text-muted)' }}>
              No containers found.
            </div>
          )}
        </div>
        </>
      )}

      {tab === 'images' && (
        <div className="panel overflow-hidden">
          <table className="w-full text-xs">
            <thead>
              <tr style={{ borderBottom: '1px solid var(--border-subtle)' }}>
                {['ID', 'Tags', 'Size', 'Created'].map((h) => (
                  <th key={h} className="px-4 py-2.5 text-left uppercase tracking-wider" style={{ color: 'var(--text-muted)' }}>{h}</th>
                ))}
              </tr>
            </thead>
            <tbody>
              {images.map((img) => (
                <tr key={img.id} style={{ borderBottom: '1px solid var(--border-subtle)' }}>
                  <td className="px-4 py-2.5 font-mono" style={{ color: 'var(--text-muted)' }}>{img.id}</td>
                  <td className="px-4 py-2.5" style={{ color: 'var(--text-primary)' }}>
                    {img.tags.map((t) => (
                      <span key={t} className="mr-2">{t}</span>
                    ))}
                    {img.tags.length === 0 && <span style={{ color: 'var(--text-muted)' }}>&lt;none&gt;</span>}
                  </td>
                  <td className="px-4 py-2.5" style={{ color: 'var(--text-secondary)' }}>{fmtSize(img.size)}</td>
                  <td className="px-4 py-2.5" style={{ color: 'var(--text-muted)' }}>
                    {new Date(img.created * 1000).toLocaleDateString()}
                  </td>
                </tr>
              ))}
              {images.length === 0 && (
                <tr>
                  <td colSpan={4} className="px-4 py-8 text-center" style={{ color: 'var(--text-muted)' }}>No images.</td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      )}

      {/* Log drawer */}
      {logsFor && (
        <div
          className="fixed inset-0 z-50 flex items-end justify-center"
          style={{ background: 'rgba(0,0,0,0.6)' }}
          onClick={() => setLogsFor(null)}
        >
          <div
            className="w-full max-w-4xl rounded-t-lg overflow-hidden"
            style={{ background: 'var(--bg-panel)', border: '1px solid var(--border-subtle)', maxHeight: '60vh' }}
            onClick={(e) => e.stopPropagation()}
          >
            <div className="flex items-center justify-between px-4 py-2 border-b" style={{ borderColor: 'var(--border-subtle)' }}>
              <span className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>
                Logs — {logsFor.name}
              </span>
              <button onClick={() => setLogsFor(null)} style={{ color: 'var(--text-muted)' }}>✕</button>
            </div>
            {logsLoading ? (
              <div className="p-4 text-xs" style={{ color: 'var(--text-muted)' }}>Loading…</div>
            ) : (
              <LogViewer lines={logLines} maxHeight={400} />
            )}
          </div>
        </div>
      )}
    </div>
  )
}
