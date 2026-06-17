import { useState, useRef } from 'react'
import { useThemeStore } from '@/store/theme'
import { exportTheme, importTheme, type Theme, GLASS_LEVELS, HOVER_FX_LEVELS } from '@/theme/themes'
import ThemeEditor from '@/components/ui/ThemeEditor'
import Button from '@/components/ui/Button'
import { notify } from '@/store/notifications'
import { Check, Shuffle, Trash2, Upload, Download, Clipboard, RefreshCw } from 'lucide-react'

// Small swatch strip showing key colors for a theme
function ThemeSwatch({ theme }: { theme: Theme }) {
  // For builtins derive swatch colors from known accent+bg combos; for custom use tokens
  const swatches: string[] = theme.tokens
    ? [
        theme.tokens['--accent-primary'] ?? '#8b5cf6',
        theme.tokens['--bg-card'] ?? '#1a1a2e',
        theme.tokens['--text-primary'] ?? '#e2e8f0',
        theme.tokens['--accent-success'] ?? '#39ff88',
      ]
    : BUILTIN_SWATCH_COLORS[theme.id] ?? ['#8b5cf6', '#0f111a', '#e2e8f0', '#39ff88']

  return (
    <div className="flex gap-1 mt-2">
      {swatches.map((c, i) => (
        <span
          key={i}
          className="rounded-sm flex-1"
          style={{ height: 6, background: c, border: '1px solid rgba(255,255,255,0.08)' }}
        />
      ))}
    </div>
  )
}

const BUILTIN_SWATCH_COLORS: Record<string, string[]> = {
  'voidtower':      ['#8b5cf6', '#0f111a', '#e2e8f0', '#39ff88'],
  'blacksite':      ['#06b6d4', '#070b0f', '#c9d1d9', '#00ff41'],
  'ghost-terminal': ['#39ff88', '#0a0a0a', '#d0d0d0', '#00bcd4'],
  'deep-grid':      ['#3b82f6', '#050d18', '#dbe6f8', '#06b6d4'],
  'solar-breach':   ['#f59e0b', '#1a0a00', '#fef3c7', '#ef4444'],
  'light-ops':      ['#7c3aed', '#f8f9fa', '#1a1a2e', '#059669'],
  'high-contrast':  ['#ffff00', '#000000', '#ffffff', '#ff0000'],
  // Odysseus themes
  'od-dark':        ['#e06c75', '#282c34', '#9cdef2', '#50fa7b'],
  'od-light':       ['#c47d5a', '#f0ebe3', '#5a5248', '#50fa7b'],
  'od-midnight':    ['#f85149', '#0d1117', '#c9d1d9', '#50fa7b'],
  'od-paper':       ['#c5ac4a', '#faf8f5', '#3b3836', '#50fa7b'],
  'od-cyberpunk':   ['#e040fb', '#0a0a0f', '#0ff0fc', '#9b30ff'],
  'od-retrowave':   ['#e94560', '#1a1a2e', '#e94560', '#533483'],
  'od-forest':      ['#7cb871', '#1b2a1b', '#a8d5a2', '#50fa7b'],
  'od-ocean':       ['#4facfe', '#0b1a2c', '#64d2ff', '#50fa7b'],
  'od-ume':         ['#f5a0c0', '#2b1b2e', '#f5c2e7', '#50fa7b'],
  'od-copper':      ['#d4764e', '#1c1410', '#e8c39e', '#50fa7b'],
  'od-terminal':    ['#00ff41', '#000000', '#00ff41', '#50fa7b'],
  'od-organs':      ['#c83240', '#0a0406', '#efe1c8', '#50fa7b'],
  'od-lavender':    ['#9b6dcc', '#f3eef8', '#3d3551', '#50fa7b'],
  'od-gpt':         ['#949494', '#212121', '#ececec', '#50fa7b'],
  'od-claude':      ['#c6613f', '#262624', '#f5f4f0', '#50fa7b'],
  'od-cute':        ['#ff6b9d', '#fff0f5', '#d4608a', '#50fa7b'],
}

