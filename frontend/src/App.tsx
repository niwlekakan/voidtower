import { useEffect } from 'react'
import { BrowserRouter, Routes, Route, Navigate } from 'react-router-dom'
import { ThemeProvider } from '@/theme/ThemeProvider'
import { useAuthStore } from '@/store/auth'
import { api } from '@/api/client'
import AppLayout from '@/components/layout/AppLayout'
import AiosLayout from '@/aios/AiosLayout'
import { useThemeStore } from '@/store/theme'
import LoginPage from '@/pages/Login'
import BootstrapPage from '@/pages/Bootstrap'
import DashboardPage from '@/pages/Dashboard'
import ServicesPage from '@/pages/Services'
import ContainersPage from '@/pages/Containers'
import ContainerDetailPage from '@/pages/ContainerDetail'
import AppVaultPage from '@/pages/AppVault'
import AlertsPage from '@/pages/Alerts'
import BackupsPage from '@/pages/Backups'
import StoragePage from '@/pages/Storage'
import NetworkPage from '@/pages/Network'
import TerminalPage from '@/pages/Terminal'
import AuditPage from '@/pages/Audit'
import AIPage from '@/pages/AI'
import FilesPage from '@/pages/Files'
import ProxiesPage from '@/pages/Proxies'
import SecurityPage from '@/pages/Security'
import SettingsPage from '@/pages/Settings'
import AutomationPage from '@/pages/Automation'
import FirewallPage from '@/pages/Firewall'
import TimelinePage from '@/pages/Timeline'
import SecretsPage from '@/pages/Secrets'
import CapabilitiesPage from '@/pages/Capabilities'
import DiagnosticsPage from '@/pages/Diagnostics'
import WireGuardPage from '@/pages/WireGuard'
import VMsPage from '@/pages/VMs'
import ProxmoxPage from '@/pages/ProxmoxPage'
import TagsPage from '@/pages/Tags'
import ThemesPage from '@/pages/Themes'
import ModelsPage from '@/pages/Models'
import UpdatesPage from '@/pages/Updates'
import IntegrationsPage from '@/pages/Integrations'
import PolicyPage from '@/pages/Policy'
import ModsPage from '@/pages/Mods'
import PluginsPage from '@/pages/Plugins'
import PluginPage from '@/pages/PluginPage'
import NotFoundPage from '@/pages/NotFound'

function RequireAuth({ children }: { children: React.ReactNode }) {
  const { status } = useAuthStore()
  if (status === 'idle' || status === 'loading') return null
  if (status === 'unauthenticated') return <Navigate to="/login" replace />
  return <>{children}</>
}

function AuthedShell() {
  const uiMode = useThemeStore((s) => s.uiMode)
  if (uiMode === 'void') return <AiosLayout />
  return <AppLayout />
}

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
      .then(({ user }) => setUser(user))
      .catch(() => setStatus('unauthenticated'))
  }, [setUser, setStatus])

  useEffect(() => {
    fetch('/api/settings/public')
      .then(r => r.ok ? r.json() : null)
      .then((d: { instance_name?: string; custom_css?: string; instance_logo?: string } | null) => {
        if (d) applyBranding(d)
      })
      .catch(() => {})
  }, [])

  return (
    <ThemeProvider>
      <BrowserRouter>
        <Routes>
          <Route path="/login" element={<LoginPage />} />
          <Route path="/bootstrap" element={<BootstrapPage />} />
          <Route path="/" element={<RequireAuth><AuthedShell /></RequireAuth>}>
            <Route index element={<Navigate to="/dashboard" replace />} />
            <Route path="dashboard" element={<DashboardPage />} />
            <Route path="services"   element={<ServicesPage />} />
            <Route path="containers"     element={<ContainersPage />} />
            <Route path="containers/:id" element={<ContainerDetailPage />} />
            <Route path="apps"       element={<AppVaultPage />} />
            <Route path="alerts"     element={<AlertsPage />} />
            <Route path="backups"    element={<BackupsPage />} />
            <Route path="storage"    element={<StoragePage />} />
            <Route path="network"    element={<NetworkPage />} />
            <Route path="ai"         element={<AIPage />} />
            <Route path="models"     element={<ModelsPage />} />
            <Route path="updates"        element={<UpdatesPage />} />
            <Route path="mods"           element={<ModsPage />} />
            <Route path="plugins"        element={<PluginsPage />} />
            <Route path="plugins/:id"    element={<PluginPage />} />
            <Route path="integrations"   element={<IntegrationsPage />} />
            <Route path="files"      element={<FilesPage />} />
            <Route path="proxies"    element={<ProxiesPage />} />
            <Route path="security"   element={<SecurityPage />} />
            <Route path="policy"     element={<PolicyPage />} />
            <Route path="terminal"   element={<TerminalPage />} />
            <Route path="audit" element={<AuditPage />} />
            <Route path="automation"      element={<AutomationPage />} />
            <Route path="firewall"        element={<FirewallPage />} />
            <Route path="timeline"       element={<TimelinePage />} />
            <Route path="secrets"        element={<SecretsPage />} />
            <Route path="capabilities"  element={<CapabilitiesPage />} />
            <Route path="diagnostics"   element={<DiagnosticsPage />} />
            <Route path="wireguard"     element={<WireGuardPage />} />
            <Route path="vms"           element={<VMsPage />} />
            <Route path="proxmox"       element={<ProxmoxPage />} />
            <Route path="tags"          element={<TagsPage />} />
            <Route path="themes"        element={<ThemesPage />} />
            <Route path="settings/*" element={<SettingsPage />} />
            <Route path="*" element={<NotFoundPage />} />
          </Route>
        </Routes>
      </BrowserRouter>
    </ThemeProvider>
  )
}
