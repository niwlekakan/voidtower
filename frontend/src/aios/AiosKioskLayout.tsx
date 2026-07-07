import { useState, useEffect, useRef, useCallback } from 'react'
import { ICON_MAP, LABEL_MAP } from '@/aios/AiosDock'
import AiosStatusBar, { STATUS_BAR_H } from '@/aios/AiosStatusBar'
import AnimatedBackground from '@/components/ui/AnimatedBackground'
import { api } from '@/api/client'

// ── Kiosk config ──────────────────────────────────────────────────────────────

interface KioskConfig {
  tiles: string[]
  cycleInterval: number
  wakePin: string  // empty string = no PIN required
}

const CONFIG_KEY = 'kiosk_layout'

const DEFAULTS: KioskConfig = {
  tiles: ['dashboard', 'containers', 'alerts'],
  cycleInterval: 30_000,
  wakePin: '',
}

function loadConfig(): KioskConfig {
  try {
    const raw = localStorage.getItem(CONFIG_KEY)
    if (raw) return { ...DEFAULTS, ...JSON.parse(raw) }
  } catch { /* ignore */ }
  return DEFAULTS
}

// ── Constants ─────────────────────────────────────────────────────────────────

const IDLE_SCREENSAVER_MS = 10 * 60 * 1000  // 10 minutes
const INTERACTIVE_DURATION_MS = 5 * 60 * 1000  // 5 minutes
const CRITICAL_FLASH_MS = 5_000

interface Props { onOpen: (key: string) => void }

// ── PIN Pad overlay ───────────────────────────────────────────────────────────

interface PinPadProps {
  onSuccess: () => void
  onCancel: () => void
  expectedPin: string
}

function PinPad({ onSuccess, onCancel, expectedPin }: PinPadProps) {
  const [digits, setDigits] = useState<string[]>([])
  const [shake, setShake] = useState(false)

  const press = useCallback((d: string) => {
    setDigits((prev) => {
      const next = [...prev, d].slice(0, 4)
      if (next.length === 4) {
        if (next.join('') === expectedPin) {
          setTimeout(onSuccess, 100)
        } else {
          setShake(true)
          setTimeout(() => { setShake(false) }, 600)
          return []
        }
      }
      return next
    })
  }, [expectedPin, onSuccess])

  const del = useCallback(() => setDigits((p) => p.slice(0, -1)), [])

  useEffect(() => {
    const down = (e: KeyboardEvent) => {
      if (e.key >= '0' && e.key <= '9') press(e.key)
      if (e.key === 'Backspace') del()
      if (e.key === 'Escape') onCancel()
    }
    window.addEventListener('keydown', down)
    return () => window.removeEventListener('keydown', down)
  }, [press, del, onCancel])

  return (
    <div
      style={{
        position: 'fixed', inset: 0, zIndex: 200,
        background: 'rgba(0,0,0,0.85)', backdropFilter: 'blur(12px)',
        display: 'flex', alignItems: 'center', justifyContent: 'center',
      }}
      onClick={(e) => { if (e.target === e.currentTarget) onCancel() }}
    >
      <div
        style={{
          background: 'var(--bg-panel)', border: '1px solid var(--border-subtle)',
          borderRadius: 16, padding: '32px 40px', textAlign: 'center',
          minWidth: 260,
          animation: shake ? 'kiosk-shake 0.5s ease' : undefined,
        }}
      >
        <style>{`
          @keyframes kiosk-shake {
            0%,100% { transform: translateX(0) }
            20%,60% { transform: translateX(-8px) }
            40%,80% { transform: translateX(8px) }
          }
        `}</style>

        <div style={{ fontSize: 15, fontWeight: 600, color: 'var(--text-primary)', marginBottom: 20 }}>
          Enter PIN to unlock
        </div>

        {/* Dot indicators */}
        <div style={{ display: 'flex', justifyContent: 'center', gap: 12, marginBottom: 24 }}>
          {[0, 1, 2, 3].map((i) => (
            <div
              key={i}
              style={{
                width: 14, height: 14, borderRadius: '50%',
                background: i < digits.length ? 'var(--accent-primary)' : 'var(--border-subtle)',
                transition: 'background 0.1s',
              }}
            />
          ))}
        </div>

        {/* Keypad */}
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', gap: 10 }}>
          {['1','2','3','4','5','6','7','8','9','','0','⌫'].map((key) => (
            <button
              key={key}
              onClick={() => {
                if (key === '⌫') del()
                else if (key !== '') press(key)
              }}
              disabled={key === ''}
              style={{
                height: 52, fontSize: 20, fontWeight: 500,
                background: key === '' ? 'transparent' : 'var(--bg-elevated)',
                border: key === '' ? 'none' : '1px solid var(--border-subtle)',
                borderRadius: 10, cursor: key === '' ? 'default' : 'pointer',
                color: 'var(--text-primary)',
                transition: 'background 0.1s',
              }}
              onMouseEnter={(e) => {
                if (key !== '') (e.currentTarget as HTMLElement).style.background = 'var(--bg-panel)'
              }}
              onMouseLeave={(e) => {
                if (key !== '') (e.currentTarget as HTMLElement).style.background = 'var(--bg-elevated)'
              }}
            >
              {key}
            </button>
          ))}
        </div>

        <button
          onClick={onCancel}
          style={{
            marginTop: 16, fontSize: 13, color: 'var(--text-muted)',
            background: 'none', border: 'none', cursor: 'pointer',
          }}
        >
          Cancel
        </button>
      </div>
    </div>
  )
}

