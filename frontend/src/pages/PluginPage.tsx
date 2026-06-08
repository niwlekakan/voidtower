import { useState, useEffect } from 'react'
import { useParams, Link } from 'react-router-dom'
import { Blocks, ChevronLeft, AlertTriangle } from 'lucide-react'
import { api } from '@/api/client'
import type { Plugin } from '@/api/types'

export default function PluginPage() {
  const { id } = useParams<{ id: string }>()
  const [plugin, setPlugin] = useState<Plugin | null>(null)
  const [error, setError] = useState('')

  useEffect(() => {
    if (!id) return
    api.plugins.list()
      .then(list => {
        const p = list.find(x => x.id === id)
        if (!p) { setError('Plugin not found'); return }
        if (!p.enabled) { setError('Plugin is disabled'); return }
        setPlugin(p)
      })
      .catch(() => setError('Failed to load plugin'))
  }, [id])

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%' }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '8px 16px', borderBottom: '1px solid var(--border-subtle)', background: 'var(--bg-panel)', flexShrink: 0 }}>
        <Link to="/plugins" style={{ display: 'flex', alignItems: 'center', gap: 4, color: 'var(--text-muted)', textDecoration: 'none', fontSize: 13 }}>
          <ChevronLeft size={14} /> Plugins
        </Link>
        {plugin && (
          <>
            <span style={{ color: 'var(--border)' }}>/</span>
            <Blocks size={14} style={{ color: 'var(--accent-primary)' }} />
            <span style={{ fontSize: 13, fontWeight: 500 }}>{plugin.name}</span>
            <span style={{ fontSize: 11, color: 'var(--text-muted)' }}>v{plugin.version}</span>
          </>
        )}
      </div>

      {error ? (
        <div style={{ flex: 1, display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center', gap: 12, color: 'var(--text-muted)' }}>
          <AlertTriangle size={32} style={{ color: 'var(--accent-warning)' }} />
          <div style={{ fontSize: 14 }}>{error}</div>
          <Link to="/plugins" style={{ fontSize: 13, color: 'var(--accent-primary)' }}>Back to Plugins</Link>
        </div>
      ) : !plugin ? (
        <div style={{ flex: 1, display: 'flex', alignItems: 'center', justifyContent: 'center', color: 'var(--text-muted)', fontSize: 13 }}>
          Loading…
        </div>
      ) : (
        <iframe
          src={`/plugin-assets/${plugin.id}/${plugin.entry}`}
          style={{ flex: 1, width: '100%', border: 'none' }}
          title={plugin.name}
          sandbox="allow-scripts allow-same-origin allow-forms allow-modals allow-popups"
        />
      )}
    </div>
  )
}
