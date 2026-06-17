import { useState, useEffect } from 'react'
import { Outlet, useLocation, useNavigate } from 'react-router-dom'
import Sidebar from './Sidebar'
import TopBar from './TopBar'
import NotificationToasts from '@/components/ui/NotificationToasts'
import CommandPalette from '@/components/ui/CommandPalette'
import ForcePasswordChange from '@/components/ui/ForcePasswordChange'
import AnimatedBackground from '@/components/ui/AnimatedBackground'
import { useMetrics } from '@/hooks/useMetrics'
import { useAuthStore } from '@/store/auth'
import { useSidebarPrefsStore } from '@/store/sidebarPrefs'
import { MAIN_SCROLL_ID } from './Sidebar'
import { BrainCircuit, Settings2 } from 'lucide-react'
import Button from '@/components/ui/Button'
import AppEmbedOverlay from '@/components/ui/AppEmbedOverlay'

const AI_WORKSPACE_KEY = 'vt-ai-workspace-url'

// ─── Persistent AI iframe ────────────────────────────────────────────────────
// Rendered once at layout level and never unmounted. Odysseus stays loaded
// in memory across navigation — show/hide with CSS only, no reload.

function PersistentAIFrame({ visible }: { visible: boolean }) {
  const navigate = useNavigate()
  const [src, setSrc] = useState<string | null>(null)
  const [loaded, setLoaded] = useState(false)

  useEffect(() => {
    fetch('/api/settings/ai-url', { credentials: 'include' })
      .then(r => r.ok ? r.json() : null)
      .then((s: { url?: string; port?: number; proxy_active?: boolean } | null) => {
        if (s?.proxy_active && s?.url) {
          setSrc(`http://${window.location.hostname}:${s.port}/`)
        } else if (s?.url) {
          setSrc(s.url)
        } else {
          const stored = localStorage.getItem(AI_WORKSPACE_KEY)
          if (stored?.trim()) setSrc(stored.trim())
        }
      })
      .catch(() => {
        const stored = localStorage.getItem(AI_WORKSPACE_KEY)
        if (stored?.trim()) setSrc(stored.trim())
      })
      .finally(() => setLoaded(true))
  }, [])

  // Not-configured empty state
  if (loaded && !src) {
    return (
      <div style={{
        position: 'absolute', inset: 0, zIndex: 10,
        display: visible ? 'flex' : 'none',
        flexDirection: 'column', alignItems: 'center', justifyContent: 'center', gap: 16,
      }}>
        <BrainCircuit size={40} style={{ color: 'var(--text-muted)' }} />
        <div style={{ textAlign: 'center' }}>
          <p style={{ fontSize: 14, fontWeight: 500, color: 'var(--text-primary)', marginBottom: 4 }}>No AI workspace configured</p>
          <p style={{ fontSize: 12, color: 'var(--text-muted)', maxWidth: 280 }}>
            Set an AI workspace URL in Settings → Integrations → AI.
          </p>
        </div>
        <Button size="sm" variant="primary" onClick={() => navigate('/settings')}>
          <Settings2 size={13} style={{ marginRight: 6 }} /> Open Settings
        </Button>
      </div>
    )
  }

  if (!src) return null

  return (
    // Wrapper always in DOM — iframe never unmounts, stays loaded across navigation
    <div style={{ position: 'absolute', inset: 0, zIndex: 10, display: visible ? 'block' : 'none' }}>
      <iframe
        src={src}
        title="AI Workspace"
        style={{ width: '100%', height: '100%', border: 'none', display: 'block' }}
        allow="microphone; camera; clipboard-write; fullscreen"
      />
    </div>
  )
}

// ─── Layout ──────────────────────────────────────────────────────────────────

export default function AppLayout() {
  useMetrics()
  const user = useAuthStore((s) => s.user)
  const { pathname } = useLocation()
  const isAI = pathname === '/ai'
  const placement = useSidebarPrefsStore((s) => s.placement)
  const horizontal = placement === 'top' || placement === 'bottom'
  const sidebarFirst = placement === 'left' || placement === 'top'

  return (
    <div className={horizontal ? 'flex flex-col h-full' : 'flex h-full'}>
      <AnimatedBackground />
      {/* Sidebar always renders in the same JSX slot (never unmounted by a placement change)
          so its own enter/slide animation — keyed on placement — can run every time.
          Visual position is controlled purely by CSS order, not by where it sits in the tree. */}
      <div style={{ order: sidebarFirst ? -1 : 1 }}>
        <Sidebar />
      </div>
      <div className="flex flex-col flex-1 min-w-0 min-h-0" style={{ order: 0 }}>
        <TopBar />
        {/* main is relative+overflow-hidden so the absolute iframe fills it exactly */}
        <main className="flex-1 relative overflow-hidden min-h-0">
          {/* Normal page content — always rendered, hidden behind iframe on /ai */}
          <div id={MAIN_SCROLL_ID} className="absolute inset-0 overflow-auto p-4" style={{ display: isAI ? 'none' : 'block' }}>
            <Outlet />
          </div>
          {/* AI page outlet still needs to mount on /ai for router correctness, but renders nothing */}
          {isAI && <div className="hidden"><Outlet /></div>}
          {/* Persistent iframe — loaded once, shown/hidden without reload */}
          <PersistentAIFrame visible={isAI} />
          {/* App embed overlay — shown when a deployed app is opened in VoidTower */}
          <AppEmbedOverlay />
        </main>
      </div>
      <NotificationToasts />
      <CommandPalette />
      {user?.force_password_change && <ForcePasswordChange />}
    </div>
  )
}
