import { useEffect } from 'react'
import { useThemeStore } from '@/store/theme'
import { applyTheme } from './themes'

export function ThemeProvider({ children }: { children: React.ReactNode }) {
  const activeTheme  = useThemeStore((s) => s.activeTheme)
  const glassLevel   = useThemeStore((s) => s.glassLevel)
  const panelOpacity = useThemeStore((s) => s.panelOpacity)
  const panelRadius  = useThemeStore((s) => s.panelRadius)
  const a11y         = useThemeStore((s) => s.a11y)

  useEffect(() => { applyTheme(activeTheme) }, [activeTheme])

  useEffect(() => {
    if (glassLevel === 'none') document.body.removeAttribute('data-glass')
    else document.body.setAttribute('data-glass', glassLevel)
  }, [glassLevel])

  useEffect(() => {
    const opacity = a11y.reduceTransparency ? 0 : panelOpacity / 100
    document.documentElement.style.setProperty('--panel-opacity', String(opacity))
    document.documentElement.style.setProperty('--panel-radius', `${panelRadius}px`)
  }, [panelOpacity, panelRadius, a11y.reduceTransparency])

  useEffect(() => {
    const cl = document.documentElement.classList
    cl.toggle('a11y-reduce-transparency', a11y.reduceTransparency)
    cl.toggle('a11y-reduce-motion',       a11y.reduceMotion)
    cl.toggle('a11y-large-controls',      a11y.largeControls)
    cl.toggle('a11y-prefer-stacked',      a11y.preferStacked)
  }, [a11y])

  return <>{children}</>
}
