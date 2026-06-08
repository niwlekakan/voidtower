// ─── Animation config ─────────────────────────────────────────────────────────

export interface AnimConfig {
  speed: number            // 0.2–4.0, universal multiplier
  opacity: number          // 0.1–1.0
  colorPrimary: string     // hex or '' = use theme CSS var
  colorSecondary: string
  colorTertiary: string
  particleCount: number    // 20–400 (void, noise)
  particleSize: number     // 0.3–5.0
  directionAngle: number   // 0–360 deg, 0=up (void, noise)
  trailOpacity: number     // 0.02–0.25 (lower = longer trail)
  gridCols: number         // 8–32
  hexSize: number          // 16–80 (hex cell size & circuit grid pitch)
  pulseSourceCount: number // 1–8
  auroraAmplitude: number  // 0.04–0.35
  glowIntensity: number    // 0–30 (shadow blur px)
}

export const DEFAULT_ANIM_CONFIG: AnimConfig = {
  speed: 1.0,
  opacity: 1.0,
  colorPrimary: '',
  colorSecondary: '',
  colorTertiary: '',
  particleCount: 70,
  particleSize: 1.0,
  directionAngle: 0,
  trailOpacity: 0.10,
  gridCols: 16,
  hexSize: 36,
  pulseSourceCount: 3,
  auroraAmplitude: 0.12,
  glowIntensity: 12,
}

