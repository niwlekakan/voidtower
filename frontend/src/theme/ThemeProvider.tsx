import { useEffect } from 'react'
import { useThemeStore } from '@/store/theme'
import { applyTheme } from './themes'

export function ThemeProvider({ children }: { children: React.ReactNode }) {
  const activeTheme = useThemeStore((s) => s.activeTheme)
  const glassLevel  = useThemeStore((s) => s.glassLevel)

  useEffect(() => { applyTheme(activeTheme) }, [activeTheme])

  useEffect(() => {
    if (glassLevel === 'none') document.body.removeAttribute('data-glass')
    else document.body.setAttribute('data-glass', glassLevel)
  }, [glassLevel])

  return <>{children}</>
}
