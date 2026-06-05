import { useState, useRef, useEffect, useCallback } from 'react'
import { Search, Command } from 'lucide-react'
import { DOCK_ITEMS } from '@/aios/AiosDock'
import type { DeviceTier } from '@/aios/hooks/useDeviceTier'

interface Props {
  tier: DeviceTier
  statusBarH: number
  dockH: number
  onOpen: (key: string) => void
  onOdysseus: (query: string) => void
}

export default function AiosCommandBar({ tier, dockH, onOpen, onOdysseus }: Props) {
  const [open, setOpen] = useState(false)
  const [query, setQuery] = useState('')
  const [selected, setSelected] = useState(0)
  const inputRef = useRef<HTMLInputElement>(null)

  const isPhone = tier === 'phone'
  const isTv = tier === 'tv' || tier === 'kiosk'

  const isOdysseus = query.startsWith('/')
  const isEmbed = query.startsWith('embed:')

  const results = isOdysseus || isEmbed ? [] : DOCK_ITEMS.filter((item) =>
    item.label.toLowerCase().includes(query.toLowerCase()) ||
    item.key.toLowerCase().includes(query.toLowerCase()),
  ).slice(0, 8)

  const commit = useCallback((key?: string) => {
    const target = key ?? results[selected]?.key
    if (!target) return
    if (isOdysseus) { onOdysseus(query.slice(1)); setOpen(false); setQuery(''); return }
    onOpen(target)
    setOpen(false)
    setQuery('')
    setSelected(0)
  }, [results, selected, isOdysseus, query, onOdysseus, onOpen])

  useEffect(() => {
    if (open) inputRef.current?.focus()
  }, [open])

  useEffect(() => {
    const down = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === 'k') { e.preventDefault(); setOpen(true) }
      if (e.key === 'Escape' && open) setOpen(false)
    }
    window.addEventListener('keydown', down)
    return () => window.removeEventListener('keydown', down)
  }, [open])

  if (isTv) return null

  // Phone: Floating Action Button
  if (isPhone) {
    return (
      <>
        <button
          onClick={() => setOpen(true)}
          style={{
            position: 'fixed', bottom: dockH + 12, right: 16, zIndex: 9997,
            width: 48, height: 48, borderRadius: '50%',
            background: 'var(--accent-primary)', border: 'none', cursor: 'pointer',
            display: 'flex', alignItems: 'center', justifyContent: 'center',
            boxShadow: '0 4px 16px rgba(0,0,0,0.4)',
          }}
          aria-label="Open command bar"
        >
          <Search size={20} style={{ color: '#fff' }} />
        </button>

        {open && <CommandSheet query={query} setQuery={setQuery} results={results}
          commit={commit} onClose={() => setOpen(false)} inputRef={inputRef}
          dockH={dockH} />}
      </>
    )
  }

  // Desktop/tablet: centered pill + dropdown
  const isVerticalDock = window.innerWidth >= 1400
  const offsetLeft = isVerticalDock ? 56 : 0

  return (
    <div style={{
      position: 'fixed',
      bottom: dockH + 10, left: `calc(50% + ${offsetLeft / 2}px)`,
      transform: 'translateX(-50%)',
      zIndex: 9997, width: Math.min(480, window.innerWidth - offsetLeft - 32),
    }}>
      <div
        onClick={() => setOpen(true)}
        style={{
          display: 'flex', alignItems: 'center', gap: 8,
          background: 'var(--bg-elevated)', border: `1px solid ${open ? 'var(--accent-primary)' : 'var(--border-subtle)'}`,
          borderRadius: 24, padding: '6px 14px', cursor: 'text',
          boxShadow: open ? '0 0 0 3px var(--accent-primary-subtle)' : '0 2px 8px rgba(0,0,0,0.3)',
          transition: 'border-color 0.15s, box-shadow 0.15s',
        }}
      >
        <Command size={13} style={{ color: 'var(--text-muted)', flexShrink: 0 }} />
        <span style={{ flex: 1, fontSize: 12, color: 'var(--text-muted)' }}>Open app or /ask AI…</span>
        <kbd style={{ fontSize: 10, color: 'var(--text-disabled)', background: 'var(--bg-panel)', padding: '1px 5px', borderRadius: 4, border: '1px solid var(--border-subtle)' }}>
          ⌘K
        </kbd>
      </div>

      {open && (
        <>
          {/* Backdrop */}
          <div style={{ position: 'fixed', inset: 0, zIndex: -1 }} onClick={() => setOpen(false)} />

          {/* Dropdown */}
          <div style={{
            position: 'absolute', bottom: '100%', left: 0, right: 0, marginBottom: 6,
            background: 'var(--bg-panel)', border: '1px solid var(--border-subtle)',
            borderRadius: 12, boxShadow: '0 8px 32px rgba(0,0,0,0.5)',
            overflow: 'hidden',
          }}>
            <div style={{ padding: '8px 10px', borderBottom: '1px solid var(--border-subtle)' }}>
              <input
                ref={inputRef}
                value={query}
                onChange={(e) => { setQuery(e.target.value); setSelected(0) }}
                onKeyDown={(e) => {
                  if (e.key === 'Enter') commit()
                  if (e.key === 'ArrowDown') setSelected((s) => Math.min(s + 1, results.length - 1))
                  if (e.key === 'ArrowUp') setSelected((s) => Math.max(s - 1, 0))
                  if (e.key === 'Escape') setOpen(false)
                }}
                placeholder="Dashboard, Containers, /ask AI anything…"
                style={{
                  width: '100%', background: 'none', border: 'none', outline: 'none',
                  fontSize: 13, color: 'var(--text-primary)',
                }}
              />
            </div>

            {isOdysseus && (
              <div
                onClick={() => commit()}
                style={{ padding: '10px 14px', cursor: 'pointer', background: 'var(--accent-primary-subtle)', display: 'flex', gap: 10, alignItems: 'center' }}
              >
                <span style={{ fontSize: 18 }}>🧠</span>
                <div>
                  <div style={{ fontSize: 12, fontWeight: 600, color: 'var(--accent-primary)' }}>Ask Odysseus</div>
                  <div style={{ fontSize: 11, color: 'var(--text-muted)' }}>{query.slice(1) || 'Type your question…'}</div>
                </div>
              </div>
            )}

            {results.map((item, i) => {
              const Icon = item.icon
              return (
                <div
                  key={item.key}
                  onClick={() => commit(item.key)}
                  onMouseEnter={() => setSelected(i)}
                  style={{
                    display: 'flex', alignItems: 'center', gap: 10,
                    padding: '9px 14px', cursor: 'pointer',
                    background: i === selected ? 'var(--bg-elevated)' : 'transparent',
                    transition: 'background 0.1s',
                  }}
                >
                  <Icon size={14} style={{ color: 'var(--text-muted)', flexShrink: 0 }} />
                  <span style={{ fontSize: 13, color: 'var(--text-primary)' }}>{item.label}</span>
                </div>
              )
            })}

            {!results.length && !isOdysseus && query && (
              <div style={{ padding: '10px 14px', fontSize: 12, color: 'var(--text-muted)' }}>
                No results — prefix with <code>/</code> to ask AI
              </div>
            )}
          </div>
        </>
      )}
    </div>
  )
}

