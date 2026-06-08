import { useCallback, useEffect, useRef, useState } from 'react'
import {
  Wand2, Image, Mic, MicOff, Music, Loader2, Download, Trash2,
  RefreshCw, Volume2, Copy, ChevronDown, ChevronRight, Send,
  Zap, Box, Cpu,
} from 'lucide-react'

// ── types ─────────────────────────────────────────────────────────────────────

interface StudioService {
  name: string; kind: string; url: string
  status: 'online' | 'offline'; version?: string
}

interface StudioStatus {
  services: StudioService[]
  gpu?: { name: string; vram_used_mb: number; vram_total_mb: number; utilization_pct: number }
}

interface GalleryItem {
  id: string; kind: 'image' | 'audio'; filename: string
  url: string; created_at: number; size_bytes: number
}

interface OllamaModel { name: string; size?: number; details?: { family?: string; parameter_size?: string } }

interface McpTool {
  name: string; description: string
  inputSchema: { type: string; properties?: Record<string, { type: string; description?: string }>; required?: string[] }
}

interface ManifestTool {
  name: string; description: string; api: string
  risk: string; input: Record<string, string>
}

const VOICES = [
  { id: 'af_heart', label: 'Heart (US F)' }, { id: 'af_bella', label: 'Bella (US F)' },
  { id: 'af_sarah', label: 'Sarah (US F)' }, { id: 'af_sky',   label: 'Sky (US F)'   },
  { id: 'am_adam',  label: 'Adam (US M)'  }, { id: 'am_michael', label: 'Michael (US M)' },
  { id: 'bf_emma',  label: 'Emma (UK F)'  }, { id: 'bm_george',  label: 'George (UK M)' },
]

const IMAGE_SIZES = ['512×512', '768×512', '512×768', '768×768', '1024×1024']
const STT_LANGS   = [
  { id: 'auto', label: 'Auto' }, { id: 'en', label: 'English' }, { id: 'es', label: 'Spanish' },
  { id: 'fr', label: 'French' }, { id: 'de', label: 'German' }, { id: 'zh', label: 'Chinese' },
  { id: 'ja', label: 'Japanese' }, { id: 'pt', label: 'Portuguese' }, { id: 'ru', label: 'Russian' },
]

// ── helpers ───────────────────────────────────────────────────────────────────


