import { useCallback, useEffect, useRef, useState } from 'react'
import {
  CheckCircle, Download, HardDrive, Loader2, Play, Trash2, XCircle,
  RefreshCw, Settings2, AlertTriangle, Cpu,
} from 'lucide-react'
import { api } from '@/api/client'
import type { DownloadStatus, LlamaConfig, ModelFile, OllamaConfig, OllamaPullStatus } from '@/api/types'

// ─── Helpers ──────────────────────────────────────────────────────────────────

function formatBytes(bytes: number, decimals = 1): string {
  if (!bytes) return '0 B'
  const k = 1024
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.floor(Math.log(bytes) / Math.log(k))
  return `${(bytes / Math.pow(k, i)).toFixed(decimals)} ${sizes[i]}`
}

const inputStyle: React.CSSProperties = {
  background: 'var(--bg-elevated)',
  border: '1px solid var(--border-default)',
  color: 'var(--text-primary)',
  borderRadius: 4,
  padding: '6px 10px',
  fontSize: 13,
  width: '100%',
  outline: 'none',
}

const numInputStyle: React.CSSProperties = {
  ...inputStyle,
  width: 90,
  textAlign: 'right' as const,
}

// ─── Presets ──────────────────────────────────────────────────────────────────

const GGUF_PRESETS = [
  { label: 'Llama 3.2 3B Q4', size: '~2.0 GB', url: 'https://huggingface.co/bartowski/Llama-3.2-3B-Instruct-GGUF/resolve/main/Llama-3.2-3B-Instruct-Q4_K_M.gguf' },
  { label: 'Llama 3.1 8B Q4', size: '~4.9 GB', url: 'https://huggingface.co/bartowski/Meta-Llama-3.1-8B-Instruct-GGUF/resolve/main/Meta-Llama-3.1-8B-Instruct-Q4_K_M.gguf' },
  { label: 'Mistral 7B Q4',   size: '~4.4 GB', url: 'https://huggingface.co/bartowski/Mistral-7B-Instruct-v0.3-GGUF/resolve/main/Mistral-7B-Instruct-v0.3-Q4_K_M.gguf' },
  { label: 'Phi-3.5 Mini Q4', size: '~2.2 GB', url: 'https://huggingface.co/bartowski/Phi-3.5-mini-instruct-GGUF/resolve/main/Phi-3.5-mini-instruct-Q4_K_M.gguf' },
  { label: 'Qwen2.5 7B Q4',   size: '~4.7 GB', url: 'https://huggingface.co/bartowski/Qwen2.5-7B-Instruct-GGUF/resolve/main/Qwen2.5-7B-Instruct-Q4_K_M.gguf' },
  { label: 'DeepSeek-R1 7B Q4', size: '~4.9 GB', url: 'https://huggingface.co/bartowski/DeepSeek-R1-Distill-Qwen-7B-GGUF/resolve/main/DeepSeek-R1-Distill-Qwen-7B-Q4_K_M.gguf' },
]

const OLLAMA_PRESETS = [
  { label: 'llama3.2', size: '~2.0 GB' },
  { label: 'llama3.1', size: '~4.9 GB' },
  { label: 'mistral',  size: '~4.1 GB' },
  { label: 'phi4',     size: '~2.5 GB' },
  { label: 'qwen2.5',  size: '~4.7 GB' },
  { label: 'gemma3',   size: '~5.2 GB' },
]

// ─── DownloadRow ──────────────────────────────────────────────────────────────

interface ActiveDownload { id: string; status: DownloadStatus | null }

function DownloadRow({ dl, onDone }: { dl: ActiveDownload; onDone: () => void }) {
  const [status, setStatus] = useState<DownloadStatus | null>(dl.status)
  const doneNotified = useRef(false)

  useEffect(() => {
    if (!dl.id) return
    const interval = setInterval(async () => {
      try {
        const s = await api.models.downloadStatus(dl.id)
        setStatus(s)
        if (s.status !== 'downloading') {
          clearInterval(interval)
          if (!doneNotified.current) { doneNotified.current = true; onDone() }
        }
      } catch { clearInterval(interval) }
    }, 1000)
    return () => clearInterval(interval)
  }, [dl.id, onDone])

  if (!status) return null
  const pct = status.total_bytes
    ? Math.min(100, (status.downloaded_bytes / status.total_bytes) * 100)
    : null

  return (
    <div className="rounded p-3 space-y-1.5" style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)' }}>
      <div className="flex items-center justify-between gap-2">
        <span className="text-sm font-medium truncate" style={{ color: 'var(--text-primary)' }}>{status.filename}</span>
        <span className="text-xs shrink-0" style={{ color: status.status === 'error' ? 'var(--accent-danger)' : 'var(--text-muted)' }}>
          {status.status === 'done' && <span style={{ color: 'var(--accent-success)' }}>Done</span>}
          {status.status === 'error' && (status.error ?? 'Error')}
          {status.status === 'downloading' && (pct !== null
            ? `${formatBytes(status.downloaded_bytes)} / ${formatBytes(status.total_bytes!)} (${pct.toFixed(1)}%)`
            : `${formatBytes(status.downloaded_bytes)}…`
          )}
        </span>
      </div>
      {status.status === 'downloading' && (
        <div className="h-1.5 rounded-full overflow-hidden" style={{ background: 'var(--border-subtle)' }}>
          <div className="h-full rounded-full transition-all" style={{
            width: pct !== null ? `${pct.toFixed(1)}%` : '100%',
            background: 'var(--accent-primary)',
            animation: pct === null ? 'pulse 1.5s ease-in-out infinite' : undefined,
          }} />
        </div>
      )}
    </div>
  )
}

