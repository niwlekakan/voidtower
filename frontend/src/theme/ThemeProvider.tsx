import { useEffect } from 'react'
import { useThemeStore } from '@/store/theme'
import { applyTheme } from './themes'

const BG_VARS = ['--bg-root', '--bg-panel']

function hexToRgba(hex: string, alpha: number): string | null {
  const h = hex.replace(/\s/g, '')
  const m = h.match(/^#([0-9a-f]{6})$/i) ?? h.match(/^#([0-9a-f]{3})$/i)
  if (!m) return null
  const full = m[1].length === 3
    ? m[1].split('').map(c => c + c).join('')
    : m[1]
  const n = parseInt(full, 16)
  return `rgba(${(n >> 16) & 255},${(n >> 8) & 255},${n & 255},${alpha.toFixed(3)})`
}

function applyBgAlpha(opacity: number, forceOpaque: boolean) {
  const alpha = forceOpaque ? 1 : 1 - opacity / 100
  const root = document.documentElement
  const computed = getComputedStyle(root)
  BG_VARS.forEach(v => {
    const raw = computed.getPropertyValue(v).trim()
    if (alpha >= 1) {
      // applyTheme already set solid values — nothing to override
      return
    }
    const rgba = hexToRgba(raw, alpha)
    if (rgba) root.style.setProperty(v, rgba)
  })
}

export function ThemeProvider({ children }: { children: React.ReactNode }) {
  const activeTheme  = useThemeStore((s) => s.activeTheme)
  const glassLevel   = useThemeStore((s) => s.glassLevel)
  const panelOpacity = useThemeStore((s) => s.panelOpacity)
  const panelRadius  = useThemeStore((s) => s.panelRadius)
  const hoverFx      = useThemeStore((s) => s.hoverFx)
  const a11y         = useThemeStore((s) => s.a11y)

  // Apply theme first (sets solid hex values), then apply alpha on top.
  // Both theme and opacity changes must rerun together so we always read fresh solid colors.
  useEffect(() => {
    applyTheme(activeTheme)
    applyBgAlpha(panelOpacity, a11y.reduceTransparency)
  }, [activeTheme, panelOpacity, a11y.reduceTransparency])

  useEffect(() => {
    document.documentElement.style.setProperty('--panel-radius', `${panelRadius}px`)
  }, [panelRadius])

  useEffect(() => {
    if (glassLevel === 'none') document.body.removeAttribute('data-glass')
    else document.body.setAttribute('data-glass', glassLevel)
  }, [glassLevel])

  useEffect(() => {
    if (hoverFx === 'off') document.body.removeAttribute('data-hoverfx')
    else document.body.setAttribute('data-hoverfx', hoverFx)
  }, [hoverFx])

  useEffect(() => {
    const cl = document.documentElement.classList
    cl.toggle('a11y-reduce-transparency', a11y.reduceTransparency)
    cl.toggle('a11y-reduce-motion',       a11y.reduceMotion)
    cl.toggle('a11y-large-controls',      a11y.largeControls)
    cl.toggle('a11y-prefer-stacked',      a11y.preferStacked)
  }, [a11y])

  return <>{children}</>
}
