import { useState, useEffect } from 'react'
import {
  GitBranch, GitMerge, RotateCcw, RefreshCw, ChevronDown, ChevronUp,
  CheckCircle, Loader2, AlertTriangle, Puzzle, ExternalLink,
} from 'lucide-react'
import { api } from '@/api/client'

// ─── Types ────────────────────────────────────────────────────────────────────

interface ModConfig { url: string; branch: string }
interface ModCommit { hash: string; subject: string; author: string; date: string }
interface ChangedFile { path: string; status: 'added' | 'modified' | 'deleted' | 'renamed' }

interface ModStatus {
  config: ModConfig | null
  applied: boolean
  applied_at: number | null
  rollback_ref: string | null
  is_git_install: boolean
}

interface ModFetchResult {
  mod_name: string
  branch: string
  commits: ModCommit[]
  changed_files: ChangedFile[]
  diff_preview: string
  commits_ahead: number
}

// ─── Shared UI ────────────────────────────────────────────────────────────────

const card: React.CSSProperties = {
  background: 'var(--bg-panel)', border: '1px solid var(--border-subtle)',
  borderRadius: 10, padding: '20px 24px', marginBottom: 16,
}

function SectionHeader({ icon: Icon, title }: { icon: React.ElementType; title: string }) {
  return (
    <div className="flex items-center gap-2 mb-4">
      <Icon size={16} style={{ color: 'var(--accent-primary)' }} />
      <h2 className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>{title}</h2>
    </div>
  )
}

function Btn({ onClick, disabled, loading, variant = 'primary', children }: {
  onClick: () => void; disabled?: boolean; loading?: boolean
  variant?: 'primary' | 'secondary' | 'danger'; children: React.ReactNode
}) {
  const bg = variant === 'primary' ? 'var(--accent-primary)' : variant === 'danger' ? 'var(--accent-danger)' : 'var(--bg-elevated)'
  const color = variant === 'secondary' ? 'var(--text-primary)' : '#fff'
  const border = variant === 'secondary' ? '1px solid var(--border-default)' : undefined
  return (
    <button onClick={onClick} disabled={disabled || loading}
      className="flex items-center gap-1.5 px-3 py-1.5 rounded text-xs font-medium transition-opacity hover:opacity-80 disabled:opacity-40"
      style={{ background: bg, color, border }}>
      {loading && <Loader2 size={12} className="animate-spin" />}
      {children}
    </button>
  )
}

function Notice({ type, children }: { type: 'info' | 'warn' | 'error'; children: React.ReactNode }) {
  const color = type === 'error' ? 'var(--accent-danger)' : type === 'warn' ? '#f59e0b' : 'var(--accent-primary)'
  return (
    <div className="flex items-start gap-2 p-3 rounded text-xs" style={{ background: `${color}18`, border: `1px solid ${color}44`, color: 'var(--text-primary)' }}>
      <AlertTriangle size={13} style={{ color, flexShrink: 0, marginTop: 1 }} />
      <span>{children}</span>
    </div>
  )
}

function FileStatusBadge({ status }: { status: string }) {
  const colors: Record<string, string> = {
    added: '#22c55e', modified: '#f59e0b', deleted: '#ef4444', renamed: '#8b5cf6',
  }
  const c = colors[status] ?? '#6b7280'
  return (
    <span className="px-1.5 py-0.5 rounded text-xs font-mono" style={{ background: `${c}22`, color: c, border: `1px solid ${c}44` }}>
      {status[0].toUpperCase()}
    </span>
  )
}

