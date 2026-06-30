import React, { useEffect, useRef, useState, useCallback } from 'react'
import { useAiosStore } from '@/aios/store/aios'
import { api } from '@/api/client'
import type { AiProviderConfig } from '@/api/types'

interface Message {
  role: 'user' | 'assistant'
  content: string
  streaming?: boolean
  provider?: string
}

interface Props {
  open: boolean
  onClose: () => void
}

export function AiosAskPopup({ open, onClose }: Props) {
  const [messages, setMessages] = useState<Message[]>([])
  const [input, setInput] = useState('')
  const [loading, setLoading] = useState(false)
  const [providers, setProviders] = useState<AiProviderConfig[]>([])
  const [selectedProviderId, setSelectedProviderId] = useState<string>('')
  const scrollRef = useRef<HTMLDivElement>(null)
  const inputRef = useRef<HTMLInputElement>(null)

  const focusedTitle = useAiosStore(
    (s) => s.panels.find((p) => p.id === s.focusedId)?.title ?? '',
  )

  // Load provider list once
  useEffect(() => {
    api.aiProviders.list()
      .then(list => setProviders(list.filter(p => p.enabled)))
      .catch(() => {})
  }, [])

  useEffect(() => {
    if (open) setTimeout(() => inputRef.current?.focus(), 50)
  }, [open])

  useEffect(() => {
    if (scrollRef.current) scrollRef.current.scrollTop = scrollRef.current.scrollHeight
  }, [messages])

  useEffect(() => {
    if (!open) return
    const handler = (e: KeyboardEvent) => {
      if (e.key === 'Escape') { e.stopPropagation(); onClose() }
    }
    window.addEventListener('keydown', handler, true)
    return () => window.removeEventListener('keydown', handler, true)
  }, [open, onClose])

  const appendDelta = useCallback((delta: string) => {
    setMessages(prev => {
      const next = [...prev]
      next[next.length - 1] = { ...next[next.length - 1], content: next[next.length - 1].content + delta }
      return next
    })
  }, [])

  async function handleSubmit() {
    const query = input.trim()
    if (!query || loading) return

    setInput('')
    setMessages(prev => [...prev, { role: 'user', content: query }])
    setLoading(true)
    setMessages(prev => [...prev, { role: 'assistant', content: '', streaming: true }])

    try {
      const body: Record<string, string> = { query }
      if (focusedTitle) body.context = focusedTitle
      if (selectedProviderId) body.provider_id = selectedProviderId

      const res = await fetch('/api/ai/ask', {
        method: 'POST',
        credentials: 'include',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body),
      })

      if (!res.ok) {
        const err = await res.json().catch(() => ({ error: 'Request failed' }))
        setMessages(prev => {
          const next = [...prev]
          next[next.length - 1] = { role: 'assistant', content: err.error ?? 'Request failed', streaming: false }
          return next
        })
        setLoading(false)
        return
      }

      const contentType = res.headers.get('content-type') ?? ''
      const reader = res.body?.getReader()
      if (!reader) {
        setMessages(prev => {
          const next = [...prev]; next[next.length - 1] = { role: 'assistant', content: 'No response body', streaming: false }; return next
        })
        setLoading(false)
        return
      }

      const decoder = new TextDecoder()
      let buffer = ''
      const isSSE = contentType.includes('text/event-stream')

      while (true) {
        const { done, value } = await reader.read()
        if (done) break

        if (isSSE) {
          buffer += decoder.decode(value, { stream: true })
          const lines = buffer.split('\n')
          buffer = lines.pop() ?? ''
          for (const line of lines) {
            if (!line.startsWith('data: ')) continue
            const chunk = line.slice(6)
            if (chunk === '[DONE]') continue
            try {
              const parsed = JSON.parse(chunk)
              const delta = parsed?.choices?.[0]?.delta?.content ?? parsed?.content ?? parsed?.text ?? ''
              if (delta) appendDelta(delta)
            } catch {
              if (chunk.trim()) appendDelta(chunk)
            }
          }
        } else {
          const chunk = decoder.decode(value, { stream: true })
          if (chunk) appendDelta(chunk)
        }
      }

      setMessages(prev => {
        const next = [...prev]; next[next.length - 1] = { ...next[next.length - 1], streaming: false }; return next
      })
    } catch {
      setMessages(prev => {
        const next = [...prev]; next[next.length - 1] = { role: 'assistant', content: 'Connection error', streaming: false }; return next
      })
    } finally {
      setLoading(false)
    }
  }

  function handleKeyDown(e: React.KeyboardEvent<HTMLInputElement>) {
    if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); handleSubmit() }
  }

  function handleOpenOdysseus() {
    useAiosStore.getState().openOdysseus()
    onClose()
  }

  const activeProvider = providers.find(p => p.id === selectedProviderId) ?? providers[0]

  return (
    <>
      {open && <div onClick={onClose} style={{ position: 'fixed', inset: 0, zIndex: 10499, background: 'transparent' }} />}

      <div style={{
        position: 'fixed', bottom: 70, left: '50%',
        transform: open ? 'translateX(-50%) translateY(0)' : 'translateX(-50%) translateY(24px)',
        opacity: open ? 1 : 0, pointerEvents: open ? 'auto' : 'none',
        transition: 'transform 0.22s cubic-bezier(0.22,1,0.36,1), opacity 0.18s',
        width: 420, maxWidth: '96vw', zIndex: 10500,
        background: 'rgba(8,6,18,0.96)', backdropFilter: 'blur(48px)',
        WebkitBackdropFilter: 'blur(48px)', border: '1px solid rgba(255,255,255,0.10)',
        borderRadius: 12, display: 'flex', flexDirection: 'column', overflow: 'hidden',
        boxShadow: '0 8px 40px rgba(0,0,0,0.6)',
      }}>
        {/* Header */}
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', padding: '8px 12px 6px', borderBottom: '1px solid rgba(255,255,255,0.07)', flexShrink: 0 }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
            <span style={{ fontSize: 11, fontWeight: 600, color: 'var(--text-muted)', letterSpacing: '0.06em', textTransform: 'uppercase' }}>
              Ask AI
            </span>
            {providers.length > 0 && (
              <select
                value={selectedProviderId || ''}
                onChange={e => setSelectedProviderId(e.target.value)}
                style={{
                  background: 'rgba(255,255,255,0.06)', border: '1px solid rgba(255,255,255,0.12)',
                  borderRadius: 5, color: 'rgba(255,255,255,0.7)', fontSize: 10,
                  padding: '1px 6px', cursor: 'pointer', outline: 'none',
                }}
                title="Select AI provider"
              >
                <option value="">Auto (priority order)</option>
                {providers.map(p => (
                  <option key={p.id} value={p.id}>{p.name}</option>
                ))}
              </select>
            )}
          </div>
          <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
            <button
              onClick={handleOpenOdysseus}
              title="Open in Odysseus"
              style={{ background: 'none', border: '1px solid rgba(255,255,255,0.12)', borderRadius: 5, color: 'rgba(255,255,255,0.6)', fontSize: 10, padding: '2px 7px', cursor: 'pointer', lineHeight: 1.5 }}
            >
              Odysseus
            </button>
            <button onClick={onClose} title="Close (Esc)" style={{ background: 'none', border: 'none', color: 'rgba(255,255,255,0.4)', fontSize: 14, lineHeight: 1, cursor: 'pointer', padding: '2px 4px', borderRadius: 4 }}>✕</button>
          </div>
        </div>

        {/* Context + active provider strip */}
        {(focusedTitle || activeProvider) && (
          <div style={{ display: 'flex', alignItems: 'center', gap: 6, padding: '3px 12px', fontSize: 10, color: 'rgba(255,255,255,0.35)', borderBottom: '1px solid rgba(255,255,255,0.05)', flexShrink: 0 }}>
            {focusedTitle && <span>context: <span style={{ color: 'rgba(255,255,255,0.55)' }}>{focusedTitle}</span></span>}
            {focusedTitle && activeProvider && <span>·</span>}
            {activeProvider && <span>via <span style={{ color: 'rgba(255,255,255,0.55)' }}>{activeProvider.name}</span></span>}
          </div>
        )}

        {/* Messages */}
        <div ref={scrollRef} style={{ flex: 1, overflowY: 'auto', padding: '8px 12px', maxHeight: 200, display: 'flex', flexDirection: 'column', gap: 6 }}>
          {messages.length === 0 && (
            <div style={{ fontSize: 11, color: 'rgba(255,255,255,0.3)', textAlign: 'center', paddingTop: 20, paddingBottom: 20 }}>
              Ask anything about your server…
            </div>
          )}
          {messages.map((msg, i) => (
            <div key={i} style={{ display: 'flex', justifyContent: msg.role === 'user' ? 'flex-end' : 'flex-start' }}>
              <div style={{
                maxWidth: '85%', padding: '5px 9px',
                borderRadius: msg.role === 'user' ? '8px 8px 2px 8px' : '8px 8px 8px 2px',
                background: msg.role === 'user' ? 'var(--accent-primary, #6366f1)' : 'rgba(255,255,255,0.07)',
                color: msg.role === 'user' ? '#fff' : 'var(--text-primary, rgba(255,255,255,0.88))',
                fontSize: 12, lineHeight: 1.5, whiteSpace: 'pre-wrap', wordBreak: 'break-word',
              }}>
                {msg.content}
                {msg.streaming && <span style={{ display: 'inline-block', width: 6, height: 12, background: 'var(--accent-primary, #6366f1)', marginLeft: 2, verticalAlign: 'text-bottom', animation: 'aios-ask-blink 0.9s step-end infinite' }} />}
              </div>
            </div>
          ))}
        </div>

        {/* Input */}
        <div style={{ display: 'flex', alignItems: 'center', gap: 6, padding: '7px 10px', borderTop: '1px solid rgba(255,255,255,0.07)', flexShrink: 0 }}>
          <input
            ref={inputRef}
            type="text"
            value={input}
            onChange={e => setInput(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="Ask a question…"
            disabled={loading}
            style={{ flex: 1, background: 'rgba(255,255,255,0.06)', border: '1px solid rgba(255,255,255,0.10)', borderRadius: 6, color: 'rgba(255,255,255,0.88)', fontSize: 12, padding: '5px 9px', outline: 'none' }}
          />
          <button onClick={handleSubmit} disabled={loading || !input.trim()} style={{ background: 'var(--accent-primary, #6366f1)', border: 'none', borderRadius: 6, color: '#fff', fontSize: 11, fontWeight: 600, padding: '5px 11px', cursor: loading || !input.trim() ? 'not-allowed' : 'pointer', opacity: loading || !input.trim() ? 0.5 : 1, transition: 'opacity 0.15s', whiteSpace: 'nowrap' }}>
            {loading ? '…' : 'Send'}
          </button>
        </div>
      </div>

      <style>{`
        @keyframes aios-ask-blink { 0%, 100% { opacity: 1; } 50% { opacity: 0; } }
      `}</style>
    </>
  )
}
