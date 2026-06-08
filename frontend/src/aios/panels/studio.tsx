import { useCallback, useEffect, useRef, useState } from 'react'
import { Download, Loader2, Volume2, Image, Music, RefreshCw } from 'lucide-react'
import NativePanelShell, { NativeRow, StatusDot, IconBtn, EmptyState, LoadingState } from './NativePanelShell'

interface StudioService {
  name: string; kind: string; status: 'online' | 'offline'; url: string
}

interface GalleryItem {
  id: string; kind: 'image' | 'audio'; filename: string; url: string
  created_at: number; size_bytes: number
}

const TABS = [
  { id: 'status',  label: 'Status'  },
  { id: 'image',   label: 'Image'   },
  { id: 'audio',   label: 'Audio'   },
  { id: 'gallery', label: 'Gallery' },
]

const inp: React.CSSProperties = {
  fontSize: 11, padding: '3px 6px', borderRadius: 4,
  border: '1px solid var(--border-subtle)', background: 'var(--bg-elevated)',
  color: 'var(--text-primary)', outline: 'none',
}

export default function NativeStudioPanel() {
  const [tab,      setTab]      = useState('status')
  const [services, setServices] = useState<StudioService[]>([])
  const [svcLoad,  setSvcLoad]  = useState(true)
  const [imgPrompt, setImgPrompt] = useState('')
  const [imgLoad,   setImgLoad]   = useState(false)
  const [imgResult, setImgResult] = useState<string | null>(null)
  const [ttsText,   setTtsText]   = useState('')
  const [ttsLoad,   setTtsLoad]   = useState(false)
  const [audioUrl,  setAudioUrl]  = useState<string | null>(null)
  const [gallery,   setGallery]   = useState<GalleryItem[]>([])
  const [galLoad,   setGalLoad]   = useState(false)
  const audioRef = useRef<HTMLAudioElement>(null)

  const loadStatus = useCallback(async () => {
    setSvcLoad(true)
    const r = await fetch('/api/studio/status', { credentials: 'include' })
    if (r.ok) { const d = await r.json(); setServices(d.services ?? []) }
    setSvcLoad(false)
  }, [])

  const loadGallery = useCallback(async () => {
    setGalLoad(true)
    const r = await fetch('/api/studio/gallery', { credentials: 'include' })
    if (r.ok) { const d = await r.json(); setGallery(d.slice(0, 9)) }
    setGalLoad(false)
  }, [])

  useEffect(() => { loadStatus() }, [loadStatus])
  useEffect(() => { if (tab === 'gallery') loadGallery() }, [tab, loadGallery])

  async function generateImage() {
    if (!imgPrompt.trim() || imgLoad) return
    setImgLoad(true)
    try {
      const r = await fetch('/api/studio/image/generate', {
        method: 'POST', credentials: 'include',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ prompt: imgPrompt, width: 512, height: 512, steps: 20 }),
      })
      const d = await r.json()
      if (r.ok) setImgResult(d.url)
    } finally { setImgLoad(false) }
  }

  async function generateAudio() {
    if (!ttsText.trim() || ttsLoad) return
    setTtsLoad(true)
    try {
      const r = await fetch('/api/studio/tts/generate', {
        method: 'POST', credentials: 'include',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ text: ttsText, voice: 'af_heart' }),
      })
      const d = await r.json()
      if (r.ok) { setAudioUrl(d.url); setTimeout(() => audioRef.current?.play(), 100) }
    } finally { setTtsLoad(false) }
  }

  const imgAvailable  = services.some(s => (s.name === 'Stable Diffusion WebUI' || s.name === 'ComfyUI') && s.status === 'online')
  const ttsAvailable  = services.some(s => s.name === 'Kokoro TTS' && s.status === 'online')

  const imgActions = imgAvailable ? (
    <div style={{ display: 'flex', alignItems: 'center', gap: 4, flex: 1 }}>
      <input value={imgPrompt} onChange={e => setImgPrompt(e.target.value)}
        onKeyDown={e => e.key === 'Enter' && generateImage()}
        placeholder="Image prompt…" style={{ ...inp, flex: 1 }} />
      <IconBtn title={imgLoad ? 'Generating…' : 'Generate'} onClick={generateImage}>
        {imgLoad ? <Loader2 size={11} style={{ opacity: 0.5 }} /> : <Image size={11} />}
      </IconBtn>
    </div>
  ) : undefined

  const ttsActions = ttsAvailable ? (
    <div style={{ display: 'flex', alignItems: 'center', gap: 4, flex: 1 }}>
      <input value={ttsText} onChange={e => setTtsText(e.target.value)}
        onKeyDown={e => e.key === 'Enter' && generateAudio()}
        placeholder="Text to speak…" style={{ ...inp, flex: 1 }} />
      <IconBtn title={ttsLoad ? 'Generating…' : 'Speak'} onClick={generateAudio}>
        {ttsLoad ? <Loader2 size={11} style={{ opacity: 0.5 }} /> : <Volume2 size={11} />}
      </IconBtn>
    </div>
  ) : undefined

  const statusActions = (
    <IconBtn title="Refresh" onClick={loadStatus}>
      <RefreshCw size={11} />
    </IconBtn>
  )

  const activeActions =
    tab === 'status'  ? statusActions :
    tab === 'image'   ? imgActions    :
    tab === 'audio'   ? ttsActions    : undefined

  return (
    <NativePanelShell tabs={TABS} activeTab={tab} onTabChange={setTab} actions={activeActions}>

      {/* Status */}
      {tab === 'status' && (
        svcLoad ? <LoadingState /> :
        services.length === 0 ? <EmptyState text="No services found" /> :
        services.map(svc => (
          <NativeRow key={svc.name}>
            <StatusDot color={svc.status === 'online' ? '#22c55e' : '#94a3b8'} />
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ fontSize: 11, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{svc.name}</div>
              <div style={{ fontSize: 9, color: 'var(--text-muted)' }}>{svc.kind} · {svc.status}</div>
            </div>
          </NativeRow>
        ))
      )}

      {/* Image */}
      {tab === 'image' && !imgAvailable && (
        <div style={{ padding: '12px 10px', fontSize: 11, color: 'var(--text-muted)', textAlign: 'center' }}>
          SD WebUI or ComfyUI not running.<br />
          <a href="/apps" style={{ color: 'var(--accent)' }}>Deploy via App Vault</a>
        </div>
      )}
      {tab === 'image' && imgAvailable && (
        imgLoad ? <LoadingState /> :
        imgResult ? (
          <NativeRow>
            <img src={imgResult} alt="" style={{ width: 40, height: 40, objectFit: 'cover', borderRadius: 3, border: '1px solid var(--border-subtle)', flexShrink: 0 }} />
            <div style={{ flex: 1, fontSize: 10, color: 'var(--text-muted)' }}>Generated</div>
            <a href={imgResult} download style={{ color: 'var(--text-muted)', display: 'flex' }}><Download size={11} /></a>
          </NativeRow>
        ) : (
          <EmptyState text="Enter a prompt above to generate" />
        )
      )}

      {/* Audio */}
      {tab === 'audio' && !ttsAvailable && (
        <div style={{ padding: '12px 10px', fontSize: 11, color: 'var(--text-muted)', textAlign: 'center' }}>
          Kokoro TTS not running.<br />
          <a href="/apps" style={{ color: 'var(--accent)' }}>Deploy via App Vault</a>
        </div>
      )}
      {tab === 'audio' && ttsAvailable && (
        ttsLoad ? <LoadingState /> :
        audioUrl ? (
          <NativeRow>
            <Music size={12} style={{ color: 'var(--text-muted)', flexShrink: 0 }} />
            <div style={{ flex: 1, minWidth: 0 }}>
              <audio ref={audioRef} src={audioUrl} controls style={{ width: '100%', height: 28 }} />
            </div>
            <a href={audioUrl} download style={{ color: 'var(--text-muted)', display: 'flex' }}><Download size={11} /></a>
          </NativeRow>
        ) : (
          <EmptyState text="Enter text above to generate speech" />
        )
      )}

      {/* Gallery */}
      {tab === 'gallery' && (
        galLoad ? <LoadingState /> :
        gallery.length === 0 ? <EmptyState text="No generated files yet" /> :
        gallery.map(item => (
          <NativeRow key={item.id}>
            {item.kind === 'image'
              ? <img src={item.url} alt="" style={{ width: 32, height: 32, objectFit: 'cover', borderRadius: 3, border: '1px solid var(--border-subtle)', flexShrink: 0 }} />
              : <Music size={12} style={{ color: 'var(--text-muted)', flexShrink: 0 }} />
            }
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ fontSize: 10, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', color: 'var(--text-primary)' }}>{item.filename}</div>
              <div style={{ fontSize: 9, color: 'var(--text-muted)' }}>{item.kind}</div>
            </div>
            <a href={item.url} download={item.filename} style={{ color: 'var(--text-muted)', display: 'flex' }}><Download size={11} /></a>
          </NativeRow>
        ))
      )}
    </NativePanelShell>
  )
}