function InterfaceCard() {
  const { glassLevel, setGlass, panelOpacity, setPanelOpacity, panelRadius, setPanelRadius, hoverFx, setHoverFx, a11y, setA11y } = useThemeStore()

  const slider = (label: string, value: number, min: number, max: number, onChange: (n: number) => void, fmt: (n: number) => string) => (
    <div>
      <div className="flex items-center justify-between mb-1">
        <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>{label}</span>
        <span className="text-xs font-mono" style={{ color: 'var(--text-muted)' }}>{fmt(value)}</span>
      </div>
      <input type="range" min={min} max={max} value={value} onChange={e => onChange(Number(e.target.value))}
        className="w-full" style={{ accentColor: 'var(--accent-primary)', height: 4 }} />
    </div>
  )

  return (
    <div className="card space-y-4">
      <p className="text-xs font-medium uppercase tracking-wider" style={{ color: 'var(--text-muted)' }}>Interface</p>

      {slider('Transparency', panelOpacity, 0, 100, setPanelOpacity, v => v === 0 ? 'Off' : `${v}%`)}
      {slider('Panel rounding', panelRadius, 0, 20, setPanelRadius, v => `${v}px`)}

      <div>
        <span className="text-xs mb-1.5 block" style={{ color: 'var(--text-secondary)' }}>Glass effect</span>
        <div className="flex flex-wrap gap-1.5">
          {GLASS_LEVELS.map(g => (
            <button key={g.id} onClick={() => setGlass(g.id)}
              className="px-3 py-1 rounded text-xs transition-colors"
              style={{
                background: glassLevel === g.id ? 'var(--accent-primary)' : 'var(--bg-elevated)',
                color: glassLevel === g.id ? '#fff' : 'var(--text-secondary)',
                border: `1px solid ${glassLevel === g.id ? 'var(--accent-primary)' : 'var(--border-subtle)'}`,
              }}>
              {g.label}
            </button>
          ))}
        </div>
        <p className="mt-1 text-xs" style={{ color: 'var(--text-muted)' }}>
          {GLASS_LEVELS.find(g => g.id === glassLevel)?.description ?? ''}
        </p>
      </div>

      <div>
        <span className="text-xs mb-1.5 block" style={{ color: 'var(--text-secondary)' }}>Hover animations</span>
        <div className="flex flex-wrap gap-1.5">
          {HOVER_FX_LEVELS.map(h => (
            <button key={h.id} onClick={() => setHoverFx(h.id)}
              className="px-3 py-1 rounded text-xs transition-colors"
              style={{
                background: hoverFx === h.id ? 'var(--accent-primary)' : 'var(--bg-elevated)',
                color: hoverFx === h.id ? '#fff' : 'var(--text-secondary)',
                border: `1px solid ${hoverFx === h.id ? 'var(--accent-primary)' : 'var(--border-subtle)'}`,
              }}>
              {h.label}
            </button>
          ))}
        </div>
        <p className="mt-1 text-xs" style={{ color: 'var(--text-muted)' }}>
          {HOVER_FX_LEVELS.find(h => h.id === hoverFx)?.description ?? ''}
        </p>
      </div>

      <div className="flex items-center justify-between pt-1 border-t" style={{ borderColor: 'var(--border-subtle)' }}>
        <div>
          <div className="text-xs font-medium" style={{ color: 'var(--text-primary)' }}>Reduce transparency</div>
          <div className="text-xs mt-0.5" style={{ color: 'var(--text-muted)' }}>Override — forces fully opaque backgrounds.</div>
        </div>
        <button role="switch" aria-checked={a11y.reduceTransparency}
          onClick={() => setA11y({ reduceTransparency: !a11y.reduceTransparency })}
          style={{ flexShrink: 0, width: 36, height: 20, borderRadius: 10, border: 'none', cursor: 'pointer', position: 'relative',
            background: a11y.reduceTransparency ? 'var(--accent-primary)' : 'var(--bg-elevated)' }}>
          <span style={{ position: 'absolute', top: 2, left: a11y.reduceTransparency ? 18 : 2, width: 16, height: 16, borderRadius: '50%', background: '#fff', transition: 'left 0.2s' }} />
        </button>
      </div>
    </div>
  )
}