function fmtDate(ts: number) {
  return new Date(ts * 1000).toLocaleString(undefined, { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' })
}

// ── shared primitives ─────────────────────────────────────────────────────────

const S = {
  card: {
    background: 'var(--bg-panel)',
    border: '1px solid var(--border-subtle)',
    borderRadius: 10,
    overflow: 'hidden',
  } as React.CSSProperties,

  inp: {
    background: 'var(--bg-card)',
    border: '1px solid var(--border-subtle)',
    borderRadius: 6,
    color: 'var(--text-primary)',
    padding: '6px 10px',
    fontSize: 12,
    width: '100%',
    outline: 'none',
  } as React.CSSProperties,

  select: {
    background: 'var(--bg-card)',
    border: '1px solid var(--border-subtle)',
    borderRadius: 6,
    color: 'var(--text-primary)',
    padding: '5px 8px',
    fontSize: 12,
    cursor: 'pointer',
    outline: 'none',
  } as React.CSSProperties,

  btnPrimary: {
    display: 'inline-flex', alignItems: 'center', gap: 5,
    padding: '6px 14px', borderRadius: 6, fontSize: 12, fontWeight: 500,
    cursor: 'pointer', border: 'none',
    background: 'var(--accent-primary)', color: '#fff',
  } as React.CSSProperties,

  btnGhost: {
    display: 'inline-flex', alignItems: 'center', gap: 5,
    padding: '5px 10px', borderRadius: 6, fontSize: 12,
    cursor: 'pointer', border: '1px solid var(--border-subtle)',
    background: 'transparent', color: 'var(--text-secondary)',
  } as React.CSSProperties,

  label: { fontSize: 11, color: 'var(--text-muted)', display: 'block', marginBottom: 4 } as React.CSSProperties,

  sectionHead: {
    display: 'flex', alignItems: 'center', gap: 8,
    padding: '10px 14px', cursor: 'pointer', userSelect: 'none' as const,
    borderBottom: '1px solid var(--border-subtle)',
  } as React.CSSProperties,

  colHead: {
    padding: '12px 14px', fontSize: 11, fontWeight: 700,
    textTransform: 'uppercase' as const, letterSpacing: '0.06em',
    color: 'var(--text-muted)', borderBottom: '1px solid var(--border-subtle)',
    display: 'flex', alignItems: 'center', gap: 7,
  } as React.CSSProperties,
}

function StatusDot({ online }: { online: boolean }) {
  return (
    <span style={{
      width: 6, height: 6, borderRadius: '50%', flexShrink: 0, display: 'inline-block',
      background: online ? 'var(--accent-success)' : 'var(--text-disabled)',
    }} />
  )
}

function Accordion({ label, icon, online, defaultOpen = false, children }: {
  label: string; icon: React.ReactNode; online: boolean
  defaultOpen?: boolean; children: React.ReactNode
}) {
  const [open, setOpen] = useState(defaultOpen)
  return (
    <div style={{ borderBottom: '1px solid var(--border-subtle)' }}>
      <div style={S.sectionHead} onClick={() => setOpen(o => !o)}>
        <StatusDot online={online} />
        {icon}
        <span style={{ fontSize: 12, fontWeight: 600, flex: 1 }}>{label}</span>
        {open ? <ChevronDown size={13} style={{ color: 'var(--text-muted)' }} />
               : <ChevronRight size={13} style={{ color: 'var(--text-muted)' }} />}
      </div>
      {open && <div style={{ padding: 14 }}>{children}</div>}
    </div>
  )
}

function OfflineNotice({ name }: { name: string }) {
  return (
    <div style={{ fontSize: 12, color: 'var(--text-muted)', lineHeight: 1.5 }}>
      <strong>{name}</strong> is not running.{' '}
      <a href="/apps" style={{ color: 'var(--accent-primary)' }}>Deploy via App Vault →</a>
    </div>
  )
}

// ── Image section ─────────────────────────────────────────────────────────────

function ImageSection({ services }: { services: StudioService[] }) {
  const sdOn = services.find(s => s.name === 'Stable Diffusion WebUI')?.status === 'online'
  const cfOn = services.find(s => s.name === 'ComfyUI')?.status === 'online'
  const online = sdOn || cfOn

  const [prompt,    setPrompt]    = useState('')
  const [negPrompt, setNegPrompt] = useState('blurry, ugly, watermark')
  const [size,      setSize]      = useState('512×512')
  const [steps,     setSteps]     = useState(20)
  const [cfg,       setCfg]       = useState(7)
  const [backend,   setBackend]   = useState('auto')
  const [loading,   setLoading]   = useState(false)
  const [err,       setErr]       = useState('')
  const [result,    setResult]    = useState<{ url: string; filename: string } | null>(null)
  const [history,   setHistory]   = useState<Array<{ url: string; filename: string }>>([])

  const [w, h] = size.split('×').map(Number)

  async function generate() {
    if (!prompt.trim()) return
    setLoading(true); setErr('')
    try {
      const r = await fetch('/api/studio/image/generate', {
        method: 'POST', credentials: 'include',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ prompt, negative_prompt: negPrompt, width: w, height: h, steps, cfg_scale: cfg, seed: -1, backend }),
      })
      const d = await r.json()
      if (!r.ok) throw new Error(d.message || 'Failed')
      setResult(d); setHistory(prev => [d, ...prev].slice(0, 8))
    } catch (e: any) { setErr(e.message) }
    finally { setLoading(false) }
  }

  return (
    <Accordion label="Image Generation" icon={<Image size={13} />} online={online} defaultOpen>
      {!online
        ? <OfflineNotice name="Stable Diffusion WebUI or ComfyUI" />
        : (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
            <div>
              <label style={S.label}>Prompt</label>
              <textarea value={prompt} onChange={e => setPrompt(e.target.value)}
                onKeyDown={e => e.ctrlKey && e.key === 'Enter' && generate()}
                placeholder="Describe the image… (Ctrl+Enter to generate)"
                rows={3} style={{ ...S.inp, resize: 'vertical' }} />
            </div>
            <div>
              <label style={S.label}>Negative prompt</label>
              <input value={negPrompt} onChange={e => setNegPrompt(e.target.value)} style={S.inp} />
            </div>
            <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 8 }}>
              <div>
                <label style={S.label}>Size</label>
                <select value={size} onChange={e => setSize(e.target.value)} style={{ ...S.select, width: '100%' }}>
                  {IMAGE_SIZES.map(s => <option key={s} value={s}>{s}</option>)}
                </select>
              </div>
              <div>
                <label style={S.label}>Backend</label>
                <select value={backend} onChange={e => setBackend(e.target.value)} style={{ ...S.select, width: '100%' }}>
                  <option value="auto">Auto</option>
                  {sdOn && <option value="sdwebui">SD WebUI</option>}
                  {cfOn && <option value="comfyui">ComfyUI</option>}
                </select>
              </div>
            </div>
            <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 8 }}>
              <div>
                <label style={S.label}>Steps: {steps}</label>
                <input type="range" min={1} max={50} value={steps} onChange={e => setSteps(+e.target.value)}
                  style={{ width: '100%', accentColor: 'var(--accent-primary)' }} />
              </div>
              <div>
                <label style={S.label}>CFG: {cfg}</label>
                <input type="range" min={1} max={20} step={0.5} value={cfg} onChange={e => setCfg(+e.target.value)}
                  style={{ width: '100%', accentColor: 'var(--accent-primary)' }} />
              </div>
            </div>
            {err && <div style={{ fontSize: 11, color: 'var(--accent-danger)', padding: '5px 8px', background: 'var(--accent-danger-subtle)', borderRadius: 4 }}>{err}</div>}
            <button onClick={generate} disabled={loading || !prompt.trim()}
              style={{ ...S.btnPrimary, opacity: loading || !prompt.trim() ? 0.6 : 1 }}>
              {loading ? <Loader2 size={12} className="animate-spin" /> : <Wand2 size={12} />}
              {loading ? 'Generating…' : 'Generate'}
            </button>
            {result && (
              <div>
                <img src={result.url} alt="" style={{ width: '100%', borderRadius: 6, border: '1px solid var(--border-subtle)' }} />
                <div style={{ display: 'flex', gap: 8, marginTop: 6 }}>
                  <a href={result.url} download={result.filename} style={{ ...S.btnGhost, textDecoration: 'none' }}>
                    <Download size={11} /> Save
                  </a>
                </div>
              </div>
            )}
            {history.length > 1 && (
              <div style={{ display: 'flex', gap: 4, flexWrap: 'wrap' }}>
                {history.slice(1).map((item, i) => (
                  <img key={i} src={item.url} alt="" onClick={() => setResult(item)}
                    style={{ width: 48, height: 48, objectFit: 'cover', borderRadius: 4, cursor: 'pointer', border: `1px solid var(--border-subtle)` }} />
                ))}
              </div>
            )}
          </div>
        )
      }
    </Accordion>
  )
}

