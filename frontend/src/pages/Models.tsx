import { useCallback, useEffect, useRef, useState } from 'react'
import {
  CheckCircle, Download, HardDrive, Loader2, Play, Trash2, XCircle, CloudDownload, RefreshCw,
} from 'lucide-react'
import { api } from '@/api/client'
import type { DownloadStatus, ModelFile, OllamaModelInfo, OllamaPullStatus } from '@/api/types'

// ─── Helpers ─────────────────────────────────────────────────────────────────

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

// ─── Presets ─────────────────────────────────────────────────────────────────

interface Preset {
  label: string
  size: string
  url: string
}

const PRESETS: Preset[] = [
  {
    label: 'Llama 3.2 3B Q4',
    size: '~2.0 GB',
    url: 'https://huggingface.co/bartowski/Llama-3.2-3B-Instruct-GGUF/resolve/main/Llama-3.2-3B-Instruct-Q4_K_M.gguf',
  },
  {
    label: 'Llama 3.1 8B Q4',
    size: '~4.9 GB',
    url: 'https://huggingface.co/bartowski/Meta-Llama-3.1-8B-Instruct-GGUF/resolve/main/Meta-Llama-3.1-8B-Instruct-Q4_K_M.gguf',
  },
  {
    label: 'Mistral 7B Q4',
    size: '~4.4 GB',
    url: 'https://huggingface.co/bartowski/Mistral-7B-Instruct-v0.3-GGUF/resolve/main/Mistral-7B-Instruct-v0.3-Q4_K_M.gguf',
  },
  {
    label: 'Phi-3.5 Mini Q4',
    size: '~2.2 GB',
    url: 'https://huggingface.co/bartowski/Phi-3.5-mini-instruct-GGUF/resolve/main/Phi-3.5-mini-instruct-Q4_K_M.gguf',
  },
  {
    label: 'Qwen2.5 7B Q4',
    size: '~4.7 GB',
    url: 'https://huggingface.co/bartowski/Qwen2.5-7B-Instruct-GGUF/resolve/main/Qwen2.5-7B-Instruct-Q4_K_M.gguf',
  },
  {
    label: 'DeepSeek-R1 7B Q4',
    size: '~4.9 GB',
    url: 'https://huggingface.co/bartowski/DeepSeek-R1-Distill-Qwen-7B-GGUF/resolve/main/DeepSeek-R1-Distill-Qwen-7B-Q4_K_M.gguf',
  },
]

// ─── DownloadRow ─────────────────────────────────────────────────────────────

interface ActiveDownload {
  id: string
  status: DownloadStatus | null
}

function DownloadRow({
  dl,
  onDone,
}: {
  dl: ActiveDownload
  onDone: () => void
}) {
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
          if (!doneNotified.current) {
            doneNotified.current = true
            onDone()
          }
        }
      } catch {
        clearInterval(interval)
      }
    }, 1000)
    return () => clearInterval(interval)
  }, [dl.id, onDone])

  if (!status) return null

  const pct = status.total_bytes
    ? Math.min(100, (status.downloaded_bytes / status.total_bytes) * 100)
    : null

  return (
    <div
      className="rounded p-3 space-y-1.5"
      style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)' }}
    >
      <div className="flex items-center justify-between gap-2">
        <span className="text-sm font-medium truncate" style={{ color: 'var(--text-primary)' }}>
          {status.filename}
        </span>
        <span className="text-xs shrink-0" style={{ color: status.status === 'error' ? 'var(--accent-danger)' : 'var(--text-muted)' }}>
          {status.status === 'done' && <span style={{ color: 'var(--accent-success)' }}>Done</span>}
          {status.status === 'error' && (status.error ?? 'Error')}
          {status.status === 'downloading' && (
            pct !== null
              ? `${formatBytes(status.downloaded_bytes)} / ${formatBytes(status.total_bytes!)} (${pct.toFixed(1)}%)`
              : `${formatBytes(status.downloaded_bytes)}…`
          )}
        </span>
      </div>
      {status.status === 'downloading' && (
        <div className="h-1.5 rounded-full overflow-hidden" style={{ background: 'var(--border-subtle)' }}>
          <div
            className="h-full rounded-full transition-all"
            style={{
              width: pct !== null ? `${pct.toFixed(1)}%` : '100%',
              background: 'var(--accent-primary)',
              animation: pct === null ? 'pulse 1.5s ease-in-out infinite' : undefined,
            }}
          />
        </div>
      )}
    </div>
  )
}

