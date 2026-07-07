import { useState, useRef, useEffect, useCallback, useMemo } from 'react'
import { Search, Command } from 'lucide-react'
import { DOCK_ITEMS } from '@/aios/AiosDock'
import type { DockItem } from '@/aios/AiosDock'
import { useAiosStore } from '@/aios/store/aios'
import type { PresetName } from '@/aios/store/aios'
import { PRESET_LIST } from '@/aios/AiosPresets'
import type { DeviceTier } from '@/aios/hooks/useDeviceTier'

// ── Props ─────────────────────────────────────────────────────────────────────

export interface AiosCommandBarProps {
  tier: DeviceTier
  /** Dock height in px — for positioning above dock (default 56) */
  dockH?: number
  /** Status bar height in px (default 28) */
  statusBarH?: number
  /** Called when user selects an app to open */
  onOpen?: (key: string) => void
  /** Called when user sends an Odysseus query */
  onOdysseus?: (query: string) => void
}

// ── Helpers ───────────────────────────────────────────────────────────────────

function isUrl(s: string): boolean {
  return s.startsWith('http://') || s.startsWith('https://')
}

function fuzzyMatch(item: DockItem, query: string): boolean {
  const q = query.toLowerCase()
  return item.label.toLowerCase().includes(q) || item.key.toLowerCase().includes(q)
}

// ── AiosCommandBar ────────────────────────────────────────────────────────────

