import { useState, useEffect, useCallback } from 'react'
import { LABEL_MAP, ICON_MAP } from '@/aios/AiosDock'
import AiosStatusBar from '@/aios/AiosStatusBar'

interface Props { onOpen: (key: string) => void }

const TILE_KEYS = ['dashboard', 'alerts', 'containers', 'services', 'ai', 'apps', 'terminal', 'network', 'backups']

export default function AiosTvLayout({ onOpen }: Props) {
  const [focused, setFocused] = useState(0)
  const statusBarH = 48

  const navigate = useCallback((delta: number) => {
    setFocused((f) => Math.max(0, Math.min(TILE_KEYS.length - 1, f + delta)))
  }, [])

  useEffect(() => {
    const down = (e: KeyboardEvent) => {
      const COLS = 3
      if (e.key === 'ArrowRight') navigate(1)
      if (e.key === 'ArrowLeft')  navigate(-1)
      if (e.key === 'ArrowDown')  navigate(COLS)
      if (e.key === 'ArrowUp')    navigate(-COLS)
      if (e.key === 'Enter' || e.key === ' ') onOpen(TILE_KEYS[focused])
    }
    window.addEventListener('keydown', down)
    return () => window.removeEventListener('keydown', down)
  }, [focused, navigate, onOpen])

  return (
    <div style={{ position: 'fixed', inset: 0, display: 'flex', flexDirection: 'column', background: 'var(--bg-base)' }}>
      <AiosStatusBar tier="tv" />

      <div style={{
        flex: 1, display: 'grid',
        gridTemplateColumns: 'repeat(3, 1fr)', gap: 24,
        padding: '32px 48px', marginTop: statusBarH,
        alignContent: 'center',
      }}>
        {TILE_KEYS.map((key, i) => {
          const Icon = ICON_MAP[key]
          const label = LABEL_MAP[key]
          const isFocused = i === focused

          return (
            <button
              key={key}
              onClick={() => onOpen(key)}
              onFocus={() => setFocused(i)}
              style={{
                display: 'flex', flexDirection: 'column',
                alignItems: 'center', justifyContent: 'center',
                gap: 16, padding: 32,
                background: isFocused ? 'var(--accent-primary-subtle)' : 'var(--bg-panel)',
                border: `3px solid ${isFocused ? 'var(--accent-primary)' : 'var(--border-subtle)'}`,
                borderRadius: 16, cursor: 'pointer',
                outline: 'none', transition: 'all 0.15s',
                boxShadow: isFocused ? '0 0 0 4px var(--accent-primary-subtle)' : 'none',
              }}
            >
              <Icon size={40} style={{ color: isFocused ? 'var(--accent-primary)' : 'var(--text-muted)' }} />
              <span style={{ fontSize: 20, fontWeight: 600, color: isFocused ? 'var(--accent-primary)' : 'var(--text-primary)' }}>
                {label}
              </span>
            </button>
          )
        })}
      </div>

      <div style={{ padding: '12px 48px', fontSize: 14, color: 'var(--text-muted)', display: 'flex', gap: 24 }}>
        <span>← → ↑ ↓ navigate</span>
        <span>Enter to open</span>
      </div>
    </div>
  )
}
