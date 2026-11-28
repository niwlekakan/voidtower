import { useEffect } from 'react'
import { BrowserRouter } from 'react-router-dom'
import { ThemeProvider } from '@/theme/ThemeProvider'
import { useAuthStore } from '@/store/auth'
import { api } from '@/api/client'
import AppRoutes from '@/AppRoutes'
import { useNavConfigStore } from '@/store/navConfig'

function applyBranding(data: { instance_name?: string; custom_css?: string; instance_logo?: string }) {
  if (data.instance_name) document.title = data.instance_name

  // Custom CSS injection
  let styleEl = document.getElementById('vt-custom-css') as HTMLStyleElement | null
  if (data.custom_css) {
    if (!styleEl) {
      styleEl = document.createElement('style')
      styleEl.id = 'vt-custom-css'
      document.head.appendChild(styleEl)
    }
    styleEl.textContent = data.custom_css
  } else if (styleEl) {
    styleEl.textContent = ''
  }

  // Favicon swap
  if (data.instance_logo) {
    const existing = document.querySelector<HTMLLinkElement>('link[rel~="icon"]')
    if (existing) {
      existing.href = data.instance_logo
    } else {
      const link = document.createElement('link')
      link.rel = 'icon'
      link.href = data.instance_logo
      document.head.appendChild(link)
    }
  }
}

export default function App() {
  const { setUser, setStatus } = useAuthStore()

  useEffect(() => {
    setStatus('loading')
    api.auth.me()
      .then(({ user }) => {
        setUser(user)
        void useNavConfigStore.getState().hydrateFromServer()
      })
      .catch(() => setStatus('unauthenticated'))
  }, [setUser, setStatus])

  useEffect(() => {
    fetch('/api/settings/public')
      .then(r => r.ok ? r.json() : null)
      .then((d: { instance_name?: string; custom_css?: string; instance_logo?: string } | null) => {
        if (d) applyBranding(d)
      })
      .catch(() => {})
    const handler = (e: Event) => {
      const detail = (e as CustomEvent<{ instance_name?: string; instance_logo?: string }>).detail
      if (detail) applyBranding(detail)
    }
    window.addEventListener('vt-settings-changed', handler)
    return () => window.removeEventListener('vt-settings-changed', handler)
  }, [])

  return (
    <ThemeProvider>
      <BrowserRouter>
        <AppRoutes />
      </BrowserRouter>
    </ThemeProvider>
  )
}