export default function AiosCommandBar({ tier, dockH = 56, statusBarH = 28, onOpen: _onOpen, onOdysseus: _onOdysseus }: AiosCommandBarProps) {
  const [open, setOpen] = useState(false)
  const [query, setQuery] = useState('')
  const [selected, setSelected] = useState(0)
  const inputRef = useRef<HTMLInputElement>(null)

  const { openPanel, activeWorkspace, panels, focusPanel, applyPreset } = useAiosStore()

  const isPhone  = tier === 'phone'
  const isTv     = tier === 'tv' || tier === 'kiosk'
  const isVerticalDock = typeof window !== 'undefined' && window.innerWidth >= 1400
  const dockOffset = isVerticalDock && !isPhone ? dockH / 2 : 0

  // Route prefix: slash = Odysseus AI, http(s) = embed URL, > = preset
  const isOdysseus = query.startsWith('/') && !isUrl(query)
  const isEmbed    = isUrl(query)
  const isPreset   = query.startsWith('>')

  const presetResults = useMemo(() => (
    isPreset
      ? PRESET_LIST.filter((p) => {
          const q = query.slice(1).toLowerCase().trim()
          return !q || p.name.includes(q) || p.label.toLowerCase().includes(q)
        })
      : []
  ), [isPreset, query])

  const results: DockItem[] = useMemo(() => (
    (isOdysseus || isEmbed || isPreset)
      ? []
      : DOCK_ITEMS.filter((item) => fuzzyMatch(item, query)).slice(0, 8)
  ), [isOdysseus, isEmbed, isPreset, query])

  // ── Open panel helper ──────────────────────────────────────────────────────

  const doOpenPanel = useCallback((key: string, title?: string, panelType: 'app' | 'embed' = 'app') => {
    const existing = panels.find(
      (p) => p.component === key && p.workspaceIndex === activeWorkspace,
    )
    if (existing) {
      focusPanel(existing.id)
      return
    }
    const vw = typeof window !== 'undefined' ? window.innerWidth : 1280
    const vh = typeof window !== 'undefined' ? window.innerHeight : 800
    const w = Math.min(900, vw - 40)
    const h = Math.min(580, vh - 80)
    openPanel({
      type: panelType,
      component: key,
      title: title ?? key,
      icon: '⬡',
      layoutMode: 'floating',
      x: Math.max(20, (vw - w) / 2),
      y: Math.max(statusBarH + 8, (vh - h) / 2),
      w, h,
      savedX: Math.max(20, (vw - w) / 2),
      savedY: Math.max(statusBarH + 8, (vh - h) / 2),
      savedW: w, savedH: h,
      pinned: false,
      workspaceIndex: activeWorkspace,
    })
  }, [openPanel, panels, activeWorkspace, focusPanel, statusBarH])

  // ── Commit (Enter / click suggestion) ─────────────────────────────────────

  const commit = useCallback((key?: string, label?: string) => {
    const q = query.trim()

    if (isPreset) {
      const presetKey = (key as PresetName | undefined) ?? (presetResults[selected]?.name as PresetName | undefined)
      if (presetKey) {
        applyPreset(presetKey)
        setOpen(false); setQuery(''); setSelected(0)
      }
      return
    }

    if (isEmbed) {
      // Open as embed panel
      doOpenPanel(q, q, 'embed')
      setOpen(false); setQuery(''); setSelected(0)
      return
    }

    if (isOdysseus) {
      const text = q.slice(1).trim()
      if (_onOdysseus) {
        // Open (or focus) the Odysseus panel via the layout callback, passing the query
        _onOdysseus(text)
      } else {
        // Fallback: try to focus existing Odysseus panel and postMessage
        const odysseusPanel = panels.find((p) => p.component === 'odysseus')
        if (odysseusPanel) {
          focusPanel(odysseusPanel.id)
          window.postMessage({ type: 'vt-command', text }, '*')
        }
      }
      setOpen(false); setQuery(''); setSelected(0)
      return
    }

    const target = key ?? results[selected]?.key
    const targetLabel = label ?? results[selected]?.label ?? target
    if (!target) return

    doOpenPanel(target, targetLabel)
    setOpen(false); setQuery(''); setSelected(0)
  }, [query, isEmbed, isOdysseus, isPreset, presetResults, results, selected, doOpenPanel, panels, focusPanel, applyPreset, _onOdysseus])

  // ── Keyboard shortcut ─────────────────────────────────────────────────────

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
        e.preventDefault()
        setOpen(true)
      }
      if (e.key === 'Escape' && open) {
        setOpen(false)
        setQuery('')
      }
    }
    window.addEventListener('keydown', handler)
    return () => window.removeEventListener('keydown', handler)
  }, [open])

  // External trigger — dispatched by the phone dock "+more" button
  useEffect(() => {
    const handler = () => setOpen(true)
    window.addEventListener('vt-open-command-bar', handler)
    return () => window.removeEventListener('vt-open-command-bar', handler)
  }, [])

  useEffect(() => {
    if (open) setTimeout(() => inputRef.current?.focus(), 50)
  }, [open])

  // TV / kiosk: no command bar
  if (isTv) return null

  // ── Phone: FAB + bottom sheet ──────────────────────────────────────────────

  if (isPhone) {
    return (
      <>
        <button
          onClick={() => setOpen(true)}
          aria-label="Open command bar"
          style={{
            position: 'fixed', bottom: dockH + 12, right: 16, zIndex: 9997,
            width: 52, height: 52, borderRadius: '50%',
            background: 'var(--accent-primary)', border: 'none', cursor: 'pointer',
            display: 'flex', alignItems: 'center', justifyContent: 'center',
            boxShadow: '0 4px 20px rgba(0,0,0,0.5)',
          }}
        >
          <Search size={22} style={{ color: '#fff' }} />
        </button>

        {open && (
          <PhoneSheet
            query={query}
            setQuery={setQuery}
            results={results}
            isOdysseus={isOdysseus}
            commit={commit}
            onClose={() => { setOpen(false); setQuery('') }}
            inputRef={inputRef}
            dockH={dockH}
          />
        )}
      </>
    )
  }

  // ── Desktop / tablet: centered floating pill ───────────────────────────────

  return (
    <div style={{
      position: 'fixed',
      bottom: dockH + 10,
      left: `calc(50% + ${dockOffset / 2}px)`,
      transform: 'translateX(-50%)',
      zIndex: 9997,
      width: Math.min(480, (typeof window !== 'undefined' ? window.innerWidth : 1280) - dockOffset - 32),
    }}>
      {/* Pill trigger */}
      <div
        role="button"
        tabIndex={0}
        onClick={() => setOpen(true)}
        onKeyDown={(e) => { if (e.key === 'Enter' || e.key === ' ') setOpen(true) }}
        style={{
          display: 'flex', alignItems: 'center', gap: 8,
          background: 'rgba(0,0,0,0.55)',
          backdropFilter: 'blur(16px)',
          WebkitBackdropFilter: 'blur(16px)',
          border: `1px solid ${open ? 'var(--accent-primary)' : 'rgba(255,255,255,0.12)'}`,
          borderRadius: 24, padding: '6px 14px', cursor: 'text',
          boxShadow: open
            ? '0 0 0 3px var(--accent-primary-subtle), 0 4px 20px rgba(0,0,0,0.4)'
            : '0 2px 12px rgba(0,0,0,0.4)',
          transition: 'border-color 0.15s, box-shadow 0.15s',
        }}
      >
        <Command size={13} style={{ color: 'rgba(255,255,255,0.4)', flexShrink: 0 }} />
        <span style={{ flex: 1, fontSize: 12, color: 'rgba(255,255,255,0.35)' }}>
          Open app or /ask Odysseus…
        </span>
        <kbd style={{
          fontSize: 10, color: 'rgba(255,255,255,0.3)',
          background: 'rgba(255,255,255,0.06)',
          padding: '1px 5px', borderRadius: 4,
          border: '1px solid rgba(255,255,255,0.12)',
        }}>
          ⌘K
        </kbd>
      </div>

      {/* Expanded dropdown */}
      {open && (
        <>
          {/* Backdrop */}
          <div
            style={{ position: 'fixed', inset: 0, zIndex: -1 }}
            onClick={() => { setOpen(false); setQuery('') }}
          />

          <div style={{
            position: 'absolute', bottom: '100%', left: 0, right: 0, marginBottom: 6,
            background: 'rgba(0,0,0,0.75)',
            backdropFilter: 'blur(24px)',
            WebkitBackdropFilter: 'blur(24px)',
            border: '1px solid rgba(255,255,255,0.12)',
            borderRadius: 14, boxShadow: '0 8px 40px rgba(0,0,0,0.6)',
            overflow: 'hidden',
          }}>
            {/* Input */}
            <div style={{ padding: '10px 12px', borderBottom: '1px solid rgba(255,255,255,0.08)' }}>
              <input
                ref={inputRef}
                value={query}
                onChange={(e) => { setQuery(e.target.value); setSelected(0) }}
                onKeyDown={(e) => {
                  if (e.key === 'Enter') commit()
                  if (e.key === 'ArrowDown') { e.preventDefault(); setSelected((s) => Math.min(s + 1, results.length - 1)) }
                  if (e.key === 'ArrowUp')   { e.preventDefault(); setSelected((s) => Math.max(s - 1, 0)) }
                  if (e.key === 'Escape')    { setOpen(false); setQuery('') }
                }}
                placeholder="Dashboard, Containers… or /ask Odysseus…"
                style={{
                  width: '100%', background: 'none', border: 'none', outline: 'none',
                  fontSize: 13, color: 'var(--text-primary)',
                }}
                autoComplete="off"
                spellCheck={false}
              />
            </div>

            {/* Odysseus row */}
            {isOdysseus && (
              <div
                role="button"
                tabIndex={0}
                onClick={() => commit()}
                onKeyDown={(e) => { if (e.key === 'Enter') commit() }}
                style={{
                  padding: '10px 14px', cursor: 'pointer',
                  background: 'rgba(139,92,246,0.12)',
                  display: 'flex', gap: 10, alignItems: 'center',
                  borderBottom: '1px solid rgba(255,255,255,0.06)',
                }}
              >
                <span style={{ fontSize: 18 }}>🧠</span>
                <div>
                  <div style={{ fontSize: 12, fontWeight: 600, color: 'var(--accent-primary)' }}>
                    Ask Odysseus
                  </div>
                  <div style={{ fontSize: 11, color: 'rgba(255,255,255,0.45)' }}>
                    {query.slice(1).trim() || 'Type your question…'}
                  </div>
                </div>
              </div>
            )}

            {/* Preset suggestions */}
            {isPreset && presetResults.map((preset, i) => {
              const active = i === selected
              return (
                <div
                  key={preset.name}
                  role="option"
                  aria-selected={active}
                  onClick={() => commit(preset.name)}
                  onMouseEnter={() => setSelected(i)}
                  style={{
                    padding: '10px 14px', cursor: 'pointer',
                    background: active ? 'rgba(255,255,255,0.08)' : 'rgba(99,102,241,0.06)',
                    display: 'flex', gap: 10, alignItems: 'center',
                    borderBottom: '1px solid rgba(255,255,255,0.06)',
                  }}
                >
                  <span style={{ fontSize: 16 }}>⊞</span>
                  <div style={{ flex: 1 }}>
                    <div style={{ fontSize: 12, fontWeight: 600, color: active ? 'var(--text-primary)' : 'rgba(255,255,255,0.8)' }}>
                      {preset.label}
                    </div>
                    <div style={{ fontSize: 11, color: 'rgba(255,255,255,0.4)' }}>
                      {preset.description}
                    </div>
                  </div>
                </div>
              )
            })}
            {isPreset && presetResults.length === 0 && (
              <div style={{ padding: '10px 14px', fontSize: 12, color: 'rgba(255,255,255,0.35)' }}>
                No matching presets — try <code style={{ color: 'var(--accent-primary)' }}>&gt;ai-assist</code>, <code style={{ color: 'var(--accent-primary)' }}>&gt;debug</code>, <code style={{ color: 'var(--accent-primary)' }}>&gt;vm</code>, <code style={{ color: 'var(--accent-primary)' }}>&gt;android</code>
              </div>
            )}

            {/* URL embed suggestion */}
            {isEmbed && (
              <div
                role="button"
                tabIndex={0}
                onClick={() => commit()}
                onKeyDown={(e) => { if (e.key === 'Enter') commit() }}
                style={{
                  padding: '10px 14px', cursor: 'pointer',
                  display: 'flex', gap: 10, alignItems: 'center',
                  borderBottom: '1px solid rgba(255,255,255,0.06)',
                }}
              >
                <span style={{ fontSize: 18 }}>🌐</span>
                <div>
                  <div style={{ fontSize: 12, fontWeight: 600, color: 'var(--text-primary)' }}>
                    Open as embed
                  </div>
                  <div style={{ fontSize: 11, color: 'rgba(255,255,255,0.45)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                    {query}
                  </div>
                </div>
              </div>
            )}

            {/* Nav results */}
            {results.map((item, i) => {
              const Icon = item.icon
              const active = i === selected
              return (
                <div
                  key={item.key}
                  role="option"
                  aria-selected={active}
                  onClick={() => commit(item.key, item.label)}
                  onMouseEnter={() => setSelected(i)}
                  style={{
                    display: 'flex', alignItems: 'center', gap: 10,
                    padding: '9px 14px', cursor: 'pointer',
                    background: active ? 'rgba(255,255,255,0.08)' : 'transparent',
                    transition: 'background 0.1s',
                  }}
                >
                  <Icon size={14} style={{ color: active ? 'var(--accent-primary)' : 'rgba(255,255,255,0.45)', flexShrink: 0 }} />
                  <span style={{ fontSize: 13, color: active ? 'var(--text-primary)' : 'rgba(255,255,255,0.7)' }}>
                    {item.label}
                  </span>
                </div>
              )
            })}

            {/* Empty state */}
            {!results.length && !isOdysseus && !isEmbed && !isPreset && query && (
              <div style={{ padding: '12px 14px', fontSize: 12, color: 'rgba(255,255,255,0.35)' }}>
                No results — prefix with <code style={{ color: 'var(--accent-primary)' }}>/</code> to ask Odysseus, or <code style={{ color: 'var(--accent-primary)' }}>&gt;</code> for presets
              </div>
            )}

            {/* Empty query hint */}
            {!query && !isOdysseus && !isEmbed && !isPreset && (
              <div style={{ padding: '10px 14px', fontSize: 11, color: 'rgba(255,255,255,0.25)' }}>
                Type an app name, <code style={{ color: 'var(--accent-primary)' }}>/</code> to ask Odysseus, <code style={{ color: 'var(--accent-primary)' }}>&gt;</code> for presets, or paste a URL to embed
              </div>
            )}
          </div>
        </>
      )}
    </div>
  )
}

// ── Phone bottom sheet ────────────────────────────────────────────────────────

function PhoneSheet({
  query, setQuery, results, isOdysseus, commit, onClose, inputRef, dockH,
}: {
  query: string
  setQuery: (q: string) => void
  results: DockItem[]
  isOdysseus: boolean
  commit: (key?: string, label?: string) => void
  onClose: () => void
  inputRef: React.RefObject<HTMLInputElement>
  dockH: number
}) {
  return (
    <>
      {/* Backdrop */}
      <div
        style={{ position: 'fixed', inset: 0, background: 'rgba(0,0,0,0.6)', zIndex: 10003 }}
        onClick={onClose}
      />

      {/* Sheet */}
      <div style={{
        position: 'fixed', left: 0, right: 0, bottom: dockH,
        background: 'rgba(0,0,0,0.85)',
        backdropFilter: 'blur(24px)',
        WebkitBackdropFilter: 'blur(24px)',
        borderRadius: '14px 14px 0 0',
        border: '1px solid rgba(255,255,255,0.10)',
        borderBottom: 'none',
        zIndex: 10004, maxHeight: '72vh',
        display: 'flex', flexDirection: 'column',
      }}>
        {/* Handle */}
        <div style={{ display: 'flex', justifyContent: 'center', padding: '10px 0 4px' }}>
          <div style={{ width: 36, height: 4, borderRadius: 2, background: 'rgba(255,255,255,0.2)' }} />
        </div>

        {/* Input */}
        <div style={{ padding: '4px 16px 12px', borderBottom: '1px solid rgba(255,255,255,0.08)' }}>
          <input
            ref={inputRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => { if (e.key === 'Enter') commit() }}
            placeholder="Open app or /ask Odysseus…"
            style={{
              width: '100%', background: 'none', border: 'none', outline: 'none',
              fontSize: 17, color: 'var(--text-primary)',
            }}
            autoCapitalize="none"
            autoComplete="off"
            spellCheck={false}
          />
        </div>

        {/* Odysseus row */}
        {isOdysseus && (
          <div
            role="button"
            tabIndex={0}
            onClick={() => commit()}
            onKeyDown={(e) => { if (e.key === 'Enter') commit() }}
            style={{
              display: 'flex', gap: 14, alignItems: 'center',
              padding: '14px 16px', cursor: 'pointer',
              background: 'rgba(139,92,246,0.12)',
              borderBottom: '1px solid rgba(255,255,255,0.06)',
            }}
          >
            <span style={{ fontSize: 22 }}>🧠</span>
            <div>
              <div style={{ fontSize: 13, fontWeight: 600, color: 'var(--accent-primary)' }}>Ask Odysseus</div>
              <div style={{ fontSize: 12, color: 'rgba(255,255,255,0.45)' }}>{query.slice(1).trim() || 'Type your question…'}</div>
            </div>
          </div>
        )}

        {/* Results */}
        <div style={{ overflowY: 'auto', flex: 1, WebkitOverflowScrolling: 'touch' } as React.CSSProperties}>
          {results.map((item) => {
            const Icon = item.icon
            return (
              <div
                key={item.key}
                onClick={() => commit(item.key, item.label)}
                style={{
                  display: 'flex', alignItems: 'center', gap: 14,
                  padding: '14px 16px', cursor: 'pointer',
                  borderBottom: '1px solid rgba(255,255,255,0.06)',
                }}
              >
                <Icon size={20} style={{ color: 'rgba(255,255,255,0.5)', flexShrink: 0 }} />
                <span style={{ fontSize: 15, color: 'var(--text-primary)' }}>{item.label}</span>
              </div>
            )
          })}
        </div>
      </div>
    </>
  )
}