export default function ThemesPage() {
  const { activeTheme, setTheme, allThemes, addCustomTheme, removeCustomTheme, randomize } = useThemeStore()
  const [importText, setImportText] = useState('')
  const [syncing, setSyncing] = useState(false)
  const fileInputRef = useRef<HTMLInputElement>(null)

  const themes = allThemes()
  const vtThemes = themes.filter(t => t.isBuiltin && !t.id.startsWith('od-'))
  const odThemes = themes.filter(t => t.isBuiltin && t.id.startsWith('od-'))
  const customs = themes.filter(t => !t.isBuiltin)

  const syncFromOdysseus = async () => {
    setSyncing(true)
    try {
      const res = await fetch('/api/integrations/odysseus/theme', { credentials: 'include' })
      if (!res.ok) throw new Error(`HTTP ${res.status}`)
      const data = await res.json() as { name: string }
      const targetId = `od-${data.name}`
      const match = themes.find(t => t.id === targetId)
      if (match) {
        setTheme(match.id)
        notify.success(`Synced to "${match.name}"`)
      } else {
        notify.warning(`Odysseus theme "${data.name}" has no VoidTower equivalent`)
      }
    } catch {
      notify.error('Could not reach Odysseus — check integration settings')
    } finally {
      setSyncing(false)
    }
  }

  const exportToClipboard = () => {
    const json = exportTheme(activeTheme)
    navigator.clipboard.writeText(json).then(
      () => notify.success('Theme JSON copied to clipboard'),
      () => notify.error('Failed to copy'),
    )
  }

  const exportToFile = () => {
    const json = exportTheme(activeTheme)
    const blob = new Blob([json], { type: 'application/json' })
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    a.download = `${activeTheme.name.toLowerCase().replace(/\s+/g, '-')}.theme.json`
    a.click()
    URL.revokeObjectURL(url)
  }

  const doImport = (json: string) => {
    try {
      const theme = importTheme(json.trim())
      addCustomTheme(theme)
      setTheme(theme.id)
      setImportText('')
      notify.success(`Theme "${theme.name}" imported`)
    } catch {
      notify.error('Invalid theme JSON — check format and try again')
    }
  }

  const handleImportText = () => doImport(importText)

  const handleImportFile = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0]
    if (!file) return
    file.text().then(doImport).catch(() => notify.error('Failed to read file'))
    e.target.value = ''
  }

  return (
    <div className="space-y-6 max-w-5xl py-6">
      <div className="flex justify-end gap-2">
        <Button size="sm" variant="ghost" onClick={syncFromOdysseus} disabled={syncing}>
          <RefreshCw size={13} className={`mr-1.5${syncing ? ' animate-spin' : ''}`} />
          {syncing ? 'Syncing…' : 'Sync from Odysseus'}
        </Button>
        <Button size="sm" variant="ghost" onClick={randomize}>
          <Shuffle size={13} className="mr-1.5" /> Randomize
        </Button>
      </div>

      <div className="grid gap-6 lg:grid-cols-[280px_1fr]">

        {/* ── Left panel: theme list ─────────────────────────────────── */}
        <div className="space-y-4">

          {/* VoidTower built-in themes */}
          <div className="card space-y-2">
            <p className="text-xs font-medium uppercase tracking-wider" style={{ color: 'var(--text-muted)' }}>VoidTower</p>
            {vtThemes.map(t => (
              <ThemeCard
                key={t.id}
                theme={t}
                active={activeTheme.id === t.id}
                onSelect={() => setTheme(t.id)}
              />
            ))}
          </div>

          {/* Odysseus themes */}
          <div className="card space-y-2">
            <p className="text-xs font-medium uppercase tracking-wider" style={{ color: 'var(--text-muted)' }}>Odysseus</p>
            {odThemes.map(t => (
              <ThemeCard
                key={t.id}
                theme={t}
                active={activeTheme.id === t.id}
                onSelect={() => setTheme(t.id)}
              />
            ))}
          </div>

          {/* Custom themes — always visible */}
          <div className="card space-y-2">
            <p className="text-xs font-medium uppercase tracking-wider" style={{ color: 'var(--text-muted)' }}>Custom themes</p>
            {customs.length === 0 ? (
              <p className="text-xs py-2" style={{ color: 'var(--text-disabled)' }}>
                No custom themes yet. Edit tokens on the right and click "Save as theme", or import a JSON file below.
              </p>
            ) : customs.map(t => (
              <ThemeCard
                key={t.id}
                theme={t}
                active={activeTheme.id === t.id}
                onSelect={() => setTheme(t.id)}
                onDelete={() => removeCustomTheme(t.id)}
              />
            ))}
          </div>

          {/* Interface sliders */}
          <InterfaceCard />

          {/* Import / Export */}
          <div className="card space-y-3">
            <p className="text-xs font-medium uppercase tracking-wider" style={{ color: 'var(--text-muted)' }}>Import / Export</p>

            <div>
              <p className="text-xs mb-2" style={{ color: 'var(--text-secondary)' }}>Export active theme</p>
              <div className="flex gap-2">
                <Button size="sm" variant="ghost" onClick={exportToClipboard}>
                  <Clipboard size={12} className="mr-1.5" /> Copy JSON
                </Button>
                <Button size="sm" variant="ghost" onClick={exportToFile}>
                  <Download size={12} className="mr-1.5" /> Download file
                </Button>
              </div>
            </div>

            <div>
              <p className="text-xs mb-2" style={{ color: 'var(--text-secondary)' }}>Import theme</p>
              <div className="flex gap-2 mb-2">
                <Button size="sm" variant="ghost" onClick={() => fileInputRef.current?.click()}>
                  <Upload size={12} className="mr-1.5" /> Choose JSON file
                </Button>
                <input ref={fileInputRef} type="file" accept=".json,application/json" className="hidden" onChange={handleImportFile} />
              </div>
              <textarea
                value={importText}
                onChange={e => setImportText(e.target.value)}
                placeholder="…or paste theme JSON here"
                rows={4}
                className="w-full px-3 py-2 rounded text-xs font-mono resize-none outline-none"
                style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-default)', color: 'var(--text-primary)' }}
              />
              <Button size="sm" variant="primary" disabled={!importText.trim()} onClick={handleImportText} className="mt-2 w-full justify-center">
                Import from JSON
              </Button>
            </div>
          </div>
        </div>

        {/* ── Right panel: live editor ───────────────────────────────── */}
        <div className="card space-y-4">
          <div>
            <p className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>Token Editor</p>
            <p className="text-xs mt-0.5" style={{ color: 'var(--text-muted)' }}>
              Edit colors live, then save as a new theme.
            </p>
          </div>
          <ThemeEditor />
        </div>
      </div>
    </div>
  )
}

