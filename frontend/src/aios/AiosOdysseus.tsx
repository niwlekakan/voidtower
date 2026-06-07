import { useEffect, useState, useRef, useCallback } from 'react'
import { BrainCircuit, ExternalLink, Columns2, AlertCircle } from 'lucide-react'
import { useAiosStore } from '@/aios/store/aios'

// ── Config fetch ──────────────────────────────────────────────────────────────

interface OdysseusConfig {
  enabled: boolean
  allowed_url?: string
  url?: string
}

let _configCache: OdysseusConfig | null | undefined = undefined
let _cacheAt = 0
const CACHE_TTL = 10_000 // 10 s — short enough to pick up URL changes

export function clearOdysseusConfigCache() { _configCache = undefined; _cacheAt = 0 }

async function fetchOdysseusConfig(): Promise<OdysseusConfig | null> {
  if (_configCache !== undefined && Date.now() - _cacheAt < CACHE_TTL) return _configCache
  try {
    const r = await fetch('/api/integrations/odysseus/config', { credentials: 'include' })
    if (!r.ok) { _configCache = null; _cacheAt = Date.now(); return null }
    _configCache = await r.json() as OdysseusConfig
    _cacheAt = Date.now()
  } catch {
    _configCache = null; _cacheAt = Date.now()
  }
  return _configCache
}

function resolveUrl(cfg: OdysseusConfig): string | null {
  const raw = cfg.allowed_url?.trim() || cfg.url?.trim() || null
  return raw || null
}

// ── AiosOdysseus ──────────────────────────────────────────────────────────────

interface Props {
  /** Optional initial query passed via URL param ?q= */
  initialQuery?: string
  /** Panel id of this panel (for snap logic) */
  panelId?: string
}

