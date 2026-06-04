import { useState, useEffect, useCallback, useRef, lazy, Suspense } from 'react'
import {
  Folder, File, FileText, FileImage, ChevronRight,
  Plus, Trash2, Edit3, RefreshCw, Save, X, Home,
  HardDrive, AlertTriangle, Star, Clock, User, Bot, Download, FilePlus,
} from 'lucide-react'
import { ApiClientError } from '@/api/client'
import { notify } from '@/store/notifications'
import Button from '@/components/ui/Button'

const MonacoEditor = lazy(() => import('@monaco-editor/react'))

// ── types ─────────────────────────────────────────────────────────────────────

interface Entry { name: string; path: string; is_dir: boolean; size: number; modified: number; permissions: string; is_symlink: boolean }
interface Root  { label: string; path: string }
interface Favorite { path: string; label: string; isDir: boolean }
interface ActivityEntry { timestamp: number; action: string; actor_type: string; username: string | null; role: string | null; details: string | null; outcome: string }

// ── constants ─────────────────────────────────────────────────────────────────

const FAV_KEY = 'vt-file-favorites'
const TEXT_EXTS = new Set(['txt','md','log','conf','yaml','yml','toml','json','env','ini','cfg','sh','bash','zsh','fish','py','rs','ts','tsx','js','jsx','css','scss','html','xml','sql','lua','go','c','cpp','h','hpp','rb','php','swift','kt','java','cs','dockerfile','makefile','gitignore'])
const IMAGE_EXTS = new Set(['png','jpg','jpeg','gif','webp','svg','bmp','ico','tiff','avif'])
const PDF_EXTS   = new Set(['pdf'])
const LANG_MAP: Record<string, string> = {
  ts:'typescript', tsx:'typescript', js:'javascript', jsx:'javascript',
  rs:'rust', py:'python', go:'go', sh:'shell', bash:'shell', fish:'shell', zsh:'shell',
  json:'json', yaml:'yaml', yml:'yaml', toml:'toml', md:'markdown',
  html:'html', css:'css', scss:'scss', sql:'sql', xml:'xml',
  lua:'lua', c:'c', cpp:'cpp', h:'c', hpp:'cpp', rb:'ruby',
  php:'php', swift:'swift', kt:'kotlin', java:'java', cs:'csharp',
}

// ── helpers ───────────────────────────────────────────────────────────────────

function loadFavs(): Favorite[] { try { return JSON.parse(localStorage.getItem(FAV_KEY) ?? '[]') } catch { return [] } }
function saveFavs(f: Favorite[]) { localStorage.setItem(FAV_KEY, JSON.stringify(f)) }

async function apiFetch<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(path, { ...init, credentials: 'include', headers: { 'Content-Type': 'application/json', ...init?.headers } })
  if (!res.ok) {
    const body = await res.json().catch(() => ({})) as { error?: { message?: string } }
    throw new ApiClientError(body?.error?.message ?? res.statusText, 'files_error', res.status)
  }
  if (res.status === 204) return undefined as T
  return res.json()
}