// ── TTS section ───────────────────────────────────────────────────────────────

function TtsSection({ services }: { services: StudioService[] }) {
  const online = services.find(s => s.name === 'Kokoro TTS')?.status === 'online'
  const [text,    setText]    = useState('')
  const [voice,   setVoice]   = useState('af_heart')
  const [speed,   setSpeed]   = useState(1.0)
  const [loading, setLoading] = useState(false)
  const [err,     setErr]     = useState('')
  const [audioUrl, setAudioUrl] = useState<string | null>(null)
  const audioRef = useRef<HTMLAudioElement>(null)

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
      if (!r.ok) throw new Error(d.message || 'Failed')
      setAudioUrl(d.url)
      setTimeout(() => audioRef.current?.play(), 100)
    } catch (e: any) { setErr(e.message) }
    finally { setLoading(false) }
  }

  return (
    <Accordion label="Text to Speech" icon={<Volume2 size={13} />} online={online}>
      {!online
        ? <OfflineNotice name="Kokoro TTS" />
        : (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
            <div>
              <label style={S.label}>Text</label>
              <textarea value={text} onChange={e => setText(e.target.value)}
                placeholder="Enter text to speak…" rows={4}
                style={{ ...S.inp, resize: 'vertical' }} />
            </div>
            <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 8 }}>
              <div>
                <label style={S.label}>Voice</label>
                <select value={voice} onChange={e => setVoice(e.target.value)} style={{ ...S.select, width: '100%' }}>
                  {VOICES.map(v => <option key={v.id} value={v.id}>{v.label}</option>)}
                </select>
              </div>
              <div>
                <label style={S.label}>Speed: {speed.toFixed(1)}×</label>
                <input type="range" min={0.5} max={2} step={0.1} value={speed} onChange={e => setSpeed(+e.target.value)}
                  style={{ width: '100%', marginTop: 8, accentColor: 'var(--accent-primary)' }} />
              </div>
            </div>
            {err && <div style={{ fontSize: 11, color: 'var(--accent-danger)', padding: '5px 8px', background: 'var(--accent-danger-subtle)', borderRadius: 4 }}>{err}</div>}
            <button onClick={generate} disabled={loading || !text.trim()}
              style={{ ...S.btnPrimary, opacity: loading || !text.trim() ? 0.6 : 1 }}>
              {loading ? <Loader2 size={12} className="animate-spin" /> : <Volume2 size={12} />}
              {loading ? 'Generating…' : 'Speak'}
            </button>
            {audioUrl && (
              <div>
                <audio ref={audioRef} src={audioUrl} controls style={{ width: '100%', marginTop: 4 }} />
                <a href={audioUrl} download style={{ ...S.btnGhost, textDecoration: 'none', marginTop: 6, fontSize: 11 }}>
                  <Download size={11} /> Save
                </a>
              </div>
            )}
          </div>
        )
      }
    </Accordion>
  )
}

// ── STT section ───────────────────────────────────────────────────────────────