export default function AiosOdysseus({ initialQuery, panelId }: Props) {
  const [config, setConfig] = useState<OdysseusConfig | null | undefined>(undefined)
  const iframeRef = useRef<HTMLIFrameElement>(null)

  const panels = useAiosStore((s) => s.panels)
  const focusedId = useAiosStore((s) => s.focusedId)
  const snapPanel = useAiosStore((s) => s.snapPanel)

  // Fetch config on mount
  useEffect(() => {
    fetchOdysseusConfig().then(setConfig)
  }, [])

  // Determine focused non-Odysseus panel for context
  const contextPanel = panels.find(
    (p) => p.id === focusedId && p.component !== 'odysseus' && p.type !== 'odysseus',
  ) ?? null

  const contextTitle = contextPanel?.title ?? null

  // Build iframe src from base URL + params
  const buildSrc = useCallback((baseUrl: string, query?: string, context?: string | null): string => {
    try {
      const u = new URL(baseUrl)
      if (query) u.searchParams.set('q', query)
      if (context) u.searchParams.set('context', context)
      return u.toString()
    } catch {
      return baseUrl
    }
  }, [])

  const baseUrl = config ? resolveUrl(config) : null
  const iframeSrc = baseUrl
    ? buildSrc(baseUrl, initialQuery, contextTitle)
    : null

  // When context changes, update iframe src
  const prevSrcRef = useRef<string | null>(null)
  useEffect(() => {
    if (!iframeSrc || !iframeRef.current) return
    if (prevSrcRef.current !== iframeSrc) {
      prevSrcRef.current = iframeSrc
      // Only update src if it actually changed (avoid full reload)
      if (iframeRef.current.src !== iframeSrc) {
        iframeRef.current.src = iframeSrc
      }
    }
  }, [iframeSrc])

  // "Open beside Odysseus" — snap contextPanel left, snap this panel right
  const handleOpenBeside = useCallback(() => {
    if (!contextPanel || !panelId) return
    snapPanel(contextPanel.id, 'left-half')
    snapPanel(panelId, 'right-half')
  }, [contextPanel, panelId, snapPanel])

  // ── Loading state ───────────────────────────────────────────────────────────
  if (config === undefined) {
    return (
      <div style={styles.centered}>
        <BrainCircuit size={28} style={{ color: 'var(--accent-primary)', opacity: 0.6 }} />
        <span style={styles.mutedText}>Loading Odysseus…</span>
      </div>
    )
  }

  // ── Not configured ──────────────────────────────────────────────────────────
  if (!config || !config.enabled || !baseUrl) {
    return (
      <div style={styles.centered}>
        <AlertCircle size={28} style={{ color: 'var(--accent-warning)', marginBottom: 12 }} />
        <div style={{ ...styles.mutedText, marginBottom: 6 }}>Odysseus is not configured</div>
        <div style={{ fontSize: 11, color: 'var(--text-muted)', marginBottom: 18, textAlign: 'center', maxWidth: 260, lineHeight: 1.6 }}>
          Set up the Odysseus integration URL to use it as a native panel.
        </div>
        <a
          href="/settings/integrations"
          style={{
            display: 'flex', alignItems: 'center', gap: 6,
            padding: '7px 16px', borderRadius: 8,
            background: 'var(--accent-primary-subtle)',
            color: 'var(--accent-primary)',
            border: '1px solid rgba(139,92,246,0.3)',
            fontSize: 12, fontWeight: 600, textDecoration: 'none',
            cursor: 'pointer', transition: 'opacity 0.15s',
          }}
          onMouseEnter={(e) => (e.currentTarget.style.opacity = '0.8')}
          onMouseLeave={(e) => (e.currentTarget.style.opacity = '1')}
        >
          <ExternalLink size={12} />
          Settings → Integrations
        </a>
      </div>
    )
  }

  // ── Main render ─────────────────────────────────────────────────────────────
  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%', overflow: 'hidden' }}>
      {/* Context bar */}
      <div style={styles.contextBar}>
        <BrainCircuit size={12} style={{ color: 'var(--accent-primary)', flexShrink: 0 }} />
        <span style={styles.contextLabel}>Context:</span>
        <span style={styles.contextValue}>
          {contextTitle ?? <em style={{ color: 'var(--text-disabled)' }}>No active context</em>}
        </span>

        <div style={{ marginLeft: 'auto', display: 'flex', alignItems: 'center', gap: 6 }}>
          {contextPanel && panelId && (
            <button
              onClick={handleOpenBeside}
              title="Snap focused panel left, Odysseus right"
              style={styles.contextBtn}
              onMouseEnter={(e) => (e.currentTarget.style.background = 'rgba(139,92,246,0.2)')}
              onMouseLeave={(e) => (e.currentTarget.style.background = 'rgba(139,92,246,0.08)')}
            >
              <Columns2 size={11} />
              Open beside
            </button>
          )}
          <a
            href={baseUrl}
            target="_blank"
            rel="noopener noreferrer"
            title="Open Odysseus in new tab"
            style={{ ...styles.contextBtn, textDecoration: 'none' }}
            onMouseEnter={(e) => (e.currentTarget.style.background = 'rgba(255,255,255,0.08)')}
            onMouseLeave={(e) => (e.currentTarget.style.background = 'transparent')}
          >
            <ExternalLink size={11} />
          </a>
        </div>
      </div>

      {/* Iframe */}
      <iframe
        ref={iframeRef}
        src={iframeSrc ?? undefined}
        title="Odysseus AI"
        style={{ flex: 1, width: '100%', border: 'none', display: 'block', minHeight: 0 }}
        allow="microphone; camera; clipboard-write; fullscreen"
        sandbox="allow-scripts allow-same-origin allow-forms allow-popups allow-modals"
      />
    </div>
  )
}

// ── Styles ────────────────────────────────────────────────────────────────────

const styles = {
  centered: {
    display: 'flex',
    flexDirection: 'column' as const,
    alignItems: 'center',
    justifyContent: 'center',
    height: '100%',
    gap: 8,
  },
  mutedText: {
    fontSize: 13,
    color: 'var(--text-muted)',
  },
  contextBar: {
    display: 'flex',
    alignItems: 'center',
    gap: 6,
    padding: '0 10px',
    height: 28,
    flexShrink: 0,
    background: 'rgba(0,0,0,0.35)',
    backdropFilter: 'blur(12px)',
    WebkitBackdropFilter: 'blur(12px)',
    borderBottom: '1px solid rgba(255,255,255,0.06)',
    fontSize: 11,
    color: 'var(--text-secondary)',
    overflow: 'hidden',
  },
  contextLabel: {
    color: 'var(--text-muted)',
    flexShrink: 0,
  },
  contextValue: {
    color: 'var(--text-primary)',
    whiteSpace: 'nowrap' as const,
    overflow: 'hidden',
    textOverflow: 'ellipsis',
    minWidth: 0,
    flex: '0 1 auto',
  },
  contextBtn: {
    display: 'flex',
    alignItems: 'center',
    gap: 4,
    padding: '2px 8px',
    borderRadius: 5,
    background: 'rgba(139,92,246,0.08)',
    border: '1px solid rgba(139,92,246,0.2)',
    color: 'var(--accent-primary)',
    fontSize: 10,
    fontWeight: 500,
    cursor: 'pointer',
    transition: 'background 0.1s',
    whiteSpace: 'nowrap' as const,
    lineHeight: 1.6,
  },
} as const
