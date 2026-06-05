import { Search, Bell, Menu } from 'lucide-react'
import { useMetricsStore } from '@/store/metrics'
import { useCmdPaletteStore } from '@/store/cmdpalette'
import { useKeyboard } from '@/hooks/useKeyboard'
import UiModeToggle from '@/components/ui/UiModeToggle'

export default function TopBar() {
  const connected = useMetricsStore((s) => s.connected)
  const toggle = useCmdPaletteStore((s) => s.toggle)

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