// ── Clock for screensaver ────────────────────────────────────────────────────

function ScreensaverClock() {
  const [time, setTime] = useState(() => new Date())
  useEffect(() => {
    const t = setInterval(() => setTime(new Date()), 1000)
    return () => clearInterval(t)
  }, [])
  return (
    <div style={{
      position: 'absolute', inset: 0, display: 'flex', flexDirection: 'column',
      alignItems: 'center', justifyContent: 'center', pointerEvents: 'none',
    }}>
      <div style={{ fontSize: 72, fontWeight: 200, color: 'rgba(255,255,255,0.7)', fontVariantNumeric: 'tabular-nums' }}>
        {time.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}
      </div>
      <div style={{ fontSize: 18, color: 'rgba(255,255,255,0.4)', marginTop: 8 }}>
        {time.toLocaleDateString([], { weekday: 'long', month: 'long', day: 'numeric' })}
      </div>
    </div>
  )
}

// ── Main component ────────────────────────────────────────────────────────────

export default function AiosKioskLayout({ onOpen }: Props) {
  const config = useRef<KioskConfig>(loadConfig())
  const { tiles, cycleInterval, wakePin } = config.current

  const [activeTile, setActiveTile] = useState(0)
  const [showPinPad, setShowPinPad] = useState(false)
  const [interactive, setInteractive] = useState(false)
  const [screensaver, setScreensaver] = useState(false)
  const [criticalFlash, setCriticalFlash] = useState(false)

  const cycleRef = useRef<ReturnType<typeof setInterval> | null>(null)
  const interactiveRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const idleRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const lastInteractionRef = useRef<number>(Date.now())

  // ── Auto-cycle tiles ──────────────────────────────────────────────────────
  const startCycle = useCallback(() => {
    if (cycleRef.current) clearInterval(cycleRef.current)
    cycleRef.current = setInterval(() => {
      setActiveTile((i) => (i + 1) % tiles.length)
    }, cycleInterval)
  }, [tiles.length, cycleInterval])

  const stopCycle = useCallback(() => {
    if (cycleRef.current) { clearInterval(cycleRef.current); cycleRef.current = null }
  }, [])

  useEffect(() => {
    startCycle()
    return stopCycle
  }, [startCycle, stopCycle])

  // ── Idle screensaver ──────────────────────────────────────────────────────
  const resetIdleTimer = useCallback(() => {
    lastInteractionRef.current = Date.now()
    setScreensaver(false)
    if (idleRef.current) clearTimeout(idleRef.current)
    idleRef.current = setTimeout(() => setScreensaver(true), IDLE_SCREENSAVER_MS)
  }, [])

  useEffect(() => {
    resetIdleTimer()
    return () => { if (idleRef.current) clearTimeout(idleRef.current) }
  }, [resetIdleTimer])

  // ── Interactive mode timer (5 min) ────────────────────────────────────────
  const enterInteractive = useCallback(() => {
    setInteractive(true)
    setShowPinPad(false)
    stopCycle()
    resetIdleTimer()
    if (interactiveRef.current) clearTimeout(interactiveRef.current)
    interactiveRef.current = setTimeout(() => {
      setInteractive(false)
      startCycle()
    }, INTERACTIVE_DURATION_MS)
  }, [stopCycle, startCycle, resetIdleTimer])

  useEffect(() => {
    return () => { if (interactiveRef.current) clearTimeout(interactiveRef.current) }
  }, [])

  // ── Critical alert polling ────────────────────────────────────────────────
  useEffect(() => {
    const checkAlerts = async () => {
      try {
        const data = await api.alerts.list('active')
        const hasCritical = data.alerts.some((a) => a.severity === 'critical')
        if (hasCritical) {
          setCriticalFlash(true)
          setTimeout(() => setCriticalFlash(false), CRITICAL_FLASH_MS)
        }
      } catch { /* ignore in kiosk mode */ }
    }
    checkAlerts()
    const t = setInterval(checkAlerts, 60_000)
    return () => clearInterval(t)
  }, [])

  // ── Wake / click handler ──────────────────────────────────────────────────
  const handleClick = useCallback(() => {
    resetIdleTimer()
    if (screensaver) { setScreensaver(false); return }
    if (interactive) { onOpen(tiles[activeTile]); return }

    if (wakePin) {
      setShowPinPad(true)
    } else {
      enterInteractive()
    }
  }, [screensaver, interactive, wakePin, tiles, activeTile, onOpen, enterInteractive, resetIdleTimer])

  const cols = tiles.length <= 4 ? 2 : 3

  return (
    <div
      style={{
        position: 'fixed', inset: 0, background: 'var(--bg-base)',
        cursor: interactive ? 'default' : 'pointer',
        opacity: screensaver ? 0.5 : 1,
        transition: 'opacity 1s ease',
        // Critical alert: red flash border
        outline: criticalFlash ? '4px solid var(--accent-danger)' : 'none',
        animation: criticalFlash ? 'kiosk-critical 0.5s ease 10' : undefined,
      }}
      onClick={handleClick}
    >
      <style>{`
        @keyframes kiosk-critical {
          0%,100% { outline-color: var(--accent-danger); }
          50% { outline-color: transparent; }
        }
      `}</style>

      <AnimatedBackground />
      <AiosStatusBar tier="kiosk" />

      {/* Tile grid */}
      <div
        style={{
          position: 'absolute', inset: 0, top: STATUS_BAR_H,
          display: 'grid', gridTemplateColumns: `repeat(${cols}, 1fr)`,
          gap: 2, padding: 2,
        }}
      >
        {tiles.map((key, i) => {
          const Icon = ICON_MAP[key]
          const label = LABEL_MAP[key]
          const isActive = i === activeTile

          return (
            <div
              key={`${key}-${i}`}
              style={{
                background: isActive ? 'rgba(0,0,0,0.7)' : 'rgba(0,0,0,0.5)',
                backdropFilter: 'blur(16px)',
                border: `1px solid ${isActive ? 'var(--accent-primary)' : 'rgba(255,255,255,0.08)'}`,
                borderRadius: 4, overflow: 'hidden',
                display: 'flex', flexDirection: 'column',
                alignItems: 'center', justifyContent: 'center',
                gap: 8, opacity: isActive ? 1 : 0.65,
                transition: 'opacity 0.5s, border-color 0.5s, background 0.5s',
              }}
            >
              {Icon && (
                <Icon
                  size={28}
                  style={{ color: isActive ? 'var(--accent-primary)' : 'var(--text-muted)' }}
                />
              )}
              <span
                style={{
                  fontSize: 13, fontWeight: 600,
                  color: isActive ? 'var(--text-primary)' : 'var(--text-muted)',
                }}
              >
                {label}
              </span>
            </div>
          )
        })}
      </div>

      {/* Screensaver overlay — clock only */}
      {screensaver && <ScreensaverClock />}

      {/* Wake hint */}
      {!interactive && !screensaver && (
        <div
          style={{
            position: 'absolute', bottom: 14, left: '50%', transform: 'translateX(-50%)',
            fontSize: 11, color: 'var(--text-disabled)',
            background: 'rgba(0,0,0,0.4)', borderRadius: 20, padding: '3px 10px',
            pointerEvents: 'none',
          }}
        >
          {wakePin ? 'Tap to enter PIN' : 'Tap to interact'}
        </div>
      )}

      {/* Interactive mode banner */}
      {interactive && (
        <div
          style={{
            position: 'absolute', bottom: 14, left: '50%', transform: 'translateX(-50%)',
            fontSize: 12, color: 'var(--accent-primary)',
            background: 'rgba(0,0,0,0.5)', borderRadius: 20, padding: '4px 14px',
            border: '1px solid var(--accent-primary)', pointerEvents: 'none',
          }}
        >
          Interactive mode — tap a tile to open
        </div>
      )}

      {/* PIN pad overlay */}
      {showPinPad && (
        <PinPad
          expectedPin={wakePin}
          onSuccess={enterInteractive}
          onCancel={() => setShowPinPad(false)}
        />
      )}
    </div>
  )
}
