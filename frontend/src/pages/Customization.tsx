import { useState, useEffect } from 'react'
import { useSearchParams } from 'react-router-dom'
import { Palette, Brush, Navigation, LayoutPanelTop } from 'lucide-react'
import ThemesPage from './Themes'
import BrandingTab from './CustomizationBranding'
import NavigationTab from './CustomizationNavigation'
import CustomizationTabs from './CustomizationTabs'
import { useAuthStore } from '@/store/auth'

export default function CustomizationPage() {
  const [searchParams, setSearchParams] = useSearchParams()
  const currentUser = useAuthStore((s) => s.user)
  const isAdmin = currentUser?.role === 'owner' || currentUser?.role === 'admin'
  const [activeTab, setActiveTab] = useState(() => {
    return (searchParams.get('tab') as 'themes' | 'branding' | 'navigation' | 'tabs') || 'themes'
  })

  useEffect(() => {
    setSearchParams({ tab: activeTab })
  }, [activeTab, setSearchParams])

  const tabs: { id: 'themes' | 'branding' | 'navigation' | 'tabs'; label: string; icon: typeof Palette }[] = [
    { id: 'themes', label: 'Themes', icon: Palette },
    { id: 'branding', label: 'Branding', icon: Brush },
    { id: 'tabs', label: 'My Tabs', icon: LayoutPanelTop },
  ]
  if (isAdmin) tabs.push({ id: 'navigation', label: 'Navigation', icon: Navigation })

  return (
    <div className="space-y-0">
      {/* Tab Navigation */}
      <div className="flex gap-0 border-b" style={{ borderColor: 'var(--border-subtle)' }}>
        {tabs.map((tab) => {
          const Icon = tab.icon
          const isActive = activeTab === tab.id
          return (
            <button
              key={tab.id}
              onClick={() => setActiveTab(tab.id)}
              className="flex items-center gap-2 px-4 py-3 text-sm font-medium transition-colors border-b-2 -mb-px"
              style={{
                color: isActive ? 'var(--accent-primary)' : 'var(--text-secondary)',
                borderColor: isActive ? 'var(--accent-primary)' : 'transparent',
              }}
            >
              <Icon size={16} />
              {tab.label}
            </button>
          )
        })}
      </div>

      {/* Tab Content */}
      <div style={{ padding: '0' }}>
        {activeTab === 'themes' && <ThemesPage />}
        {activeTab === 'branding' && <BrandingTab />}
        {activeTab === 'tabs' && <CustomizationTabs />}
        {activeTab === 'navigation' && isAdmin && <NavigationTab />}
      </div>
    </div>
  )
}
