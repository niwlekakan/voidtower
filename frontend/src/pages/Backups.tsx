import { useState, useEffect, useCallback } from 'react'
import { ShieldCheck, ShieldAlert, ShieldX, Shield, Play, Search, Undo2, Trash2, Tag as TagIcon } from 'lucide-react'
import { api } from '@/api/client'
import type { Tag, TagMap } from '@/api/types'
import { notify } from '@/store/notifications'
import { useFiltersStore } from '@/store/filters'
import { TagPill, TagPopover } from '@/components/ui/TagPill'
import Button from '@/components/ui/Button'

async function apiFetch<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(path, { ...init, credentials: 'include', headers: { 'Content-Type': 'application/json', ...init?.headers } })
  if (!res.ok) throw new Error((await res.json().catch(() => ({}))).error?.message ?? res.statusText)
  return res.json()
}

type Confidence = 'high' | 'medium' | 'low' | 'critical' | 'unknown'

interface BackupConfig {
  id: string
  name: string
  source_path: string
  repo_path: string
  retention_days: number
  enabled: boolean
  last_run_at: number | null
  last_status: string | null
  last_check_at: number | null
  last_check_status: string | null
  last_restore_test_at: number | null
  last_restore_test_status: string | null
  confidence: Confidence
}

const CONFIDENCE: Record<Confidence, { label: string; icon: typeof ShieldCheck; color: string }> = {
  high:     { label: 'High',     icon: ShieldCheck, color: 'var(--accent-success)'  },
  medium:   { label: 'Medium',   icon: Shield,      color: 'var(--accent-warning)'  },
  low:      { label: 'Low',      icon: ShieldAlert, color: 'color-mix(in srgb, var(--accent-error) 70%, var(--accent-warning))' },
  critical: { label: 'Critical', icon: ShieldX,     color: 'var(--accent-error)'    },
  unknown:  { label: 'Unknown',  icon: Shield,      color: 'var(--text-muted)'      },
}

