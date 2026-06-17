import { useEffect, useState } from 'react'
import { useParams } from 'react-router-dom'
import { LayoutPanelTop } from 'lucide-react'
import { api } from '@/api/client'
import type { CustomTab } from '@/api/types'

export default function CustomTabView() {
  const { id } = useParams<{ id: string }>()
  const [tab, setTab] = useState<CustomTab | null>(null)
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    setLoading(true)
    api.tabs.list()
      .then(tabs => setTab(tabs.find(t => t.id === id) ?? null))
      .finally(() => setLoading(false))
  }, [id])

  if (loading) return null

  if (!tab) {
    return (
      <div className="flex flex-col items-center justify-center h-full gap-2">
        <LayoutPanelTop size={32} style={{ color: 'var(--text-muted)' }} />
        <p className="text-sm" style={{ color: 'var(--text-muted)' }}>Tab not found.</p>
      </div>
    )
  }

  if (tab.kind === 'iframe' && typeof tab.config.url === 'string') {
    return (
      <iframe
        src={tab.config.url}
        title={tab.title}
        sandbox="allow-scripts allow-same-origin allow-forms allow-popups"
        style={{ width: '100%', height: '100%', border: 'none', display: 'block' }}
      />
    )
  }

  if (tab.kind === 'markdown') {
    return (
      <div className="p-4 text-sm whitespace-pre-wrap" style={{ color: 'var(--text-primary)' }}>
        {typeof tab.config.content === 'string' ? tab.config.content : ''}
      </div>
    )
  }

  return (
    <div className="flex flex-col items-center justify-center h-full gap-2">
      <LayoutPanelTop size={32} style={{ color: 'var(--text-muted)' }} />
      <p className="text-sm" style={{ color: 'var(--text-muted)' }}>This tab kind isn't supported in Tower Mode yet.</p>
    </div>
  )
}