function ThemeCard({ theme, active, onSelect, onDelete }: {
  theme: Theme
  active: boolean
  onSelect: () => void
  onDelete?: () => void
}) {
  return (
    <button
      onClick={onSelect}
      className="w-full text-left px-3 py-2.5 rounded transition-colors hover:opacity-90"
      style={{
        background: active ? 'var(--accent-primary-subtle)' : 'var(--bg-elevated)',
        border: `1px solid ${active ? 'var(--accent-primary)' : 'var(--border-subtle)'}`,
      }}
    >
      <div className="flex items-center justify-between gap-2">
        <span className="text-sm truncate" style={{ color: active ? 'var(--accent-primary)' : 'var(--text-secondary)' }}>
          {theme.name}
        </span>
        <div className="flex items-center gap-1 flex-shrink-0">
          {active && <Check size={12} style={{ color: 'var(--accent-primary)' }} />}
          {onDelete && (
            <span
              role="button"
              tabIndex={0}
              onClick={(e) => { e.stopPropagation(); onDelete() }}
              onKeyDown={(e) => { if (e.key === 'Enter') { e.stopPropagation(); onDelete() } }}
              className="p-0.5 rounded hover:opacity-80"
              style={{ color: 'var(--accent-danger)' }}
              title="Delete theme"
            >
              <Trash2 size={12} />
            </span>
          )}
        </div>
      </div>
      <ThemeSwatch theme={theme} />
    </button>
  )
}