function fmtSize(b: number) {
  if (!b) return '—'
  if (b < 1024) return `${b} B`
  if (b < 1024 ** 2) return `${(b / 1024).toFixed(1)} KB`
  if (b < 1024 ** 3) return `${(b / 1024 ** 2).toFixed(1)} MB`
  return `${(b / 1024 ** 3).toFixed(1)} GB`
}
function fmtDate(ts: number) {
  if (!ts) return '—'
  return new Date(ts * 1000).toLocaleString(undefined, { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' })
}
function fmtAction(a: string) { return a.replace('file.', '').replace('_', ' ') }

function ext(name: string) { return name.split('.').pop()?.toLowerCase() ?? '' }
function isText(e: Entry)  { return !e.is_dir && TEXT_EXTS.has(ext(e.name)) }
function isImage(e: Entry) { return !e.is_dir && IMAGE_EXTS.has(ext(e.name)) }
function isPDF(e: Entry)   { return !e.is_dir && PDF_EXTS.has(ext(e.name)) }

function fileIcon(e: Entry) {
  if (e.is_dir) return Folder
  if (isImage(e)) return FileImage
  if (isText(e)) return FileText
  return File
}

function rawUrl(path: string) { return `/api/files/raw?path=${encodeURIComponent(path)}` }

// ── Breadcrumbs ───────────────────────────────────────────────────────────────

function Breadcrumbs({ path, onNav }: { path: string; onNav: (p: string) => void }) {
  const parts = path.split('/').filter(Boolean)
  return (
    <div className="flex items-center gap-1 text-xs flex-wrap" style={{ color: 'var(--text-muted)' }}>
      <button onClick={() => onNav('/')} style={{ color: 'var(--accent-primary)' }}>/</button>
      {parts.map((part, i) => (
        <span key={i} className="flex items-center gap-1">
          <ChevronRight size={11} />
          <button onClick={() => onNav('/' + parts.slice(0, i + 1).join('/'))}
            style={{ color: i === parts.length - 1 ? 'var(--text-primary)' : 'var(--accent-primary)' }}>
            {part}
          </button>
        </span>
      ))}
    </div>
  )
}

// ── Activity panel ────────────────────────────────────────────────────────────

function ActorBadge({ e }: { e: ActivityEntry }) {
  const isAI = e.actor_type === 'service' || e.actor_type === 'agent'
  const Icon = isAI ? Bot : User
  const color = isAI ? 'var(--accent-secondary)' : e.role === 'owner' ? 'var(--accent-primary)' : 'var(--text-secondary)'
  return (
    <span className="flex items-center gap-1 text-xs" style={{ color }}>
      <Icon size={11} />
      {isAI ? 'AI' : (e.username ?? e.actor_type)}
      {e.role && !isAI && <span className="opacity-50">({e.role})</span>}
    </span>
  )
}

function ActivityPanel({ path, onClose }: { path: string; onClose: () => void }) {
  const [entries, setEntries] = useState<ActivityEntry[]>([])
  const [loading, setLoading] = useState(true)
  useEffect(() => {
    setLoading(true)
    apiFetch<{ entries: ActivityEntry[] }>(`/api/files/activity?path=${encodeURIComponent(path)}`)
      .then(r => setEntries(r.entries)).catch(() => notify.error('Failed to load activity')).finally(() => setLoading(false))
  }, [path])
  return (
    <div className="flex flex-col border-l flex-shrink-0 overflow-hidden"
      style={{ width: 272, background: 'var(--bg-panel)', borderColor: 'var(--border-subtle)' }}>
      <div className="flex items-center gap-2 px-3 py-2.5 border-b flex-shrink-0" style={{ borderColor: 'var(--border-subtle)' }}>
        <Clock size={13} style={{ color: 'var(--text-muted)' }} />
        <span className="text-xs font-medium flex-1" style={{ color: 'var(--text-primary)' }}>Activity</span>
        <button onClick={onClose} className="hover:opacity-70" style={{ color: 'var(--text-muted)' }}><X size={14} /></button>
      </div>
      <p className="px-3 py-1.5 text-xs font-mono truncate" style={{ color: 'var(--text-muted)', borderBottom: '1px solid var(--border-subtle)' }}>
        {path.split('/').pop()}
      </p>
      <div className="flex-1 overflow-y-auto">
        {loading && <p className="px-3 py-4 text-xs" style={{ color: 'var(--text-muted)' }}>Loading…</p>}
        {!loading && entries.length === 0 && <p className="px-3 py-4 text-xs" style={{ color: 'var(--text-muted)' }}>No recorded activity yet.</p>}
        {entries.map((e, i) => (
          <div key={i} className="px-3 py-2.5 space-y-1" style={{ borderBottom: '1px solid var(--border-subtle)' }}>
            <div className="flex items-center justify-between gap-2">
              <span className="text-xs px-1.5 py-0.5 rounded font-mono"
                style={{ background: e.action === 'file.delete' ? 'var(--accent-danger-subtle)' : 'var(--accent-primary-subtle)', color: e.action === 'file.delete' ? 'var(--accent-danger)' : 'var(--accent-primary)' }}>
                {fmtAction(e.action)}
              </span>
              {e.outcome !== 'success' && <span className="text-xs" style={{ color: 'var(--accent-danger)' }}>{e.outcome}</span>}
            </div>
            <ActorBadge e={e} />
            <p className="text-xs" style={{ color: 'var(--text-muted)' }}>{fmtDate(e.timestamp)}</p>
            {e.details && <p className="text-xs font-mono truncate" style={{ color: 'var(--text-muted)' }}>{e.details}</p>}
          </div>
        ))}
      </div>
    </div>
  )
}

// ── Monaco file editor ────────────────────────────────────────────────────────

function FileEditor({ path, onClose }: { path: string; onClose: () => void }) {
  const [content, setContent] = useState('')
  const [original, setOriginal] = useState('')
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const [truncated, setTruncated] = useState(false)
  const language = LANG_MAP[ext(path)] ?? 'plaintext'

  useEffect(() => {
    setLoading(true)
    apiFetch<{ content: string; truncated: boolean }>(`/api/files/read?path=${encodeURIComponent(path)}`)
      .then(r => { setContent(r.content); setOriginal(r.content); setTruncated(r.truncated) })
      .catch(() => notify.error('Failed to read file'))
      .finally(() => setLoading(false))
  }, [path])

  const save = async () => {
    setSaving(true)
    try {
      await apiFetch('/api/files/write', { method: 'POST', body: JSON.stringify({ path, content }) })
      setOriginal(content); notify.success('Saved')
    } catch (e) { notify.error(e instanceof ApiClientError ? e.message : 'Save failed') }
    finally { setSaving(false) }
  }

  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center gap-2 px-3 py-2 border-b flex-shrink-0"
        style={{ borderColor: 'var(--border-subtle)', background: 'var(--bg-panel)' }}>
        <span className="text-xs font-mono flex-1 truncate" style={{ color: 'var(--text-secondary)' }}>{path}</span>
        <span className="text-xs px-1.5 py-0.5 rounded font-mono" style={{ background: 'var(--bg-code)', color: 'var(--text-muted)' }}>{language}</span>
        {truncated && <span className="flex items-center gap-1 text-xs" style={{ color: 'var(--accent-warning)' }}><AlertTriangle size={11} /> Truncated</span>}
        <Button size="sm" variant="primary" onClick={save} loading={saving} disabled={content === original || loading}>
          <Save size={12} className="mr-1" /> Save
        </Button>
        <a href={rawUrl(path)} download style={{ display: 'flex', alignItems: 'center', padding: '3px 6px', borderRadius: 4, color: 'var(--text-muted)', textDecoration: 'none' }} title="Download">
          <Download size={13} />
        </a>
        <button onClick={onClose} className="p-1 hover:opacity-70" style={{ color: 'var(--text-muted)' }}><X size={14} /></button>
      </div>
      {loading
        ? <div className="flex-1 flex items-center justify-center text-xs" style={{ color: 'var(--text-muted)' }}>Loading…</div>
        : (
          <Suspense fallback={<div className="flex-1 flex items-center justify-center text-xs" style={{ color: 'var(--text-muted)' }}>Loading editor…</div>}>
            <MonacoEditor
              height="100%"
              language={language}
              value={content}
              onChange={v => setContent(v ?? '')}
              theme="vs-dark"
              options={{
                fontSize: 13,
                fontFamily: 'JetBrains Mono, Fira Code, Cascadia Code, monospace',
                minimap: { enabled: false },
                scrollBeyondLastLine: false,
                wordWrap: 'on',
                lineNumbersMinChars: 3,
                padding: { top: 8, bottom: 8 },
              }}
            />
          </Suspense>
        )}
    </div>
  )
}

// ── Image viewer ──────────────────────────────────────────────────────────────

function ImageViewer({ path, onClose }: { path: string; onClose: () => void }) {
  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center gap-2 px-3 py-2 border-b flex-shrink-0"
        style={{ borderColor: 'var(--border-subtle)', background: 'var(--bg-panel)' }}>
        <FileImage size={13} style={{ color: 'var(--text-muted)' }} />
        <span className="text-xs font-mono flex-1 truncate" style={{ color: 'var(--text-secondary)' }}>{path}</span>
        <a href={rawUrl(path)} download style={{ display: 'flex', alignItems: 'center', padding: '3px 6px', borderRadius: 4, color: 'var(--text-muted)', textDecoration: 'none' }} title="Download">
          <Download size={13} />
        </a>
        <button onClick={onClose} className="p-1 hover:opacity-70" style={{ color: 'var(--text-muted)' }}><X size={14} /></button>
      </div>
      <div className="flex-1 flex items-center justify-center p-6" style={{ background: 'var(--bg-root)' }}>
        <img src={rawUrl(path)} alt={path.split('/').pop()} style={{ maxWidth: '100%', maxHeight: '100%', objectFit: 'contain', borderRadius: 4 }} />
      </div>
    </div>
  )
}