function timeAgo(epochSec: number) {
  const diff = Math.floor(Date.now() / 1000 - epochSec)
  if (diff < 60) return 'just now'
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`
  return `${Math.floor(diff / 86400)}d ago`
}

// ─── Configure panel ─────────────────────────────────────────────────────────

function ConfigurePanel({
  config, onSaved,
}: { config: ModConfig | null; onSaved: (c: ModConfig) => void }) {
  const [url, setUrl] = useState(config?.url ?? '')
  const [branch, setBranch] = useState(config?.branch ?? 'main')
  const [saving, setSaving] = useState(false)
  const [err, setErr] = useState<string | null>(null)

  async function save() {
    setSaving(true); setErr(null)
    try {
      await api.mods.saveConfig({ url: url.trim(), branch: branch.trim() })
      onSaved({ url: url.trim(), branch: branch.trim() })
    } catch (e: any) { setErr(e.message) }
    finally { setSaving(false) }
  }

  return (
    <div style={card}>
      <SectionHeader icon={GitBranch} title="Mod source" />
      <div className="flex flex-col gap-3">
        <div>
          <label className="block text-xs mb-1" style={{ color: 'var(--text-muted)' }}>Fork URL</label>
          <input
            value={url} onChange={e => setUrl(e.target.value)}
            placeholder="https://github.com/someone/voidtower"
            className="w-full px-3 py-2 rounded text-sm"
            style={{ background: 'var(--bg-input)', border: '1px solid var(--border-default)', color: 'var(--text-primary)' }}
          />
        </div>
        <div>
          <label className="block text-xs mb-1" style={{ color: 'var(--text-muted)' }}>Branch</label>
          <input
            value={branch} onChange={e => setBranch(e.target.value)}
            placeholder="main"
            className="w-full px-3 py-2 rounded text-sm"
            style={{ background: 'var(--bg-input)', border: '1px solid var(--border-default)', color: 'var(--text-primary)' }}
          />
        </div>
        {err && <Notice type="error">{err}</Notice>}
        <div className="flex justify-end">
          <Btn onClick={save} loading={saving} disabled={!url.trim() || !branch.trim()}>
            Save
          </Btn>
        </div>
      </div>
    </div>
  )
}

// ─── Review panel ────────────────────────────────────────────────────────────

function ReviewPanel({
  result, onApply,
}: { result: ModFetchResult; onApply: () => Promise<void> }) {
  const [filesOpen, setFilesOpen] = useState(false)
  const [diffOpen, setDiffOpen] = useState(false)
  const [applying, setApplying] = useState(false)
  const [err, setErr] = useState<string | null>(null)
  const [confirmed, setConfirmed] = useState(false)

  async function apply() {
    setApplying(true); setErr(null)
    try { await onApply() }
    catch (e: any) { setErr(e.message) }
    finally { setApplying(false) }
  }

  return (
    <div style={card}>
      <SectionHeader icon={GitMerge} title="Review mod" />

      {/* Header row */}
      <div className="flex items-center gap-3 mb-4 p-3 rounded" style={{ background: 'var(--bg-elevated)' }}>
        <Puzzle size={20} style={{ color: 'var(--accent-primary)', flexShrink: 0 }} />
        <div className="flex-1 min-w-0">
          <div className="text-sm font-semibold truncate" style={{ color: 'var(--text-primary)' }}>
            {result.mod_name}
          </div>
          <div className="text-xs" style={{ color: 'var(--text-muted)' }}>
            branch: {result.branch}
          </div>
        </div>
        <div className="text-right shrink-0">
          <div className="text-xs font-medium" style={{ color: 'var(--accent-primary)' }}>
            {result.commits_ahead} commit{result.commits_ahead !== 1 ? 's' : ''} ahead
          </div>
          <div className="text-xs" style={{ color: 'var(--text-muted)' }}>
            {result.changed_files.length} file{result.changed_files.length !== 1 ? 's' : ''} changed
          </div>
        </div>
      </div>

      {/* Commits */}
      {result.commits.length > 0 && (
        <div className="mb-4">
          <div className="text-xs font-medium mb-2" style={{ color: 'var(--text-muted)' }}>COMMITS</div>
          <div className="flex flex-col gap-1">
            {result.commits.map(c => (
              <div key={c.hash} className="flex items-start gap-2 p-2 rounded text-xs" style={{ background: 'var(--bg-elevated)' }}>
                <code className="shrink-0 mt-0.5" style={{ color: 'var(--accent-primary)' }}>{c.hash}</code>
                <span className="flex-1" style={{ color: 'var(--text-primary)' }}>{c.subject}</span>
                <span className="shrink-0" style={{ color: 'var(--text-muted)' }}>{c.author}</span>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Changed files */}
      <div className="mb-4">
        <button
          onClick={() => setFilesOpen(v => !v)}
          className="flex items-center gap-1 text-xs font-medium mb-2"
          style={{ color: 'var(--text-muted)' }}
        >
          {filesOpen ? <ChevronUp size={12} /> : <ChevronDown size={12} />}
          CHANGED FILES ({result.changed_files.length})
        </button>
        {filesOpen && (
          <div className="flex flex-col gap-1">
            {result.changed_files.map((f, i) => (
              <div key={i} className="flex items-center gap-2 p-1.5 rounded text-xs" style={{ background: 'var(--bg-elevated)' }}>
                <FileStatusBadge status={f.status} />
                <code style={{ color: 'var(--text-primary)' }}>{f.path}</code>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Diff preview */}
      {result.diff_preview && (
        <div className="mb-4">
          <button
            onClick={() => setDiffOpen(v => !v)}
            className="flex items-center gap-1 text-xs font-medium mb-2"
            style={{ color: 'var(--text-muted)' }}
          >
            {diffOpen ? <ChevronUp size={12} /> : <ChevronDown size={12} />}
            DIFF PREVIEW
          </button>
          {diffOpen && (
            <pre className="text-xs p-3 rounded overflow-x-auto" style={{
              background: 'var(--bg-code, #0d1117)', color: 'var(--text-code, #e6edf3)',
              border: '1px solid var(--border-subtle)', maxHeight: 400, overflowY: 'auto',
            }}>
              {result.diff_preview.split('\n').map((line, i) => {
                const color = line.startsWith('+') && !line.startsWith('+++') ? '#3fb950'
                  : line.startsWith('-') && !line.startsWith('---') ? '#f85149'
                  : line.startsWith('@@') ? '#79c0ff'
                  : undefined
                return <span key={i} style={color ? { color } : {}}>{line}{'\n'}</span>
              })}
            </pre>
          )}
        </div>
      )}

      {err && <div className="mb-3"><Notice type="error">{err}</Notice></div>}

      <div className="flex items-center justify-between pt-2" style={{ borderTop: '1px solid var(--border-subtle)' }}>
        <label className="flex items-center gap-2 text-xs cursor-pointer" style={{ color: 'var(--text-muted)' }}>
          <input type="checkbox" checked={confirmed} onChange={e => setConfirmed(e.target.checked)} />
          I've reviewed the changes and want to apply this mod
        </label>
        <Btn onClick={apply} loading={applying} disabled={!confirmed} variant="primary">
          <GitMerge size={13} /> Apply mod
        </Btn>
      </div>
    </div>
  )
}

// ─── Applied panel ───────────────────────────────────────────────────────────

function AppliedPanel({
  status, onRollback,
}: { status: ModStatus; onRollback: () => Promise<void> }) {
  const [rolling, setRolling] = useState(false)
  const [err, setErr] = useState<string | null>(null)

  async function rollback() {
    if (!confirm('Roll back to the pre-mod state? This will reset the git tree to the saved rollback point.')) return
    setRolling(true); setErr(null)
    try { await onRollback() }
    catch (e: any) { setErr(e.message) }
    finally { setRolling(false) }
  }

  return (
    <div style={card}>
      <SectionHeader icon={CheckCircle} title="Applied mod" />
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <CheckCircle size={16} style={{ color: '#22c55e' }} />
          <div>
            <div className="text-sm" style={{ color: 'var(--text-primary)' }}>
              {status.config?.url.split('/').slice(-2).join('/')}
            </div>
            <div className="text-xs" style={{ color: 'var(--text-muted)' }}>
              branch: {status.config?.branch}
              {status.applied_at && ` · applied ${timeAgo(status.applied_at)}`}
            </div>
          </div>
        </div>
        <Btn onClick={rollback} loading={rolling} variant="danger">
          <RotateCcw size={13} /> Rollback
        </Btn>
      </div>
      {err && <div className="mt-3"><Notice type="error">{err}</Notice></div>}
    </div>
  )
}

// ─── Page ─────────────────────────────────────────────────────────────────────

export default function ModsPage() {
  const [status, setStatus] = useState<ModStatus | null>(null)
  const [fetchResult, setFetchResult] = useState<ModFetchResult | null>(null)
  const [fetching, setFetching] = useState(false)
  const [fetchErr, setFetchErr] = useState<string | null>(null)

  useEffect(() => { loadStatus() }, [])

  async function loadStatus() {
    try { setStatus(await api.mods.getStatus()) } catch { /* ignore */ }
  }

  async function handleFetch() {
    setFetching(true); setFetchErr(null); setFetchResult(null)
    try { setFetchResult(await api.mods.fetch()) }
    catch (e: any) { setFetchErr(e.message) }
    finally { setFetching(false) }
  }

  async function handleApply() {
    await api.mods.apply()
    await loadStatus()
    setFetchResult(null)
  }

  async function handleRollback() {
    await api.mods.rollback()
    await loadStatus()
  }

  if (!status) {
    return (
      <div className="p-6 flex items-center gap-2 text-sm" style={{ color: 'var(--text-muted)' }}>
        <Loader2 size={16} className="animate-spin" /> Loading…
      </div>
    )
  }

  return (
    <div className="p-6 max-w-3xl">
      <div className="flex items-center gap-2 mb-6">
        <Puzzle size={20} style={{ color: 'var(--accent-primary)' }} />
        <h1 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>Mods</h1>
      </div>

      {!status.is_git_install && (
        <div className="mb-4">
          <Notice type="warn">
            Mods require a git-based VoidTower installation. Docker installs can switch images via the Updates page instead.
          </Notice>
        </div>
      )}

      {status.is_git_install && (
        <>
          {/* Applied banner */}
          {status.applied && (
            <AppliedPanel status={status} onRollback={handleRollback} />
          )}

          {/* Configure */}
          <ConfigurePanel
            config={status.config}
            onSaved={async (c) => {
              setStatus(s => s ? { ...s, config: c } : s)
              setFetchResult(null)
            }}
          />

          {/* Fetch button */}
          {status.config && !fetchResult && (
            <div style={card}>
              <SectionHeader icon={RefreshCw} title="Review mod" />
              <p className="text-xs mb-4" style={{ color: 'var(--text-muted)' }}>
                Fetch <strong>{status.config.url.split('/').slice(-2).join('/')}</strong> @{' '}
                <code>{status.config.branch}</code> and compare it with your running instance.
              </p>
              {fetchErr && <div className="mb-3"><Notice type="error">{fetchErr}</Notice></div>}
              <Btn onClick={handleFetch} loading={fetching}>
                <RefreshCw size={13} /> Fetch &amp; review
              </Btn>
            </div>
          )}

          {/* Review */}
          {fetchResult && (
            fetchResult.commits_ahead === 0 ? (
              <div style={card}>
                <div className="flex items-center gap-2 text-sm" style={{ color: '#22c55e' }}>
                  <CheckCircle size={16} /> Already up to date with this mod source.
                </div>
              </div>
            ) : (
              <ReviewPanel result={fetchResult} onApply={handleApply} />
            )
          )}
        </>
      )}

      <div className="mt-4 text-xs" style={{ color: 'var(--text-muted)' }}>
        Mods are community forks of VoidTower — review all changes before applying.{' '}
        <a href="https://github.com/niwlekakan/voidtower" target="_blank" rel="noopener noreferrer"
          className="inline-flex items-center gap-1 hover:underline" style={{ color: 'var(--accent-primary)' }}>
          Browse forks on GitHub <ExternalLink size={11} />
        </a>
      </div>
    </div>
  )
}
