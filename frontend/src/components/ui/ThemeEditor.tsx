import React, { useState, useCallback, useEffect } from 'react'
import { useThemeStore } from '@/store/theme'
import { notify } from '@/store/notifications'
import { BG_PRESETS, GLASS_LEVELS, type GlassLevel, type BgPreset, type AnimConfig } from '@/theme/themes'
import Button from '@/components/ui/Button'
import { RotateCcw, Save, Shuffle, Sparkles } from 'lucide-react'

// Solid-color tokens only — rgba/shadow tokens are derived and excluded
const TOKEN_GROUPS: { label: string; tokens: { var: string; label: string }[] }[] = [
  {
    label: 'Backgrounds',
    tokens: [
      { var: '--bg-root', label: 'Root' },
      { var: '--bg-panel', label: 'Panel' },
      { var: '--bg-card', label: 'Card' },
      { var: '--bg-elevated', label: 'Elevated' },
    ],
  },
  {
    label: 'Borders',
    tokens: [
      { var: '--border-subtle', label: 'Subtle' },
      { var: '--border-default', label: 'Default' },
      { var: '--border-strong', label: 'Strong' },
    ],
  },
  {
    label: 'Text',
    tokens: [
      { var: '--text-primary', label: 'Primary' },
      { var: '--text-secondary', label: 'Secondary' },
      { var: '--text-muted', label: 'Muted' },
    ],
  },
  {
    label: 'Accent',
    tokens: [
      { var: '--accent-primary', label: 'Primary' },
      { var: '--accent-primary-hover', label: 'Primary hover' },
      { var: '--accent-secondary', label: 'Secondary' },
    ],
  },
  {
    label: 'Semantic',
    tokens: [
      { var: '--accent-success', label: 'Success' },
      { var: '--accent-warning', label: 'Warning' },
      { var: '--accent-danger', label: 'Danger' },
    ],
  },
  {
    label: 'Terminal',
    tokens: [
      { var: '--terminal-green', label: 'Green' },
      { var: '--terminal-bg', label: 'Background' },
      { var: '--terminal-cursor', label: 'Cursor' },
    ],
  },
]

