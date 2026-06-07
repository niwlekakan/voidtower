import { useEffect, useState, useCallback } from 'react'
import { Plus, Play, Trash2, ChevronDown, ChevronRight, X, ToggleLeft, ToggleRight, Clock, Tag as TagIcon } from 'lucide-react'
import { api } from '@/api/client'
import type { Tag, TagMap } from '@/api/types'
import { notify } from '@/store/notifications'
import { useFiltersStore } from '@/store/filters'
import { TagPill, TagPopover } from '@/components/ui/TagPill'

async function apiFetch<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(path, { ...init, credentials: 'include', headers: { 'Content-Type': 'application/json', ...init?.headers } })
  if (!res.ok) throw new Error((await res.json().catch(() => ({}))).error?.message ?? res.statusText)
  return res.json()
}

interface Job {
  id: string; name: string; description: string | null; command: string
  schedule: string | null; enabled: boolean; timeout_secs: number
  last_run_at: number | null; last_status: string | null; last_exit_code: number | null
}
interface Run {
  id: string; started_at: number; finished_at: number | null
  status: string; exit_code: number | null; output: string
}

const STATUS_COLOR: Record<string, string> = {
  success: 'var(--accent-success)', failed: 'var(--accent-error)',
  timeout: 'var(--accent-warning)', running: 'var(--accent-secondary)',
}

const SCHEDULE_HINTS = ['@hourly', '@daily', '@weekly', '@monthly', '*/5', '*/15', '*/30']

function fmtDate(ts: number | null) {
  if (!ts) return '—'
  const d = Math.floor(Date.now() / 1000) - ts
  if (d < 60) return `${d}s ago`
  if (d < 3600) return `${Math.floor(d/60)}m ago`
  if (d < 86400) return `${Math.floor(d/3600)}h ago`
  return `${Math.floor(d/86400)}d ago`
}

