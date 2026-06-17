import { useState, useEffect } from 'react'
import { useSearchParams } from 'react-router-dom'
import { Palette, Brush } from 'lucide-react'
import ThemesPage from './Themes'
import BrandingTab from './CustomizationBranding'

export default function CustomizationPage() {
  const [searchParams, setSearchParams] = useSearchParams()
  const [activeTab, setActiveTab] = useState(() => {
    return (searchParams.get('tab') as 'themes' | 'branding') || 'themes'
  })

  useEffect(() => {
    setSearchParams({ tab: activeTab })
  }, [activeTab, setSearchParams])

  const tabs = [
    { id: 'themes', label: 'Themes', icon: Palette },
    { id: 'branding', label: 'Branding', icon: Brush },
  ] as const

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
      </div>
    </div>
  )
}
