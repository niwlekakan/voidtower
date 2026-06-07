import { create } from 'zustand'
import { persist } from 'zustand/middleware'
import {
  type Theme, type GlassLevel, type BgPreset, type AnimConfig,
  BUILTIN_THEMES, BG_PRESETS, DEFAULT_ANIM_CONFIG, randomizeAnimConfig, importTheme, applyTheme, hsl2hex,
} from '@/theme/themes'
import { type DeviceTier } from '../aios/store/aios'

export type UiMode = 'tower' | 'void'

export interface A11yConfig {
  reduceTransparency: boolean
  reduceMotion: boolean
  largeControls: boolean
  preferStacked: boolean
}

const DEFAULT_A11Y: A11yConfig = {
  reduceTransparency: false,
  reduceMotion: false,
  largeControls: false,
  preferStacked: false,
}

interface ThemeStore {
  activeTheme: Theme
  customThemes: Theme[]
  glassLevel: GlassLevel
  panelOpacity: number   // 0 = fully opaque, 100 = max transparency
  panelRadius: number    // 0-20 px, border-radius for panels/cards
  bgPreset: BgPreset
  animConfig: AnimConfig
  uiMode: UiMode
  deviceMode: DeviceTier | 'auto'
  a11y: A11yConfig
  setTheme: (id: string) => void
  setGlass: (level: GlassLevel) => void
  setPanelOpacity: (n: number) => void
  setPanelRadius: (n: number) => void
  setBgPreset: (preset: BgPreset) => void
  setAnimConfig: (patch: Partial<AnimConfig>) => void
  resetAnimConfig: () => void
  randomize: () => void
  randomizeAnim: () => void
  addCustomTheme: (theme: Theme) => void
  removeCustomTheme: (id: string) => void
  importFromJson: (json: string) => void
  allThemes: () => Theme[]
  setUiMode: (mode: UiMode) => void
  setDeviceMode: (mode: DeviceTier | 'auto') => void
  setA11y: (patch: Partial<A11yConfig>) => void
}

const GLASS_LEVELS: GlassLevel[] = ['none', 'blur', 'acrylic', 'frosted']