function SttSection({ services }: { services: StudioService[] }) {
  const online = services.find(s => s.name === 'Whisper')?.status === 'online'
  const [lang,      setLang]      = useState('auto')
  const [loading,   setLoading]   = useState(false)
  const [recording, setRecording] = useState(false)
  const [transcript, setTranscript] = useState('')
  const [err,       setErr]       = useState('')
  const fileRef = useRef<HTMLInputElement>(null)
  const mrRef   = useRef<MediaRecorder | null>(null)
  const chunks  = useRef<Blob[]>([])

  async function transcribe(file: File | Blob, name: string) {
    setLoading(true); setErr('')
    try {
      const form = new FormData()
      form.append('file', file, name)
      if (lang !== 'auto') form.append('language', lang)
      const r = await fetch('/api/studio/stt/transcribe', { method: 'POST', credentials: 'include', body: form })
      const d = await r.json()
      if (!r.ok) throw new Error(d.message || 'Failed')
      setTranscript(d.text)
    } catch (e: any) { setErr(e.message) }
    finally { setLoading(false) }
  }

  async function toggleRecord() {
    if (recording) { mrRef.current?.stop(); setRecording(false); return }
    try {
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true })
      const mr = new MediaRecorder(stream, { mimeType: 'audio/webm' })
      chunks.current = []
      mr.ondataavailable = e => { if (e.data.size > 0) chunks.current.push(e.data) }
      mr.onstop = () => {
        transcribe(new Blob(chunks.current, { type: 'audio/webm' }), 'recording.webm')
        stream.getTracks().forEach(t => t.stop())
      }
      mr.start(); mrRef.current = mr; setRecording(true)
    } catch { setErr('Microphone access denied') }
  }

  return (
    <Accordion label="Speech to Text" icon={<Mic size={13} />} online={online}>
      {!online
        ? <OfflineNotice name="Whisper" />
        : (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
            <div style={{ display: 'flex', gap: 8 }}>
              <button onClick={() => fileRef.current?.click()} style={{ ...S.btnGhost, flex: 1, justifyContent: 'center' }}>
                Upload audio
              </button>
              <button onClick={toggleRecord}
                style={{ ...S.btnGhost, flex: 1, justifyContent: 'center', borderColor: recording ? 'var(--accent-danger)' : undefined, color: recording ? 'var(--accent-danger)' : undefined }}>
                {recording ? <><MicOff size={12} /> Stop</> : <><Mic size={12} /> Record</>}
              </button>
              <input ref={fileRef} type="file" accept="audio/*,video/*"
                onChange={e => { const f = e.target.files?.[0]; if (f) transcribe(f, f.name) }}
                style={{ display: 'none' }} />
            </div>
            <div>
              <label style={S.label}>Language</label>
              <select value={lang} onChange={e => setLang(e.target.value)} style={S.select}>
                {STT_LANGS.map(l => <option key={l.id} value={l.id}>{l.label}</option>)}
              </select>
            </div>
            {loading && <div style={{ fontSize: 11, color: 'var(--text-muted)', display: 'flex', alignItems: 'center', gap: 6 }}><Loader2 size={12} className="animate-spin" /> Transcribing…</div>}
            {err && <div style={{ fontSize: 11, color: 'var(--accent-danger)', padding: '5px 8px', background: 'var(--accent-danger-subtle)', borderRadius: 4 }}>{err}</div>}
            {transcript && (
              <div>
                <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 6 }}>
                  <label style={S.label}>Transcript</label>
                  <div style={{ display: 'flex', gap: 4 }}>
                    <button onClick={() => navigator.clipboard.writeText(transcript)} style={{ ...S.btnGhost, padding: '3px 8px', fontSize: 11 }}><Copy size={10} /></button>
                    <a href={`/ai?q=${encodeURIComponent('Summarize: ' + transcript.slice(0, 200))}`}
                      style={{ ...S.btnGhost, padding: '3px 8px', fontSize: 11, textDecoration: 'none' }}><Send size={10} /></a>
                  </div>
                </div>
                <textarea readOnly value={transcript} rows={5} style={{ ...S.inp, resize: 'vertical' }} />
              </div>
            )}
          </div>
        )
      }
    </Accordion>
  )
}

// ── Gallery strip ─────────────────────────────────────────────────────────────

