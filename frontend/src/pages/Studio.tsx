import { useCallback, useEffect, useRef, useState } from 'react'
import {
  Wand2, Image, Mic, MicOff, Music, Search, Loader2, Download, Trash2,
  RefreshCw, Volume2, Copy, Send, ChevronRight,
} from 'lucide-react'

// ── types ─────────────────────────────────────────────────────────────────────

interface StudioService {
  name: string
  kind: string
  url: string
  status: 'online' | 'offline'
  version?: string
}

interface StudioStatus {
  services: StudioService[]
  gpu?: { name: string; vram_used_mb: number; vram_total_mb: number; utilization_pct: number }
}

interface GalleryItem {
  id: string
  kind: 'image' | 'audio'
  filename: string
  url: string
  created_at: number
  size_bytes: number
}

const VOICES = [
  { id: 'af_heart',    label: 'Heart (US F)' },
  { id: 'af_bella',    label: 'Bella (US F)' },
  { id: 'af_sarah',    label: 'Sarah (US F)' },
  { id: 'af_sky',      label: 'Sky (US F)'   },
  { id: 'af_nicole',   label: 'Nicole (US F)'},
  { id: 'am_adam',     label: 'Adam (US M)'  },
  { id: 'am_michael',  label: 'Michael (US M)'},
  { id: 'bf_emma',     label: 'Emma (UK F)'  },
  { id: 'bf_isabella', label: 'Isabella (UK F)'},
  { id: 'bm_george',   label: 'George (UK M)'},
  { id: 'bm_lewis',    label: 'Lewis (UK M)' },
]

const STT_LANGUAGES = [
  { id: 'auto', label: 'Auto detect' }, { id: 'en', label: 'English' },
  { id: 'es', label: 'Spanish' }, { id: 'fr', label: 'French' },
  { id: 'de', label: 'German' }, { id: 'zh', label: 'Chinese' },
  { id: 'ja', label: 'Japanese' }, { id: 'ko', label: 'Korean' },
  { id: 'pt', label: 'Portuguese' }, { id: 'ru', label: 'Russian' },
  { id: 'ar', label: 'Arabic' }, { id: 'it', label: 'Italian' },
]

const IMAGE_SIZES = ['512×512', '768×512', '512×768', '768×768', '1024×512', '512×1024', '1024×1024']

// ── helpers ───────────────────────────────────────────────────────────────────

function fmtBytes(b: number) {
  if (b < 1024) return `${b} B`
  if (b < 1024 * 1024) return `${(b / 1024).toFixed(1)} KB`
  return `${(b / 1024 / 1024).toFixed(1)} MB`
}

function fmtDate(ts: number) {
  return new Date(ts * 1000).toLocaleString(undefined, {
    month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit',
  })
}

// ── shared ui ─────────────────────────────────────────────────────────────────

const card: React.CSSProperties = {
  background: 'var(--bg-panel)',
  border: '1px solid var(--border-default)',
  borderRadius: 10,
  padding: 16,
}

const input: React.CSSProperties = {
  background: 'var(--bg-card)',
  border: '1px solid var(--border-default)',
  borderRadius: 6,
  color: 'var(--text-primary)',
  padding: '6px 10px',
  fontSize: 13,
  width: '100%',
  outline: 'none',
}

const select: React.CSSProperties = { ...input, cursor: 'pointer' }

const btn = (variant: 'primary' | 'ghost' | 'danger' = 'primary'): React.CSSProperties => ({
  display: 'inline-flex', alignItems: 'center', gap: 6,
  padding: '7px 14px', borderRadius: 6, fontSize: 13, fontWeight: 500,
  cursor: 'pointer', border: 'none',
  background: variant === 'primary' ? 'var(--accent-primary)' : variant === 'danger' ? 'var(--accent-danger-subtle)' : 'var(--bg-elevated)',
  color: variant === 'primary' ? '#fff' : variant === 'danger' ? 'var(--accent-danger)' : 'var(--text-primary)',
})

function StatusDot({ online }: { online: boolean }) {
  return (
    <span style={{
      width: 7, height: 7, borderRadius: '50%', flexShrink: 0,
      background: online ? 'var(--accent-success)' : 'var(--text-disabled)',
      display: 'inline-block',
    }} />
  )
}

// ── Overview tab ──────────────────────────────────────────────────────────────