// ─── OllamaPullRow ────────────────────────────────────────────────────────────

interface ActivePull { id: string; status: OllamaPullStatus | null }

function OllamaPullRow({ pull, onDone }: { pull: ActivePull; onDone: () => void }) {
  const [status, setStatus] = useState<OllamaPullStatus | null>(pull.status)
  const doneNotified = useRef(false)

  useEffect(() => {
    if (!pull.id) return
    const interval = setInterval(async () => {
      try {
        const s = await api.models.ollamaPullStatus(pull.id)
        setStatus(s)
        if (s.status !== 'pulling') {
          clearInterval(interval)
          if (!doneNotified.current) { doneNotified.current = true; onDone() }
        }
      } catch { clearInterval(interval) }
    }, 1200)
    return () => clearInterval(interval)
  }, [pull.id, onDone])

  if (!status) return null
  const pct = status.total_bytes && status.pulled_bytes != null
    ? Math.min(100, (status.pulled_bytes / status.total_bytes) * 100)
    : null

  return (
    <div className="rounded p-3 space-y-1.5" style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)' }}>
      <div className="flex items-center justify-between gap-2">
        <span className="text-sm font-medium truncate" style={{ color: 'var(--text-primary)' }}>{status.model}</span>
        <span className="text-xs shrink-0" style={{ color: status.status === 'error' ? 'var(--accent-danger)' : 'var(--text-muted)' }}>
          {status.status === 'done' && <span style={{ color: 'var(--accent-success)' }}>Done</span>}
          {status.status === 'error' && (status.error ?? 'Error')}
          {status.status === 'pulling' && (
            pct !== null ? `${pct.toFixed(1)}%` : (status.current_layer ?? 'Pulling…')
          )}
        </span>
      </div>
      {status.status === 'pulling' && (
        <div className="h-1.5 rounded-full overflow-hidden" style={{ background: 'var(--border-subtle)' }}>
          <div className="h-full rounded-full transition-all" style={{
            width: pct !== null ? `${pct.toFixed(1)}%` : '40%',
            background: 'var(--accent-primary)',
            animation: pct === null ? 'pulse 1.5s ease-in-out infinite' : undefined,
          }} />
        </div>
      )}
    </div>
  )
}

// ─── Toggle ───────────────────────────────────────────────────────────────────

function Toggle({ value, onChange }: { value: boolean; onChange: (v: boolean) => void }) {
  return (
    <button
      type="button"
      onClick={() => onChange(!value)}
      style={{
        width: 40, height: 22, borderRadius: 11,
        background: value ? 'var(--accent-primary)' : 'var(--border-default)',
        border: 'none', cursor: 'pointer', position: 'relative', transition: 'background 0.2s',
        flexShrink: 0,
      }}
    >
      <span style={{
        position: 'absolute', top: 3, left: value ? 21 : 3,
        width: 16, height: 16, borderRadius: '50%',
        background: '#fff', transition: 'left 0.2s',
      }} />
    </button>
  )
}

// ─── Tab bar ──────────────────────────────────────────────────────────────────

type Tab = 'library' | 'get' | 'llama'

const TABS: { id: Tab; label: string }[] = [
  { id: 'library', label: 'Library' },
  { id: 'get',     label: 'Get Models' },
  { id: 'llama',   label: 'llama.cpp' },
]

// ─── Models page ─────────────────────────────────────────────────────────────