function GalleryStrip() {
  const [items, setItems] = useState<GalleryItem[]>([])

  useEffect(() => {
    fetch('/api/studio/gallery', { credentials: 'include' })
      .then(r => r.json()).then(d => setItems(d.slice(0, 12)))
      .catch(() => {})
  }, [])

  const del = async (item: GalleryItem) => {
    await fetch(`/api/studio/gallery/${item.kind}/${item.filename}`, { method: 'DELETE', credentials: 'include' })
    setItems(prev => prev.filter(i => i.id !== item.id))
  }

  if (items.length === 0) return null

  return (
    <div style={{ padding: '12px 14px', borderTop: '1px solid var(--border-subtle)' }}>
      <div style={{ fontSize: 10, fontWeight: 600, color: 'var(--text-muted)', textTransform: 'uppercase', letterSpacing: '0.05em', marginBottom: 8 }}>
        Gallery
      </div>
      <div style={{ display: 'flex', gap: 6, flexWrap: 'wrap' }}>
        {items.map(item => (
          <div key={item.id} style={{ position: 'relative', flexShrink: 0 }} title={`${item.filename}\n${fmtDate(item.created_at)}`}>
            {item.kind === 'image'
              ? <img src={item.url} alt="" style={{ width: 52, height: 52, objectFit: 'cover', borderRadius: 4, border: '1px solid var(--border-subtle)', display: 'block' }} />
              : <div style={{ width: 52, height: 52, borderRadius: 4, background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)', display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
                  <Music size={18} style={{ color: 'var(--text-muted)' }} />
                </div>
            }
            <div style={{ position: 'absolute', inset: 0, opacity: 0, background: 'rgba(0,0,0,0.6)', borderRadius: 4, display: 'flex', alignItems: 'center', justifyContent: 'center', gap: 4, transition: 'opacity 0.15s' }}
              onMouseEnter={e => (e.currentTarget.style.opacity = '1')}
              onMouseLeave={e => (e.currentTarget.style.opacity = '0')}>
              <a href={item.url} download={item.filename} style={{ color: '#fff', display: 'flex' }}><Download size={13} /></a>
              <button onClick={() => del(item)} style={{ background: 'none', border: 'none', color: 'var(--accent-danger)', cursor: 'pointer', padding: 0, display: 'flex' }}><Trash2 size={13} /></button>
            </div>
          </div>
        ))}
      </div>
    </div>
  )
}

// ── Generation column ─────────────────────────────────────────────────────────

function GenerationColumn({ services }: { services: StudioService[] }) {
  return (
    <div style={S.card}>
      <div style={S.colHead}>
        <Wand2 size={13} style={{ color: 'var(--accent-primary)' }} />
        Generation
      </div>
      <ImageSection services={services} />
      <TtsSection   services={services} />
      <SttSection   services={services} />
      <GalleryStrip />
    </div>
  )
}

// ── Models column ─────────────────────────────────────────────────────────────

function ModelsColumn({ gpu }: { gpu?: StudioStatus['gpu'] }) {
  const [ollama, setOllama] = useState<OllamaModel[]>([])
  const [llama,  setLlama]  = useState<{ pid: number; name: string }[]>([])
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    Promise.all([
      fetch('/api/models/ollama', { credentials: 'include' }).then(r => r.ok ? r.json() : { models: [] }),
      fetch('/api/ai/llama', { credentials: 'include' }).then(r => r.ok ? r.json() : { processes: [] }),
    ]).then(([om, lm]) => {
      setOllama(om.models ?? [])
      setLlama(lm.processes ?? [])
    }).finally(() => setLoading(false))
  }, [])

  const vramPct = gpu ? Math.round((gpu.vram_used_mb / gpu.vram_total_mb) * 100) : 0

  return (
    <div style={S.card}>
      <div style={S.colHead}>
        <Cpu size={13} style={{ color: 'var(--accent-secondary)' }} />
        Models
      </div>

      {/* GPU */}
      <div style={{ padding: '12px 14px', borderBottom: '1px solid var(--border-subtle)' }}>
        <div style={{ fontSize: 11, fontWeight: 600, color: 'var(--text-muted)', marginBottom: 8 }}>GPU</div>
        {gpu ? (
          <>
            <div style={{ fontSize: 12, fontWeight: 500, marginBottom: 6 }}>{gpu.name}</div>
            <div style={{ height: 5, background: 'var(--bg-elevated)', borderRadius: 3, overflow: 'hidden', marginBottom: 4 }}>
              <div style={{ height: '100%', width: `${vramPct}%`, background: vramPct > 90 ? 'var(--accent-danger)' : vramPct > 70 ? 'var(--accent-warning)' : 'var(--accent-primary)', borderRadius: 3 }} />
            </div>
            <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: 10, color: 'var(--text-muted)' }}>
              <span>VRAM {gpu.vram_used_mb} / {gpu.vram_total_mb} MB</span>
              <span>{gpu.utilization_pct}% util</span>
            </div>
          </>
        ) : (
          <div style={{ fontSize: 12, color: 'var(--text-muted)' }}>No GPU detected</div>
        )}
      </div>

      {/* llama.cpp */}
      <div style={{ padding: '12px 14px', borderBottom: '1px solid var(--border-subtle)' }}>
        <div style={{ fontSize: 11, fontWeight: 600, color: 'var(--text-muted)', marginBottom: 6 }}>llama.cpp</div>
        {loading ? <div style={{ fontSize: 11, color: 'var(--text-muted)' }}>Loading…</div>
          : llama.length === 0
            ? <div style={{ fontSize: 11, color: 'var(--text-muted)' }}>Not running</div>
            : llama.map(p => (
              <div key={p.pid} style={{ fontSize: 11, color: 'var(--accent-success)', display: 'flex', alignItems: 'center', gap: 5 }}>
                <StatusDot online /> {p.name} (pid {p.pid})
              </div>
            ))
        }
      </div>

      {/* Ollama */}
      <div style={{ padding: '12px 14px' }}>
        <div style={{ fontSize: 11, fontWeight: 600, color: 'var(--text-muted)', marginBottom: 6 }}>
          Ollama {ollama.length > 0 && <span style={{ color: 'var(--text-disabled)', fontWeight: 400 }}>({ollama.length})</span>}
        </div>
        {loading ? <div style={{ fontSize: 11, color: 'var(--text-muted)' }}>Loading…</div>
          : ollama.length === 0
            ? <div style={{ fontSize: 11, color: 'var(--text-muted)' }}>No models pulled. <a href="/models" style={{ color: 'var(--accent-primary)' }}>Manage →</a></div>
            : ollama.map(m => (
              <div key={m.name} style={{ display: 'flex', alignItems: 'center', gap: 6, padding: '5px 0', borderBottom: '1px solid var(--border-subtle)' }}>
                <Box size={11} style={{ color: 'var(--text-muted)', flexShrink: 0 }} />
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div style={{ fontSize: 11, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{m.name}</div>
                  {m.details?.parameter_size && <div style={{ fontSize: 10, color: 'var(--text-muted)' }}>{m.details.parameter_size}</div>}
                </div>
              </div>
            ))
        }
        <a href="/models" style={{ ...S.btnGhost, textDecoration: 'none', marginTop: 10, fontSize: 11, display: 'inline-flex', width: '100%', justifyContent: 'center' }}>
          Manage models →
        </a>
      </div>
    </div>
  )
}