function OverviewTab({ studioStatus, onRefresh }: { studioStatus: StudioStatus | null; onRefresh: () => void }) {
  const services = studioStatus?.services ?? []
  const gpu = studioStatus?.gpu

  const kindIcon = (kind: string) => {
    if (kind === 'image') return <Image size={13} />
    if (kind === 'tts')   return <Volume2 size={13} />
    if (kind === 'stt')   return <Mic size={13} />
    return <Wand2 size={13} />
  }

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 20 }}>
      {/* Services */}
      <div>
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 12 }}>
          <div style={{ fontSize: 12, fontWeight: 600, color: 'var(--text-muted)', textTransform: 'uppercase', letterSpacing: '0.05em' }}>
            Local AI Services
          </div>
          <button onClick={onRefresh} style={btn('ghost')}>
            <RefreshCw size={13} /> Refresh
          </button>
        </div>
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(220px, 1fr))', gap: 10 }}>
          {services.map(svc => (
            <div key={svc.name} style={{ ...card, display: 'flex', alignItems: 'center', gap: 10, padding: 14 }}>
              <StatusDot online={svc.status === 'online'} />
              <div style={{ flex: 1, minWidth: 0 }}>
                <div style={{ fontSize: 12, fontWeight: 500, display: 'flex', alignItems: 'center', gap: 5 }}>
                  {kindIcon(svc.kind)}
                  <span style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{svc.name}</span>
                </div>
                <div style={{ fontSize: 10, color: 'var(--text-muted)', marginTop: 1 }}>
                  {svc.status === 'online'
                    ? <span style={{ color: 'var(--accent-success)' }}>Online{svc.version ? ` · v${svc.version}` : ''}</span>
                    : <span>Offline · <a href="/apps" style={{ color: 'var(--accent-primary)', textDecoration: 'none' }}>Deploy via App Vault <ChevronRight size={10} style={{ verticalAlign: 'middle' }} /></a></span>
                  }
                </div>
              </div>
            </div>
          ))}
          {services.length === 0 && (
            <div style={{ gridColumn: '1/-1', color: 'var(--text-muted)', fontSize: 13, padding: 20 }}>
              Loading services…
            </div>
          )}
        </div>
      </div>

      {/* GPU */}
      {gpu && (
        <div>
          <div style={{ fontSize: 12, fontWeight: 600, color: 'var(--text-muted)', textTransform: 'uppercase', letterSpacing: '0.05em', marginBottom: 10 }}>
            GPU
          </div>
          <div style={{ ...card, display: 'flex', gap: 24 }}>
            <div>
              <div style={{ fontSize: 11, color: 'var(--text-muted)' }}>Device</div>
              <div style={{ fontSize: 13, fontWeight: 500 }}>{gpu.name}</div>
            </div>
            <div>
              <div style={{ fontSize: 11, color: 'var(--text-muted)' }}>VRAM</div>
              <div style={{ fontSize: 13, fontWeight: 500 }}>{gpu.vram_used_mb} / {gpu.vram_total_mb} MB</div>
            </div>
            <div>
              <div style={{ fontSize: 11, color: 'var(--text-muted)' }}>Utilization</div>
              <div style={{ fontSize: 13, fontWeight: 500 }}>{gpu.utilization_pct}%</div>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}

// ── Image tab ─────────────────────────────────────────────────────────────────

function ImageTab({ services }: { services: StudioService[] }) {
  const sdOnline  = services.find(s => s.name === 'Stable Diffusion WebUI')?.status === 'online'
  const cfOnline  = services.find(s => s.name === 'ComfyUI')?.status === 'online'
  const anyOnline = sdOnline || cfOnline

  const [prompt,    setPrompt]   = useState('')
  const [negPrompt, setNegPrompt] = useState('blurry, ugly, watermark, text, deformed')
  const [size,      setSize]     = useState('512×512')
  const [steps,     setSteps]    = useState(20)
  const [cfg,       setCfg]      = useState(7)
  const [seed,      setSeed]     = useState('-1')
  const [backend,   setBackend]  = useState('auto')
  const [loading,   setLoading]  = useState(false)
  const [err,       setErr]      = useState('')
  const [result,    setResult]   = useState<{ url: string; filename: string } | null>(null)
  const [history,   setHistory]  = useState<Array<{ url: string; filename: string }>>([])

  const [w, h] = size.split('×').map(Number)

  async function generate() {
    if (!prompt.trim()) return
    setLoading(true); setErr('')
    try {
      const r = await fetch('/api/studio/image/generate', {
        method: 'POST', credentials: 'include',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          prompt, negative_prompt: negPrompt, width: w, height: h,
          steps, cfg_scale: cfg, seed: parseInt(seed) || -1, backend,
        }),
      })
      const d = await r.json()
      if (!r.ok) throw new Error(d.message || 'Generation failed')
      setResult(d)
      setHistory(prev => [d, ...prev].slice(0, 12))
    } catch (e: any) { setErr(e.message) }
    finally { setLoading(false) }
  }

  if (!anyOnline) {
    return (
      <div style={{ ...card, textAlign: 'center', padding: 40 }}>
        <Image size={32} style={{ color: 'var(--text-disabled)', marginBottom: 12 }} />
        <div style={{ fontWeight: 500, marginBottom: 6 }}>No image generation service running</div>
        <div style={{ fontSize: 13, color: 'var(--text-muted)', marginBottom: 16 }}>
          Deploy <strong>Stable Diffusion WebUI</strong> or <strong>ComfyUI</strong> from the App Vault to enable image generation.
        </div>
        <a href="/apps" style={{ ...btn(), textDecoration: 'none' }}>Open App Vault</a>
      </div>
    )
  }

  return (
    <div style={{ display: 'grid', gridTemplateColumns: '320px 1fr', gap: 20 }}>
      {/* Form */}
      <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
        <div style={card}>
          <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
            <div>
              <label style={{ fontSize: 11, color: 'var(--text-muted)', display: 'block', marginBottom: 4 }}>Prompt</label>
              <textarea
                value={prompt}
                onChange={e => setPrompt(e.target.value)}
                onKeyDown={e => e.key === 'Enter' && e.ctrlKey && generate()}
                placeholder="Describe the image…"
                rows={4}
                style={{ ...input, resize: 'vertical' }}
              />
            </div>
            <div>
              <label style={{ fontSize: 11, color: 'var(--text-muted)', display: 'block', marginBottom: 4 }}>Negative prompt</label>
              <textarea value={negPrompt} onChange={e => setNegPrompt(e.target.value)} rows={2} style={{ ...input, resize: 'vertical' }} />
            </div>
            <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 8 }}>
              <div>
                <label style={{ fontSize: 11, color: 'var(--text-muted)', display: 'block', marginBottom: 4 }}>Size</label>
                <select value={size} onChange={e => setSize(e.target.value)} style={select}>
                  {IMAGE_SIZES.map(s => <option key={s} value={s}>{s}</option>)}
                </select>
              </div>
              <div>
                <label style={{ fontSize: 11, color: 'var(--text-muted)', display: 'block', marginBottom: 4 }}>Backend</label>
                <select value={backend} onChange={e => setBackend(e.target.value)} style={select}>
                  <option value="auto">Auto</option>
                  {sdOnline && <option value="sdwebui">SD WebUI</option>}
                  {cfOnline && <option value="comfyui">ComfyUI</option>}
                </select>
              </div>
            </div>
            <div>
              <label style={{ fontSize: 11, color: 'var(--text-muted)', display: 'block', marginBottom: 4 }}>Steps: {steps}</label>
              <input type="range" min={1} max={50} value={steps} onChange={e => setSteps(+e.target.value)} style={{ width: '100%', accentColor: 'var(--accent-primary)' }} />
            </div>
            <div>
              <label style={{ fontSize: 11, color: 'var(--text-muted)', display: 'block', marginBottom: 4 }}>CFG scale: {cfg}</label>
              <input type="range" min={1} max={20} step={0.5} value={cfg} onChange={e => setCfg(+e.target.value)} style={{ width: '100%', accentColor: 'var(--accent-primary)' }} />
            </div>
            <div>
              <label style={{ fontSize: 11, color: 'var(--text-muted)', display: 'block', marginBottom: 4 }}>Seed</label>
              <input value={seed} onChange={e => setSeed(e.target.value)} placeholder="-1 (random)" style={input} />
            </div>
            {err && <div style={{ fontSize: 12, color: 'var(--accent-danger)', padding: '6px 8px', background: 'var(--accent-danger-subtle)', borderRadius: 4 }}>{err}</div>}
            <button onClick={generate} disabled={loading || !prompt.trim()} style={{ ...btn(), opacity: loading || !prompt.trim() ? 0.6 : 1 }}>
              {loading ? <Loader2 size={14} className="animate-spin" /> : <Wand2 size={14} />}
              {loading ? 'Generating…' : 'Generate (Ctrl+Enter)'}
            </button>
          </div>
        </div>
      </div>

      {/* Result + history */}
      <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
        <div style={{ ...card, minHeight: 300, display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
          {loading ? (
            <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 12, color: 'var(--text-muted)' }}>
              <Loader2 size={28} className="animate-spin" style={{ color: 'var(--accent-primary)' }} />
              <span style={{ fontSize: 13 }}>Generating image…</span>
            </div>
          ) : result ? (
            <div style={{ width: '100%', textAlign: 'center' }}>
              <img
                src={result.url}
                alt="Generated"
                style={{ maxWidth: '100%', maxHeight: 480, borderRadius: 6, border: '1px solid var(--border-default)' }}
              />
              <div style={{ display: 'flex', justifyContent: 'center', gap: 8, marginTop: 10 }}>
                <a href={result.url} download={result.filename} style={{ ...btn('ghost'), textDecoration: 'none' }}>
                  <Download size={13} /> Download
                </a>
              </div>
            </div>
          ) : (
            <div style={{ color: 'var(--text-disabled)', fontSize: 13, display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 8 }}>
              <Image size={32} style={{ opacity: 0.3 }} />
              Enter a prompt and click Generate
            </div>
          )}
        </div>

        {history.length > 0 && (
          <div>
            <div style={{ fontSize: 11, color: 'var(--text-muted)', marginBottom: 8 }}>History</div>
            <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
              {history.map((item, i) => (
                <img
                  key={i}
                  src={item.url}
                  alt=""
                  onClick={() => setResult(item)}
                  style={{
                    width: 72, height: 72, objectFit: 'cover', borderRadius: 4,
                    border: `2px solid ${result?.filename === item.filename ? 'var(--accent-primary)' : 'var(--border-default)'}`,
                    cursor: 'pointer',
                  }}
                />
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  )
}

// ── TTS tab ───────────────────────────────────────────────────────────────────

function TtsTab({ services }: { services: StudioService[] }) {
  const online = services.find(s => s.name === 'Kokoro TTS')?.status === 'online'

  const [text,    setText]    = useState('')
  const [voice,   setVoice]   = useState('af_heart')
  const [speed,   setSpeed]   = useState(1.0)
  const [loading, setLoading] = useState(false)
  const [err,     setErr]     = useState('')
  const [audioUrl, setAudioUrl] = useState<string | null>(null)
  const [recent,  setRecent]  = useState<GalleryItem[]>([])
  const audioRef = useRef<HTMLAudioElement>(null)

  const loadRecent = useCallback(async () => {
    const r = await fetch('/api/studio/gallery', { credentials: 'include' })
    if (r.ok) {
      const all: GalleryItem[] = await r.json()
      setRecent(all.filter(i => i.kind === 'audio').slice(0, 5))
    }
  }, [])

  useEffect(() => { loadRecent() }, [loadRecent])

  async function generate() {
    if (!text.trim()) return
    setLoading(true); setErr('')
    try {
      const r = await fetch('/api/studio/tts/generate', {
        method: 'POST', credentials: 'include',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ text, voice, speed }),
      })
      const d = await r.json()
      if (!r.ok) throw new Error(d.message || 'TTS failed')
      setAudioUrl(d.url)
      setTimeout(() => audioRef.current?.play(), 100)
      loadRecent()
    } catch (e: any) { setErr(e.message) }
    finally { setLoading(false) }
  }

  if (!online) {
    return (
      <div style={{ ...card, textAlign: 'center', padding: 40 }}>
        <Volume2 size={32} style={{ color: 'var(--text-disabled)', marginBottom: 12 }} />
        <div style={{ fontWeight: 500, marginBottom: 6 }}>Kokoro TTS not running</div>
        <div style={{ fontSize: 13, color: 'var(--text-muted)', marginBottom: 16 }}>
          Deploy <strong>Kokoro TTS</strong> from the App Vault to enable text-to-speech generation.
        </div>
        <a href="/apps" style={{ ...btn(), textDecoration: 'none' }}>Open App Vault</a>
      </div>
    )
  }

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
      <div style={card}>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
          <div>
            <label style={{ fontSize: 11, color: 'var(--text-muted)', display: 'block', marginBottom: 4 }}>Text to speak</label>
            <textarea
              value={text}
              onChange={e => setText(e.target.value)}
              placeholder="Enter text to convert to speech…"
              rows={5}
              style={{ ...input, resize: 'vertical' }}
            />
          </div>
          <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 12 }}>
            <div>
              <label style={{ fontSize: 11, color: 'var(--text-muted)', display: 'block', marginBottom: 4 }}>Voice</label>
              <select value={voice} onChange={e => setVoice(e.target.value)} style={select}>
                {VOICES.map(v => <option key={v.id} value={v.id}>{v.label}</option>)}
              </select>
            </div>
            <div>
              <label style={{ fontSize: 11, color: 'var(--text-muted)', display: 'block', marginBottom: 4 }}>Speed: {speed.toFixed(1)}×</label>
              <input type="range" min={0.5} max={2.0} step={0.1} value={speed} onChange={e => setSpeed(+e.target.value)} style={{ width: '100%', marginTop: 10, accentColor: 'var(--accent-primary)' }} />
            </div>
          </div>
          {err && <div style={{ fontSize: 12, color: 'var(--accent-danger)', padding: '6px 8px', background: 'var(--accent-danger-subtle)', borderRadius: 4 }}>{err}</div>}
          <button onClick={generate} disabled={loading || !text.trim()} style={{ ...btn(), opacity: loading || !text.trim() ? 0.6 : 1, alignSelf: 'flex-start' }}>
            {loading ? <Loader2 size={14} className="animate-spin" /> : <Volume2 size={14} />}
            {loading ? 'Generating…' : 'Generate speech'}
          </button>
        </div>
      </div>

      {audioUrl && (
        <div style={card}>
          <div style={{ fontSize: 12, fontWeight: 500, marginBottom: 8 }}>Output</div>
          <audio ref={audioRef} src={audioUrl} controls style={{ width: '100%' }} />
          <div style={{ display: 'flex', gap: 8, marginTop: 10 }}>
            <a href={audioUrl} download style={{ ...btn('ghost'), textDecoration: 'none' }}>
              <Download size={13} /> Download
            </a>
          </div>
        </div>
      )}

      {recent.length > 0 && (
        <div style={card}>
          <div style={{ fontSize: 12, fontWeight: 500, marginBottom: 10 }}>Recent</div>
          <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
            {recent.map(item => (
              <div key={item.id} style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '6px 0', borderBottom: '1px solid var(--border-subtle)' }}>
                <Music size={12} style={{ color: 'var(--text-muted)', flexShrink: 0 }} />
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div style={{ fontSize: 11, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{item.filename}</div>
                  <div style={{ fontSize: 10, color: 'var(--text-muted)' }}>{fmtBytes(item.size_bytes)} · {fmtDate(item.created_at)}</div>
                </div>
                <button onClick={() => setAudioUrl(item.url)} style={{ ...btn('ghost'), padding: '4px 8px', fontSize: 11 }}>
                  Play
                </button>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  )
}

// ── STT tab ───────────────────────────────────────────────────────────────────

function SttTab({ services }: { services: StudioService[] }) {
  const online  = services.find(s => s.name === 'Whisper')?.status === 'online'
  const [lang,     setLang]    = useState('auto')
  const [loading,  setLoading] = useState(false)
  const [err,      setErr]     = useState('')
  const [transcript, setTranscript] = useState('')
  const [recording, setRecording]  = useState(false)
  const [mediaRec,  setMediaRec]   = useState<MediaRecorder | null>(null)
  const fileRef   = useRef<HTMLInputElement>(null)
  const chunks    = useRef<Blob[]>([])

  async function transcribe(file: File | Blob, name: string) {
    setLoading(true); setErr('')
    try {
      const form = new FormData()
      form.append('file', file, name)
      if (lang !== 'auto') form.append('language', lang)
      const r = await fetch('/api/studio/stt/transcribe', {
        method: 'POST', credentials: 'include', body: form,
      })
      const d = await r.json()
      if (!r.ok) throw new Error(d.message || 'Transcription failed')
      setTranscript(d.text)
    } catch (e: any) { setErr(e.message) }
    finally { setLoading(false) }
  }

  function onFileChange(e: React.ChangeEvent<HTMLInputElement>) {
    const f = e.target.files?.[0]
    if (f) transcribe(f, f.name)
  }

  async function toggleRecord() {
    if (recording) {
      mediaRec?.stop()
      setRecording(false)
      return
    }
    try {
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true })
      const mr = new MediaRecorder(stream, { mimeType: 'audio/webm' })
      chunks.current = []
      mr.ondataavailable = e => { if (e.data.size > 0) chunks.current.push(e.data) }
      mr.onstop = () => {
        const blob = new Blob(chunks.current, { type: 'audio/webm' })
        transcribe(blob, 'recording.webm')
        stream.getTracks().forEach(t => t.stop())
      }
      mr.start()
      setMediaRec(mr)
      setRecording(true)
    } catch { setErr('Microphone access denied') }
  }

  if (!online) {
    return (
      <div style={{ ...card, textAlign: 'center', padding: 40 }}>
        <Mic size={32} style={{ color: 'var(--text-disabled)', marginBottom: 12 }} />
        <div style={{ fontWeight: 500, marginBottom: 6 }}>Whisper not running</div>
        <div style={{ fontSize: 13, color: 'var(--text-muted)', marginBottom: 16 }}>
          Deploy <strong>faster-whisper-server</strong> from the App Vault to enable speech-to-text transcription.
        </div>
        <a href="/apps" style={{ ...btn(), textDecoration: 'none' }}>Open App Vault</a>
      </div>
    )
  }

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
      <div style={card}>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
          <div style={{ display: 'flex', gap: 10, alignItems: 'flex-start' }}>
            <div
              onClick={() => fileRef.current?.click()}
              style={{
                flex: 1, border: '2px dashed var(--border-default)', borderRadius: 8,
                padding: '28px 20px', textAlign: 'center', cursor: 'pointer',
                background: 'var(--bg-card)',
              }}
            >
              <Search size={22} style={{ color: 'var(--text-disabled)', marginBottom: 8 }} />
              <div style={{ fontSize: 13, color: 'var(--text-muted)' }}>Drop audio / video file or click to browse</div>
              <div style={{ fontSize: 11, color: 'var(--text-disabled)', marginTop: 4 }}>MP3, WAV, M4A, WebM, OGG, MP4</div>
            </div>
            <input ref={fileRef} type="file" accept="audio/*,video/*" onChange={onFileChange} style={{ display: 'none' }} />
            <button
              onClick={toggleRecord}
              style={{
                ...btn(recording ? 'danger' : 'ghost'),
                flexDirection: 'column', gap: 4, padding: '16px 20px', alignSelf: 'stretch',
              }}
            >
              {recording ? <MicOff size={20} /> : <Mic size={20} />}
              <span style={{ fontSize: 11 }}>{recording ? 'Stop' : 'Record'}</span>
            </button>
          </div>

          <div>
            <label style={{ fontSize: 11, color: 'var(--text-muted)', display: 'block', marginBottom: 4 }}>Language</label>
            <select value={lang} onChange={e => setLang(e.target.value)} style={{ ...select, width: 'auto' }}>
              {STT_LANGUAGES.map(l => <option key={l.id} value={l.id}>{l.label}</option>)}
            </select>
          </div>

          {loading && (
            <div style={{ display: 'flex', alignItems: 'center', gap: 8, color: 'var(--text-muted)', fontSize: 13 }}>
              <Loader2 size={14} className="animate-spin" style={{ color: 'var(--accent-primary)' }} /> Transcribing…
            </div>
          )}
          {err && <div style={{ fontSize: 12, color: 'var(--accent-danger)', padding: '6px 8px', background: 'var(--accent-danger-subtle)', borderRadius: 4 }}>{err}</div>}
        </div>
      </div>

      {transcript && (
        <div style={card}>
          <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 10 }}>
            <div style={{ fontSize: 12, fontWeight: 500 }}>Transcript</div>
            <div style={{ display: 'flex', gap: 8 }}>
              <button onClick={() => navigator.clipboard.writeText(transcript)} style={{ ...btn('ghost'), padding: '4px 10px', fontSize: 11 }}>
                <Copy size={11} /> Copy
              </button>
              <a
                href={`/ai?q=${encodeURIComponent('Summarize: ' + transcript.slice(0, 200))}`}
                style={{ ...btn('ghost'), padding: '4px 10px', fontSize: 11, textDecoration: 'none' }}
              >
                <Send size={11} /> Send to Odysseus
              </a>
            </div>
          </div>
          <textarea
            readOnly
            value={transcript}
            rows={8}
            style={{ ...input, resize: 'vertical', lineHeight: 1.5 }}
          />
        </div>
      )}
    </div>
  )
}