function JobModal({ job, onClose, onSaved }: { job?: Job; onClose: () => void; onSaved: () => void }) {
  const [name, setName] = useState(job?.name ?? '')
  const [desc, setDesc] = useState(job?.description ?? '')
  const [cmd, setCmd] = useState(job?.command ?? '')
  const [sched, setSched] = useState(job?.schedule ?? '')
  const [timeout, setTimeout_] = useState(String(job?.timeout_secs ?? 300))
  const [busy, setBusy] = useState(false)

  const submit = async () => {
    if (!name.trim() || !cmd.trim()) return
    setBusy(true)
    try {
      const body = { name, description: desc || undefined, command: cmd, schedule: sched || undefined, timeout_secs: Number(timeout) }
      if (job) {
        await apiFetch(`/api/automation/${job.id}`, { method: 'PATCH', body: JSON.stringify(body) })
      } else {
        await apiFetch('/api/automation', { method: 'POST', body: JSON.stringify(body) })
      }
      notify.success(job ? 'Job updated' : 'Job created')
      onSaved(); onClose()
    } catch (e: any) { notify.error(e.message) }
    finally { setBusy(false) }
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4" style={{ background: 'rgba(0,0,0,0.6)' }}>
      <div className="w-full max-w-lg rounded-lg p-5 space-y-4" style={{ background: 'var(--bg-panel)', border: '1px solid var(--border-subtle)' }}>
        <div className="flex items-center justify-between">
          <h2 className="font-semibold text-sm" style={{ color: 'var(--text-primary)' }}>{job ? 'Edit Job' : 'New Automation Job'}</h2>
          <button onClick={onClose}><X size={15} style={{ color: 'var(--text-muted)' }} /></button>
        </div>

        <div className="space-y-3">
          {([
            { label: 'Name', value: name, set: setName, placeholder: 'e.g. nightly-cleanup' },
            { label: 'Description', value: desc, set: setDesc, placeholder: 'What does this job do?' },
          ] as const).map(({ label, value, set, placeholder }) => (
            <div key={label} className="space-y-1">
              <label className="text-xs" style={{ color: 'var(--text-muted)' }}>{label}</label>
              <input value={value} onChange={e => set(e.target.value)} placeholder={placeholder}
                className="w-full rounded px-3 py-2 text-sm outline-none"
                style={{ background: 'var(--bg-surface)', border: '1px solid var(--border-subtle)', color: 'var(--text-primary)' }} />
            </div>
          ))}

          <div className="space-y-1">
            <label className="text-xs" style={{ color: 'var(--text-muted)' }}>Command (runs via <code>sh -c</code>)</label>
            <textarea value={cmd} onChange={e => setCmd(e.target.value)} rows={3} placeholder="docker system prune -f"
              className="w-full rounded px-3 py-2 text-sm font-mono outline-none resize-none"
              style={{ background: 'var(--bg-surface)', border: '1px solid var(--border-subtle)', color: 'var(--text-primary)' }} />
          </div>

          <div className="grid grid-cols-2 gap-3">
            <div className="space-y-1">
              <label className="text-xs" style={{ color: 'var(--text-muted)' }}>Schedule (optional)</label>
              <input value={sched} onChange={e => setSched(e.target.value)} placeholder="@daily"
                className="w-full rounded px-3 py-2 text-sm font-mono outline-none"
                style={{ background: 'var(--bg-surface)', border: '1px solid var(--border-subtle)', color: 'var(--text-primary)' }} />
              <div className="flex flex-wrap gap-1 pt-0.5">
                {SCHEDULE_HINTS.map(h => (
                  <button key={h} onClick={() => setSched(h)} className="text-xs px-1.5 py-0.5 rounded hover:opacity-80"
                    style={{ background: sched === h ? 'var(--accent-primary-subtle)' : 'var(--bg-elevated)',
                             color: sched === h ? 'var(--accent-primary)' : 'var(--text-muted)',
                             border: '1px solid var(--border-subtle)' }}>{h}</button>
                ))}
              </div>
            </div>
            <div className="space-y-1">
              <label className="text-xs" style={{ color: 'var(--text-muted)' }}>Timeout (seconds)</label>
              <input value={timeout} onChange={e => setTimeout_(e.target.value)} type="number" min="1"
                className="w-full rounded px-3 py-2 text-sm font-mono outline-none"
                style={{ background: 'var(--bg-surface)', border: '1px solid var(--border-subtle)', color: 'var(--text-primary)' }} />
            </div>
          </div>
        </div>

        <div className="flex gap-2 justify-end pt-1">
          <button onClick={onClose} className="px-3 py-1.5 rounded text-sm" style={{ background: 'var(--bg-surface)', color: 'var(--text-secondary)', border: '1px solid var(--border-subtle)' }}>Cancel</button>
          <button onClick={submit} disabled={busy || !name.trim() || !cmd.trim()} className="px-3 py-1.5 rounded text-sm disabled:opacity-50" style={{ background: 'var(--accent-primary)', color: '#fff' }}>
            {busy ? 'Saving…' : job ? 'Update' : 'Create'}
          </button>
        </div>
      </div>
    </div>
  )
}

function RunHistory({ jobId }: { jobId: string }) {
  const [runs, setRuns] = useState<Run[] | null>(null)
  useEffect(() => {
    apiFetch<{ runs: Run[] }>(`/api/automation/${jobId}/runs`).then(r => setRuns(r.runs)).catch(() => {})
  }, [jobId])
  if (!runs) return <div className="px-4 py-3 text-xs" style={{ color: 'var(--text-muted)' }}>Loading…</div>
  if (!runs.length) return <div className="px-4 py-3 text-xs" style={{ color: 'var(--text-muted)' }}>No runs yet.</div>
  return (
    <div className="space-y-1 px-4 pb-3">
      {runs.map(r => (
        <div key={r.id} className="rounded p-2 text-xs" style={{ background: 'var(--bg-surface)', border: '1px solid var(--border-subtle)' }}>
          <div className="flex items-center gap-3 flex-wrap">
            <span className="font-medium" style={{ color: STATUS_COLOR[r.status] ?? 'var(--text-secondary)' }}>{r.status}</span>
            <span style={{ color: 'var(--text-muted)' }}>{fmtDate(r.started_at)}</span>
            {r.finished_at && <span style={{ color: 'var(--text-muted)' }}>{r.finished_at - r.started_at}s</span>}
            {r.exit_code != null && <span className="font-mono" style={{ color: 'var(--text-muted)' }}>exit {r.exit_code}</span>}
          </div>
          {r.output && (
            <pre className="mt-1.5 whitespace-pre-wrap font-mono text-xs max-h-32 overflow-y-auto" style={{ color: 'var(--text-secondary)' }}>
              {r.output.slice(0, 2000)}{r.output.length > 2000 ? '\n…truncated' : ''}
            </pre>
          )}
        </div>
      ))}
    </div>
  )
}

