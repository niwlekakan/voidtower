import { useEffect, useState, useCallback, useRef } from 'react'
import { Search, Bell, Menu, Tag as TagIcon, X, Cpu, Zap, ChevronDown } from 'lucide-react'
import { useMetricsStore } from '@/store/metrics'
import { useCmdPaletteStore } from '@/store/cmdpalette'
import { useKeyboard } from '@/hooks/useKeyboard'
import { useFiltersStore } from '@/store/filters'
import { api } from '@/api/client'
import type { Tag } from '@/api/types'
import UiModeToggle from '@/components/ui/UiModeToggle'
import Button from '@/components/ui/Button'
import { useSidebarPrefsStore } from '@/store/sidebarPrefs'

// ─── GPU / llama widget ───────────────────────────────────────────────────────

interface LlamaProcess { pid: number; name: string; cmd: string }
interface GpuInfo { name: string; vram_used_mb: number; vram_total_mb: number; utilization_pct: number }
interface LlamaStatus { processes: LlamaProcess[]; gpu: GpuInfo | null }

function GpuWidget() {
  const [status, setStatus] = useState<LlamaStatus | null>(null)
  const [open, setOpen] = useState(false)
  const [unloading, setUnloading] = useState(false)
  const ref = useRef<HTMLDivElement>(null)

  const refresh = useCallback(async () => {
    try {
      const res = await fetch('/api/ai/llama', { credentials: 'include' })
      if (res.ok) setStatus(await res.json())
    } catch { /* ignore */ }
  }, [])

  useEffect(() => {
    refresh()
    const t = setInterval(refresh, 5000)
    return () => clearInterval(t)
  }, [refresh])

  useEffect(() => {
    if (!open) return
    const handler = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false)
    }
    document.addEventListener('mousedown', handler)
    return () => document.removeEventListener('mousedown', handler)
  }, [open])

  if (!status) return null

  const hasLlama = status.processes.length > 0
  const gpu = status.gpu
  const pct = gpu && gpu.vram_total_mb > 0 ? Math.round((gpu.vram_used_mb / gpu.vram_total_mb) * 100) : null
  const barColor = pct == null ? 'var(--text-muted)' : pct > 85 ? 'var(--accent-danger)' : pct > 60 ? 'var(--accent-warning)' : 'var(--accent-success)'

  const unload = async () => {
    setUnloading(true)
    try {
      await fetch('/api/ai/llama/unload', { method: 'POST', credentials: 'include' })
      setTimeout(refresh, 1000)
      setStatus(s => s ? { ...s, processes: [] } : s)
    } catch { /* empty */ } finally { setUnloading(false) }
  }

  return (
    <div ref={ref} style={{ position: 'relative' }}>
      <button
        onClick={() => setOpen(o => !o)}
        title="GPU controls"
        style={{
          display: 'flex', alignItems: 'center', gap: 5,
          padding: '4px 8px', borderRadius: 20, cursor: 'pointer',
          border: '1px solid var(--border-subtle)',
          background: open ? 'var(--bg-elevated)' : 'transparent',
          color: hasLlama ? 'var(--accent-warning)' : 'var(--text-muted)',
          fontSize: 11, fontWeight: 600, transition: 'all 0.15s',
        }}
      >
        <Cpu size={12} style={{ flexShrink: 0 }} />
        {pct != null && <span style={{ color: barColor }}>{pct}%</span>}
        <ChevronDown size={10} style={{ color: 'var(--text-muted)', transform: open ? 'rotate(180deg)' : 'none', transition: 'transform 0.15s' }} />
      </button>

      {open && (
        <div style={{
          position: 'absolute', top: 'calc(100% + 6px)', right: 0,
          background: 'var(--bg-panel)', border: '1px solid var(--border-subtle)',
          borderRadius: 10, minWidth: 260, padding: 12,
          boxShadow: '0 4px 20px rgba(0,0,0,0.4)', zIndex: 200,
          display: 'flex', flexDirection: 'column', gap: 10,
        }}>
          {gpu && (
            <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
              <div style={{ display: 'flex', justifyContent: 'space-between' }}>
                <span style={{ fontSize: 11, color: 'var(--text-muted)' }}>{gpu.name}</span>
                <span style={{ fontSize: 11, color: 'var(--text-muted)' }}>
                  <Zap size={10} style={{ display: 'inline', verticalAlign: 'middle', marginRight: 2 }} />
                  {gpu.utilization_pct}%
                </span>
              </div>
              <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                <div style={{ flex: 1, height: 6, background: 'var(--bg-elevated)', borderRadius: 3, overflow: 'hidden' }}>
                  <div style={{ width: `${pct ?? 0}%`, height: '100%', background: barColor, borderRadius: 3, transition: 'width 0.3s' }} />
                </div>
                <span style={{ fontSize: 11, color: 'var(--text-muted)', whiteSpace: 'nowrap' }}>
                  {gpu.vram_used_mb} / {gpu.vram_total_mb} MB
                </span>
              </div>
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

// ─────────────────────────────────────────────────────────────────────────────

/**
 * Search trigger + GPU/UI-mode/tag/status/bell cluster — extracted so the merged
 * top/bottom nav bar (rendered by Sidebar.tsx when placement is horizontal) can
 * embed the same utilities instead of stacking a second, redundant bar.
 */
export function TopBarUtilities({ compact = false }: { compact?: boolean }) {
  const connected = useMetricsStore((s) => s.connected)
  const toggle = useCmdPaletteStore((s) => s.toggle)
  const { globalTag, setGlobalTag } = useFiltersStore()
  const [allTags, setAllTags] = useState<Tag[]>([])
  const [tagOpen, setTagOpen] = useState(false)

  useEffect(() => {
    api.tags.list().then(setAllTags).catch(() => {})
  }, [])

  useKeyboard([{ key: 'k', mods: ['ctrl'], handler: (e) => { e.preventDefault(); toggle() } }])

  return (
    <>
      {/* Search / command trigger */}
      <button
        onClick={toggle}
        className={compact ? 'flex items-center gap-2 px-3 py-1.5 rounded text-sm w-48 flex-shrink-0 text-left transition-colors' : 'flex items-center gap-2 px-3 py-1.5 rounded text-sm flex-1 max-w-xs text-left transition-colors'}
        style={{
          background: 'var(--bg-elevated)',
          border: '1px solid var(--border-subtle)',
          color: 'var(--text-muted)',
        }}
      >
        <Search size={14} />
        <span>{compact ? 'Search' : 'Search or press Ctrl+K'}</span>
      </button>

      <div className={compact ? 'flex items-center gap-3 flex-shrink-0' : 'flex items-center gap-3 ml-auto'}>
        <GpuWidget />
        <UiModeToggle />

        {/* Global tag filter — hidden on mobile */}
        {allTags.length > 0 && (
          <div className="hidden md:flex items-center gap-1.5 relative">
            {globalTag ? (
              <button
                onClick={() => setGlobalTag(null)}
                style={{
                  display: 'flex', alignItems: 'center', gap: 5,
                  padding: '2px 8px', borderRadius: 12, fontSize: 11, fontWeight: 600,
                  background: (allTags.find(t => t.id === globalTag)?.color ?? '#888') + '33',
                  color: allTags.find(t => t.id === globalTag)?.color ?? 'var(--text-muted)',
                  border: `1px solid ${(allTags.find(t => t.id === globalTag)?.color ?? '#888') + '55'}`,
                  cursor: 'pointer',
                }}
                title="Clear tag filter"
              >
                <TagIcon size={10} />
                {allTags.find(t => t.id === globalTag)?.name}
                <X size={10} />
              </button>
            ) : (
              <button
                onClick={() => setTagOpen(o => !o)}
                style={{
                  display: 'flex', alignItems: 'center', gap: 5,
                  padding: '2px 8px', borderRadius: 12, fontSize: 11,
                  background: 'transparent',
                  border: '1px dashed var(--border-subtle)',
                  color: 'var(--text-muted)', cursor: 'pointer',
                }}
                title="Filter by tag"
              >
                <TagIcon size={10} /> Tag
              </button>
            )}
            {tagOpen && !globalTag && (
              <div style={{
                position: 'absolute', top: '100%', right: 0, marginTop: 4,
                background: 'var(--bg-panel)', border: '1px solid var(--border-subtle)',
                borderRadius: 8, padding: 6, minWidth: 140,
                boxShadow: '0 4px 16px rgba(0,0,0,0.3)', zIndex: 100,
                display: 'flex', flexDirection: 'column', gap: 2,
              }}>
                {allTags.map(tag => (
                  <button key={tag.id} onClick={() => { setGlobalTag(tag.id); setTagOpen(false) }} style={{
                    display: 'flex', alignItems: 'center', gap: 8, padding: '4px 8px',
                    borderRadius: 5, background: 'transparent', border: 'none',
                    cursor: 'pointer', width: '100%', textAlign: 'left',
                  }}>
                    <span style={{ width: 8, height: 8, borderRadius: '50%', background: tag.color, flexShrink: 0 }} />
                    <span style={{ fontSize: 12, color: 'var(--text-primary)' }}>{tag.name}</span>
                  </button>
                ))}
              </div>
            )}
          </div>
        )}

        {/* WS status */}
        <span
          className="status-dot"
          title={connected ? 'Metrics live' : 'Metrics disconnected'}
          style={{ background: connected ? 'var(--accent-success)' : 'var(--text-muted)' }}
        />
        <button style={{ color: 'var(--text-muted)' }}>
          <Bell size={16} />
        </button>
      </div>
    </>
  )
}

export default function TopBar() {
  const connected = useMetricsStore((s) => s.connected)
  const placement = useSidebarPrefsStore((s) => s.placement)
  const horizontal = placement === 'top' || placement === 'bottom'

  return (
    <>
      {!connected && (
        <div className="flex items-center justify-center gap-2 px-4 py-1 text-xs" style={{ background: 'var(--accent-danger)', color: '#fff' }}>
          <span>⚠ Backend disconnected — metrics unavailable. Unsafe actions disabled.</span>
        </div>
      )}
      {/* Header is skipped when placement is horizontal — Sidebar.tsx renders these
          same utilities merged into its single top/bottom bar instead. */}
      {!horizontal && (
        <header
          className="flex items-center gap-3 px-4 border-b"
          style={{
            height: 'var(--topbar-height)',
            background: 'var(--bg-panel)',
            borderColor: 'var(--border-subtle)',
          }}
        >
          {/* Mobile hamburger */}
          <button
            className="md:hidden p-1.5 rounded"
            style={{ color: 'var(--text-muted)' }}
            onClick={() => document.body.classList.toggle('mobile-nav-open')}
            aria-label="Toggle navigation"
          >
            <Menu size={18} />
          </button>

          <TopBarUtilities />
        </header>
      )}
    </>
  )
}