// ── Gallery tab ───────────────────────────────────────────────────────────────

function GalleryTab() {
  const [items,   setItems]   = useState<GalleryItem[]>([])
  const [loading, setLoading] = useState(true)
  const [filter,  setFilter]  = useState<'all' | 'image' | 'audio'>('all')

  const load = useCallback(async () => {
    setLoading(true)
    const r = await fetch('/api/studio/gallery', { credentials: 'include' })
    if (r.ok) setItems(await r.json())
    setLoading(false)
  }, [])

  useEffect(() => { load() }, [load])

  async function del(item: GalleryItem) {
    await fetch(`/api/studio/gallery/${item.kind}/${item.filename}`, { method: 'DELETE', credentials: 'include' })
    load()
  }

  const visible = items.filter(i => filter === 'all' || i.kind === filter)

  return (
    <div>
      <div style={{ display: 'flex', alignItems: 'center', gap: 10, marginBottom: 16 }}>
        {(['all', 'image', 'audio'] as const).map(f => (
          <button key={f} onClick={() => setFilter(f)} style={{
            ...btn(filter === f ? 'primary' : 'ghost'),
            padding: '5px 12px', fontSize: 12, textTransform: 'capitalize',
          }}>{f}</button>
        ))}
        <div style={{ flex: 1 }} />
        <button onClick={load} style={btn('ghost')}><RefreshCw size={13} /></button>
      </div>

      {loading ? (
        <div style={{ textAlign: 'center', padding: 40, color: 'var(--text-muted)' }}>
          <Loader2 size={20} className="animate-spin" style={{ color: 'var(--accent-primary)' }} />
        </div>
      ) : visible.length === 0 ? (
        <div style={{ ...card, textAlign: 'center', padding: 40, color: 'var(--text-muted)', fontSize: 13 }}>
          No generated files yet. Use the Image or TTS tabs to create something.
        </div>
      ) : (
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(180px, 1fr))', gap: 12 }}>
          {visible.map(item => (
            <div key={item.id} style={{ ...card, padding: 0, overflow: 'hidden' }}>
              {item.kind === 'image' ? (
                <img src={item.url} alt={item.filename} style={{ width: '100%', aspectRatio: '1', objectFit: 'cover', display: 'block' }} />
              ) : (
                <div style={{ padding: 16, background: 'var(--bg-elevated)', display: 'flex', flexDirection: 'column', gap: 6, aspectRatio: '1', alignItems: 'center', justifyContent: 'center' }}>
                  <Music size={28} style={{ color: 'var(--text-muted)' }} />
                  <audio src={item.url} controls style={{ width: '100%', marginTop: 8 }} />
                </div>
              )}
              <div style={{ padding: '8px 10px' }}>
                <div style={{ fontSize: 10, color: 'var(--text-muted)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', marginBottom: 4 }}>{item.filename}</div>
                <div style={{ fontSize: 10, color: 'var(--text-disabled)' }}>{fmtBytes(item.size_bytes)} · {fmtDate(item.created_at)}</div>
                <div style={{ display: 'flex', gap: 6, marginTop: 8 }}>
                  <a href={item.url} download={item.filename} style={{ ...btn('ghost'), padding: '3px 8px', fontSize: 10, textDecoration: 'none', flex: 1, justifyContent: 'center' }}>
                    <Download size={10} />
                  </a>
                  <button onClick={() => del(item)} style={{ ...btn('danger'), padding: '3px 8px', fontSize: 10, flex: 1, justifyContent: 'center' }}>
                    <Trash2 size={10} />
                  </button>
                </div>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}

// ── MCP Tools tab ─────────────────────────────────────────────────────────────

interface McpTool {
  name: string
  description: string
  inputSchema: {
    type: string
    properties?: Record<string, { type: string; description?: string }>
    required?: string[]
  }
}

function McpToolsTab() {
  const [tools,    setTools]    = useState<McpTool[]>([])
  const [selected, setSelected] = useState<McpTool | null>(null)
  const [args,     setArgs]     = useState<Record<string, string>>({})
  const [loading,  setLoading]  = useState(true)
  const [running,  setRunning]  = useState(false)
  const [result,   setResult]   = useState<{ ok: boolean; text: string } | null>(null)

  useEffect(() => {
    fetch('/api/studio/mcp/tools', { credentials: 'include' })
      .then(r => r.json())
      .then(d => { setTools(d.tools ?? []); setLoading(false) })
      .catch(() => setLoading(false))
  }, [])

  function selectTool(t: McpTool) {
    setSelected(t)
    setArgs({})
    setResult(null)
  }

  async function invoke() {
    if (!selected) return
    setRunning(true); setResult(null)
    try {
      const parsedArgs: Record<string, unknown> = {}
      for (const [k, v] of Object.entries(args)) {
        const propType = selected.inputSchema.properties?.[k]?.type
        parsedArgs[k] = propType === 'number' || propType === 'integer' ? Number(v) : v
      }
      const r = await fetch('/api/studio/mcp/invoke', {
        method: 'POST', credentials: 'include',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ name: selected.name, arguments: parsedArgs }),
      })
      const d = await r.json()
      let text = d.result ?? d.error ?? ''
      try { text = JSON.stringify(JSON.parse(text), null, 2) } catch { /* use raw */ }
      setResult({ ok: d.ok, text })
    } catch (e: any) { setResult({ ok: false, text: e.message }) }
    finally { setRunning(false) }
  }

  const props = selected?.inputSchema.properties ?? {}
  const required = new Set(selected?.inputSchema.required ?? [])

  return (
    <div style={{ display: 'grid', gridTemplateColumns: '240px 1fr', gap: 16, minHeight: 400 }}>
      {/* Tool list */}
      <div style={{ ...card, padding: 0, overflow: 'hidden' }}>
        <div style={{ padding: '10px 12px', fontSize: 11, fontWeight: 600, color: 'var(--text-muted)', textTransform: 'uppercase', letterSpacing: '0.05em', borderBottom: '1px solid var(--border-default)' }}>
          VoidTower MCP Tools
        </div>
        {loading
          ? <div style={{ padding: 20, color: 'var(--text-muted)', fontSize: 13 }}>Loading…</div>
          : tools.map(t => (
            <div
              key={t.name}
              onClick={() => selectTool(t)}
              style={{
                padding: '9px 12px', cursor: 'pointer', borderBottom: '1px solid var(--border-subtle)',
                background: selected?.name === t.name ? 'var(--accent-primary)1a' : 'transparent',
                borderLeft: `3px solid ${selected?.name === t.name ? 'var(--accent-primary)' : 'transparent'}`,
              }}
            >
              <div style={{ fontSize: 12, fontWeight: 500, fontFamily: 'monospace' }}>{t.name}</div>
              <div style={{ fontSize: 10, color: 'var(--text-muted)', marginTop: 2, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{t.description}</div>
            </div>
          ))
        }
      </div>

      {/* Tool detail */}
      {selected ? (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 14 }}>
          <div style={card}>
            <div style={{ fontSize: 14, fontWeight: 600, fontFamily: 'monospace', marginBottom: 4 }}>{selected.name}</div>
            <div style={{ fontSize: 13, color: 'var(--text-muted)' }}>{selected.description}</div>
          </div>

          <div style={card}>
            <div style={{ fontSize: 11, fontWeight: 600, color: 'var(--text-muted)', textTransform: 'uppercase', letterSpacing: '0.05em', marginBottom: 12 }}>
              Arguments
            </div>
            {Object.keys(props).length === 0
              ? <div style={{ fontSize: 13, color: 'var(--text-muted)' }}>No arguments required</div>
              : Object.entries(props).map(([name, schema]) => (
                <div key={name} style={{ marginBottom: 10 }}>
                  <label style={{ fontSize: 11, color: 'var(--text-muted)', display: 'block', marginBottom: 4 }}>
                    <code style={{ fontFamily: 'monospace' }}>{name}</code>
                    {required.has(name) && <span style={{ color: 'var(--accent-danger)', marginLeft: 4 }}>*</span>}
                    {schema.description && <span style={{ marginLeft: 6, fontStyle: 'italic' }}>{schema.description}</span>}
                    <span style={{ marginLeft: 6, opacity: 0.5 }}>({schema.type})</span>
                  </label>
                  <input
                    value={args[name] ?? ''}
                    onChange={e => setArgs(a => ({ ...a, [name]: e.target.value }))}
                    placeholder={schema.type}
                    style={input}
                  />
                </div>
              ))
            }
            <button onClick={invoke} disabled={running} style={{ ...btn(), opacity: running ? 0.6 : 1, marginTop: 8 }}>
              {running ? <Loader2 size={13} className="animate-spin" /> : null}
              {running ? 'Running…' : 'Invoke'}
            </button>
          </div>

          {result && (
            <div style={{ ...card, borderLeft: `3px solid ${result.ok ? 'var(--accent-primary)' : 'var(--accent-danger)'}` }}>
              <div style={{ fontSize: 11, fontWeight: 600, color: result.ok ? 'var(--accent-primary)' : 'var(--accent-danger)', marginBottom: 8, textTransform: 'uppercase', letterSpacing: '0.05em' }}>
                {result.ok ? 'Result' : 'Error'}
              </div>
              <pre style={{ margin: 0, fontSize: 11, fontFamily: 'monospace', color: 'var(--text-primary)', whiteSpace: 'pre-wrap', wordBreak: 'break-word', maxHeight: 400, overflow: 'auto' }}>
                {result.text}
              </pre>
            </div>
          )}
        </div>
      ) : (
        <div style={{ ...card, display: 'flex', alignItems: 'center', justifyContent: 'center', color: 'var(--text-disabled)', fontSize: 13 }}>
          Select a tool from the list
        </div>
      )}
    </div>
  )
}

// ── main page ─────────────────────────────────────────────────────────────────

type Tab = 'overview' | 'image' | 'tts' | 'stt' | 'gallery' | 'mcp'

const TABS: { id: Tab; label: string; icon: React.ReactNode }[] = [
  { id: 'overview', label: 'Overview',  icon: <Wand2 size={13} /> },
  { id: 'image',    label: 'Image',     icon: <Image size={13} /> },
  { id: 'tts',      label: 'TTS',       icon: <Volume2 size={13} /> },
  { id: 'stt',      label: 'STT',       icon: <Mic size={13} /> },
  { id: 'gallery',  label: 'Gallery',   icon: <Search size={13} /> },
  { id: 'mcp',      label: 'MCP Tools', icon: <ChevronRight size={13} /> },
]

export default function StudioPage() {
  const [tab,         setTab]         = useState<Tab>('overview')
  const [studioStatus, setStudioStatus] = useState<StudioStatus | null>(null)
  const [statusLoading, setStatusLoading] = useState(true)

  const loadStatus = useCallback(async () => {
    setStatusLoading(true)
    try {
      const r = await fetch('/api/studio/status', { credentials: 'include' })
      if (r.ok) setStudioStatus(await r.json())
    } finally { setStatusLoading(false) }
  }, [])

  useEffect(() => { loadStatus() }, [loadStatus])

  const services = studioStatus?.services ?? []
  const onlineCount = services.filter(s => s.status === 'online').length

  return (
    <div style={{ padding: 24, maxWidth: 1200, margin: '0 auto' }}>
      {/* Header */}
      <div style={{ display: 'flex', alignItems: 'center', gap: 12, marginBottom: 24 }}>
        <Wand2 size={20} style={{ color: 'var(--accent-primary)' }} />
        <div>
          <h1 style={{ margin: 0, fontSize: 18, fontWeight: 600 }}>AI Creative Studio</h1>
          <div style={{ fontSize: 12, color: 'var(--text-muted)', marginTop: 2 }}>
            {statusLoading
              ? 'Detecting services…'
              : `${onlineCount} of ${services.length} services online`}
          </div>
        </div>
        <div style={{ flex: 1 }} />
        {/* Online service dots */}
        <div style={{ display: 'flex', gap: 6, alignItems: 'center' }}>
          {services.map(svc => (
            <span key={svc.name} title={`${svc.name}: ${svc.status}`}>
              <StatusDot online={svc.status === 'online'} />
            </span>
          ))}
        </div>
      </div>

      {/* Tab bar */}
      <div style={{ display: 'flex', gap: 4, marginBottom: 20, borderBottom: '1px solid var(--border-default)', paddingBottom: 0 }}>
        {TABS.map(t => (
          <button
            key={t.id}
            onClick={() => setTab(t.id)}
            style={{
              display: 'inline-flex', alignItems: 'center', gap: 6,
              padding: '8px 14px', fontSize: 13, fontWeight: 500,
              background: 'none', border: 'none', cursor: 'pointer',
              color: tab === t.id ? 'var(--accent-primary)' : 'var(--text-muted)',
              borderBottom: `2px solid ${tab === t.id ? 'var(--accent-primary)' : 'transparent'}`,
              marginBottom: -1,
            }}
          >
            {t.icon}{t.label}
          </button>
        ))}
      </div>

      {/* Content */}
      {tab === 'overview' && <OverviewTab studioStatus={studioStatus} onRefresh={loadStatus} />}
      {tab === 'image'    && <ImageTab services={services} />}
      {tab === 'tts'      && <TtsTab services={services} />}
      {tab === 'stt'      && <SttTab services={services} />}
      {tab === 'gallery'  && <GalleryTab />}
      {tab === 'mcp'      && <McpToolsTab />}
    </div>
  )
}