// ─── OllamaPullRow ───────────────────────────────────────────────────────────

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
    <div
      className="rounded p-3 space-y-1.5"
      style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)' }}
    >
      <div className="flex items-center justify-between gap-2">
        <span className="text-sm font-medium truncate font-mono" style={{ color: 'var(--text-primary)' }}>
          {status.model}
        </span>
        <span className="text-xs shrink-0" style={{ color: status.status === 'error' ? 'var(--accent-danger)' : 'var(--text-muted)' }}>
          {status.status === 'done'    && <span style={{ color: 'var(--accent-success)' }}>Done</span>}
          {status.status === 'error'   && (status.error ?? 'Error')}
          {status.status === 'pulling' && (
            pct !== null
              ? `${formatBytes(status.pulled_bytes!)} / ${formatBytes(status.total_bytes!)} (${pct.toFixed(1)}%)`
              : (status.current_layer ?? 'Pulling…')
          )}
        </span>
      </div>
      {status.status === 'pulling' && (
        <div className="h-1.5 rounded-full overflow-hidden" style={{ background: 'var(--border-subtle)' }}>
          <div
            className="h-full rounded-full transition-all"
            style={{
              width: pct !== null ? `${pct.toFixed(1)}%` : '100%',
              background: 'var(--accent-warning, #f59e0b)',
              animation: pct === null ? 'pulse 1.5s ease-in-out infinite' : undefined,
            }}
          />
        </div>
      )}
      {status.status === 'pulling' && status.current_layer && pct !== null && (
        <p className="text-xs" style={{ color: 'var(--text-disabled)' }}>{status.current_layer}</p>
      )}
    </div>
  )
}

// ─── Ollama model presets ─────────────────────────────────────────────────────

const OLLAMA_PRESETS = [
  { label: 'Llama 3.2 3B',   model: 'llama3.2:3b',         size: '~2.0 GB' },
  { label: 'Llama 3.2 1B',   model: 'llama3.2:1b',         size: '~1.3 GB' },
  { label: 'Gemma 3 4B',     model: 'gemma3:4b',           size: '~3.3 GB' },
  { label: 'Qwen2.5 7B',     model: 'qwen2.5:7b',          size: '~4.7 GB' },
  { label: 'Mistral 7B',     model: 'mistral:7b',          size: '~4.1 GB' },
  { label: 'Phi-4 Mini',     model: 'phi4-mini',           size: '~2.5 GB' },
  { label: 'nomic-embed',    model: 'nomic-embed-text',    size: '~274 MB' },
  { label: 'DeepSeek-R1 7B', model: 'deepseek-r1:7b',     size: '~4.7 GB' },
]

// ─── Ollama Models section ────────────────────────────────────────────────────