// ── Pipelines column ──────────────────────────────────────────────────────────

function PipelinesColumn() {
  return (
    <div style={S.card}>
      <div style={S.colHead}>
        <Zap size={13} style={{ color: 'var(--accent-warning)' }} />
        Pipelines
      </div>
      <div style={{ padding: 20, display: 'flex', flexDirection: 'column', gap: 14 }}>
        <div style={{ padding: '10px 12px', borderRadius: 8, background: 'var(--accent-primary-subtle)', border: '1px solid var(--accent-primary)', fontSize: 12, lineHeight: 1.6, color: 'var(--text-secondary)' }}>
          <strong style={{ color: 'var(--text-primary)' }}>Visual pipeline editor</strong> is planned — build multi-step AI workflows with Image Gen, TTS, STT, HTTP, and shell nodes.
        </div>
        <div style={{ fontSize: 12, color: 'var(--text-muted)', lineHeight: 1.6 }}>
          In the meantime, use <strong>Automation</strong> for scheduled shell jobs and webhook triggers.
        </div>
        <a href="/automation" style={{ ...S.btnGhost, textDecoration: 'none', justifyContent: 'center', fontSize: 12 }}>
          Open Automation →
        </a>
        <div style={{ borderTop: '1px solid var(--border-subtle)', paddingTop: 14 }}>
          <div style={{ fontSize: 11, fontWeight: 600, color: 'var(--text-muted)', marginBottom: 8 }}>Planned node types</div>
          {['Text Prompt', 'Image Gen', 'TTS', 'STT', 'HTTP Request', 'Shell Command', 'Save to Files', 'Send to Odysseus'].map(n => (
            <div key={n} style={{ display: 'flex', alignItems: 'center', gap: 6, padding: '4px 0', fontSize: 11, color: 'var(--text-muted)' }}>
              <span style={{ width: 5, height: 5, borderRadius: '50%', background: 'var(--border-strong)', flexShrink: 0 }} />
              {n}
            </div>
          ))}
        </div>
      </div>
    </div>
  )
}

// ── Agents column ─────────────────────────────────────────────────────────────