// Read a CSS variable from the document root and return a 6-char hex string
function readToken(cssVar: string): string {
  const raw = getComputedStyle(document.documentElement)
    .getPropertyValue(cssVar)
    .trim()
  // Already 6-char hex
  if (/^#[0-9a-fA-F]{6}$/.test(raw)) return raw
  // 3-char hex
  if (/^#[0-9a-fA-F]{3}$/.test(raw)) {
    return '#' + raw[1] + raw[1] + raw[2] + raw[2] + raw[3] + raw[3]
  }
  // rgb/rgba
  const m = raw.match(/rgba?\(\s*(\d+)[, ]+(\d+)[, ]+(\d+)/)
  if (m) {
    return '#' + [m[1], m[2], m[3]]
      .map((n) => parseInt(n).toString(16).padStart(2, '0'))
      .join('')
  }
  return '#000000'
}

// Read all token groups from DOM into a flat map
function snapshotTokens(): Record<string, string> {
  const out: Record<string, string> = {}
  for (const g of TOKEN_GROUPS) {
    for (const t of g.tokens) {
      out[t.var] = readToken(t.var)
    }
  }
  return out
}

// ─── Slider + Color helpers ───────────────────────────────────────────────────

function Slider({ label, value, min, max, step = 0.01, fmt, onChange }: {
  label: string; value: number; min: number; max: number; step?: number
  fmt?: (v: number) => string; onChange: (v: number) => void
}) {
  const display = fmt ? fmt(value) : value.toFixed(step < 0.1 ? 2 : 0)
  return (
    <div>
      <div className="flex justify-between mb-1">
        <span className="text-xs" style={{ color: 'var(--text-muted)' }}>{label}</span>
        <span className="text-xs font-mono" style={{ color: 'var(--accent-primary)' }}>{display}</span>
      </div>
      <input
        type="range" min={min} max={max} step={step} value={value}
        onChange={(e) => onChange(parseFloat(e.target.value))}
        className="w-full h-1 rounded-full appearance-none cursor-pointer"
        style={{ accentColor: 'var(--accent-primary)' }}
      />
    </div>
  )
}

function ColorRow({ label, value, onChange }: { label: string; value: string; onChange: (v: string) => void }) {
  return (
    <div className="flex items-center gap-2.5">
      <label className="relative flex-shrink-0 cursor-pointer" style={{
        width: 24, height: 24, borderRadius: 5,
        background: value || 'var(--accent-primary)',
        border: '2px solid var(--border-default)', overflow: 'hidden',
      }}>
        <input type="color" value={value || '#8b5cf6'} onChange={(e) => onChange(e.target.value)}
          className="absolute inset-0 opacity-0 cursor-pointer" style={{ width: '100%', height: '100%' }} />
      </label>
      <span className="text-xs flex-1" style={{ color: 'var(--text-muted)' }}>{label}</span>
      {value && (
        <button onClick={() => onChange('')} className="text-xs px-1 rounded hover:opacity-80"
          style={{ color: 'var(--text-disabled)' }} title="Use theme color">×</button>
      )}
    </div>
  )
}

// ─── Per-preset param descriptions ────────────────────────────────────────────

const PRESET_PARAMS: Record<Exclude<BgPreset, 'none'>, (cfg: AnimConfig, set: (p: Partial<AnimConfig>) => void) => React.ReactNode> = {
  void: (cfg, set) => <>
    <Slider label="Particles" value={cfg.particleCount} min={20} max={400} step={1}
      fmt={v => Math.round(v).toString()} onChange={v => set({ particleCount: v })} />
    <Slider label="Size" value={cfg.particleSize} min={0.3} max={5} step={0.05}
      onChange={v => set({ particleSize: v })} />
    <Slider label="Direction" value={cfg.directionAngle} min={0} max={359} step={1}
      fmt={v => `${Math.round(v)}°`} onChange={v => set({ directionAngle: v })} />
    <Slider label="Trail length" value={cfg.trailOpacity} min={0.01} max={0.3} step={0.005}
      fmt={v => v.toFixed(3)} onChange={v => set({ trailOpacity: v })} />
  </>,
  grid: (cfg, set) => <>
    <Slider label="Columns" value={cfg.gridCols} min={4} max={36} step={1}
      fmt={v => Math.round(v).toString()} onChange={v => set({ gridCols: Math.round(v) })} />
    <Slider label="Beam size" value={cfg.particleSize} min={0.3} max={4} step={0.05}
      onChange={v => set({ particleSize: v })} />
  </>,
  aurora: (cfg, set) => <>
    <Slider label="Wave amplitude" value={cfg.auroraAmplitude} min={0.02} max={0.4} step={0.005}
      fmt={v => v.toFixed(3)} onChange={v => set({ auroraAmplitude: v })} />
    <ColorRow label="Tertiary color" value={cfg.colorTertiary} onChange={v => set({ colorTertiary: v })} />
  </>,
  pulse: (cfg, set) => <>
    <Slider label="Sources" value={cfg.pulseSourceCount} min={1} max={8} step={1}
      fmt={v => Math.round(v).toString()} onChange={v => set({ pulseSourceCount: Math.round(v) })} />
    <Slider label="Source size" value={cfg.particleSize} min={0.3} max={4} step={0.05}
      onChange={v => set({ particleSize: v })} />
    <Slider label="Trail length" value={cfg.trailOpacity} min={0.01} max={0.25} step={0.005}
      fmt={v => v.toFixed(3)} onChange={v => set({ trailOpacity: v })} />
  </>,
  noise: (cfg, set) => <>
    <Slider label="Particles" value={cfg.particleCount} min={50} max={600} step={10}
      fmt={v => Math.round(v).toString()} onChange={v => set({ particleCount: Math.round(v) })} />
    <Slider label="Size" value={cfg.particleSize} min={0.3} max={4} step={0.05}
      onChange={v => set({ particleSize: v })} />
    <Slider label="Direction bias" value={cfg.directionAngle} min={0} max={359} step={1}
      fmt={v => `${Math.round(v)}°`} onChange={v => set({ directionAngle: v })} />
    <Slider label="Trail length" value={cfg.trailOpacity} min={0.01} max={0.25} step={0.005}
      fmt={v => v.toFixed(3)} onChange={v => set({ trailOpacity: v })} />
  </>,
  hex: (cfg, set) => <>
    <Slider label="Cell size" value={cfg.hexSize} min={14} max={90} step={1}
      fmt={v => `${Math.round(v)}px`} onChange={v => set({ hexSize: Math.round(v) })} />
  </>,
  'hex-classic': (cfg, set) => <>
    <Slider label="Cell size" value={cfg.hexSize} min={14} max={90} step={1}
      fmt={v => `${Math.round(v)}px`} onChange={v => set({ hexSize: Math.round(v) })} />
  </>,
  circuit: (cfg, set) => <>
    <Slider label="Grid pitch" value={cfg.hexSize} min={20} max={96} step={4}
      fmt={v => `${Math.round(v)}px`} onChange={v => set({ hexSize: Math.round(v) })} />
    <Slider label="Node size" value={cfg.particleSize} min={0.3} max={3} step={0.05}
      onChange={v => set({ particleSize: v })} />
    <Slider label="Trail length" value={cfg.trailOpacity} min={0.01} max={0.25} step={0.005}
      fmt={v => v.toFixed(3)} onChange={v => set({ trailOpacity: v })} />
  </>,
}

function AnimParamsPanel() {
  const { bgPreset, animConfig, setAnimConfig, resetAnimConfig, randomizeAnim } = useThemeStore()
  if (bgPreset === 'none') return null

  const presetParams = PRESET_PARAMS[bgPreset as Exclude<BgPreset, 'none'>]

  return (
    <div className="space-y-3 pt-3 mt-3" style={{ borderTop: '1px solid var(--border-subtle)' }}>
      <div className="flex items-center justify-between">
        <p className="text-xs font-medium" style={{ color: 'var(--text-secondary)' }}>Animation parameters</p>
        <div className="flex gap-1.5">
          <button
            onClick={resetAnimConfig}
            className="flex items-center gap-1 px-2 py-1 rounded text-xs hover:opacity-80 transition-colors"
            style={{ background: 'var(--bg-elevated)', color: 'var(--text-muted)', border: '1px solid var(--border-subtle)' }}
          >
            <RotateCcw size={10} /> Reset
          </button>
          <button
            onClick={randomizeAnim}
            className="flex items-center gap-1 px-2 py-1 rounded text-xs hover:opacity-80 transition-colors"
            style={{ background: 'var(--accent-primary-subtle)', color: 'var(--accent-primary)', border: '1px solid var(--accent-primary)' }}
          >
            <Sparkles size={10} /> Generate
          </button>
        </div>
      </div>

      <div className="grid gap-3 sm:grid-cols-2">
        <div className="space-y-3 p-3 rounded-lg" style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)' }}>
          <p className="text-xs uppercase tracking-wider" style={{ color: 'var(--text-muted)' }}>Universal</p>
          <Slider label="Speed" value={animConfig.speed} min={0.1} max={5} step={0.05}
            onChange={v => setAnimConfig({ speed: v })} />
          <Slider label="Opacity" value={animConfig.opacity} min={0.1} max={1} step={0.02}
            onChange={v => setAnimConfig({ opacity: v })} />
          <Slider label="Glow intensity" value={animConfig.glowIntensity} min={0} max={30} step={1}
            fmt={v => `${Math.round(v)}px`} onChange={v => setAnimConfig({ glowIntensity: Math.round(v) })} />
          <ColorRow label="Primary color" value={animConfig.colorPrimary} onChange={v => setAnimConfig({ colorPrimary: v })} />
          <ColorRow label="Secondary color" value={animConfig.colorSecondary} onChange={v => setAnimConfig({ colorSecondary: v })} />
        </div>

        <div className="space-y-3 p-3 rounded-lg" style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)' }}>
          <p className="text-xs uppercase tracking-wider" style={{ color: 'var(--text-muted)' }}>
            {BG_PRESETS.find(p => p.id === bgPreset)?.label} specific
          </p>
          {presetParams(animConfig, setAnimConfig)}
        </div>
      </div>

      <p className="text-xs" style={{ color: 'var(--text-disabled)' }}>
        Colors with × use your theme accent. Generate creates harmonious random combinations.
      </p>
    </div>
  )
}

