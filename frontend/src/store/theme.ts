import { create } from 'zustand'
import { persist } from 'zustand/middleware'
import {
  type Theme, type GlassLevel, type BgPreset, type AnimConfig,
  BUILTIN_THEMES, BG_PRESETS, DEFAULT_ANIM_CONFIG, randomizeAnimConfig, importTheme, applyTheme,
} from '@/theme/themes'

interface ThemeStore {
  activeTheme: Theme
  customThemes: Theme[]
  glassLevel: GlassLevel
  bgPreset: BgPreset
  animConfig: AnimConfig
  setTheme: (id: string) => void
  setGlass: (level: GlassLevel) => void
  setBgPreset: (preset: BgPreset) => void
  setAnimConfig: (patch: Partial<AnimConfig>) => void
  resetAnimConfig: () => void
  randomize: () => void
  randomizeAnim: () => void
  addCustomTheme: (theme: Theme) => void
  removeCustomTheme: (id: string) => void
  importFromJson: (json: string) => void
  allThemes: () => Theme[]
}

const GLASS_LEVELS: GlassLevel[] = ['none', 'blur', 'acrylic', 'frosted']
const ACCENT_COLORS = [
  '#8b5cf6','#06b6d4','#f59e0b','#ef4444','#39ff88','#ec4899','#3b82f6','#f97316',
]

export const useThemeStore = create<ThemeStore>()(
  persist(
    (set, get) => ({
      activeTheme: BUILTIN_THEMES[0],
      customThemes: [],
      glassLevel: 'none',
      bgPreset: 'none',
      animConfig: { ...DEFAULT_ANIM_CONFIG },

      setTheme: (id) => {
        const all = [...BUILTIN_THEMES, ...get().customThemes]
        const theme = all.find((t) => t.id === id)
        if (theme) set({ activeTheme: theme })
      },

      setGlass: (level) => set({ glassLevel: level }),

      setBgPreset: (preset) => set({ bgPreset: preset }),

      setAnimConfig: (patch) =>
        set((s) => ({ animConfig: { ...s.animConfig, ...patch } })),

      resetAnimConfig: () => set({ animConfig: { ...DEFAULT_ANIM_CONFIG } }),

      randomize: () => {
        const preset = BG_PRESETS.filter(p => p.id !== 'none')[Math.floor(Math.random() * (BG_PRESETS.length - 1))].id
        const glass  = GLASS_LEVELS[Math.floor(Math.random() * GLASS_LEVELS.length)]
        const accent = ACCENT_COLORS[Math.floor(Math.random() * ACCENT_COLORS.length)]
        // Store accent override in the active theme's tokens so it survives theme switches
        // and is handled by applyTheme() rather than a raw DOM write
        const current = get().activeTheme
        const updated: Theme = {
          ...current,
          tokens: { ...(current.tokens ?? {}), '--accent-primary': accent },
        }
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
    }),
    {
      name: 'vt-theme',
      partialize: (s) => ({
        activeTheme: s.activeTheme,
        customThemes: s.customThemes,
        glassLevel: s.glassLevel,
        bgPreset: s.bgPreset,
        animConfig: s.animConfig,
      }),
    },
  ),
)