// Phone bottom sheet variant
function CommandSheet({ query, setQuery, results, commit, onClose, inputRef, dockH }: {
  query: string; setQuery: (q: string) => void
  results: typeof DOCK_ITEMS
  commit: (key?: string) => void; onClose: () => void
  inputRef: React.RefObject<HTMLInputElement>
  dockH: number
}) {
  return (
    <>
      <div style={{ position: 'fixed', inset: 0, background: 'rgba(0,0,0,0.5)', zIndex: 10003 }} onClick={onClose} />
      <div style={{
        position: 'fixed', left: 0, right: 0, bottom: dockH,
        background: 'var(--bg-panel)', borderRadius: '12px 12px 0 0',
        border: '1px solid var(--border-subtle)', zIndex: 10004,
        maxHeight: '70vh', display: 'flex', flexDirection: 'column',
      }}>
        <div style={{ padding: '12px 16px', borderBottom: '1px solid var(--border-subtle)' }}>
          <input
            ref={inputRef}
            value={query}
            onChange={(e) => { setQuery(e.target.value) }}
            onKeyDown={(e) => { if (e.key === 'Enter') commit() }}
            placeholder="Open app or /ask AI…"
            style={{ width: '100%', background: 'none', border: 'none', outline: 'none', fontSize: 16, color: 'var(--text-primary)' }}
            autoCapitalize="none"
          />
        </div>
        <div style={{ overflowY: 'auto', flex: 1 }}>
          {results.map((item) => {
            const Icon = item.icon
            return (
              <div key={item.key} onClick={() => commit(item.key)}
                style={{ display: 'flex', alignItems: 'center', gap: 12, padding: '14px 16px', cursor: 'pointer', borderBottom: '1px solid var(--border-subtle)' }}>
                <Icon size={18} style={{ color: 'var(--text-muted)' }} />
                <span style={{ fontSize: 15, color: 'var(--text-primary)' }}>{item.label}</span>
              </div>
            )
          })}
        </div>
      </div>
    </>
  )
}