function JobRow({ job, allTags, assigned, onRefresh, onTagsChanged }: {
  job: Job; allTags: Tag[]; assigned: Tag[]; onRefresh: () => void; onTagsChanged: () => void
}) {
  const [expanded, setExpanded] = useState(false)
  const [running, setRunning] = useState(false)
  const [editing, setEditing] = useState(false)
  const [popover, setPopover] = useState(false)

  const runNow = async () => {
    setRunning(true)
    try {
      const r = await apiFetch<{ status: string }>(`/api/automation/${job.id}/run`, { method: 'POST' })
      notify.success(`Run ${r.status}`)
      onRefresh()
    } catch (e: any) { notify.error(e.message) }
    finally { setRunning(false) }
  }

  const toggle = async () => {
    try {
      await apiFetch(`/api/automation/${job.id}`, { method: 'PATCH', body: JSON.stringify({ enabled: !job.enabled }) })
      onRefresh()
    } catch (e: any) { notify.error(e.message) }
  }

  const del = async () => {
    if (!confirm(`Delete job "${job.name}"?`)) return
    try {
      await apiFetch(`/api/automation/${job.id}`, { method: 'DELETE' })
      notify.success('Deleted')
      onRefresh()
    } catch (e: any) { notify.error(e.message) }
  }

  return (
    <>
      <tr style={{ borderBottom: expanded ? 'none' : '1px solid var(--border-subtle)', opacity: job.enabled ? 1 : 0.6 }}>
        <td className="px-4 py-3">
          <button onClick={() => setExpanded(e => !e)} className="flex items-center gap-1" style={{ color: 'var(--text-muted)' }}>
            {expanded ? <ChevronDown size={13} /> : <ChevronRight size={13} />}
          </button>
        </td>
        <td className="px-4 py-3">
          <div className="font-medium text-sm cursor-pointer" onClick={() => setEditing(true)} style={{ color: 'var(--text-primary)' }}>{job.name}</div>
          {job.description && <div className="text-xs mt-0.5" style={{ color: 'var(--text-muted)' }}>{job.description}</div>}
          <div style={{ display: 'flex', gap: 4, flexWrap: 'wrap', marginTop: 3, alignItems: 'center', position: 'relative' }}>
            {assigned.map(t => <TagPill key={t.id} tag={t} />)}
            <button onClick={() => setPopover(p => !p)} style={{
              background: 'none', border: '1px dashed var(--border-subtle)', borderRadius: 10,
              cursor: 'pointer', color: 'var(--text-muted)', fontSize: 11, padding: '0px 6px', lineHeight: '18px',
            }}><TagIcon size={10} style={{ display: 'inline', verticalAlign: 'middle' }} /></button>
            {popover && (
              <TagPopover resourceType="automation" resourceId={job.id} allTags={allTags} assigned={assigned} onClose={() => { setPopover(false); onTagsChanged() }} />
            )}
          </div>
        </td>
        <td className="px-4 py-3 font-mono text-xs max-w-[16rem] truncate" style={{ color: 'var(--text-secondary)' }} title={job.command}>{job.command}</td>
        <td className="px-4 py-3 text-xs">
          {job.schedule ? (
            <span className="flex items-center gap-1" style={{ color: 'var(--accent-secondary)' }}>
              <Clock size={11} />{job.schedule}
            </span>
          ) : <span style={{ color: 'var(--text-disabled)' }}>manual</span>}
        </td>
        <td className="px-4 py-3 text-xs" style={{ color: 'var(--text-muted)' }}>
          {fmtDate(job.last_run_at)}
          {job.last_status && <span className="ml-1.5" style={{ color: STATUS_COLOR[job.last_status] ?? 'var(--text-muted)' }}>{job.last_status}</span>}
        </td>
        <td className="px-4 py-3">
          <div className="flex items-center gap-1.5">
            <button onClick={runNow} disabled={running} title="Run now" className="flex items-center gap-1 px-2 py-1 rounded text-xs hover:opacity-80 disabled:opacity-40" style={{ background: 'var(--bg-elevated)', color: 'var(--text-secondary)', border: '1px solid var(--border-subtle)' }}>
              <Play size={10} />{running ? '…' : 'Run'}
            </button>
            <button onClick={toggle} title={job.enabled ? 'Disable' : 'Enable'} className="p-1 rounded hover:opacity-80" style={{ color: job.enabled ? 'var(--accent-success)' : 'var(--text-muted)' }}>
              {job.enabled ? <ToggleRight size={16} /> : <ToggleLeft size={16} />}
            </button>
            <button onClick={del} title="Delete" className="p-1 rounded hover:opacity-80" style={{ color: 'var(--accent-error)' }}>
              <Trash2 size={12} />
            </button>
          </div>
        </td>
      </tr>
      {expanded && (
        <tr style={{ borderBottom: '1px solid var(--border-subtle)' }}>
          <td colSpan={6} className="bg-transparent pb-1">
            <RunHistory jobId={job.id} />
          </td>
        </tr>
      )}
      {editing && <JobModal job={job} onClose={() => setEditing(false)} onSaved={onRefresh} />}
    </>
  )
}