export const useThemeStore = create<ThemeStore>()(
  persist(
    (set, get) => ({
      activeTheme: BUILTIN_THEMES[0],
      customThemes: [],
      glassLevel: 'none',
      panelOpacity: 0,
      panelRadius: 8,
      bgPreset: 'none',
      animConfig: { ...DEFAULT_ANIM_CONFIG },
      uiMode: 'tower' as UiMode,
      deviceMode: 'auto' as DeviceTier | 'auto',
      a11y: { ...DEFAULT_A11Y },

      setTheme: (id) => {
        const all = [...BUILTIN_THEMES, ...get().customThemes]
        const theme = all.find((t) => t.id === id)
        if (theme) set({ activeTheme: theme })
      },

      setGlass: (level) => set({ glassLevel: level }),
      setPanelOpacity: (n) => set({ panelOpacity: Math.max(0, Math.min(100, n)) }),
      setPanelRadius: (n) => set({ panelRadius: Math.max(0, Math.min(20, n)) }),

      setBgPreset: (preset) => set({ bgPreset: preset }),

      setAnimConfig: (patch) =>
        set((s) => ({ animConfig: { ...s.animConfig, ...patch } })),

      resetAnimConfig: () => set({ animConfig: { ...DEFAULT_ANIM_CONFIG } }),

      randomize: () => {
        const preset = BG_PRESETS.filter(p => p.id !== 'none')[Math.floor(Math.random() * (BG_PRESETS.length - 1))].id
        const glass  = GLASS_LEVELS[Math.floor(Math.random() * GLASS_LEVELS.length)]
        // Generate a full coherent token palette from a random base hue
        const h  = Math.random() * 360
        const hA = (h + 140 + Math.random() * 80) % 360  // secondary accent hue
        const isDark = Math.random() > 0.15               // ~85% dark themes
        // Backgrounds: 4 layered dark/light surfaces
        const bgL0 = isDark ?  6 + Math.random() *  6 :  94 + Math.random() *  4
        const bgL1 = isDark ?  9 + Math.random() *  6 :  90 + Math.random() *  4
        const bgL2 = isDark ? 12 + Math.random() *  6 :  86 + Math.random() *  4
        const bgL3 = isDark ? 16 + Math.random() *  6 :  82 + Math.random() *  4
        const bgSat = 8 + Math.random() * 14
        // Borders: slightly lighter than surface
        const bdBase = isDark ? bgL2 + 6 : bgL2 - 6
        // Text: high contrast against bg
        const txL0 = isDark ? 88 + Math.random() * 10 : 10 + Math.random() *  8
        const txL1 = isDark ? 68 + Math.random() * 12 : 28 + Math.random() * 10
        const txL2 = isDark ? 46 + Math.random() * 12 : 46 + Math.random() * 10
        // Accents
        const acS = 70 + Math.random() * 25
        const acL = 55 + Math.random() * 15
        const accent        = hsl2hex(h,  acS,     acL)
        const accentHover   = hsl2hex(h,  acS,     Math.min(acL + 8, 85))
        const accentSecond  = hsl2hex(hA, acS - 5, acL + 5)
        // Semantic
        const successH = (120 + Math.random() * 30) % 360
        const warningH = (38  + Math.random() * 20) % 360
        const dangerH  = (0   + Math.random() * 20) % 360
        // Terminal
        const termGreenH = (135 + Math.random() * 25) % 360
        const tokens: Record<string, string> = {
          '--bg-root':              hsl2hex(h, bgSat,     bgL0),
          '--bg-panel':             hsl2hex(h, bgSat,     bgL1),
          '--bg-card':              hsl2hex(h, bgSat,     bgL2),
          '--bg-elevated':          hsl2hex(h, bgSat,     bgL3),
          '--border-subtle':        hsl2hex(h, bgSat + 4, bdBase),
          '--border-default':       hsl2hex(h, bgSat + 4, bdBase + (isDark ? 6 : -6)),
          '--border-strong':        hsl2hex(h, bgSat + 4, bdBase + (isDark ? 14 : -14)),
          '--text-primary':         hsl2hex(h, 12,        txL0),
          '--text-secondary':       hsl2hex(h, 10,        txL1),
          '--text-muted':           hsl2hex(h,  8,        txL2),
          '--accent-primary':       accent,
          '--accent-primary-hover': accentHover,
          '--accent-secondary':     accentSecond,
          '--accent-success':       hsl2hex(successH, 70, 50 + Math.random() * 15),
          '--accent-warning':       hsl2hex(warningH, 85, 52 + Math.random() * 12),
          '--accent-danger':        hsl2hex(dangerH,  75, 52 + Math.random() * 12),
          '--terminal-green':       hsl2hex(termGreenH, 80, 52 + Math.random() * 15),
          '--terminal-bg':          hsl2hex(h, bgSat, Math.max(bgL0 - 3, 3)),
          '--terminal-cursor':      accent,
        }
        const current = get().activeTheme
        const updated: Theme = { ...current, tokens }
        applyTheme(updated)
        set({ activeTheme: updated, bgPreset: preset, glassLevel: glass, animConfig: randomizeAnimConfig() })
      },

      randomizeAnim: () => set({ animConfig: randomizeAnimConfig() }),

      addCustomTheme: (theme) =>
        set((s) => ({ customThemes: [...s.customThemes.filter((t) => t.id !== theme.id), theme] })),

      removeCustomTheme: (id) =>
        set((s) => ({ customThemes: s.customThemes.filter((t) => t.id !== id) })),

      importFromJson: (json) => {
        const theme = importTheme(json)
        get().addCustomTheme(theme)
        get().setTheme(theme.id)
      },

      allThemes: () => [...BUILTIN_THEMES, ...get().customThemes],

      setUiMode: (mode) => set({ uiMode: mode }),

      setDeviceMode: (mode) => set({ deviceMode: mode }),

      setA11y: (patch) => set((s) => ({ a11y: { ...s.a11y, ...patch } })),
    }),
    {
      name: 'vt-theme',
      partialize: (s) => ({
        activeTheme: s.activeTheme,
        customThemes: s.customThemes,
        glassLevel: s.glassLevel,
        panelOpacity: s.panelOpacity,
        panelRadius: s.panelRadius,
        bgPreset: s.bgPreset,
        animConfig: s.animConfig,
        uiMode: s.uiMode,
        deviceMode: s.deviceMode,
        a11y: s.a11y,
      }),
    },
  ),
)