// ── PDF viewer ────────────────────────────────────────────────────────────────

function PDFViewer({ path, onClose }: { path: string; onClose: () => void }) {
  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center gap-2 px-3 py-2 border-b flex-shrink-0"
        style={{ borderColor: 'var(--border-subtle)', background: 'var(--bg-panel)' }}>
        <File size={13} style={{ color: 'var(--accent-danger)' }} />
        <span className="text-xs font-mono flex-1 truncate" style={{ color: 'var(--text-secondary)' }}>{path}</span>
        <a href={rawUrl(path)} download style={{ display: 'flex', alignItems: 'center', padding: '3px 6px', borderRadius: 4, color: 'var(--text-muted)', textDecoration: 'none' }} title="Download">
          <Download size={13} />
        </a>
        <button onClick={onClose} className="p-1 hover:opacity-70" style={{ color: 'var(--text-muted)' }}><X size={14} /></button>
      </div>
      <iframe src={rawUrl(path)} title="PDF" style={{ flex: 1, border: 'none', background: '#fff' }} />
    </div>
  )
}

// ── View type ─────────────────────────────────────────────────────────────────

type OpenFile = { path: string; kind: 'text' | 'image' | 'pdf' }

// ── Main page ─────────────────────────────────────────────────────────────────

export default function FilesPage() {
  const [roots, setRoots] = useState<Root[]>([])
  const [currentPath, setCurrentPath] = useState('/')
  const [entries, setEntries] = useState<Entry[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState('')
  const [open, setOpen] = useState<OpenFile | null>(null)
  const [activityPath, setActivityPath] = useState<string | null>(null)
  const [newFolderMode, setNewFolderMode] = useState(false)
  const [newFolderName, setNewFolderName] = useState('')
  const [newFileName, setNewFileName] = useState<string | null>(null)
  const [renamingEntry, setRenamingEntry] = useState<Entry | null>(null)
  const [renameValue, setRenameValue] = useState('')
  const [favorites, setFavorites] = useState<Favorite[]>(loadFavs)
  const timer = useRef<ReturnType<typeof setInterval> | null>(null)

  useEffect(() => {
    apiFetch<{ roots: Root[] }>('/api/files/roots').then(r => setRoots(r.roots)).catch(() => {})
  }, [])

  const loadDir = useCallback((path: string) => {
    setLoading(true); setError('')
    apiFetch<{ entries: Entry[]; path: string }>(`/api/files/list?path=${encodeURIComponent(path)}`)
      .then(r => { setEntries(r.entries); setCurrentPath(r.path) })
      .catch(e => setError(e instanceof ApiClientError ? e.message : 'Failed'))
      .finally(() => setLoading(false))
  }, [])

  useEffect(() => {
    loadDir(currentPath)
    timer.current = setInterval(() => loadDir(currentPath), 5000)
    return () => { if (timer.current) clearInterval(timer.current) }
  }, [currentPath, loadDir])

  const navigate = (path: string) => { setOpen(null); setActivityPath(null); setCurrentPath(path) }
  const closeFile = () => { setOpen(null); loadDir(currentPath) }

  const toggleFav = (entry: Entry) => {
    const next = favorites.some(f => f.path === entry.path)
      ? favorites.filter(f => f.path !== entry.path)
      : [...favorites, { path: entry.path, label: entry.name, isDir: entry.is_dir }]
    setFavorites(next); saveFavs(next)
  }

  const openEntry = (entry: Entry) => {
    if (entry.is_dir) { navigate(entry.path); return }
    if (isImage(entry)) { setOpen({ path: entry.path, kind: 'image' }); return }
    if (isPDF(entry))   { setOpen({ path: entry.path, kind: 'pdf'   }); return }
    if (isText(entry))  { setOpen({ path: entry.path, kind: 'text'  }); return }
    // non-previewable: trigger download
    const a = document.createElement('a'); a.href = rawUrl(entry.path); a.download = entry.name; a.click()
  }

  const handleDelete = async (entry: Entry) => {
    if (!confirm(`Delete "${entry.name}"?`)) return
    try {
      await apiFetch(`/api/files/delete?path=${encodeURIComponent(entry.path)}`, { method: 'DELETE' })
      loadDir(currentPath)
    } catch (e) { notify.error(e instanceof ApiClientError ? e.message : 'Delete failed') }
  }

  const handleMkdir = async () => {
    if (!newFolderName.trim()) return
    const target = `${currentPath.replace(/\/$/, '')}/${newFolderName.trim()}`
    try {
      await apiFetch('/api/files/mkdir', { method: 'POST', body: JSON.stringify({ path: target }) })
      setNewFolderMode(false); setNewFolderName(''); loadDir(currentPath)
    } catch (e) { notify.error(e instanceof ApiClientError ? e.message : 'Create failed') }
  }

  const handleNewFile = async () => {
    if (!newFileName?.trim()) return
    const target = `${currentPath.replace(/\/$/, '')}/${newFileName.trim()}`
    try {
      await apiFetch('/api/files/write', { method: 'POST', body: JSON.stringify({ path: target, content: '' }) })
      setNewFileName(null); loadDir(currentPath)
      setOpen({ path: target, kind: 'text' })
    } catch (e) { notify.error(e instanceof ApiClientError ? e.message : 'Create failed') }
  }

  const handleRename = async () => {
    if (!renamingEntry || !renameValue.trim()) return
    const dir = renamingEntry.path.substring(0, renamingEntry.path.lastIndexOf('/'))
    try {
      await apiFetch('/api/files/rename', { method: 'POST', body: JSON.stringify({ from: renamingEntry.path, to: `${dir}/${renameValue.trim()}` }) })
      setRenamingEntry(null); setRenameValue(''); loadDir(currentPath)
    } catch (e) { notify.error(e instanceof ApiClientError ? e.message : 'Rename failed') }
  }

  return (
    <div className="flex min-h-0" style={{ height: 'calc(100vh - 88px)', gap: 12 }}>

      {/* Sidebar */}
      <div className="w-44 flex-shrink-0 rounded overflow-y-auto p-2 space-y-3"
        style={{ background: 'var(--bg-panel)', border: '1px solid var(--border-subtle)' }}>
        {favorites.length > 0 && (
          <div>
            <p className="text-xs uppercase tracking-wider px-2 pb-1" style={{ color: 'var(--text-muted)' }}>Favorites</p>
            {favorites.map(fav => (
              <button key={fav.path} onClick={() => fav.isDir ? navigate(fav.path) : openEntry({ path: fav.path, name: fav.label, is_dir: fav.isDir, size: 0, modified: 0, permissions: '', is_symlink: false })}
                className="flex items-center gap-2 w-full px-2 py-1.5 rounded text-xs text-left hover:opacity-80"
                style={{ color: 'var(--accent-warning)' }}>
                {fav.isDir ? <Folder size={12} /> : <FileText size={12} />}
                <span className="truncate">{fav.label}</span>
              </button>
            ))}
          </div>
        )}
        <div>
          <p className="text-xs uppercase tracking-wider px-2 pb-1" style={{ color: 'var(--text-muted)' }}>Locations</p>
          {roots.map(r => {
            const active = currentPath === r.path || (r.path !== '/' && currentPath.startsWith(r.path))
            return (
              <button key={r.path} onClick={() => navigate(r.path)}
                className="flex items-center gap-2 w-full px-2 py-1.5 rounded text-xs text-left hover:opacity-80"
                style={{ background: active ? 'var(--accent-primary-subtle)' : 'transparent', color: active ? 'var(--accent-primary)' : 'var(--text-secondary)' }}>
                {r.path === '/' ? <Home size={12} /> : <HardDrive size={12} />}
                <span className="truncate">{r.label}</span>
              </button>
            )
          })}
        </div>
      </div>

      {/* Main + activity */}
      <div className="flex flex-1 min-w-0 min-h-0 rounded overflow-hidden" style={{ border: '1px solid var(--border-subtle)' }}>
        <div className="flex flex-col flex-1 min-w-0 min-h-0">

          {/* Toolbar */}
          <div className="flex items-center gap-2 px-3 py-2 flex-shrink-0 border-b"
            style={{ background: 'var(--bg-panel)', borderColor: 'var(--border-subtle)' }}>
            <Breadcrumbs path={currentPath} onNav={navigate} />
            <div className="ml-auto flex items-center gap-1.5">
              <button onClick={() => loadDir(currentPath)} className="p-1.5 rounded hover:opacity-70"
                style={{ color: 'var(--text-muted)' }} title="Refresh"><RefreshCw size={13} /></button>
              <button onClick={() => { setNewFileName(''); setNewFolderMode(false) }}
                className="flex items-center gap-1 px-2 py-1 rounded text-xs hover:opacity-80"
                style={{ background: 'var(--accent-primary-subtle)', color: 'var(--accent-primary)' }}>
                <FilePlus size={12} /> New file
              </button>
              <button onClick={() => { setNewFolderMode(true); setNewFileName(null); setNewFolderName('') }}
                className="flex items-center gap-1 px-2 py-1 rounded text-xs hover:opacity-80"
                style={{ background: 'var(--bg-code)', color: 'var(--text-secondary)' }}>
                <Plus size={12} /> New folder
              </button>
            </div>
          </div>

          {open ? (
            open.kind === 'text'  ? <FileEditor  path={open.path} onClose={closeFile} /> :
            open.kind === 'image' ? <ImageViewer path={open.path} onClose={closeFile} /> :
                                    <PDFViewer   path={open.path} onClose={closeFile} />
          ) : (
            <div className="flex-1 overflow-y-auto" style={{ background: 'var(--bg-root)' }}>

              {/* New file input */}
              {newFileName !== null && (
                <div className="flex items-center gap-2 px-4 py-2 border-b" style={{ borderColor: 'var(--border-subtle)' }}>
                  <FileText size={14} style={{ color: 'var(--accent-primary)', flexShrink: 0 }} />
                  <input autoFocus value={newFileName} onChange={e => setNewFileName(e.target.value)}
                    onKeyDown={e => { if (e.key === 'Enter') handleNewFile(); if (e.key === 'Escape') setNewFileName(null) }}
                    placeholder="filename.txt" className="flex-1 bg-transparent text-xs outline-none font-mono"
                    style={{ color: 'var(--text-primary)' }} />
                  <button onClick={handleNewFile} className="text-xs px-2 py-1 rounded" style={{ color: 'var(--accent-primary)' }}>Create</button>
                  <button onClick={() => setNewFileName(null)} style={{ color: 'var(--text-muted)' }}><X size={13} /></button>
                </div>
              )}

              {/* New folder input */}
              {newFolderMode && (
                <div className="flex items-center gap-2 px-4 py-2 border-b" style={{ borderColor: 'var(--border-subtle)' }}>
                  <Folder size={14} style={{ color: 'var(--accent-primary)', flexShrink: 0 }} />
                  <input autoFocus value={newFolderName} onChange={e => setNewFolderName(e.target.value)}
                    onKeyDown={e => { if (e.key === 'Enter') handleMkdir(); if (e.key === 'Escape') setNewFolderMode(false) }}
                    placeholder="Folder name…" className="flex-1 bg-transparent text-xs outline-none font-mono"
                    style={{ color: 'var(--text-primary)' }} />
                  <button onClick={handleMkdir} className="text-xs px-2 py-1 rounded" style={{ color: 'var(--accent-primary)' }}>Create</button>
                  <button onClick={() => setNewFolderMode(false)} style={{ color: 'var(--text-muted)' }}><X size={13} /></button>
                </div>
              )}

              {error && <div className="px-4 py-3 text-xs" style={{ color: 'var(--accent-danger)' }}>{error}</div>}
              {loading && !entries.length
                ? <div className="px-4 py-8 text-xs text-center" style={{ color: 'var(--text-muted)' }}>Loading…</div>
                : (
                  <table className="w-full text-xs">
                    <thead style={{ position: 'sticky', top: 0, background: 'var(--bg-panel)', zIndex: 1 }}>
                      <tr style={{ borderBottom: '1px solid var(--border-subtle)' }}>
                        {['Name','Size','Modified','Perms',''].map(h => (
                          <th key={h} className="px-4 py-2 text-left uppercase tracking-wider" style={{ color: 'var(--text-muted)' }}>{h}</th>
                        ))}
                      </tr>
                    </thead>
                    <tbody>
                      {entries.length === 0 && (
                        <tr><td colSpan={5} className="px-4 py-8 text-center" style={{ color: 'var(--text-muted)' }}>Empty directory</td></tr>
                      )}
                      {entries.map(entry => {
                        const Icon = fileIcon(entry)
                        const isFav = favorites.some(f => f.path === entry.path)
                        const isRenaming = renamingEntry?.path === entry.path
                        const actOpen = activityPath === entry.path
                        const previewable = isImage(entry) || isPDF(entry) || isText(entry)
                        return (
                          <tr key={entry.path} className="group" style={{ borderBottom: '1px solid var(--border-subtle)' }}>
                            <td className="px-4 py-2">
                              <div className="flex items-center gap-2">
                                <Icon size={14} style={{
                                  color: entry.is_dir ? 'var(--accent-primary)' : isImage(entry) ? 'var(--accent-secondary)' : isText(entry) ? 'var(--text-muted)' : 'var(--text-disabled)',
                                  flexShrink: 0,
                                }} />
                                {isRenaming ? (
                                  <input autoFocus value={renameValue} onChange={e => setRenameValue(e.target.value)}
                                    onKeyDown={e => { if (e.key === 'Enter') handleRename(); if (e.key === 'Escape') setRenamingEntry(null) }}
                                    className="bg-transparent outline-none font-mono flex-1 text-xs"
                                    style={{ color: 'var(--text-primary)', borderBottom: '1px solid var(--accent-primary)' }} />
                                ) : (
                                  <button onClick={() => openEntry(entry)}
                                    className="text-left hover:underline truncate max-w-xs text-xs font-mono"
                                    style={{ color: entry.is_dir ? 'var(--text-primary)' : previewable ? 'var(--text-secondary)' : 'var(--text-muted)' }}>
                                    {entry.name}{entry.is_symlink && <span className="ml-1 opacity-50">→</span>}
                                  </button>
                                )}
                              </div>
                            </td>
                            <td className="px-4 py-2 font-mono" style={{ color: 'var(--text-muted)' }}>{fmtSize(entry.size)}</td>
                            <td className="px-4 py-2" style={{ color: 'var(--text-muted)' }}>{fmtDate(entry.modified)}</td>
                            <td className="px-4 py-2 font-mono" style={{ color: 'var(--text-muted)' }}>{entry.permissions}</td>
                            <td className="px-4 py-2">
                              <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
                                <button onClick={() => toggleFav(entry)} className="p-1 rounded hover:opacity-70"
                                  style={{ color: isFav ? 'var(--accent-warning)' : 'var(--text-muted)' }} title={isFav ? 'Unfavorite' : 'Favorite'}>
                                  <Star size={12} fill={isFav ? 'currentColor' : 'none'} />
                                </button>
                                {!entry.is_dir && (
                                  <a href={rawUrl(entry.path)} download={entry.name}
                                    className="p-1 rounded hover:opacity-70" style={{ color: 'var(--text-muted)' }} title="Download">
                                    <Download size={12} />
                                  </a>
                                )}
                                <button onClick={() => setActivityPath(actOpen ? null : entry.path)} className="p-1 rounded hover:opacity-70"
                                  style={{ color: actOpen ? 'var(--accent-secondary)' : 'var(--text-muted)' }} title="Activity">
                                  <Clock size={12} />
                                </button>
                                <button onClick={() => { setRenamingEntry(entry); setRenameValue(entry.name) }} className="p-1 rounded hover:opacity-70"
                                  style={{ color: 'var(--text-muted)' }} title="Rename"><Edit3 size={12} /></button>
                                <button onClick={() => handleDelete(entry)} className="p-1 rounded hover:opacity-70"
                                  style={{ color: 'var(--accent-danger)' }} title="Delete"><Trash2 size={12} /></button>
                              </div>
                            </td>
                          </tr>
                        )
                      })}
                    </tbody>
                  </table>
                )}
            </div>
          )}
        </div>
        {activityPath && <ActivityPanel path={activityPath} onClose={() => setActivityPath(null)} />}
      </div>
    </div>
  )
}