function fmtDate(ts: number | null) {
  if (!ts) return '—'
  const diff = Math.floor(Date.now() / 1000) - ts
  if (diff < 3600)  return `${Math.floor(diff / 60)}m ago`
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`
  return `${Math.floor(diff / 86400)}d ago`
}

function ConfidenceBadge({ c }: { c: Confidence }) {
  const cfg = CONFIDENCE[c] ?? CONFIDENCE.unknown
  const Icon = cfg.icon
  return (
    <span className="flex items-center gap-1 text-xs font-medium" style={{ color: cfg.color }}>
      <Icon size={12} />
      {cfg.label}
    </span>
  )
}

function StatusDot({ status }: { status: string | null }) {
  if (!status) return <span style={{ color: 'var(--text-disabled)' }}>—</span>
  const color = status === 'ok' || status === 'success' ? 'var(--accent-success)'
    : status === 'failed' ? 'var(--accent-error)' : 'var(--text-muted)'
  return <span style={{ color }}>{status}</span>
}

export default function BackupsPage() {
  const [configs, setConfigs] = useState<BackupConfig[]>([])
  const [resticAvailable, setResticAvailable] = useState(false)
  const [loading, setLoading] = useState(true)
  const [busy, setBusy] = useState<Record<string, string>>({})  // id -> action
  const [showAdd, setShowAdd] = useState(false)
  const [form, setForm] = useState({ name: '', source_path: '', repo_path: '', retention_days: '30' })
  const [allTags, setAllTags] = useState<Tag[]>([])
  const [tagMap, setTagMap] = useState<TagMap>({})
  const [popover, setPopover] = useState<string | null>(null)
  const globalTag = useFiltersStore((s) => s.globalTag)

  const load = useCallback(async () => {
    setLoading(true)
    try {
      const data = await apiFetch<{ configs: BackupConfig[]; restic_available: boolean }>('/api/backups')
      setConfigs(data.configs)
      setResticAvailable(data.restic_available)
    } catch { notify.error('Failed to load backups') }
    finally { setLoading(false) }
  }, [])

  const loadTags = useCallback(async () => {
    try {
      const [tags, map] = await Promise.all([api.tags.list(), api.tags.map('backup')])
      setAllTags(tags)
      setTagMap(map)
    } catch { /* empty */ }
  }, [])

  useEffect(() => { load(); loadTags() }, [load, loadTags])

  const action = async (id: string, label: string, path: string) => {
    setBusy(b => ({ ...b, [id]: label }))
    try {
      const r = await apiFetch<{ status: string; message?: string }>(path, { method: 'POST' })
      if (r.status === 'ok' || r.status === 'success') {
        notify.success(`${label}: ${r.status}`)
      } else {
        notify.error(`${label} failed${r.message ? ': ' + r.message : ''}`)
      }
      await load()
    } catch (e: any) { notify.error(e.message ?? `${label} failed`) }
    finally { setBusy(b => { const n = { ...b }; delete n[id]; return n }) }
  }

  const submit = async () => {
    try {
      await apiFetch('/api/backups', { method: 'POST', body: JSON.stringify({ ...form, retention_days: Number(form.retention_days) }) })
      notify.success('Backup config created')
      setShowAdd(false)
      setForm({ name: '', source_path: '', repo_path: '', retention_days: '30' })
      await load()
    } catch (e: any) { notify.error(e.message ?? 'Failed to create') }
  }

  const del = async (id: string, name: string) => {
    if (!confirm(`Delete backup config "${name}"?`)) return
    try {
      await apiFetch(`/api/backups/${id}`, { method: 'DELETE' })
      notify.success('Deleted')
      await load()
    } catch (e: any) { notify.error(e.message ?? 'Failed to delete') }
  }

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between flex-wrap gap-2">
        <div>
          <h1 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>Backups</h1>
          {!resticAvailable && <p className="text-xs mt-0.5" style={{ color: 'var(--accent-warning)' }}>restic not found — install it to enable backups</p>}
        </div>
        <div className="flex gap-2">
          <Button size="sm" onClick={load} loading={loading}>Refresh</Button>
          <Button size="sm" onClick={() => setShowAdd(s => !s)}>Add Backup</Button>
        </div>
      </div>

      {/* Confidence summary cards */}
      {configs.length > 0 && (
        <div className="flex gap-3 flex-wrap">
          {(['high','medium','low','critical'] as Confidence[]).map(c => {
            const count = configs.filter(cfg => cfg.confidence === c).length
            if (!count) return null
            const cfg = CONFIDENCE[c]
            const Icon = cfg.icon
            return (
              <div key={c} className="flex items-center gap-2 rounded px-3 py-2 text-sm" style={{ background: 'var(--bg-panel)', border: '1px solid var(--border-subtle)' }}>
                <Icon size={14} style={{ color: cfg.color }} />
                <span className="font-medium tabular-nums" style={{ color: cfg.color }}>{count}</span>
                <span className="text-xs" style={{ color: 'var(--text-muted)' }}>{cfg.label}</span>
              </div>
            )
          })}
        </div>
      )}

      {showAdd && (
        <div className="card space-y-3">
          <div className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>New Backup Config</div>
          {[
            { key: 'name', label: 'Name', placeholder: 'e.g. home-backup' },
            { key: 'source_path', label: 'Source Path', placeholder: '/home/user' },
            { key: 'repo_path', label: 'Repo Path', placeholder: '/mnt/backup/repo' },
            { key: 'retention_days', label: 'Retention (days)', placeholder: '30' },
          ].map(({ key, label, placeholder }) => (
            <div key={key}>
              <label className="block text-xs mb-1" style={{ color: 'var(--text-muted)' }}>{label}</label>
              <input
                value={form[key as keyof typeof form]}
                onChange={e => setForm(f => ({ ...f, [key]: e.target.value }))}
                placeholder={placeholder}
                className="w-full px-3 py-1.5 rounded text-sm outline-none"
                style={{ background: 'var(--bg-surface)', border: '1px solid var(--border-subtle)', color: 'var(--text-primary)' }}
              />
            </div>
          ))}
          <div className="flex gap-2">
            <Button size="sm" onClick={submit}>Save</Button>
            <Button size="sm" variant="ghost" onClick={() => setShowAdd(false)}>Cancel</Button>
          </div>
        </div>
      )}

      <div className="panel overflow-hidden">
        <div className="overflow-x-auto">
        <table className="w-full text-xs">
          <thead>
            <tr style={{ borderBottom: '1px solid var(--border-subtle)' }}>
              {['Name', 'Source', 'Last Backup', 'Check', 'Restore Test', 'Confidence', 'Actions'].map(h => (
                <th key={h} className="px-4 py-2.5 text-left uppercase tracking-wider" style={{ color: 'var(--text-muted)' }}>{h}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {(globalTag ? configs.filter(c => (tagMap[c.id] || []).some(t => t.id === globalTag)) : configs).map(c => (
              <tr key={c.id} style={{ borderBottom: '1px solid var(--border-subtle)' }}>
                <td className="px-4 py-3 font-medium" style={{ color: 'var(--text-primary)' }}>
                  <div>{c.name}</div>
                  <div style={{ display: 'flex', gap: 4, flexWrap: 'wrap', marginTop: 3, alignItems: 'center', position: 'relative' }}>
                    {(tagMap[c.id] || []).map(t => <TagPill key={t.id} tag={t} />)}
                    <button onClick={() => setPopover(popover === c.id ? null : c.id)} style={{
                      background: 'none', border: '1px dashed var(--border-subtle)', borderRadius: 10,
                      cursor: 'pointer', color: 'var(--text-muted)', fontSize: 11, padding: '0px 6px', lineHeight: '18px',
                    }}><TagIcon size={10} style={{ display: 'inline', verticalAlign: 'middle' }} /></button>
                    {popover === c.id && (
                      <TagPopover resourceType="backup" resourceId={c.id} allTags={allTags} assigned={tagMap[c.id] || []} onClose={() => { setPopover(null); loadTags() }} />
                    )}
                  </div>
                </td>
                <td className="px-4 py-3 font-mono" style={{ color: 'var(--text-muted)', maxWidth: '12rem', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{c.source_path}</td>
                <td className="px-4 py-3">
                  <div style={{ color: c.last_status === 'success' ? 'var(--accent-success)' : c.last_status === 'failed' ? 'var(--accent-error)' : 'var(--text-muted)' }}>
                    {fmtDate(c.last_run_at)}
                  </div>
                  {c.last_status && <div className="mt-0.5" style={{ color: 'var(--text-muted)' }}>{c.last_status}</div>}
                </td>
                <td className="px-4 py-3">
                  <div>{fmtDate(c.last_check_at)}</div>
                  <div className="mt-0.5"><StatusDot status={c.last_check_status} /></div>
                </td>
                <td className="px-4 py-3">
                  <div>{fmtDate(c.last_restore_test_at)}</div>
                  <div className="mt-0.5"><StatusDot status={c.last_restore_test_status} /></div>
                </td>
                <td className="px-4 py-3"><ConfidenceBadge c={c.confidence} /></td>
                <td className="px-4 py-3">
                  <div className="flex items-center gap-1.5 flex-wrap">
                    <button
                      onClick={() => action(c.id, 'Backup', `/api/backups/${c.id}/run`)}
                      disabled={!resticAvailable || !!busy[c.id]}
                      title="Run backup"
                      className="flex items-center gap-1 px-2 py-1 rounded text-xs disabled:opacity-40 hover:opacity-80"
                      style={{ background: 'var(--bg-elevated)', color: 'var(--text-secondary)', border: '1px solid var(--border-subtle)' }}>
                      <Play size={10} />{busy[c.id] === 'Backup' ? '…' : 'Run'}
                    </button>
                    <button
                      onClick={() => action(c.id, 'Check', `/api/backups/${c.id}/check`)}
                      disabled={!resticAvailable || !!busy[c.id]}
                      title="Verify repository integrity"
                      className="flex items-center gap-1 px-2 py-1 rounded text-xs disabled:opacity-40 hover:opacity-80"
                      style={{ background: 'var(--bg-elevated)', color: 'var(--text-secondary)', border: '1px solid var(--border-subtle)' }}>
                      <Search size={10} />{busy[c.id] === 'Check' ? '…' : 'Check'}
                    </button>
                    <button
                      onClick={() => action(c.id, 'Restore test', `/api/backups/${c.id}/restore-test`)}
                      disabled={!resticAvailable || !!busy[c.id]}
                      title="Verify data can be restored"
                      className="flex items-center gap-1 px-2 py-1 rounded text-xs disabled:opacity-40 hover:opacity-80"
                      style={{ background: 'var(--bg-elevated)', color: 'var(--text-secondary)', border: '1px solid var(--border-subtle)' }}>
                      <Undo2 size={10} />{busy[c.id] === 'Restore test' ? '…' : 'Test'}
                    </button>
                    <button
                      onClick={() => del(c.id, c.name)}
                      title="Delete config"
                      className="p-1 rounded hover:opacity-80"
                      style={{ color: 'var(--accent-error)' }}>
                      <Trash2 size={12} />
                    </button>
                  </div>
                </td>
              </tr>
            ))}
            {!loading && configs.length === 0 && (
              <tr><td colSpan={7} className="px-4 py-10 text-center" style={{ color: 'var(--text-muted)' }}>No backup configs. Add one to get started.</td></tr>
            )}
          </tbody>
        </table>
        </div>
      </div>
    </div>
  )
}