export default function AutomationPage() {
  const [jobs, setJobs] = useState<Job[]>([])
  const [loading, setLoading] = useState(true)
  const [adding, setAdding] = useState(false)
  const [allTags, setAllTags] = useState<Tag[]>([])
  const [tagMap, setTagMap] = useState<TagMap>({})
  const globalTag = useFiltersStore((s) => s.globalTag)

  const load = () => {
    setLoading(true)
    apiFetch<{ jobs: Job[] }>('/api/automation').then(r => setJobs(r.jobs)).catch(() => notify.error('Failed to load jobs')).finally(() => setLoading(false))
  }

  const loadTags = useCallback(async () => {
    try {
      const [tags, map] = await Promise.all([api.tags.list(), api.tags.map('automation')])
      setAllTags(tags)
      setTagMap(map)
    } catch { /* empty */ }
  }, [])

  useEffect(() => { load(); loadTags() }, [loadTags])

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>Automation</h1>
          <p className="text-xs mt-0.5" style={{ color: 'var(--text-muted)' }}>Schedule shell commands. Runs via <code>sh -c</code>.</p>
        </div>
        <button onClick={() => setAdding(true)} className="flex items-center gap-1.5 px-3 py-1.5 rounded text-sm" style={{ background: 'var(--accent-primary)', color: '#fff' }}>
          <Plus size={13} /> New Job
        </button>
      </div>

      <div className="panel overflow-hidden">
        <table className="w-full text-xs">
          <thead>
            <tr style={{ borderBottom: '1px solid var(--border-subtle)' }}>
              <th className="px-4 py-2.5 w-8" />
              {['Job', 'Command', 'Schedule', 'Last Run', 'Actions'].map(h => (
                <th key={h} className="px-4 py-2.5 text-left uppercase tracking-wider" style={{ color: 'var(--text-muted)' }}>{h}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {(globalTag ? jobs.filter(j => (tagMap[j.id] || []).some(t => t.id === globalTag)) : jobs).map(j => (
              <JobRow key={j.id} job={j} allTags={allTags} assigned={tagMap[j.id] || []} onRefresh={load} onTagsChanged={loadTags} />
            ))}
            {!loading && jobs.length === 0 && (
              <tr><td colSpan={6} className="px-4 py-10 text-center text-sm" style={{ color: 'var(--text-muted)' }}>No automation jobs. Create one to get started.</td></tr>
            )}
          </tbody>
        </table>
      </div>

      {adding && <JobModal onClose={() => setAdding(false)} onSaved={load} />}
    </div>
  )
}
