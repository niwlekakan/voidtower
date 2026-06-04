import { useState, useEffect, useCallback } from 'react'
import { Outlet, useLocation, useNavigate } from 'react-router-dom'
import Sidebar from './Sidebar'
import TopBar from './TopBar'
import NotificationToasts from '@/components/ui/NotificationToasts'
import CommandPalette from '@/components/ui/CommandPalette'
import ForcePasswordChange from '@/components/ui/ForcePasswordChange'
import AnimatedBackground from '@/components/ui/AnimatedBackground'
import { useMetrics } from '@/hooks/useMetrics'
import { useAuthStore } from '@/store/auth'
import { BrainCircuit, Settings2, Cpu, Zap, ChevronDown, ChevronUp } from 'lucide-react'
import Button from '@/components/ui/Button'

const AI_WORKSPACE_KEY = 'vt-ai-workspace-url'

// ─── GPU / llama panel ───────────────────────────────────────────────────────

interface LlamaProcess { pid: number; name: string; cmd: string }
interface GpuInfo { name: string; vram_used_mb: number; vram_total_mb: number; utilization_pct: number }
interface LlamaStatus { processes: LlamaProcess[]; gpu: GpuInfo | null }

function GpuBar({ used, total }: { used: number; total: number }) {
  const pct = total > 0 ? Math.round((used / total) * 100) : 0
  const color = pct > 85 ? 'var(--accent-danger)' : pct > 60 ? 'var(--accent-warning)' : 'var(--accent-success)'
  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
      <div style={{ flex: 1, height: 6, background: 'var(--bg-elevated)', borderRadius: 3, overflow: 'hidden' }}>
        <div style={{ width: `${pct}%`, height: '100%', background: color, borderRadius: 3, transition: 'width 0.3s' }} />
      </div>
      <span style={{ fontSize: 11, color: 'var(--text-muted)', whiteSpace: 'nowrap' }}>{used} / {total} MB ({pct}%)</span>
    </div>
  )
}

function LlamaPanel() {
  const [status, setStatus] = useState<LlamaStatus | null>(null)
  const [expanded, setExpanded] = useState(false)
  const [unloading, setUnloading] = useState(false)

  const refresh = useCallback(async () => {
    try {
      const res = await fetch('/api/ai/llama', { credentials: 'include' })
      if (res.ok) setStatus(await res.json())
    } catch {}
  }, [])

  useEffect(() => {
    refresh()
    const t = setInterval(refresh, 5000)
    return () => clearInterval(t)
  }, [refresh])

  const unload = async () => {
    setUnloading(true)
    try {
      await fetch('/api/ai/llama/unload', { method: 'POST', credentials: 'include' })
      setTimeout(refresh, 1000)
      setStatus(s => s ? { ...s, processes: [] } : s)
    } catch {} finally { setUnloading(false) }
  }

  if (!status) return null
  const hasLlama = status.processes.length > 0
  const gpu = status.gpu

  return (
    <div style={{
      position: 'absolute', top: 8, right: 8, zIndex: 20,
      background: 'var(--bg-panel)', border: '1px solid var(--border-subtle)',
      borderRadius: 10, minWidth: 260, boxShadow: '0 4px 20px rgba(0,0,0,0.4)',
      overflow: 'hidden',
    }}>
      <button
        onClick={() => setExpanded(e => !e)}
        style={{ display: 'flex', alignItems: 'center', gap: 8, width: '100%', padding: '8px 12px', background: 'none', border: 'none', cursor: 'pointer' }}
      >
        <Cpu size={14} style={{ color: hasLlama ? 'var(--accent-warning)' : 'var(--text-muted)', flexShrink: 0 }} />
        <span style={{ fontSize: 12, fontWeight: 600, color: 'var(--text-primary)', flex: 1, textAlign: 'left' }}>GPU controls</span>
        {gpu && <span style={{ fontSize: 11, color: 'var(--text-muted)' }}>{Math.round((gpu.vram_used_mb / gpu.vram_total_mb) * 100)}% VRAM</span>}
        {expanded ? <ChevronUp size={12} style={{ color: 'var(--text-muted)' }} /> : <ChevronDown size={12} style={{ color: 'var(--text-muted)' }} />}
      </button>

      {expanded && (
        <div style={{ padding: '0 12px 12px', display: 'flex', flexDirection: 'column', gap: 10 }}>
          {gpu && (
            <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
              <div style={{ display: 'flex', justifyContent: 'space-between' }}>
                <span style={{ fontSize: 11, color: 'var(--text-muted)' }}>{gpu.name}</span>
                <span style={{ fontSize: 11, color: 'var(--text-muted)' }}>
                  <Zap size={10} style={{ display: 'inline', verticalAlign: 'middle', marginRight: 2 }} />
                  {gpu.utilization_pct}%
                </span>
              </div>
              <GpuBar used={gpu.vram_used_mb} total={gpu.vram_total_mb} />
            </div>
          )}
          <div style={{ borderTop: '1px solid var(--border-subtle)', paddingTop: 8 }}>
            <span style={{ fontSize: 11, fontWeight: 600, color: 'var(--text-secondary)' }}>
              llama.cpp {hasLlama ? `(${status.processes.length} running)` : '(not running)'}
            </span>
            {hasLlama && (
              <>
                {status.processes.map(p => (
                  <div key={p.pid} style={{ fontSize: 10, fontFamily: 'monospace', color: 'var(--text-muted)', marginTop: 3, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                    [{p.pid}] {p.name}
                  </div>
                ))}
                <Button size="sm" variant="danger" onClick={unload} loading={unloading} style={{ width: '100%', marginTop: 6 }}>
                  Unload from GPU
                </Button>
              </>
            )}
            {!hasLlama && <p style={{ fontSize: 11, color: 'var(--text-muted)', marginTop: 4 }}>No llama.cpp processes found.</p>}
          </div>
        </div>
      )}
    </div>
  )
}

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
      <LlamaPanel />
    </div>
  )
}

// ─── Layout ──────────────────────────────────────────────────────────────────

export default function AppLayout() {
  useMetrics()
  const user = useAuthStore((s) => s.user)
  const { pathname } = useLocation()
  const isAI = pathname === '/ai'

  return (
    <div className="flex h-full">
      <AnimatedBackground />
      <Sidebar />
      <div className="flex flex-col flex-1 min-w-0">
        <TopBar />
        {/* main is relative+overflow-hidden so the absolute iframe fills it exactly */}
        <main className="flex-1 relative overflow-hidden min-h-0">
          {/* Normal page content — always rendered, hidden behind iframe on /ai */}
          <div className="absolute inset-0 overflow-auto p-4" style={{ display: isAI ? 'none' : 'block' }}>
            <Outlet />
          </div>
          {/* AI page outlet still needs to mount on /ai for router correctness, but renders nothing */}
          {isAI && <div className="hidden"><Outlet /></div>}
          {/* Persistent iframe — loaded once, shown/hidden without reload */}
          <PersistentAIFrame visible={isAI} />
        </main>
      </div>
      <NotificationToasts />
      <CommandPalette />
      {user?.force_password_change && <ForcePasswordChange />}
    </div>
  )
}