function VisualEffectsPanel() {
  const { glassLevel, bgPreset, setGlass, setBgPreset, randomize } = useThemeStore()
  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <p className="text-xs font-medium" style={{ color: 'var(--text-secondary)' }}>Visual effects</p>
        <button
          onClick={randomize}
          className="flex items-center gap-1.5 px-2.5 py-1 rounded text-xs transition-colors hover:opacity-80"
          style={{ background: 'var(--accent-primary-subtle)', color: 'var(--accent-primary)', border: '1px solid var(--accent-primary)' }}
        >
          <Shuffle size={11} /> Randomize all
        </button>
      </div>

      <div>
        <p className="text-xs mb-2" style={{ color: 'var(--text-muted)' }}>Glass effect</p>
        <div className="grid grid-cols-4 gap-1.5">
          {GLASS_LEVELS.map((g) => (
            <button
              key={g.id}
              onClick={() => setGlass(g.id as GlassLevel)}
              className="px-2 py-2 rounded text-xs transition-colors hover:opacity-90"
              title={g.description}
              style={{
                background: glassLevel === g.id ? 'var(--accent-primary-subtle)' : 'var(--bg-elevated)',
                border: `1px solid ${glassLevel === g.id ? 'var(--accent-primary)' : 'var(--border-subtle)'}`,
                color: glassLevel === g.id ? 'var(--accent-primary)' : 'var(--text-muted)',
              }}
            >
              {g.label}
            </button>
          ))}
        </div>
      </div>

      <div>
        <p className="text-xs mb-2" style={{ color: 'var(--text-muted)' }}>Background pattern</p>
        <div className="grid grid-cols-4 gap-1.5">
          {BG_PRESETS.map((p) => (
            <button
              key={p.id}
              onClick={() => setBgPreset(p.id as BgPreset)}
              className="px-2 py-2 rounded text-xs transition-colors hover:opacity-90"
              title={p.description}
              style={{
                background: bgPreset === p.id ? 'var(--accent-primary-subtle)' : 'var(--bg-elevated)',
                border: `1px solid ${bgPreset === p.id ? 'var(--accent-primary)' : 'var(--border-subtle)'}`,
                color: bgPreset === p.id ? 'var(--accent-primary)' : 'var(--text-muted)',
              }}
            >
              {p.label}
            </button>
          ))}
        </div>
      </div>

      <AnimParamsPanel />
    </div>
  )
}

