import { useState, useEffect, useRef } from 'react'
import { ICON_MAP, LABEL_MAP } from '@/aios/AiosDock'
import AiosStatusBar from '@/aios/AiosStatusBar'

const KIOSK_TILES_KEY = 'vt-kiosk-tiles'
const KIOSK_INTERVAL_KEY = 'vt-kiosk-interval'

const DEFAULT_TILES = ['dashboard', 'containers', 'alerts', 'services', 'network', 'backups']
const DEFAULT_INTERVAL_MS = 30_000

interface Props { onOpen: (key: string) => void }

export default function AiosKioskLayout({ onOpen }: Props) {
  const tiles: string[] = JSON.parse(localStorage.getItem(KIOSK_TILES_KEY) ?? JSON.stringify(DEFAULT_TILES))
  const interval = parseInt(localStorage.getItem(KIOSK_INTERVAL_KEY) ?? String(DEFAULT_INTERVAL_MS))
  const [activeTile, setActiveTile] = useState(0)
  const [waking, setWaking] = useState(false)
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null)
  const statusBarH = 28

  // Auto-cycle
  useEffect(() => {
    timerRef.current = setInterval(() => {
      setActiveTile((i) => (i + 1) % tiles.length)
    }, interval)
    return () => { if (timerRef.current) clearInterval(timerRef.current) }
  }, [tiles.length, interval])

  // Wake on click
  const handleWake = () => {
    if (!waking) { setWaking(true); return }
    // second click = open the focused tile
    onOpen(tiles[activeTile])
    setWaking(false)
  }

  const cols = tiles.length <= 4 ? 2 : 3

  return (
    <div
      style={{ position: 'fixed', inset: 0, background: 'var(--bg-base)', cursor: waking ? 'pointer' : 'default' }}
      onClick={handleWake}
    >
      <AiosStatusBar tier="kiosk" />

      <div style={{
        position: 'absolute', inset: 0, top: statusBarH,
        display: 'grid', gridTemplateColumns: `repeat(${cols}, 1fr)`,
        gap: 2, padding: 2,
      }}>
        {tiles.map((key, i) => {
          const Icon = ICON_MAP[key]
          const label = LABEL_MAP[key]
          const isActive = i === activeTile

          return (
            <div
              key={key}
              style={{
                background: isActive ? 'var(--bg-elevated)' : 'var(--bg-panel)',
                border: `1px solid ${isActive ? 'var(--accent-primary)' : 'var(--border-subtle)'}`,
                borderRadius: 4, overflow: 'hidden',
                display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center',
                gap: 8, opacity: isActive ? 1 : 0.7,
                transition: 'opacity 0.5s, border-color 0.5s',
              }}
            >
              {Icon && <Icon size={24} style={{ color: isActive ? 'var(--accent-primary)' : 'var(--text-muted)' }} />}
              <span style={{ fontSize: 12, fontWeight: 600, color: isActive ? 'var(--text-primary)' : 'var(--text-muted)' }}>
                {label}
              </span>
            </div>
          )
        })}
      </div>

      {/* Wake hint */}
      {!waking && (
        <div style={{
          position: 'absolute', bottom: 12, left: '50%', transform: 'translateX(-50%)',
          fontSize: 11, color: 'var(--text-disabled)',
        }}>
          Tap to interact
        </div>
      )}
      {waking && (
        <div style={{
          position: 'absolute', inset: 0, display: 'flex', alignItems: 'center', justifyContent: 'center',
          background: 'rgba(0,0,0,0.6)', zIndex: 100,
        }}>
          <div style={{ textAlign: 'center' }}>
            <p style={{ fontSize: 18, fontWeight: 600, color: 'var(--text-primary)', marginBottom: 8 }}>Open {LABEL_MAP[tiles[activeTile]]}?</p>
            <p style={{ fontSize: 13, color: 'var(--text-muted)' }}>Tap again to confirm · tap elsewhere to cancel</p>
          </div>
        </div>
      )}
    </div>
  )
}