export default function ModelsPage() {
  const [tab, setTab] = useState<Tab>('library')

  // Library
  const [models, setModels] = useState<ModelFile[]>([])
  const [modelsLoading, setModelsLoading] = useState(true)
  const [loadingModel, setLoadingModel]   = useState<string | null>(null)
  const [deletingModel, setDeletingModel] = useState<string | null>(null)
  const [creatingModel, setCreatingModel] = useState<string | null>(null)

  // Get Models
  const [url, setUrl]             = useState('')
  const [filename, setFilename]   = useState('')
  const [downloading, setDownloading] = useState(false)
  const [activeDownloads, setActiveDownloads] = useState<ActiveDownload[]>([])
  const [ollamaModel, setOllamaModel] = useState('')
  const [pulling, setPulling]     = useState(false)
  const [activePulls, setActivePulls] = useState<ActivePull[]>([])
  const [activeCreates, setActiveCreates] = useState<ActivePull[]>([])

  // llama.cpp config
  const [llamaCfg, setLlamaCfg]   = useState<LlamaConfig | null>(null)
  const [llamaLoading, setLlamaLoading] = useState(false)
  const [llamaSaving, setLlamaSaving]   = useState(false)
  const [draft, setDraft]         = useState<LlamaConfig | null>(null)

  // Ollama config
  const [ollamaCfg, setOllamaCfg]     = useState<OllamaConfig | null>(null)
  const [ollamaSaving, setOllamaSaving] = useState(false)
  const [keepAliveDraft, setKeepAliveDraft] = useState<number>(300)

  // Notification
  const [notice, setNotice]       = useState<{ kind: 'ok' | 'err'; msg: string } | null>(null)
  const noticeTimer               = useRef<ReturnType<typeof setTimeout> | null>(null)

  const notify = (kind: 'ok' | 'err', msg: string) => {
    setNotice({ kind, msg })
    if (noticeTimer.current) clearTimeout(noticeTimer.current)
    noticeTimer.current = setTimeout(() => setNotice(null), 3500)
  }

  // ── Fetch models ────────────────────────────────────────────────────────────

  const fetchModels = useCallback(async () => {
    setModelsLoading(true)
    try {
      const list = await api.models.list()
      setModels(list)
    } finally {
      setModelsLoading(false)
    }
  }, [])

  useEffect(() => { fetchModels() }, [fetchModels])

  // ── Fetch llama + ollama config when tab opens ─────────────────────────────

  useEffect(() => {
    if (tab !== 'llama') return
    if (llamaCfg === null) {
      setLlamaLoading(true)
      api.models.getLlamaConfig()
        .then(cfg => { setLlamaCfg(cfg); setDraft(cfg) })
        .catch(() => notify('err', 'Failed to load llama.cpp config'))
        .finally(() => setLlamaLoading(false))
    }
    if (ollamaCfg === null) {
      api.models.getOllamaConfig()
        .then(cfg => { setOllamaCfg(cfg); setKeepAliveDraft(cfg.keep_alive_secs) })
        .catch(() => {})
    }
  }, [tab, llamaCfg, ollamaCfg])

  // ── Handlers ────────────────────────────────────────────────────────────────

  const handleLoad = async (filename: string) => {
    setLoadingModel(filename)
    try {
      await api.models.loadModel(filename)
      notify('ok', `llama.cpp restarted in multi-model mode`)
      setLlamaCfg(null) // refresh config on next llama tab open
    } catch (e: unknown) {
      notify('err', e instanceof Error ? e.message : 'Failed')
    } finally { setLoadingModel(null) }
  }

  const handleDelete = async (filename: string) => {
    setDeletingModel(filename)
    try {
      await api.models.deleteModel(filename)
      notify('ok', `Deleted ${filename}`)
      fetchModels()
    } catch (e: unknown) {
      notify('err', e instanceof Error ? e.message : 'Failed')
    } finally { setDeletingModel(null) }
  }

  const handleOllamaCreate = async (filename: string) => {
    setCreatingModel(filename)
    try {
      const res = await api.models.ollamaCreate(filename)
      setActiveCreates(prev => [...prev, { id: res.id, status: null }])
      notify('ok', `Importing ${filename} into Ollama…`)
      fetchModels()
    } catch (e: unknown) {
      notify('err', e instanceof Error ? e.message : 'Failed')
    } finally { setCreatingModel(null) }
  }

  const handleDownload = async () => {
    if (!url.trim()) return
    setDownloading(true)
    try {
      const res = await api.models.startDownload(url.trim(), filename.trim() || undefined)
      setActiveDownloads(prev => [...prev, { id: res.id, status: null }])
      setUrl(''); setFilename('')
    } catch (e: unknown) {
      notify('err', e instanceof Error ? e.message : 'Download failed')
    } finally { setDownloading(false) }
  }

  const handlePull = async () => {
    if (!ollamaModel.trim()) return
    setPulling(true)
    try {
      const res = await api.models.ollamaPull(ollamaModel.trim())
      setActivePulls(prev => [...prev, { id: res.id, status: null }])
      setOllamaModel('')
    } catch (e: unknown) {
      notify('err', e instanceof Error ? e.message : 'Pull failed')
    } finally { setPulling(false) }
  }

  const handleSaveOllama = async () => {
    if (!ollamaCfg) return
    setOllamaSaving(true)
    try {
      await api.models.saveOllamaConfig({ ...ollamaCfg, keep_alive_secs: keepAliveDraft })
      setOllamaCfg(prev => prev ? { ...prev, keep_alive_secs: keepAliveDraft } : prev)
      notify('ok', 'Saved — Ollama is restarting')
    } catch (e: unknown) {
      notify('err', e instanceof Error ? e.message : 'Save failed')
    } finally { setOllamaSaving(false) }
  }

  const handleSaveLlama = async () => {
    if (!draft) return
    setLlamaSaving(true)
    try {
      await api.models.saveLlamaConfig(draft)
      setLlamaCfg(draft)
      notify('ok', 'Saved — llama.cpp is restarting')
    } catch (e: unknown) {
      notify('err', e instanceof Error ? e.message : 'Save failed')
    } finally { setLlamaSaving(false) }
  }

  const setDraftField = <K extends keyof LlamaConfig>(key: K, val: LlamaConfig[K]) =>
    setDraft(d => d ? { ...d, [key]: val } : d)

  // ── Render ──────────────────────────────────────────────────────────────────

  const ggufModels  = models.filter(m => m.source === 'voidtower')
  const ollamaModels = models.filter(m => m.source === 'ollama')

  return (
    <div style={{ padding: '24px 28px', maxWidth: 900, margin: '0 auto' }}>
      {/* Header */}
      <div className="flex items-center justify-between mb-6">
        <div className="flex items-center gap-3">
          <HardDrive size={20} style={{ color: 'var(--accent-primary)' }} />
          <h1 className="text-xl font-semibold" style={{ color: 'var(--text-primary)' }}>
            Models
          </h1>
          {!modelsLoading && (
            <span className="text-xs px-2 py-0.5 rounded-full font-medium"
              style={{ background: 'var(--bg-elevated)', color: 'var(--text-muted)', border: '1px solid var(--border-subtle)' }}>
              {models.length}
            </span>
          )}
        </div>
        <button onClick={fetchModels} disabled={modelsLoading}
          className="flex items-center gap-1.5 px-3 py-1.5 rounded text-sm"
          style={{ background: 'var(--bg-elevated)', color: 'var(--text-secondary)', border: '1px solid var(--border-subtle)', cursor: 'pointer' }}>
          <RefreshCw size={13} className={modelsLoading ? 'animate-spin' : ''} />
          Refresh
        </button>
      </div>

      {/* Tab bar */}
      <div className="flex gap-0 mb-6" style={{ borderBottom: '1px solid var(--border-subtle)' }}>
        {TABS.map(t => (
          <button key={t.id} onClick={() => setTab(t.id)}
            style={{
              padding: '8px 18px',
              fontSize: 13,
              fontWeight: tab === t.id ? 600 : 400,
              color: tab === t.id ? 'var(--accent-primary)' : 'var(--text-muted)',
              background: 'none',
              border: 'none',
              borderBottom: tab === t.id ? '2px solid var(--accent-primary)' : '2px solid transparent',
              marginBottom: -1,
              cursor: 'pointer',
              transition: 'color 0.15s',
            }}
          >
            {t.label}
          </button>
        ))}
      </div>

      {/* Notification */}
      {notice && (
        <div className="flex items-center gap-2 px-3 py-2.5 rounded mb-5 text-sm"
          style={{
            background: notice.kind === 'ok' ? 'color-mix(in srgb, var(--accent-success) 12%, transparent)' : 'color-mix(in srgb, var(--accent-danger) 12%, transparent)',
            border: `1px solid ${notice.kind === 'ok' ? 'var(--accent-success)' : 'var(--accent-danger)'}`,
            color: notice.kind === 'ok' ? 'var(--accent-success)' : 'var(--accent-danger)',
          }}>
          {notice.kind === 'ok' ? <CheckCircle size={14} /> : <XCircle size={14} />}
          {notice.msg}
        </div>
      )}

      {/* ── Library tab ── */}
      {tab === 'library' && (
        <div className="space-y-3">
          {modelsLoading ? (
            <div className="flex items-center gap-2 py-8 justify-center" style={{ color: 'var(--text-muted)' }}>
              <Loader2 size={16} className="animate-spin" /> Loading…
            </div>
          ) : models.length === 0 ? (
            <div className="text-center py-12" style={{ color: 'var(--text-muted)' }}>
              <HardDrive size={32} className="mx-auto mb-3 opacity-30" />
              <p className="text-sm">No models yet. Go to <button onClick={() => setTab('get')} style={{ color: 'var(--accent-primary)', background: 'none', border: 'none', cursor: 'pointer', padding: 0 }}>Get Models</button> to download one.</p>
            </div>
          ) : (
            <>
              {/* GGUF section */}
              {ggufModels.length > 0 && (
                <div>
                  <p className="text-xs font-medium mb-2 uppercase tracking-wider" style={{ color: 'var(--text-muted)' }}>GGUF</p>
                  <div className="rounded overflow-hidden" style={{ border: '1px solid var(--border-subtle)' }}>
                    {ggufModels.map((m, i) => (
                      <div key={m.filename}
                        className="flex items-center gap-3 px-4 py-3"
                        style={{
                          borderTop: i > 0 ? '1px solid var(--border-subtle)' : undefined,
                          background: 'var(--bg-surface)',
                        }}>
                        <div className="flex-1 min-w-0">
                          <p className="text-sm font-medium truncate" style={{ color: 'var(--text-primary)' }}>{m.filename}</p>
                          <p className="text-xs mt-0.5" style={{ color: 'var(--text-muted)' }}>{formatBytes(m.size_bytes)}</p>
                        </div>
                        {m.active && (
                          <span className="text-xs px-2 py-0.5 rounded-full font-medium shrink-0"
                            style={{ background: 'color-mix(in srgb, var(--accent-success) 15%, transparent)', color: 'var(--accent-success)', border: '1px solid var(--accent-success)' }}>
                            Active
                          </span>
                        )}
                        <div className="flex items-center gap-1.5 shrink-0">
                          <button
                            onClick={() => handleLoad(m.filename)}
                            disabled={loadingModel === m.filename}
                            title="Load into llama.cpp"
                            className="flex items-center gap-1 px-2.5 py-1.5 rounded text-xs"
                            style={{ background: 'var(--bg-elevated)', color: 'var(--text-secondary)', border: '1px solid var(--border-subtle)', cursor: 'pointer' }}>
                            {loadingModel === m.filename ? <Loader2 size={11} className="animate-spin" /> : <Play size={11} />}
                            llama.cpp
                          </button>
                          <button
                            onClick={() => handleOllamaCreate(m.filename)}
                            disabled={creatingModel === m.filename}
                            title="Import into Ollama"
                            className="flex items-center gap-1 px-2.5 py-1.5 rounded text-xs"
                            style={{ background: 'var(--bg-elevated)', color: 'var(--text-secondary)', border: '1px solid var(--border-subtle)', cursor: 'pointer' }}>
                            {creatingModel === m.filename ? <Loader2 size={11} className="animate-spin" /> : <Download size={11} />}
                            Ollama
                          </button>
                          <button
                            onClick={() => handleDelete(m.filename)}
                            disabled={deletingModel === m.filename}
                            title="Delete"
                            className="flex items-center justify-center p-1.5 rounded"
                            style={{ background: 'none', color: 'var(--text-muted)', border: '1px solid transparent', cursor: 'pointer' }}>
                            {deletingModel === m.filename ? <Loader2 size={13} className="animate-spin" /> : <Trash2 size={13} />}
                          </button>
                        </div>
                      </div>
                    ))}
                  </div>
                </div>
              )}

              {/* Ollama section */}
              {ollamaModels.length > 0 && (
                <div className="mt-5">
                  <p className="text-xs font-medium mb-2 uppercase tracking-wider" style={{ color: 'var(--text-muted)' }}>Ollama</p>
                  <div className="rounded overflow-hidden" style={{ border: '1px solid var(--border-subtle)' }}>
                    {ollamaModels.map((m, i) => (
                      <div key={m.filename}
                        className="flex items-center gap-3 px-4 py-3"
                        style={{
                          borderTop: i > 0 ? '1px solid var(--border-subtle)' : undefined,
                          background: 'var(--bg-surface)',
                        }}>
                        <div className="flex-1 min-w-0">
                          <p className="text-sm font-medium truncate" style={{ color: 'var(--text-primary)' }}>{m.filename}</p>
                          <p className="text-xs mt-0.5" style={{ color: 'var(--text-muted)' }}>{formatBytes(m.size_bytes)}</p>
                        </div>
                        <button
                          onClick={() => handleDelete(m.filename)}
                          disabled={deletingModel === m.filename}
                          title="Delete"
                          className="flex items-center justify-center p-1.5 rounded"
                          style={{ background: 'none', color: 'var(--text-muted)', border: '1px solid transparent', cursor: 'pointer' }}>
                          {deletingModel === m.filename ? <Loader2 size={13} className="animate-spin" /> : <Trash2 size={13} />}
                        </button>
                      </div>
                    ))}
                  </div>
                </div>
              )}

              {/* Active Ollama creates from library */}
              {activeCreates.length > 0 && (
                <div className="mt-4 space-y-2">
                  {activeCreates.map(c => (
                    <OllamaPullRow key={c.id} pull={c} onDone={fetchModels} />
                  ))}
                </div>
              )}
            </>
          )}
        </div>
      )}

      {/* ── Get Models tab ── */}
      {tab === 'get' && (
        <div className="space-y-8">
          {/* GGUF Download */}
          <section>
            <div className="flex items-center gap-2 mb-4">
              <Download size={15} style={{ color: 'var(--accent-primary)' }} />
              <h2 className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>Download GGUF</h2>
            </div>

            {/* Presets */}
            <div className="grid grid-cols-3 gap-2 mb-4">
              {GGUF_PRESETS.map(p => (
                <button key={p.label} onClick={() => setUrl(p.url)}
                  className="text-left px-3 py-2.5 rounded text-xs"
                  style={{
                    background: url === p.url ? 'color-mix(in srgb, var(--accent-primary) 12%, transparent)' : 'var(--bg-elevated)',
                    border: `1px solid ${url === p.url ? 'var(--accent-primary)' : 'var(--border-subtle)'}`,
                    color: 'var(--text-primary)', cursor: 'pointer',
                  }}>
                  <div className="font-medium">{p.label}</div>
                  <div style={{ color: 'var(--text-muted)' }}>{p.size}</div>
                </button>
              ))}
            </div>

            <div className="space-y-2">
              <input value={url} onChange={e => setUrl(e.target.value)}
                placeholder="https://huggingface.co/…/model.gguf"
                style={inputStyle} />
              <div className="flex gap-2">
                <input value={filename} onChange={e => setFilename(e.target.value)}
                  placeholder="Filename (optional — inferred from URL)"
                  style={{ ...inputStyle, flex: 1 }} />
                <button onClick={handleDownload} disabled={downloading || !url.trim()}
                  className="flex items-center gap-1.5 px-4 py-2 rounded text-sm font-medium shrink-0"
                  style={{
                    background: 'var(--accent-primary)', color: '#fff', border: 'none',
                    cursor: downloading || !url.trim() ? 'not-allowed' : 'pointer',
                    opacity: downloading || !url.trim() ? 0.6 : 1,
                  }}>
                  {downloading ? <Loader2 size={13} className="animate-spin" /> : <Download size={13} />}
                  Download
                </button>
              </div>
            </div>

            {activeDownloads.length > 0 && (
              <div className="mt-3 space-y-2">
                {activeDownloads.map(dl => (
                  <DownloadRow key={dl.id} dl={dl} onDone={fetchModels} />
                ))}
              </div>
            )}
          </section>

          {/* Divider */}
          <div style={{ borderTop: '1px solid var(--border-subtle)' }} />

          {/* Ollama Pull */}
          <section>
            <div className="flex items-center gap-2 mb-4">
              <svg width="15" height="15" viewBox="0 0 24 24" fill="none" style={{ color: 'var(--accent-primary)' }}>
                <circle cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="2"/>
                <circle cx="12" cy="12" r="4" fill="currentColor"/>
              </svg>
              <h2 className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>Pull via Ollama</h2>
            </div>

            {/* Ollama presets */}
            <div className="grid grid-cols-3 gap-2 mb-4">
              {OLLAMA_PRESETS.map(p => (
                <button key={p.label} onClick={() => setOllamaModel(p.label)}
                  className="text-left px-3 py-2.5 rounded text-xs"
                  style={{
                    background: ollamaModel === p.label ? 'color-mix(in srgb, var(--accent-primary) 12%, transparent)' : 'var(--bg-elevated)',
                    border: `1px solid ${ollamaModel === p.label ? 'var(--accent-primary)' : 'var(--border-subtle)'}`,
                    color: 'var(--text-primary)', cursor: 'pointer',
                  }}>
                  <div className="font-medium">{p.label}</div>
                  <div style={{ color: 'var(--text-muted)' }}>{p.size}</div>
                </button>
              ))}
            </div>

            <div className="flex gap-2">
              <input value={ollamaModel} onChange={e => setOllamaModel(e.target.value)}
                placeholder="model:tag (e.g. llama3.2:3b)"
                style={{ ...inputStyle, flex: 1 }}
                onKeyDown={e => { if (e.key === 'Enter') handlePull() }} />
              <button onClick={handlePull} disabled={pulling || !ollamaModel.trim()}
                className="flex items-center gap-1.5 px-4 py-2 rounded text-sm font-medium shrink-0"
                style={{
                  background: 'var(--accent-primary)', color: '#fff', border: 'none',
                  cursor: pulling || !ollamaModel.trim() ? 'not-allowed' : 'pointer',
                  opacity: pulling || !ollamaModel.trim() ? 0.6 : 1,
                }}>
                {pulling ? <Loader2 size={13} className="animate-spin" /> : <Download size={13} />}
                Pull
              </button>
            </div>

            {activePulls.length > 0 && (
              <div className="mt-3 space-y-2">
                {activePulls.map(p => (
                  <OllamaPullRow key={p.id} pull={p} onDone={fetchModels} />
                ))}
              </div>
            )}
          </section>
        </div>
      )}

      {/* ── llama.cpp tab ── */}
      {tab === 'llama' && (
        <div>
          {llamaLoading ? (
            <div className="flex items-center gap-2 py-12 justify-center" style={{ color: 'var(--text-muted)' }}>
              <Loader2 size={16} className="animate-spin" /> Loading…
            </div>
          ) : !llamaCfg?.deployed ? (
            <div className="text-center py-14" style={{ color: 'var(--text-muted)' }}>
              <Cpu size={36} className="mx-auto mb-3 opacity-30" />
              <p className="text-sm font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>llama.cpp is not deployed</p>
              <p className="text-xs">Deploy it from the <span style={{ color: 'var(--accent-primary)' }}>App Vault</span> to configure it here.</p>
            </div>
          ) : draft ? (
            <div className="space-y-6">
              {/* Performance card */}
              <div className="rounded-lg p-5" style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)' }}>
                <div className="flex items-center gap-2 mb-4">
                  <Settings2 size={14} style={{ color: 'var(--accent-primary)' }} />
                  <span className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>Performance</span>
                </div>
                <div className="grid grid-cols-4 gap-4">
                  {([
                    { key: 'threads',    label: 'Threads',      hint: 'rec: 10' },
                    { key: 'ctx_size',   label: 'Context Size', hint: 'rec: 4096' },
                    { key: 'batch_size', label: 'Batch Size',   hint: 'rec: 512' },
                    { key: 'parallel',   label: 'Parallel Slots', hint: 'rec: 1' },
                  ] as const).map(({ key, label, hint }) => (
                    <div key={key}>
                      <label className="block text-xs mb-1" style={{ color: 'var(--text-muted)' }}>{label}</label>
                      <input
                        type="number"
                        value={draft[key]}
                        onChange={e => setDraftField(key, parseInt(e.target.value) || 0)}
                        style={numInputStyle}
                        min={1}
                      />
                      <p className="text-xs mt-1" style={{ color: 'var(--text-muted)' }}>{hint}</p>
                    </div>
                  ))}
                </div>
              </div>

              {/* GPU & toggles card */}
              <div className="rounded-lg p-5" style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)' }}>
                <div className="flex items-center gap-2 mb-4">
                  <Cpu size={14} style={{ color: 'var(--accent-primary)' }} />
                  <span className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>GPU & Options</span>
                </div>
                <div className="grid grid-cols-3 gap-4">
                  <div>
                    <label className="block text-xs mb-1" style={{ color: 'var(--text-muted)' }}>GPU Layers</label>
                    <input
                      type="number"
                      value={draft.n_gpu_layers}
                      onChange={e => setDraftField('n_gpu_layers', parseInt(e.target.value) || 0)}
                      style={numInputStyle}
                      min={0} max={999}
                    />
                    <p className="text-xs mt-1" style={{ color: 'var(--text-muted)' }}>999 = all layers</p>
                  </div>
                  <div>
                    <label className="block text-xs mb-2" style={{ color: 'var(--text-muted)' }}>Flash Attention</label>
                    <div className="flex items-center gap-2">
                      <Toggle value={draft.flash_attn} onChange={v => setDraftField('flash_attn', v)} />
                      <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>{draft.flash_attn ? 'Enabled' : 'Disabled'}</span>
                    </div>
                    <p className="text-xs mt-1" style={{ color: 'var(--text-muted)' }}>rec: enabled</p>
                  </div>
                  <div>
                    <label className="block text-xs mb-2" style={{ color: 'var(--text-muted)' }}>Continuous Batching</label>
                    <div className="flex items-center gap-2">
                      <Toggle value={draft.cont_batching} onChange={v => setDraftField('cont_batching', v)} />
                      <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>{draft.cont_batching ? 'Enabled' : 'Disabled'}</span>
                    </div>
                    <p className="text-xs mt-1" style={{ color: 'var(--text-muted)' }}>rec: enabled</p>
                  </div>
                </div>
              </div>

              {/* KV Cache card */}
              <div className="rounded-lg p-5" style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)' }}>
                <div className="flex items-center gap-2 mb-4">
                  <HardDrive size={14} style={{ color: 'var(--accent-primary)' }} />
                  <span className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>KV Cache</span>
                </div>
                <div className="grid grid-cols-2 gap-4">
                  {([
                    { key: 'cache_type_k', label: 'Type K' },
                    { key: 'cache_type_v', label: 'Type V' },
                  ] as const).map(({ key, label }) => (
                    <div key={key}>
                      <label className="block text-xs mb-1" style={{ color: 'var(--text-muted)' }}>{label}</label>
                      <select
                        value={draft[key]}
                        onChange={e => setDraftField(key, e.target.value)}
                        style={{ ...inputStyle, width: 'auto', minWidth: 120 }}>
                        <option value="f16">f16 (default)</option>
                        <option value="q8_0">q8_0 (smaller)</option>
                        <option value="q4_0">q4_0 (smallest)</option>
                      </select>
                      <p className="text-xs mt-1" style={{ color: 'var(--text-muted)' }}>rec: q8_0</p>
                    </div>
                  ))}
                </div>
              </div>

              {/* Warning + Save llama.cpp */}
              <div className="flex items-center gap-3 pt-1">
                <div className="flex items-center gap-2 text-xs flex-1" style={{ color: 'var(--text-muted)' }}>
                  <AlertTriangle size={13} style={{ color: 'var(--accent-warning)' }} />
                  Saving will restart the llama.cpp container.
                </div>
                <button
                  onClick={handleSaveLlama}
                  disabled={llamaSaving}
                  className="flex items-center gap-2 px-4 py-2 rounded text-sm font-medium"
                  style={{
                    background: 'var(--accent-warning, #f59e0b)',
                    color: '#000',
                    border: 'none',
                    cursor: llamaSaving ? 'not-allowed' : 'pointer',
                    opacity: llamaSaving ? 0.7 : 1,
                  }}>
                  {llamaSaving ? <Loader2 size={14} className="animate-spin" /> : <RefreshCw size={14} />}
                  Save & Restart
                </button>
              </div>

              {/* Ollama card */}
              {ollamaCfg && (
                <div className="rounded-lg p-5" style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)' }}>
                  <div className="flex items-center gap-2 mb-4">
                    <Settings2 size={14} style={{ color: 'var(--accent-primary)' }} />
                    <span className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>Ollama</span>
                    {!ollamaCfg.deployed && (
                      <span className="text-xs ml-1" style={{ color: 'var(--text-muted)' }}>(not deployed via App Vault — read-only)</span>
                    )}
                  </div>
                  <div className="grid grid-cols-3 gap-4 items-end">
                    <div>
                      <label className="block text-xs mb-1" style={{ color: 'var(--text-muted)' }}>Keep-Alive (seconds)</label>
                      <input
                        type="number"
                        value={keepAliveDraft}
                        onChange={e => setKeepAliveDraft(parseInt(e.target.value) || 0)}
                        disabled={!ollamaCfg.deployed}
                        style={{ ...numInputStyle, opacity: ollamaCfg.deployed ? 1 : 0.5 }}
                        min={-1}
                      />
                      <p className="text-xs mt-1" style={{ color: 'var(--text-muted)' }}>0 = unload immediately, -1 = never</p>
                    </div>
                    <div className="col-span-2 flex items-end justify-end gap-3 pb-0.5">
                      {ollamaCfg.deployed && (
                        <>
                          <div className="flex items-center gap-2 text-xs" style={{ color: 'var(--text-muted)' }}>
                            <AlertTriangle size={13} style={{ color: 'var(--accent-warning)' }} />
                            Saving will restart the Ollama container.
                          </div>
                          <button
                            onClick={handleSaveOllama}
                            disabled={ollamaSaving}
                            className="flex items-center gap-2 px-4 py-2 rounded text-sm font-medium"
                            style={{
                              background: 'var(--accent-warning, #f59e0b)',
                              color: '#000',
                              border: 'none',
                              cursor: ollamaSaving ? 'not-allowed' : 'pointer',
                              opacity: ollamaSaving ? 0.7 : 1,
                            }}>
                            {ollamaSaving ? <Loader2 size={14} className="animate-spin" /> : <RefreshCw size={14} />}
                            Save & Restart
                          </button>
                        </>
                      )}
                    </div>
                  </div>
                </div>
              )}
            </div>
          ) : null}
        </div>
      )}
    </div>
  )
}