export default function ThemeEditor() {
  const { addCustomTheme, setTheme, activeTheme } = useThemeStore()
  // Initialize from live DOM values so the picker starts matching current theme
  const [overrides, setOverrides] = useState<Record<string, string>>(() => snapshotTokens())
  const [themeName, setThemeName] = useState('My Theme')
  const [dirty, setDirty] = useState(false)

  // Re-read DOM when active theme changes externally (e.g. switching presets)
  useEffect(() => {
    // Small rAF delay so applyTheme() has set the CSS vars before we snapshot
    const id = requestAnimationFrame(() => {
      setOverrides(snapshotTokens())
      setDirty(false)
    })
    return () => cancelAnimationFrame(id)
  }, [activeTheme.id])

  const applyLive = useCallback((cssVar: string, value: string) => {
    document.documentElement.style.setProperty(cssVar, value)
  }, [])

  const handleChange = (cssVar: string, value: string) => {
    applyLive(cssVar, value)
    setOverrides((prev) => ({ ...prev, [cssVar]: value }))
    setDirty(true)
  }

  const reset = () => {
    // Remove all inline overrides — reverts to the base theme's CSS
    for (const g of TOKEN_GROUPS) {
      for (const t of g.tokens) {
        document.documentElement.style.removeProperty(t.var)
      }
    }
    setOverrides(snapshotTokens())
    setDirty(false)
  }

  const save = () => {
    if (!themeName.trim()) { notify.error('Enter a theme name'); return }
    const id = `custom-${Date.now()}`
    const theme = {
      id,
      name: themeName.trim(),
      mode: 'custom' as const,
      isBuiltin: false,
      tokens: { ...overrides },
    }
    addCustomTheme(theme)
    setTheme(id)
    setDirty(false)
    notify.success(`Theme "${themeName}" saved`)
  }

  return (
    <div className="space-y-6">
      <VisualEffectsPanel />
      <hr style={{ borderColor: 'var(--border-subtle)' }} />
      <div className="flex items-center gap-3 flex-wrap">
        <input
          value={themeName}
          onChange={(e) => setThemeName(e.target.value)}
          placeholder="Theme name…"
          className="px-3 py-1.5 rounded text-sm outline-none"
          style={{
            background: 'var(--bg-elevated)',
            border: '1px solid var(--border-default)',
            color: 'var(--text-primary)',
            minWidth: 160,
          }}
        />
        <Button size="sm" variant="primary" onClick={save} disabled={!themeName.trim()}>
          <Save size={12} className="mr-1.5" /> Save as theme
        </Button>
        <Button size="sm" variant="ghost" onClick={reset} disabled={!dirty}>
          <RotateCcw size={12} className="mr-1.5" /> Reset
        </Button>
      </div>

      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
        {TOKEN_GROUPS.map((group) => (
          <div
            key={group.label}
            className="rounded-lg p-3 space-y-2"
            style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)' }}
          >
            <p className="text-xs font-medium uppercase tracking-wider mb-3" style={{ color: 'var(--text-muted)' }}>
              {group.label}
            </p>
            {group.tokens.map(({ var: cssVar, label }) => {
              const current = overrides[cssVar] ?? '#000000'
              return (
                <div key={cssVar} className="flex items-center gap-2.5">
                  {/* Colour swatch + native picker */}
                  <label
                    className="relative flex-shrink-0 cursor-pointer"
                    title={cssVar}
                    style={{
                      width: 28,
                      height: 28,
                      borderRadius: 6,
                      background: current,
                      border: '2px solid var(--border-default)',
                      overflow: 'hidden',
                    }}
                  >
                    <input
                      type="color"
                      value={current}
                      onChange={(e) => handleChange(cssVar, e.target.value)}
                      className="absolute inset-0 opacity-0 cursor-pointer"
                      style={{ width: '100%', height: '100%' }}
                    />
                  </label>
                  <div className="flex-1 min-w-0">
                    <p className="text-xs truncate" style={{ color: 'var(--text-secondary)' }}>{label}</p>
                    <p className="text-xs font-mono truncate" style={{ color: 'var(--text-muted)' }}>{current}</p>
                  </div>
                </div>
              )
            })}
          </div>
        ))}
      </div>
    </div>
  )
}
