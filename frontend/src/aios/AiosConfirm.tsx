import { useEffect, useRef, useState } from 'react'
import { AlertTriangle, XOctagon } from 'lucide-react'

// ── PostMessage event type ────────────────────────────────────────────────────

export type ConfirmRisk = 'warn' | 'dangerous'

export interface ConfirmRequest {
  type: 'voidtower:confirm'
  /** Unique id so callers can match the response */
  requestId?: string
  action: string
  description?: string
  risk: ConfirmRisk
}

// ── AiosConfirm ───────────────────────────────────────────────────────────────

/**
 * Listens for `window.postMessage({ type: 'voidtower:confirm', ... })` events.
 * Renders a modal overlay when triggered.
 * Posts back `{ type: 'voidtower:confirm:response', requestId, confirmed: boolean }`
 * to the source window (or `window` if no source) when the user responds.
 */
export default function AiosConfirm() {
  const [req, setReq] = useState<ConfirmRequest | null>(null)
  const [typed, setTyped] = useState('')
  const sourceRef = useRef<MessageEventSource | null>(null)
  const inputRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    const handler = (e: MessageEvent) => {
      if (!e.data || e.data.type !== 'voidtower:confirm') return
      const payload = e.data as ConfirmRequest
      sourceRef.current = e.source
      setReq(payload)
      setTyped('')
    }
    window.addEventListener('message', handler)
    return () => window.removeEventListener('message', handler)
  }, [])

  // Focus the typed input when a dangerous modal opens
  useEffect(() => {
    if (req?.risk === 'dangerous') {
      setTimeout(() => inputRef.current?.focus(), 80)
    }
  }, [req])

  const respond = (confirmed: boolean) => {
    const response = {
      type: 'voidtower:confirm:response',
      requestId: req?.requestId,
      confirmed,
    }
    try {
      if (sourceRef.current && 'postMessage' in sourceRef.current) {
        (sourceRef.current as Window).postMessage(response, '*')
      } else {
        window.postMessage(response, '*')
      }
    } catch {
      window.postMessage(response, '*')
    }
    setReq(null)
    setTyped('')
    sourceRef.current = null
  }

  if (!req) return null

  const isDangerous = req.risk === 'dangerous'
  const canConfirm = isDangerous ? typed === 'CONFIRM' : true

  const riskColor = isDangerous ? 'var(--accent-danger)' : 'var(--accent-warning)'
  const riskSubtle = isDangerous ? 'var(--accent-danger-subtle)' : 'var(--accent-warning-subtle)'
  const RiskIcon = isDangerous ? XOctagon : AlertTriangle

  return (
    <>
      {/* Backdrop */}
      <div
        style={{
          position: 'fixed', inset: 0,
          background: 'rgba(0,0,0,0.65)',
          backdropFilter: 'blur(6px)',
          WebkitBackdropFilter: 'blur(6px)',
          zIndex: 19998,
        }}
        onClick={() => respond(false)}
      />

      {/* Modal */}
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="aios-confirm-title"
        style={{
          position: 'fixed',
          top: '50%', left: '50%',
          transform: 'translate(-50%, -50%)',
          zIndex: 19999,
          width: 400, maxWidth: 'calc(100vw - 32px)',
          background: 'var(--bg-panel)',
          border: `1px solid ${riskColor}44`,
          borderRadius: 12,
          boxShadow: `0 8px 40px rgba(0,0,0,0.7), 0 0 0 1px ${riskColor}22`,
          overflow: 'hidden',
        }}
      >
        {/* Header band */}
        <div style={{
          display: 'flex', alignItems: 'center', gap: 10,
          padding: '14px 18px',
          background: riskSubtle,
          borderBottom: `1px solid ${riskColor}33`,
        }}>
          <RiskIcon size={18} style={{ color: riskColor, flexShrink: 0 }} />
          <span id="aios-confirm-title" style={{ fontSize: 13, fontWeight: 700, color: riskColor }}>
            {isDangerous ? 'Dangerous action' : 'Confirm action'}
          </span>
          <span style={{
            marginLeft: 'auto', fontSize: 9, fontWeight: 700, letterSpacing: '0.08em',
            padding: '2px 7px', borderRadius: 4,
            background: riskColor + '22',
            color: riskColor,
            textTransform: 'uppercase',
          }}>
            {req.risk}
          </span>
        </div>

        {/* Body */}
        <div style={{ padding: '18px 18px 6px' }}>
          <div style={{ fontSize: 13, fontWeight: 600, color: 'var(--text-primary)', marginBottom: 8 }}>
            {req.action}
          </div>
          {req.description && (
            <div style={{ fontSize: 12, color: 'var(--text-secondary)', lineHeight: 1.6 }}>
              {req.description}
            </div>
          )}
        </div>

        {/* Dangerous: require typing CONFIRM */}
        {isDangerous && (
          <div style={{ padding: '14px 18px 0' }}>
            <label style={{ fontSize: 11, color: 'var(--text-muted)', display: 'block', marginBottom: 6 }}>
              Type <strong style={{ color: riskColor }}>CONFIRM</strong> to proceed
            </label>
            <input
              ref={inputRef}
              value={typed}
              onChange={(e) => setTyped(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter' && canConfirm) respond(true)
                if (e.key === 'Escape') respond(false)
              }}
              placeholder="CONFIRM"
              autoComplete="off"
              spellCheck={false}
              style={{
                width: '100%', boxSizing: 'border-box',
                padding: '7px 10px', borderRadius: 6,
                background: 'var(--bg-elevated)',
                border: `1px solid ${typed === 'CONFIRM' ? riskColor : 'var(--border-default)'}`,
                color: 'var(--text-primary)', fontSize: 13,
                outline: 'none', transition: 'border-color 0.15s',
                fontFamily: 'var(--font-mono)',
              }}
            />
          </div>
        )}

        {/* Buttons */}
        <div style={{
          display: 'flex', gap: 8, justifyContent: 'flex-end',
          padding: '16px 18px 18px',
        }}>
          <button
            onClick={() => respond(false)}
            style={{
              padding: '7px 18px', borderRadius: 7, fontSize: 12, fontWeight: 500,
              background: 'var(--bg-elevated)',
              border: '1px solid var(--border-default)',
              color: 'var(--text-secondary)', cursor: 'pointer',
              transition: 'opacity 0.1s',
            }}
            onMouseEnter={(e) => (e.currentTarget.style.opacity = '0.75')}
            onMouseLeave={(e) => (e.currentTarget.style.opacity = '1')}
          >
            Cancel
          </button>
          <button
            onClick={() => canConfirm && respond(true)}
            disabled={!canConfirm}
            style={{
              padding: '7px 18px', borderRadius: 7, fontSize: 12, fontWeight: 600,
              background: canConfirm ? riskColor : 'var(--bg-elevated)',
              border: `1px solid ${canConfirm ? riskColor : 'var(--border-subtle)'}`,
              color: canConfirm ? '#fff' : 'var(--text-disabled)',
              cursor: canConfirm ? 'pointer' : 'not-allowed',
              transition: 'opacity 0.1s, background 0.15s',
              opacity: canConfirm ? 1 : 0.5,
            }}
            onMouseEnter={(e) => { if (canConfirm) e.currentTarget.style.opacity = '0.85' }}
            onMouseLeave={(e) => { e.currentTarget.style.opacity = '1' }}
          >
            Confirm
          </button>
        </div>
      </div>
    </>
  )
}