function OllamaModelsSection() {
  const [data, setData] = useState<{ available: boolean; models: OllamaModelInfo[] } | null>(null)
  const [loading, setLoading] = useState(true)

  const refresh = useCallback(async () => {
    setLoading(true)
    try {
      const resp = await api.models.ollamaTags()
      setData(resp)
    } catch {
      setData({ available: false, models: [] })
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => { refresh() }, [refresh])

  return (
    <div className="card" style={{ padding: 0 }}>
      <div className="flex items-center justify-between px-5 py-4" style={{ borderBottom: '1px solid var(--border-subtle)' }}>
        <div className="flex items-center gap-2">
          <CloudDownload size={15} style={{ color: 'var(--accent-warning, #f59e0b)' }} />
          <h2 className="font-medium text-sm" style={{ color: 'var(--text-primary)' }}>Ollama Models</h2>
        </div>
        <button
          onClick={refresh}
          disabled={loading}
          className="flex items-center gap-1.5 text-xs transition-opacity hover:opacity-70 disabled:opacity-40"
          style={{ background: 'none', border: 'none', cursor: 'pointer', color: 'var(--text-muted)' }}
          title="Refresh"
        >
          <RefreshCw size={12} className={loading ? 'animate-spin' : ''} />
          Refresh
        </button>
      </div>

      {loading ? (
        <div className="flex items-center justify-center gap-2 py-8" style={{ color: 'var(--text-muted)' }}>
          <Loader2 size={14} className="animate-spin" />
          <span className="text-sm">Checking Ollama…</span>
        </div>
      ) : !data?.available ? (
        <div className="flex flex-col items-center justify-center gap-2 py-10" style={{ color: 'var(--text-muted)' }}>
          <XCircle size={24} style={{ opacity: 0.4 }} />
          <p className="text-sm">Ollama is not running.</p>
          <p className="text-xs" style={{ color: 'var(--text-disabled)' }}>
            Deploy it from the{' '}
            <a href="/apps" style={{ color: 'var(--accent-primary)', textDecoration: 'underline' }}>App Vault</a>.
          </p>
        </div>
      ) : data.models.length === 0 ? (
        <div className="flex flex-col items-center justify-center gap-2 py-10" style={{ color: 'var(--text-muted)' }}>
          <HardDrive size={24} style={{ opacity: 0.3 }} />
          <p className="text-sm">Ollama is running but no models are installed.</p>
          <p className="text-xs" style={{ color: 'var(--text-disabled)' }}>Use the "Pull via Ollama" section below to add one.</p>
        </div>
      ) : (
        <table className="w-full text-sm" style={{ borderCollapse: 'collapse' }}>
          <thead>
            <tr style={{ borderBottom: '1px solid var(--border-subtle)' }}>
              {['Model', 'Size', 'Modified'].map(h => (
                <th
                  key={h}
                  className="px-5 py-2 text-left text-xs font-medium uppercase tracking-wide"
                  style={{ color: 'var(--text-muted)' }}
                >
                  {h}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {data.models.map(m => {
              const modDate = m.modified_at
                ? new Date(m.modified_at).toLocaleDateString(undefined, { year: 'numeric', month: 'short', day: 'numeric' })
                : '—'
              return (
                <tr key={m.name} style={{ borderBottom: '1px solid var(--border-subtle)' }}>
                  <td className="px-5 py-3 font-mono text-xs" style={{ color: 'var(--text-primary)' }}>{m.name}</td>
                  <td className="px-5 py-3 text-xs" style={{ color: 'var(--text-secondary)' }}>{formatBytes(m.size)}</td>
                  <td className="px-5 py-3 text-xs" style={{ color: 'var(--text-secondary)' }}>{modDate}</td>
                </tr>
              )
            })}
          </tbody>
        </table>
      )}
    </div>
  )
}

// ─── Main page ────────────────────────────────────────────────────────────────

export default function ModelsPage() {
  const [models, setModels] = useState<ModelFile[]>([])
  const [loading, setLoading] = useState(true)
  const [loadingModel, setLoadingModel] = useState<string | null>(null)
  const [deletingModel, setDeletingModel] = useState<string | null>(null)

  const [url, setUrl] = useState('')
  const [filename, setFilename] = useState('')
  const [downloading, setDownloading] = useState(false)
  const [activeDownloads, setActiveDownloads] = useState<ActiveDownload[]>([])

  const [ollamaModel, setOllamaModel] = useState('')
  const [ollamaPulling, setOllamaPulling] = useState(false)
  const [activePulls, setActivePulls] = useState<ActivePull[]>([])

  const [activeCreates, setActiveCreates] = useState<ActivePull[]>([])
  const [creatingModel, setCreatingModel] = useState<string | null>(null)

  const [notification, setNotification] = useState<{ text: string; ok: boolean } | null>(null)
  const notifTimer = useRef<ReturnType<typeof setTimeout> | null>(null)

  const notify = useCallback((text: string, ok = true) => {
    setNotification({ text, ok })
    if (notifTimer.current) clearTimeout(notifTimer.current)
    notifTimer.current = setTimeout(() => setNotification(null), 4000)
  }, [])

  const fetchModels = useCallback(async () => {
    try {
      const list = await api.models.list()
      setModels(list)
    } catch {
      // silently ignore if auth not ready yet
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    fetchModels()
  }, [fetchModels])

  // Auto-refresh while any download is active
  useEffect(() => {
    const hasActive = activeDownloads.some(d => !d.status || d.status.status === 'downloading')
    if (!hasActive) return
    const interval = setInterval(fetchModels, 3000)
    return () => clearInterval(interval)
  }, [activeDownloads, fetchModels])

  const handleLoad = async (f: ModelFile) => {
    if (!window.confirm(`Load "${f.filename}" into llama.cpp? This will restart the container.`)) return
    setLoadingModel(f.filename)
    try {
      await api.models.loadModel(f.filename)
      notify(`Model "${f.filename}" loaded successfully.`)
      await fetchModels()
    } catch (e: any) {
      notify(e?.message ?? 'Failed to load model', false)
    } finally {
      setLoadingModel(null)
    }
  }

  const handleDelete = async (f: ModelFile) => {
    if (!window.confirm(`Delete "${f.filename}"? This cannot be undone.`)) return
    setDeletingModel(f.filename)
    try {
      await api.models.deleteModel(f.filename)
      notify(`Deleted "${f.filename}".`)
      setModels(prev => prev.filter(m => m.filename !== f.filename))
    } catch (e: any) {
      notify(e?.message ?? 'Failed to delete model', false)
    } finally {
      setDeletingModel(null)
    }
  }

  const handleDownload = async () => {
    if (!url.trim()) return
    setDownloading(true)
    try {
      const { id } = await api.models.startDownload(url.trim(), filename.trim() || undefined)
      setActiveDownloads(prev => [...prev, { id, status: null }])
      setUrl('')
      setFilename('')
    } catch (e: any) {
      notify(e?.message ?? 'Failed to start download', false)
    } finally {
      setDownloading(false)
    }
  }

  const handleDownloadDone = useCallback((id: string, status: DownloadStatus) => {
    if (status.status === 'done') {
      notify(`Downloaded "${status.filename}" successfully.`)
      fetchModels()
    } else if (status.status === 'error') {
      notify(`Download failed: ${status.error ?? 'unknown error'}`, false)
    }
    setTimeout(() => {
      setActiveDownloads(prev => prev.filter(d => d.id !== id))
    }, 4000)
  }, [notify, fetchModels])

  const handleOllamaPull = async () => {
    const model = ollamaModel.trim()
    if (!model) return
    setOllamaPulling(true)
    try {
      const { id } = await api.models.ollamaPull(model)
      setActivePulls(prev => [...prev, { id, status: null }])
      setOllamaModel('')
    } catch (e: any) {
      notify(e?.message ?? 'Failed to start pull', false)
    } finally {
      setOllamaPulling(false)
    }
  }

  const handleOllamaPullDone = useCallback((id: string, status: OllamaPullStatus) => {
    if (status.status === 'done') {
      notify(`Pulled "${status.model}" successfully.`)
      fetchModels()
    } else if (status.status === 'error') {
      notify(`Pull failed: ${status.error ?? 'unknown error'}`, false)
    }
    setTimeout(() => setActivePulls(prev => prev.filter(p => p.id !== id)), 4000)
  }, [notify, fetchModels])

  const handleOllamaCreate = async (filename: string) => {
    setCreatingModel(filename)
    try {
      const { id, model_name } = await api.models.ollamaCreate(filename)
      setActiveCreates(prev => [...prev, { id, status: null }])
      notify(`Loading "${model_name}" into Ollama…`)
    } catch (e: any) {
      notify(e?.message ?? 'Failed to load into Ollama', false)
    } finally {
      setCreatingModel(null)
    }
  }

  const handleOllamaCreateDone = useCallback((id: string, status: OllamaPullStatus) => {
    if (status.status === 'done') {
      notify(`"${status.model}" is ready in Ollama.`)
      fetchModels()
    } else if (status.status === 'error') {
      notify(`Load failed: ${status.error ?? 'unknown error'}`, false)
    }
    setTimeout(() => setActiveCreates(prev => prev.filter(p => p.id !== id)), 4000)
  }, [notify, fetchModels])

  return (
    <div className="p-6 space-y-6 max-w-5xl mx-auto">
      {/* Header */}
      <div className="flex items-center gap-3">
        <HardDrive size={22} style={{ color: 'var(--accent-primary)' }} />
        <h1 className="text-xl font-semibold" style={{ color: 'var(--text-primary)' }}>Models</h1>
      </div>

      {/* Notification */}
      {notification && (
        <div
          className="flex items-center gap-2 px-4 py-3 rounded text-sm"
          style={{
            background: notification.ok ? 'var(--accent-success)22' : 'var(--accent-danger)22',
            border: `1px solid ${notification.ok ? 'var(--accent-success)' : 'var(--accent-danger)'}44`,
            color: notification.ok ? 'var(--accent-success)' : 'var(--accent-danger)',
          }}
        >
          {notification.ok
            ? <CheckCircle size={14} />
            : <XCircle size={14} />}
          {notification.text}
        </div>
      )}

      {/* Your Models */}
      <div className="card" style={{ padding: 0 }}>
        <div className="flex items-center justify-between px-5 py-4" style={{ borderBottom: '1px solid var(--border-subtle)' }}>
          <h2 className="font-medium text-sm" style={{ color: 'var(--text-primary)' }}>Your Models</h2>
          <span className="text-xs" style={{ color: 'var(--text-muted)' }}>
            {models.filter(m => m.source === 'voidtower').length} local
            {models.some(m => m.source === 'ollama') && ` · ${models.filter(m => m.source === 'ollama').length} Ollama`}
          </span>
        </div>

        {loading ? (
          <div className="flex items-center justify-center gap-2 py-10" style={{ color: 'var(--text-muted)' }}>
            <Loader2 size={16} className="animate-spin" />
            <span className="text-sm">Loading…</span>
          </div>
        ) : models.length === 0 ? (
          <div className="flex flex-col items-center justify-center gap-2 py-12" style={{ color: 'var(--text-muted)' }}>
            <HardDrive size={28} style={{ opacity: 0.3 }} />
            <p className="text-sm">No models yet. Download one below.</p>
          </div>
        ) : (
          <table className="w-full text-sm" style={{ borderCollapse: 'collapse' }}>
            <thead>
              <tr style={{ borderBottom: '1px solid var(--border-subtle)' }}>
                {['Filename', 'Size', 'Source', 'Status', 'Actions'].map(h => (
                  <th
                    key={h}
                    className="px-5 py-2 text-left text-xs font-medium uppercase tracking-wide"
                    style={{ color: 'var(--text-muted)' }}
                  >
                    {h}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {models.map(m => (
                <tr
                  key={`${m.source}:${m.filename}`}
                  style={{ borderBottom: '1px solid var(--border-subtle)' }}
                >
                  <td className="px-5 py-3 font-mono text-xs" style={{ color: 'var(--text-primary)' }}>
                    {m.filename}
                  </td>
                  <td className="px-5 py-3 text-xs" style={{ color: 'var(--text-secondary)' }}>
                    {m.size_bytes ? formatBytes(m.size_bytes) : '—'}
                  </td>
                  <td className="px-5 py-3">
                    {m.source === 'ollama' ? (
                      <span
                        className="inline-flex items-center px-2 py-0.5 rounded text-xs font-medium"
                        style={{ background: 'var(--accent-warning, #f59e0b)22', color: 'var(--accent-warning, #f59e0b)', border: '1px solid var(--accent-warning, #f59e0b)44' }}
                      >
                        Ollama
                      </span>
                    ) : (
                      <span
                        className="inline-flex items-center px-2 py-0.5 rounded text-xs font-medium"
                        style={{ background: 'var(--accent-primary)22', color: 'var(--accent-primary)', border: '1px solid var(--accent-primary)44' }}
                      >
                        VoidTower
                      </span>
                    )}
                  </td>
                  <td className="px-5 py-3">
                    {m.active ? (
                      <span
                        className="inline-flex items-center gap-1 px-2 py-0.5 rounded text-xs font-medium"
                        style={{ background: 'var(--accent-success)22', color: 'var(--accent-success)' }}
                      >
                        <CheckCircle size={11} /> Active
                      </span>
                    ) : (
                      <span className="text-xs" style={{ color: 'var(--text-disabled)' }}>—</span>
                    )}
                  </td>
                  <td className="px-5 py-3">
                    {m.source === 'voidtower' ? (
                      <div className="flex items-center gap-2">
                        <button
                          disabled={loadingModel === m.filename}
                          onClick={() => handleLoad(m)}
                          className="flex items-center gap-1.5 px-3 py-1.5 rounded text-xs font-medium transition-opacity hover:opacity-80 disabled:opacity-40"
                          style={{ background: 'var(--accent-primary)22', color: 'var(--accent-primary)', border: '1px solid var(--accent-primary)44' }}
                          title="Load into llama.cpp"
                        >
                          {loadingModel === m.filename ? <Loader2 size={11} className="animate-spin" /> : <Play size={11} />}
                          llama.cpp
                        </button>
                        <button
                          disabled={creatingModel === m.filename}
                          onClick={() => handleOllamaCreate(m.filename)}
                          className="flex items-center gap-1.5 px-3 py-1.5 rounded text-xs font-medium transition-opacity hover:opacity-80 disabled:opacity-40"
                          style={{ background: 'var(--accent-warning, #f59e0b)22', color: 'var(--accent-warning, #f59e0b)', border: '1px solid var(--accent-warning, #f59e0b)44' }}
                          title="Import into Ollama"
                        >
                          {creatingModel === m.filename ? <Loader2 size={11} className="animate-spin" /> : <CloudDownload size={11} />}
                          Ollama
                        </button>
                        <button
                          disabled={deletingModel === m.filename}
                          onClick={() => handleDelete(m)}
                          className="flex items-center gap-1.5 px-3 py-1.5 rounded text-xs font-medium transition-opacity hover:opacity-80 disabled:opacity-40"
                          style={{ background: 'var(--accent-danger)22', color: 'var(--accent-danger)', border: '1px solid var(--accent-danger)44' }}
                          title="Delete model file"
                        >
                          {deletingModel === m.filename ? <Loader2 size={11} className="animate-spin" /> : <Trash2 size={11} />}
                          Delete
                        </button>
                      </div>
                    ) : (
                      <span className="text-xs" style={{ color: 'var(--text-disabled)' }}>Managed by Ollama</span>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>

      {/* Ollama Models */}
      <OllamaModelsSection />

      {/* Active Ollama imports */}
      {activeCreates.length > 0 && (
        <div className="card space-y-2">
          <p className="text-xs font-medium" style={{ color: 'var(--text-muted)' }}>Importing into Ollama</p>
          {activeCreates.map(pull => (
            <OllamaPullRow
              key={pull.id}
              pull={pull}
              onDone={() => {
                api.models.ollamaCreateStatus(pull.id).then(s => handleOllamaCreateDone(pull.id, s)).catch(() => {})
              }}
            />
          ))}
        </div>
      )}

      {/* Pull via Ollama */}
      <div className="card space-y-5">
        <div className="flex items-center gap-2">
          <CloudDownload size={16} style={{ color: 'var(--accent-warning, #f59e0b)' }} />
          <h2 className="font-medium text-sm" style={{ color: 'var(--text-primary)' }}>Pull via Ollama</h2>
        </div>

        {/* Ollama presets */}
        <div>
          <p className="text-xs mb-3" style={{ color: 'var(--text-muted)' }}>Popular models — click to fill</p>
          <div className="grid gap-2" style={{ gridTemplateColumns: 'repeat(auto-fill, minmax(160px, 1fr))' }}>
            {OLLAMA_PRESETS.map(p => (
              <button
                key={p.model}
                onClick={() => setOllamaModel(p.model)}
                className="text-left rounded p-3 transition-all hover:opacity-80"
                style={{
                  background: ollamaModel === p.model ? 'var(--accent-warning, #f59e0b)22' : 'var(--bg-elevated)',
                  border: `1px solid ${ollamaModel === p.model ? 'var(--accent-warning, #f59e0b)' : 'var(--border-default)'}`,
                }}
              >
                <div className="text-xs font-medium" style={{ color: 'var(--text-primary)' }}>{p.label}</div>
                <div className="text-xs mt-0.5 font-mono" style={{ color: 'var(--text-muted)' }}>{p.model}</div>
                <div className="text-xs mt-0.5" style={{ color: 'var(--text-disabled)' }}>{p.size}</div>
              </button>
            ))}
          </div>
        </div>

        {/* Model name input */}
        <div className="flex gap-2">
          <input
            style={{ ...inputStyle, flex: 1 }}
            placeholder="llama3.2, gemma3:4b, …"
            value={ollamaModel}
            onChange={e => setOllamaModel(e.target.value)}
            onKeyDown={e => e.key === 'Enter' && handleOllamaPull()}
          />
          <button
            onClick={handleOllamaPull}
            disabled={ollamaPulling || !ollamaModel.trim()}
            className="flex items-center gap-2 px-4 py-2 rounded text-sm font-medium transition-opacity hover:opacity-80 disabled:opacity-40"
            style={{ background: 'var(--accent-warning, #f59e0b)', color: 'white', whiteSpace: 'nowrap' }}
          >
            {ollamaPulling ? <Loader2 size={14} className="animate-spin" /> : <CloudDownload size={14} />}
            Pull
          </button>
        </div>

        {/* Active pulls */}
        {activePulls.length > 0 && (
          <div className="space-y-2">
            <p className="text-xs font-medium" style={{ color: 'var(--text-muted)' }}>Active Pulls</p>
            {activePulls.map(pull => (
              <OllamaPullRow
                key={pull.id}
                pull={pull}
                onDone={() => {
                  api.models.ollamaPullStatus(pull.id).then(s => handleOllamaPullDone(pull.id, s)).catch(() => {})
                }}
              />
            ))}
          </div>
        )}
      </div>

      {/* Download Model (GGUF) */}
      <div className="card space-y-5">
        <div className="flex items-center gap-2">
          <Download size={16} style={{ color: 'var(--accent-primary)' }} />
          <h2 className="font-medium text-sm" style={{ color: 'var(--text-primary)' }}>Download GGUF Model</h2>
        </div>

        {/* Popular presets */}
        <div>
          <p className="text-xs mb-3" style={{ color: 'var(--text-muted)' }}>Popular presets — click to fill URL</p>
          <div className="grid grid-cols-2 gap-2" style={{ gridTemplateColumns: 'repeat(auto-fill, minmax(200px, 1fr))' }}>
            {PRESETS.map(p => (
              <button
                key={p.url}
                onClick={() => {
                  setUrl(p.url)
                  const name = p.url.split('/').pop()?.split('?')[0] ?? ''
                  setFilename(name)
                }}
                className="text-left rounded p-3 transition-all hover:opacity-80"
                style={{
                  background: url === p.url ? 'var(--accent-primary)22' : 'var(--bg-elevated)',
                  border: `1px solid ${url === p.url ? 'var(--accent-primary)' : 'var(--border-default)'}`,
                }}
              >
                <div className="text-xs font-medium" style={{ color: 'var(--text-primary)' }}>{p.label}</div>
                <div className="text-xs mt-0.5" style={{ color: 'var(--text-muted)' }}>{p.size}</div>
              </button>
            ))}
          </div>
        </div>

        {/* URL + filename inputs */}
        <div className="space-y-3">
          <div>
            <label className="block text-xs mb-1" style={{ color: 'var(--text-muted)' }}>URL</label>
            <input
              style={inputStyle}
              placeholder="https://huggingface.co/…/model.gguf"
              value={url}
              onChange={e => setUrl(e.target.value)}
              onKeyDown={e => e.key === 'Enter' && handleDownload()}
            />
          </div>
          <div>
            <label className="block text-xs mb-1" style={{ color: 'var(--text-muted)' }}>
              Filename <span style={{ color: 'var(--text-disabled)' }}>(optional — auto-detected from URL)</span>
            </label>
            <input
              style={inputStyle}
              placeholder="my-model.gguf"
              value={filename}
              onChange={e => setFilename(e.target.value)}
              onKeyDown={e => e.key === 'Enter' && handleDownload()}
            />
          </div>
          <button
            onClick={handleDownload}
            disabled={downloading || !url.trim()}
            className="flex items-center gap-2 px-4 py-2 rounded text-sm font-medium transition-opacity hover:opacity-80 disabled:opacity-40"
            style={{ background: 'var(--accent-primary)', color: 'white' }}
          >
            {downloading ? <Loader2 size={14} className="animate-spin" /> : <Download size={14} />}
            Download
          </button>
        </div>

        {/* Active downloads */}
        {activeDownloads.length > 0 && (
          <div className="space-y-2">
            <p className="text-xs font-medium" style={{ color: 'var(--text-muted)' }}>Active Downloads</p>
            {activeDownloads.map(dl => (
              <DownloadRow
                key={dl.id}
                dl={dl}
                onDone={() => {
                  api.models.downloadStatus(dl.id).then(s => handleDownloadDone(dl.id, s)).catch(() => {})
                }}
              />
            ))}
          </div>
        )}
      </div>
    </div>
  )
}
