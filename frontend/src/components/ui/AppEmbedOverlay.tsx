import { useEffect, useRef, useState } from 'react'
import { ExternalLink, X } from 'lucide-react'
import { api } from '@/api/client'
import { useEmbedStore } from '@/store/embedStore'

export default function AppEmbedOverlay() {
  const app   = useEmbedStore(s => s.app)
  const def   = useEmbedStore(s => s.def)
  const close = useEmbedStore(s => s.close)

  const [iframeSrc, setIframeSrc] = useState<string | null>(null)
  const [embedUrl,  setEmbedUrl]  = useState<string | null>(null)
  const [loading,   setLoading]   = useState(false)
  const [showBadge, setShowBadge] = useState(false)

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') close() }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [close])

  const resolveRef = useRef(0)
  useEffect(() => {
    if (!app || app.primary_port === null) {
      setIframeSrc(null)
      setLoading(false)
      setShowBadge(false)
      return
    }

    const seq = ++resolveRef.current
    setIframeSrc(null)
    setEmbedUrl(null)
    setLoading(true)
    setShowBadge(false)

    const path = def?.links?.web_ui ?? def?.web_path ?? '/'
    const uiPort = def?.web_port ?? app.primary_port

    // Use the embed proxy so X-Frame-Options / CSP headers are stripped server-side
    const cleanPath = path.startsWith('/') ? path.slice(1) : path
    const proxyUrl = `/api/apps/embed/${app.project_name}/${cleanPath}`

    // Call openUi to provision the port-based LAN proxy (nginx, firewall)
    api.apps.openUi(app.project_name, uiPort ?? 0).then(r => {
      if (r.embed_url) setEmbedUrl(r.embed_url + (path.startsWith('/') ? path : '/' + path))
      if (r.proxy_created) setShowBadge(true)
    }).catch(() => {})

    if (seq !== resolveRef.current) return
    setIframeSrc(proxyUrl)
    setLoading(false)
  }, [app, def])

  if (!app) return null

  const running = app.status === 'running'

  return (
    <div style={{
      position: 'absolute',
      inset: 0,
      zIndex: 20,
      background: 'var(--bg-base)',
      display: 'flex',
      flexDirection: 'column',
    }}>
      {/* Toolbar */}
      <div style={{
        display: 'flex',
        alignItems: 'center',
        gap: 10,
        padding: '0 12px',
        height: 42,
        flexShrink: 0,
        background: 'var(--bg-panel)',
        borderBottom: '1px solid var(--border-subtle)',
      }}>
        <span style={{
          width: 8, height: 8, borderRadius: '50%', flexShrink: 0,
          background: running ? 'var(--accent-success)' : 'var(--text-disabled)',
        }} />
        <span style={{
          fontSize: 13, fontWeight: 600, color: 'var(--text-primary)',
          flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
        }}>
          {app.app_name}
        </span>

        {showBadge && (
          <span style={{
            fontSize: 11, padding: '2px 8px', borderRadius: 4,
            background: 'var(--accent-success-subtle)',
            color: 'var(--accent-success)',
            border: '1px solid var(--accent-success)',
            flexShrink: 0,
          }}>
            Proxy created
          </span>
        )}

        {(iframeSrc || embedUrl) && (
          <a
            href={embedUrl ?? iframeSrc ?? '#'}
            target="_blank"
            rel="noopener noreferrer"
            title={embedUrl ? `Open on LAN: ${embedUrl}` : 'Open in new tab'}
            style={{
              display: 'flex', alignItems: 'center', gap: 4,
              fontSize: 12, color: 'var(--accent-primary)',
              padding: '4px 8px', borderRadius: 4,
              background: 'var(--accent-primary-subtle)',
              border: '1px solid var(--accent-primary)',
              textDecoration: 'none', flexShrink: 0,
            }}
          >
            <ExternalLink size={12} />
            Open in new tab
          </a>
        )}

        <button
          onClick={close}
          title="Close (Esc)"
          style={{
            display: 'flex', alignItems: 'center', justifyContent: 'center',
            width: 28, height: 28, borderRadius: 4, border: 'none',
            background: 'none', cursor: 'pointer', color: 'var(--text-muted)', flexShrink: 0,
          }}
          onMouseEnter={e => (e.currentTarget.style.background = 'var(--bg-elevated)')}
          onMouseLeave={e => (e.currentTarget.style.background = 'none')}
        >
          <X size={16} />
        </button>
      </div>

      {/* Content */}
      {loading ? (
        <div style={{
          flex: 1, display: 'flex', alignItems: 'center', justifyContent: 'center',
          flexDirection: 'column', gap: 12,
        }}>
          <span style={{ fontSize: 13, color: 'var(--text-muted)' }}>Preparing embed…</span>
        </div>
      ) : iframeSrc ? (
        <iframe
          src={iframeSrc}
          title={app.app_name}
          style={{ flex: 1, border: 'none', display: 'block' }}
          sandbox="allow-scripts allow-same-origin allow-forms allow-popups allow-popups-to-escape-sandbox"
          allow="microphone; camera; clipboard-write; fullscreen"
        />
      ) : (
        <div style={{
          flex: 1, display: 'flex', flexDirection: 'column',
          alignItems: 'center', justifyContent: 'center', gap: 16,
        }}>
          <span style={{ fontSize: 40, opacity: 0.25 }}>🔌</span>
          <div style={{ textAlign: 'center' }}>
            <p style={{ fontSize: 14, fontWeight: 600, color: 'var(--text-primary)', marginBottom: 6 }}>
              {app.app_name}
            </p>
            <p style={{ fontSize: 12, color: 'var(--text-muted)', maxWidth: 320 }}>
              {app.primary_port === null
                ? 'No primary port configured for this app.'
                : 'The app is not currently running.'}
            </p>
          </div>
          <button
            onClick={close}
            style={{
              fontSize: 13, padding: '6px 16px', borderRadius: 6,
              border: 'none', background: 'var(--bg-elevated)',
              color: 'var(--text-primary)', cursor: 'pointer',
            }}
          >
            Close
          </button>
        </div>
      )}
    </div>
  )
}
