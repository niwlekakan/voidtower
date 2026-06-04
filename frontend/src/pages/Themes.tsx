import { useState } from 'react'
import { useThemeStore } from '@/store/theme'
import { exportTheme, type Theme } from '@/theme/themes'
import ThemeEditor from '@/components/ui/ThemeEditor'
import Button from '@/components/ui/Button'
import { notify } from '@/store/notifications'
import { Check, Shuffle, Trash2, ChevronDown, ChevronUp } from 'lucide-react'

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
}

export default function ThemesPage() {
  const { activeTheme, setTheme, allThemes, removeCustomTheme, randomize, importFromJson } = useThemeStore()
  const [importText, setImportText] = useState('')
  const [importOpen, setImportOpen] = useState(false)

  const themes = allThemes()
  const builtins = themes.filter(t => t.isBuiltin)
  const customs = themes.filter(t => !t.isBuiltin)

  const handleExport = () => {
    const json = exportTheme(activeTheme)
    navigator.clipboard.writeText(json).then(
      () => notify.success('Theme JSON copied to clipboard'),
      () => notify.error('Failed to copy'),
    )
  }

  const handleImport = () => {
    try {
      importFromJson(importText)
      setImportText('')
      setImportOpen(false)
      notify.success('Theme imported')
    } catch {
      notify.error('Invalid theme JSON')
    }
  }

  return (
    <div className="space-y-6 max-w-5xl">
      <div className="flex items-center justify-between">
        <h1 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>Themes</h1>
        <Button size="sm" variant="ghost" onClick={randomize}>
          <Shuffle size={13} className="mr-1.5" /> Randomize
        </Button>
      </div>

      <div className="grid gap-6 lg:grid-cols-[280px_1fr]">

        {/* ── Left panel: theme list ─────────────────────────────────── */}
        <div className="space-y-4">

          {/* Builtin themes */}
          <div className="card space-y-2">
            <p className="text-xs font-medium uppercase tracking-wider" style={{ color: 'var(--text-muted)' }}>Presets</p>
            {builtins.map(t => (
              <ThemeCard
                key={t.id}
                theme={t}
                active={activeTheme.id === t.id}
                onSelect={() => setTheme(t.id)}
              />
            ))}
          </div>

          {/* Custom / saved themes */}
          {customs.length > 0 && (
            <div className="card space-y-2">
              <p className="text-xs font-medium uppercase tracking-wider" style={{ color: 'var(--text-muted)' }}>Saved</p>
              {customs.map(t => (
                <ThemeCard
                  key={t.id}
                  theme={t}
                  active={activeTheme.id === t.id}
                  onSelect={() => setTheme(t.id)}
                  onDelete={() => removeCustomTheme(t.id)}
                />
              ))}
            </div>
          )}

          {/* Import / export (collapsed by default) */}
          <div className="card space-y-3">
            <button
              className="flex items-center justify-between w-full text-xs font-medium"
              style={{ color: 'var(--text-secondary)' }}
              onClick={() => setImportOpen(v => !v)}
            >
              <span>Import / Export JSON</span>
              {importOpen ? <ChevronUp size={13} /> : <ChevronDown size={13} />}
            </button>
            {importOpen && (
              <div className="space-y-2 pt-1">
                <Button size="sm" variant="ghost" onClick={handleExport} className="w-full justify-center">
                  Export active theme
                </Button>
                <textarea
                  value={importText}
                  onChange={(e) => setImportText(e.target.value)}
                  placeholder="Paste theme JSON…"
                  rows={5}
                  className="w-full px-3 py-2 rounded text-xs font-mono resize-none outline-none"
                  style={{
                    background: 'var(--bg-elevated)',
                    border: '1px solid var(--border-default)',
                    color: 'var(--text-primary)',
                  }}
                />
                <Button size="sm" variant="primary" disabled={!importText.trim()} onClick={handleImport} className="w-full justify-center">
                  Import
                </Button>
              </div>
            )}
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