function AgentsColumn() {
  const [mcpTools,    setMcpTools]    = useState<McpTool[]>([])
  const [manifestTools, setManifestTools] = useState<ManifestTool[]>([])
  const [selected,    setSelected]    = useState<McpTool | null>(null)
  const [args,        setArgs]        = useState<Record<string, string>>({})
  const [running,     setRunning]     = useState(false)
  const [result,      setResult]      = useState<{ ok: boolean; text: string } | null>(null)
  const [mcpOpen,     setMcpOpen]     = useState(true)
  const [manifestOpen, setManifestOpen] = useState(false)

  useEffect(() => {
    fetch('/api/studio/mcp/tools', { credentials: 'include' })
      .then(r => r.json()).then(d => setMcpTools(d.tools ?? []))
      .catch(() => {})
    fetch('/api/integrations/odysseus/manifest', { credentials: 'include' })
      .then(r => r.json()).then(d => setManifestTools(d.tools ?? []))
      .catch(() => {})
  }, [])

  function pick(t: McpTool) { setSelected(t); setArgs({}); setResult(null) }

  async function invoke() {
    if (!selected) return
    setRunning(true); setResult(null)
    try {
      const parsed: Record<string, unknown> = {}
      for (const [k, v] of Object.entries(args)) {
        const pt = selected.inputSchema.properties?.[k]?.type
        parsed[k] = pt === 'number' || pt === 'integer' ? Number(v) : v
      }
      const r = await fetch('/api/studio/mcp/invoke', {
        method: 'POST', credentials: 'include',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ name: selected.name, arguments: parsed }),
      })
      const d = await r.json()
      let text = d.result ?? d.error ?? ''
      try { text = JSON.stringify(JSON.parse(text), null, 2) } catch { /* raw */ }
      setResult({ ok: d.ok, text })
    } catch (e: any) { setResult({ ok: false, text: e.message }) }
    finally { setRunning(false) }
  }

  const props    = selected?.inputSchema.properties ?? {}
  const required = new Set(selected?.inputSchema.required ?? [])

  const riskColor = (risk: string) => {
    if (risk === 'read-only') return 'var(--accent-success)'
    if (risk === 'low-risk')  return 'var(--accent-secondary)'
    if (risk === 'medium-risk') return 'var(--accent-warning)'
    return 'var(--accent-danger)'
  }

  return (
    <div style={S.card}>
      <div style={S.colHead}>
        <Zap size={13} style={{ color: 'var(--accent-success)' }} />
        Agents
      </div>

      {/* VoidTower MCP tools */}
      <div style={{ borderBottom: '1px solid var(--border-subtle)' }}>
        <div style={S.sectionHead} onClick={() => setMcpOpen(o => !o)}>
          <span style={{ fontSize: 12, fontWeight: 600, flex: 1 }}>VoidTower MCP Tools</span>
          <span style={{ fontSize: 10, color: 'var(--text-muted)', marginRight: 6 }}>{mcpTools.length}</span>
          {mcpOpen ? <ChevronDown size={13} style={{ color: 'var(--text-muted)' }} />
                   : <ChevronRight size={13} style={{ color: 'var(--text-muted)' }} />}
        </div>
        {mcpOpen && (
          <div style={{ padding: '0 0 10px' }}>
            {mcpTools.map(t => (
              <div key={t.name} onClick={() => pick(t)}
                style={{
                  padding: '8px 14px', cursor: 'pointer',
                  background: selected?.name === t.name ? 'var(--accent-primary-subtle)' : 'transparent',
                  borderLeft: `3px solid ${selected?.name === t.name ? 'var(--accent-primary)' : 'transparent'}`,
                }}>
                <div style={{ fontSize: 11, fontFamily: 'var(--font-mono)', color: 'var(--text-primary)' }}>{t.name}</div>
                <div style={{ fontSize: 10, color: 'var(--text-muted)', marginTop: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{t.description}</div>
              </div>
            ))}

            {selected && (
              <div style={{ padding: '10px 14px', borderTop: '1px solid var(--border-subtle)', display: 'flex', flexDirection: 'column', gap: 8 }}>
                <div style={{ fontSize: 11, fontFamily: 'var(--font-mono)', fontWeight: 600 }}>{selected.name}</div>
                {Object.keys(props).length === 0
                  ? <div style={{ fontSize: 11, color: 'var(--text-muted)' }}>No arguments</div>
                  : Object.entries(props).map(([name, schema]) => (
                    <div key={name}>
                      <label style={S.label}>
                        <code style={{ fontFamily: 'var(--font-mono)' }}>{name}</code>
                        {required.has(name) && <span style={{ color: 'var(--accent-danger)', marginLeft: 3 }}>*</span>}
                        <span style={{ marginLeft: 5, opacity: 0.5 }}>({schema.type})</span>
                      </label>
                      <input value={args[name] ?? ''} onChange={e => setArgs(a => ({ ...a, [name]: e.target.value }))}
                        placeholder={schema.description ?? schema.type} style={S.inp} />
                    </div>
                  ))
                }
                <button onClick={invoke} disabled={running}
                  style={{ ...S.btnPrimary, opacity: running ? 0.6 : 1 }}>
                  {running ? <Loader2 size={11} className="animate-spin" /> : null}
                  {running ? 'Running…' : 'Invoke'}
                </button>
                {result && (
                  <div style={{ borderLeft: `3px solid ${result.ok ? 'var(--accent-primary)' : 'var(--accent-danger)'}`, paddingLeft: 8 }}>
                    <pre style={{ margin: 0, fontSize: 10, fontFamily: 'var(--font-mono)', whiteSpace: 'pre-wrap', wordBreak: 'break-all', maxHeight: 200, overflow: 'auto', color: 'var(--text-secondary)' }}>
                      {result.text}
                    </pre>
                  </div>
                )}
              </div>
            )}
          </div>
        )}
      </div>

      {/* Odysseus manifest tools */}
      <div>
        <div style={S.sectionHead} onClick={() => setManifestOpen(o => !o)}>
          <span style={{ fontSize: 12, fontWeight: 600, flex: 1 }}>Odysseus Integration Tools</span>
          <span style={{ fontSize: 10, color: 'var(--text-muted)', marginRight: 6 }}>{manifestTools.length}</span>
          {manifestOpen ? <ChevronDown size={13} style={{ color: 'var(--text-muted)' }} />
                        : <ChevronRight size={13} style={{ color: 'var(--text-muted)' }} />}
        </div>
        {manifestOpen && (
          <div>
            {manifestTools.map(t => (
              <div key={t.name} style={{ padding: '8px 14px', borderBottom: '1px solid var(--border-subtle)' }}>
                <div style={{ display: 'flex', alignItems: 'center', gap: 6, marginBottom: 2 }}>
                  <span style={{ fontSize: 11, fontFamily: 'var(--font-mono)', color: 'var(--text-primary)', flex: 1 }}>{t.name}</span>
                  <span style={{ fontSize: 10, color: riskColor(t.risk), background: 'var(--bg-elevated)', padding: '1px 5px', borderRadius: 3 }}>{t.risk}</span>
                </div>
                <div style={{ fontSize: 10, color: 'var(--text-muted)' }}>{t.description}</div>
                <div style={{ fontSize: 10, fontFamily: 'var(--font-mono)', color: 'var(--text-disabled)', marginTop: 3 }}>{t.api}</div>
              </div>
            ))}
            {manifestTools.length === 0 && (
              <div style={{ padding: '10px 14px', fontSize: 12, color: 'var(--text-muted)' }}>
                Enable Odysseus integration in Settings to see tools.
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  )
}

// ── Service status bar ────────────────────────────────────────────────────────

function ServiceBar({ services, onRefresh }: { services: StudioService[]; onRefresh: () => void }) {
  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 8, flexWrap: 'wrap', padding: '10px 14px', background: 'var(--bg-panel)', border: '1px solid var(--border-subtle)', borderRadius: 8, marginBottom: 20 }}>
      {services.map(svc => (
        <div key={svc.name} style={{ display: 'flex', alignItems: 'center', gap: 5, padding: '3px 8px', borderRadius: 5, background: 'var(--bg-elevated)', fontSize: 11 }}>
          <StatusDot online={svc.status === 'online'} />
          <span style={{ color: svc.status === 'online' ? 'var(--text-primary)' : 'var(--text-muted)' }}>{svc.name}</span>
          {svc.version && <span style={{ color: 'var(--text-disabled)', fontSize: 10 }}>v{svc.version}</span>}
        </div>
      ))}
      <button onClick={onRefresh} style={{ ...S.btnGhost, marginLeft: 'auto', padding: '3px 8px', fontSize: 11 }}>
        <RefreshCw size={11} /> Refresh
      </button>
    </div>
  )
}

// ── main page ─────────────────────────────────────────────────────────────────

export default function StudioPage() {
  const [status,        setStatus]        = useState<StudioStatus | null>(null)
  const [statusLoading, setStatusLoading] = useState(true)

  const loadStatus = useCallback(async () => {
    setStatusLoading(true)
    try {
      const r = await fetch('/api/studio/status', { credentials: 'include' })
      if (r.ok) setStatus(await r.json())
    } finally { setStatusLoading(false) }
  }, [])

  useEffect(() => { loadStatus() }, [loadStatus])

  const services = status?.services ?? []

  return (
    <div style={{ padding: '24px 28px' }}>
      {/* Header */}
      <div style={{ display: 'flex', alignItems: 'center', gap: 10, marginBottom: 20 }}>
        <Wand2 size={20} style={{ color: 'var(--accent-primary)' }} />
        <div>
          <h1 style={{ margin: 0, fontSize: 18, fontWeight: 600 }}>AI Creative Studio</h1>
          <p style={{ margin: 0, fontSize: 12, color: 'var(--text-muted)' }}>
            {statusLoading ? 'Detecting services…' : `${services.filter(s => s.status === 'online').length} of ${services.length} services online`}
          </p>
        </div>
      </div>

      {/* Service status bar */}
      {services.length > 0 && <ServiceBar services={services} onRefresh={loadStatus} />}

      {/* Four-column grid */}
      <div style={{
        display: 'grid',
        gridTemplateColumns: 'repeat(auto-fill, minmax(280px, 1fr))',
        gap: 16,
        alignItems: 'start',
      }}>
        <GenerationColumn services={services} />
        <ModelsColumn     gpu={status?.gpu} />
        <PipelinesColumn />
        <AgentsColumn />
      </div>
    </div>
  )
}
