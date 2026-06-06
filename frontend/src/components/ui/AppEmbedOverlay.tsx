import { ExternalLink, X } from 'lucide-react'
import { useEmbedStore } from '@/store/embedStore'

function resolveUrl(app: ReturnType<typeof useEmbedStore.getState>['app'], def: ReturnType<typeof useEmbedStore.getState>['def']): string | null {
  if (!app || app.primary_port === null) return null
  const base = `http://${window.location.hostname}:${app.primary_port}`
  const path = def?.links?.web_ui ?? '/'
  // If the link is already a full URL, use it directly; otherwise treat as path suffix
  if (path.startsWith('http://') || path.startsWith('https://')) return path
  return `${base}${path.startsWith('/') ? path : `/${path}`}`
}

export default function AppEmbedOverlay() {
  const app = useEmbedStore(s => s.app)
  const def = useEmbedStore(s => s.def)
  const close = useEmbedStore(s => s.close)

  if (!app) return null

  const url = resolveUrl(app, def)
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
        {/* Status dot + app name */}
        <span style={{
          width: 8,
          height: 8,
          borderRadius: '50%',
          flexShrink: 0,
          background: running ? 'var(--accent-success)' : 'var(--text-disabled)',
        }} />
        <span style={{
          fontSize: 13,
          fontWeight: 600,
          color: 'var(--text-primary)',
          flex: 1,
          overflow: 'hidden',
          textOverflow: 'ellipsis',
          whiteSpace: 'nowrap',
        }}>
          {app.app_name}
        </span>

        {url && (
          <a
            href={url}
            target="_blank"
            rel="noopener noreferrer"
            title="Open in new tab"
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: 4,
              fontSize: 12,
              color: 'var(--accent-primary)',
              padding: '4px 8px',
              borderRadius: 4,
              background: 'var(--accent-primary-subtle)',
              border: '1px solid var(--accent-primary)',
              textDecoration: 'none',
              flexShrink: 0,
            }}
          >
            <ExternalLink size={12} />
            Open in new tab
          </a>
        )}

        <button
          onClick={close}
          title="Close"
          style={{
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            width: 28,
            height: 28,
            borderRadius: 4,
            border: 'none',
            background: 'none',
            cursor: 'pointer',
            color: 'var(--text-muted)',
            flexShrink: 0,
          }}
          onMouseEnter={e => (e.currentTarget.style.background = 'var(--bg-elevated)')}
          onMouseLeave={e => (e.currentTarget.style.background = 'none')}
        >
          <X size={16} />
        </button>
      </div>

      {/* Content */}
      {url ? (
        <iframe
          src={url}
          title={app.app_name}
          style={{ flex: 1, border: 'none', display: 'block' }}
          sandbox="allow-scripts allow-same-origin allow-forms allow-popups allow-popups-to-escape-sandbox"
          allow="microphone; camera; clipboard-write; fullscreen"
        />
      ) : (
        /* Fallback — no primary_port or not running */
        <div style={{
          flex: 1,
          display: 'flex',
          flexDirection: 'column',
          alignItems: 'center',
          justifyContent: 'center',
          gap: 16,
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
              fontSize: 13,
              padding: '6px 16px',
              borderRadius: 6,
              border: 'none',
              background: 'var(--bg-elevated)',
              color: 'var(--text-primary)',
              cursor: 'pointer',
            }}
          >
            Close
          </button>
        </div>
      )}
    </div>
  )
}