export function hex2rgba(hex: string, alpha: number): string {
  const clean = hex.replace(/\s/g, '')
  const m = clean.match(/^#([0-9a-f]{6})$/i) ?? clean.match(/^#([0-9a-f]{3})$/i)
  if (!m) return `rgba(0,0,0,${alpha})`
  const full = m[1].length === 3 ? m[1].split('').map(c => c + c).join('') : m[1]
  const n = parseInt(full, 16)
  return `rgba(${(n >> 16) & 255},${(n >> 8) & 255},${n & 255},${alpha})`
}

export function hsl2hex(h: number, s: number, l: number): string {
  s /= 100; l /= 100
  const k = (n: number) => (n + h / 30) % 12
  const a = s * Math.min(l, 1 - l)
  const f = (n: number) => l - a * Math.max(-1, Math.min(k(n) - 3, Math.min(9 - k(n), 1)))
  return '#' + [f(0), f(8), f(4)].map(x => Math.round(x * 255).toString(16).padStart(2, '0')).join('')
}

export function randomizeAnimConfig(): AnimConfig {
  const h = Math.random() * 360
  const s1 = 65 + Math.random() * 30
  const l1 = 55 + Math.random() * 15
  const primary   = hsl2hex(h, s1, l1)
  const secondary = hsl2hex((h + 100 + Math.random() * 80) % 360, s1 - 5, l1 + (Math.random() > 0.5 ? 10 : -5))
  const tertiary  = hsl2hex((h + 210 + Math.random() * 60) % 360, s1 - 10, l1 + 5)
  return {
    speed:             0.3 + Math.random() * 3.2,
    opacity:           0.5 + Math.random() * 0.5,
    colorPrimary:      primary,
    colorSecondary:    secondary,
    colorTertiary:     tertiary,
    particleCount:     Math.floor(30 + Math.random() * 250),
    particleSize:      0.4 + Math.random() * 3.5,
    directionAngle:    Math.floor(Math.random() * 360),
    trailOpacity:      0.02 + Math.random() * 0.18,
    gridCols:          Math.floor(8 + Math.random() * 22),
    hexSize:           Math.floor(18 + Math.random() * 58),
    pulseSourceCount:  Math.floor(1 + Math.random() * 7),
    auroraAmplitude:   0.05 + Math.random() * 0.28,
    glowIntensity:     Math.floor(Math.random() * 28),
  }
}

// ─── Theme types ──────────────────────────────────────────────────────────────

export type ThemeId =
  | 'voidtower'
  | 'blacksite'
  | 'ghost-terminal'
  | 'deep-grid'
  | 'solar-breach'
  | 'light-ops'
  | 'high-contrast'
  | 'custom'

export type GlassLevel = 'none' | 'blur' | 'acrylic' | 'frosted'
export type BgPreset  = 'none' | 'void' | 'grid' | 'aurora' | 'pulse' | 'noise' | 'hex' | 'hex-classic' | 'circuit'

export interface Theme {
  id: string
  name: string
  mode: 'dark' | 'light' | 'custom'
  isBuiltin: boolean
  tokens?: Record<string, string>
  density?: 'compact' | 'normal' | 'comfortable'
  radius?: 'sharp' | 'slight' | 'rounded'
  glow?: 'off' | 'low' | 'medium' | 'high'
  animations?: 'off' | 'reduced' | 'normal'
  glass?: GlassLevel
  bgPreset?: BgPreset
}

export const BG_PRESETS: { id: BgPreset; label: string; description: string }[] = [
  { id: 'none',    label: 'None',    description: 'Solid background' },
  { id: 'void',    label: 'Void',    description: 'Drifting particles' },
  { id: 'grid',    label: 'Grid',    description: 'Scrolling perspective grid' },
  { id: 'aurora',  label: 'Aurora',  description: 'Shifting gradient bands' },
  { id: 'pulse',   label: 'Pulse',   description: 'Radial glow breathe' },
  { id: 'noise',   label: 'Noise',   description: 'Subtle film grain' },
  { id: 'hex',         label: 'Hex',         description: 'Honeycomb ripple wave' },
  { id: 'hex-classic', label: 'Hex Classic', description: 'Simple honeycomb pulse' },
  { id: 'circuit', label: 'Circuit', description: 'Trace lines & nodes' },
]

export const GLASS_LEVELS: { id: GlassLevel; label: string; description: string }[] = [
  { id: 'none',    label: 'Solid',   description: 'No transparency' },
  { id: 'blur',    label: 'Blur',    description: 'Subtle gaussian blur' },
  { id: 'acrylic', label: 'Acrylic', description: 'Windows-style layered blur' },
  { id: 'frosted', label: 'Frosted', description: 'Heavy diffusion, macOS-style' },
]

export const BUILTIN_THEMES: Theme[] = [
  { id: 'voidtower',      name: 'VoidTower Default', mode: 'dark',   isBuiltin: true },
  { id: 'blacksite',      name: 'Blacksite',          mode: 'dark',   isBuiltin: true },
  { id: 'ghost-terminal', name: 'Ghost Terminal',     mode: 'dark',   isBuiltin: true },
  { id: 'deep-grid',      name: 'Deep Grid',          mode: 'dark',   isBuiltin: true },
  { id: 'solar-breach',   name: 'Solar Breach',       mode: 'dark',   isBuiltin: true },
  { id: 'light-ops',      name: 'Light Ops',          mode: 'light',  isBuiltin: true },
  { id: 'high-contrast',  name: 'High Contrast',      mode: 'dark',   isBuiltin: true },
]

// All CSS variable names that can be overridden via theme tokens
const CLEARABLE_VARS = [
  '--bg-root','--bg-panel','--bg-card','--bg-elevated',
  '--border-subtle','--border-default','--border-strong',
  '--text-primary','--text-secondary','--text-muted','--text-disabled',
  '--accent-primary','--accent-primary-hover','--accent-secondary',
  '--accent-primary-subtle','--accent-secondary-subtle',
  '--accent-success','--accent-success-subtle',
  '--accent-warning','--accent-warning-subtle',
  '--accent-danger','--accent-danger-subtle',
  '--terminal-green','--terminal-bg','--terminal-cursor',
]

export function clearThemeOverrides(): void {
  const root = document.documentElement
  for (const v of CLEARABLE_VARS) root.style.removeProperty(v)
}

export function applyTheme(theme: Theme): void {
  const root = document.documentElement
  // Clear any previously applied inline token overrides first
  clearThemeOverrides()
  // Set/clear the data-theme attribute for CSS-class-based builtin themes
  root.removeAttribute('data-theme')
  if (theme.id !== 'voidtower') {
    root.setAttribute('data-theme', theme.id)
  }
  // Apply custom token overrides
  if (theme.tokens) {
    for (const [key, value] of Object.entries(theme.tokens)) {
      // Validate: only allow CSS variable names and safe color values
      if (/^--[\w-]+$/.test(key) && /^#[0-9a-fA-F]{3,8}$|^rgba?\([\d,. ]+\)$|^[a-zA-Z]+$/.test(value)) {
        root.style.setProperty(key, value)
      }
    }
  }
}

export function exportTheme(theme: Theme): string {
  return JSON.stringify(theme, null, 2)
}

export function importTheme(json: string): Theme {
  const parsed = JSON.parse(json) as Theme
  if (!parsed.id || !parsed.name) throw new Error('Invalid theme JSON: missing id or name')
  return { ...parsed, isBuiltin: false }
}
