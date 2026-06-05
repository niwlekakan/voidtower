import { useState, useEffect, useCallback } from 'react'
import { LABEL_MAP, ICON_MAP } from '@/aios/AiosDock'
import { useAiosStore } from '@/aios/store/aios'
import AiosStatusBar from '@/aios/AiosStatusBar'
import AnimatedBackground from '@/components/ui/AnimatedBackground'

interface Props { onOpen: (key: string) => void }

const GRID_KEYS = ['dashboard', 'alerts', 'containers', 'services', 'ai', 'apps', 'terminal', 'network', 'backups', 'storage', 'security', 'diagnostics']

export default function AiosTvLayout({ onOpen }: Props) {
  const [focused, setFocused] = useState(0)
  const [expandedKey, setExpandedKey] = useState<string | null>(null)
  const { panels, activeWorkspace, snapPanel } = useAiosStore()
  const statusBarH = 52

  // Determine columns: 2×2 for ≤4 tiles, 3×2 for more
  const tiles = GRID_KEYS.slice(0, 6)
  const COLS = tiles.length <= 4 ? 2 : 3

  const navigate = useCallback((delta: number) => {
    setFocused((f) => Math.max(0, Math.min(tiles.length - 1, f + delta)))
  }, [tiles.length])

  const handleSelect = useCallback((key: string) => {
    // If the panel is already open, set it to fullscreen via snapPanel
    const existingPanel = panels.find(
      (p) => p.workspaceIndex === activeWorkspace && p.component === key,
    )
    if (existingPanel) {
      snapPanel(existingPanel.id, 'fullscreen')
      setExpandedKey(key)
    } else {
      onOpen(key)
      setExpandedKey(key)
    }
  }, [panels, activeWorkspace, snapPanel, onOpen])

  const handleBack = useCallback(() => {
    setExpandedKey(null)
  }, [])

  useEffect(() => {
    const down = (e: KeyboardEvent) => {
      if (expandedKey) {
        if (e.key === 'Escape' || e.key === 'Backspace') { e.preventDefault(); handleBack() }
        return
      }
      if (e.key === 'ArrowRight') { e.preventDefault(); navigate(1) }
      if (e.key === 'ArrowLeft')  { e.preventDefault(); navigate(-1) }
      if (e.key === 'ArrowDown')  { e.preventDefault(); navigate(COLS) }
      if (e.key === 'ArrowUp')    { e.preventDefault(); navigate(-COLS) }
      if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); handleSelect(tiles[focused]) }
    }
    window.addEventListener('keydown', down)
    return () => window.removeEventListener('keydown', down)
  }, [focused, navigate, handleSelect, handleBack, tiles, COLS, expandedKey])

  // Fullscreen expanded view
  if (expandedKey) {
    return (
      <div style={{ position: 'fixed', inset: 0, background: 'var(--bg-base)', display: 'flex', flexDirection: 'column' }}>
        <AnimatedBackground />
        <AiosStatusBar tier="tv" />
        <div style={{ flex: 1, marginTop: statusBarH, overflow: 'hidden', position: 'relative' }}>
          <button
            onClick={handleBack}
            style={{
              position: 'absolute', top: 16, right: 24, zIndex: 100,
              padding: '8px 20px', borderRadius: 8, fontSize: 16,
              background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)',
              color: 'var(--text-primary)', cursor: 'pointer',
            }}
          >
            ← Back
          </button>
          <div style={{ height: '100%', overflow: 'auto' }}>
            {/* Content rendered via the parent AiosLayout's panel system */}
            <div style={{ padding: 32, color: 'var(--text-muted)', fontSize: 18, textAlign: 'center', marginTop: 80 }}>
              {LABEL_MAP[expandedKey] ?? expandedKey} is open in panel mode.
            </div>
          </div>
        </div>
      </div>
    )
  }

  return (
    <div style={{ position: 'fixed', inset: 0, display: 'flex', flexDirection: 'column', background: 'var(--bg-base)' }}>
      <AnimatedBackground />
      <AiosStatusBar tier="tv" />

      <div
        style={{
          flex: 1,
          display: 'grid',
          gridTemplateColumns: `repeat(${COLS}, 1fr)`,
          gap: 24,
          padding: '32px 48px',
          marginTop: statusBarH,
          alignContent: 'center',
        }}
      >
        {tiles.map((key, i) => {
          const Icon = ICON_MAP[key]
          const label = LABEL_MAP[key]
          const isFocused = i === focused

          return (
            <button
              key={key}
              onClick={() => handleSelect(key)}
              onFocus={() => setFocused(i)}
              style={{
                display: 'flex', flexDirection: 'column',
                alignItems: 'center', justifyContent: 'center',
                gap: 20, padding: '40px 32px',
                background: isFocused ? 'rgba(0,0,0,0.6)' : 'rgba(0,0,0,0.6)',
                backdropFilter: 'blur(20px)',
                border: `2px solid ${isFocused ? 'var(--accent-primary)' : 'rgba(255,255,255,0.1)'}`,
                borderRadius: 16, cursor: 'pointer',
                outline: 'none', transition: 'all 0.15s',
                boxShadow: isFocused ? '0 0 0 3px var(--accent-primary-subtle), 0 8px 32px rgba(0,0,0,0.4)' : '0 4px 16px rgba(0,0,0,0.3)',
                transform: isFocused ? 'scale(1.04)' : 'scale(1)',
              }}
            >
              {Icon && (
                <Icon
                  size={52}
                  style={{ color: isFocused ? 'var(--accent-primary)' : 'var(--text-muted)' }}
                />
              )}
              <span
                style={{
                  // 1.5× base text size (base ~14px → 21px)
                  fontSize: 21,
                  fontWeight: 600,
                  color: isFocused ? 'var(--accent-primary)' : 'var(--text-primary)',
                }}
              >
                {label}
              </span>
            </button>
          )
        })}
      </div>

      <div style={{ padding: '16px 48px', fontSize: 15, color: 'var(--text-muted)', display: 'flex', gap: 32 }}>
        <span>← → ↑ ↓ navigate</span>
        <span>Enter to open</span>
        <span>Esc to go back</span>
      </div>
    </div>
  )
}
