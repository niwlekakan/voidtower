import { useEffect, useState } from 'react'
import { Search, Bell, Menu, Tag as TagIcon, X } from 'lucide-react'
import { useMetricsStore } from '@/store/metrics'
import { useCmdPaletteStore } from '@/store/cmdpalette'
import { useKeyboard } from '@/hooks/useKeyboard'
import { useFiltersStore } from '@/store/filters'
import { api } from '@/api/client'
import type { Tag } from '@/api/types'
import UiModeToggle from '@/components/ui/UiModeToggle'

export default function TopBar() {
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
    {!connected && (
      <div className="flex items-center justify-center gap-2 px-4 py-1 text-xs" style={{ background: 'var(--accent-danger)', color: '#fff' }}>
        <span>⚠ Backend disconnected — metrics unavailable. Unsafe actions disabled.</span>
      </div>
    )}
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

      {/* Search / command trigger */}
      <button
        onClick={toggle}
        className="flex items-center gap-2 px-3 py-1.5 rounded text-sm flex-1 max-w-xs text-left transition-colors"
        style={{
          background: 'var(--bg-elevated)',
          border: '1px solid var(--border-subtle)',
          color: 'var(--text-muted)',
        }}
      >
        <Search size={14} />
        <span>Search or press Ctrl+K</span>
      </button>

      <div className="flex items-center gap-3 ml-auto">
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
    </header>
    </>
  )
}
