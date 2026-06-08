import { useState, useEffect, useCallback } from 'react'
import { Blocks, Plus, Trash2, ToggleLeft, ToggleRight, ExternalLink, Loader2 } from 'lucide-react'
import { api } from '@/api/client'
import type { Plugin } from '@/api/types'

function InstallModal({ onClose, onInstalled }: { onClose: () => void; onInstalled: (p: Plugin) => void }) {
  const [url, setUrl] = useState('')
  const [loading, setLoading] = useState(false)
  const [err, setErr] = useState('')

  const submit = async () => {
    if (!url.trim()) { setErr('URL is required'); return }
    setLoading(true); setErr('')
    try {
      const plugin = await api.plugins.install(url.trim())
      onInstalled(plugin)
    } catch (e: any) {
      setErr(e.message || 'Install failed')
    } finally {
      setLoading(false)
    }
  }

  return (
    <div
      style={{ position: 'fixed', inset: 0, background: 'rgba(0,0,0,0.5)', display: 'flex', alignItems: 'center', justifyContent: 'center', zIndex: 1000 }}
      onClick={e => e.target === e.currentTarget && onClose()}
    >
      <div style={{ background: 'var(--bg-surface)', border: '1px solid var(--border)', borderRadius: 12, padding: 24, width: 460, display: 'flex', flexDirection: 'column', gap: 16 }}>
        <div style={{ fontWeight: 600, fontSize: 15 }}>Install plugin</div>
        <div style={{ fontSize: 12, color: 'var(--text-muted)', lineHeight: 1.5 }}>
          Provide a direct URL to a <code>.zip</code> file containing a <code>plugin.json</code> manifest.
          The zip may have files at its root or inside a single top-level directory.
        </div>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
          <label style={{ fontSize: 11, color: 'var(--text-muted)', fontWeight: 500 }}>Plugin URL</label>
          <input
            value={url}
            onChange={e => setUrl(e.target.value)}
            onKeyDown={e => e.key === 'Enter' && submit()}
            placeholder="https://example.com/my-plugin.zip"
            autoFocus
            style={{ background: 'var(--bg-base)', border: '1px solid var(--border)', borderRadius: 6, color: 'var(--text-primary)', padding: '6px 10px', fontSize: 13 }}
          />
        </div>
        {err && <div style={{ color: 'var(--accent-danger)', fontSize: 12 }}>{err}</div>}
        <div style={{ display: 'flex', gap: 8, justifyContent: 'flex-end' }}>
          <button onClick={onClose} style={{ padding: '6px 14px', borderRadius: 6, border: '1px solid var(--border)', background: 'transparent', color: 'var(--text-primary)', cursor: 'pointer', fontSize: 13 }}>Cancel</button>
          <button onClick={submit} disabled={loading} style={{ padding: '6px 14px', borderRadius: 6, border: 'none', background: 'var(--accent-primary)', color: '#fff', cursor: 'pointer', fontSize: 13, opacity: loading ? 0.7 : 1, display: 'flex', alignItems: 'center', gap: 6 }}>
            {loading && <Loader2 size={13} className="animate-spin" />}
            {loading ? 'Installing…' : 'Install'}
          </button>
        </div>
      </div>
    </div>
  )
}

export default function PluginsPage() {
  const [plugins, setPlugins] = useState<Plugin[]>([])
  const [loading, setLoading] = useState(true)
  const [showInstall, setShowInstall] = useState(false)

  const load = useCallback(async () => {
    try { setPlugins(await api.plugins.list()) }
    catch { /* ignore */ }
    finally { setLoading(false) }
  }, [])

  useEffect(() => { load() }, [load])

  const toggle = async (p: Plugin) => {
    await api.plugins.update(p.id, { enabled: !p.enabled })
    load()
  }

  const remove = async (p: Plugin) => {
    if (!confirm(`Uninstall plugin "${p.name}"? This will delete all plugin files.`)) return
    await api.plugins.uninstall(p.id)
    load()
  }

  return (
    <div style={{ padding: '24px 28px', maxWidth: 860, display: 'flex', flexDirection: 'column', gap: 20 }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
        <Blocks size={20} style={{ color: 'var(--accent-primary)' }} />
        <div>
          <h1 style={{ fontSize: 18, fontWeight: 600, margin: 0 }}>Plugins</h1>
          <p style={{ fontSize: 12, color: 'var(--text-muted)', margin: 0 }}>
            Standalone extensions that add new pages and tools to VoidTower.
          </p>
        </div>
        <button
          onClick={() => setShowInstall(true)}
          style={{ marginLeft: 'auto', display: 'flex', alignItems: 'center', gap: 6, padding: '6px 14px', borderRadius: 7, border: 'none', background: 'var(--accent-primary)', color: '#fff', cursor: 'pointer', fontSize: 13 }}
        >
          <Plus size={14} /> Install plugin
        </button>
      </div>

      <div style={{ padding: '10px 14px', borderRadius: 8, fontSize: 12, lineHeight: 1.5, background: 'var(--accent-primary)11', border: '1px solid var(--accent-primary)33', color: 'var(--text-secondary)' }}>
        Plugins are served as iframes from <code>/plugin-assets/{'{id}'}/*</code>. They run in the same browser session and can call <code>/api/*</code> endpoints directly. Each plugin must include a <code>plugin.json</code> manifest.
      </div>

      {loading ? (
        <div style={{ color: 'var(--text-muted)', fontSize: 13 }}>Loading…</div>
      ) : plugins.length === 0 ? (
        <div style={{ textAlign: 'center', padding: 48, color: 'var(--text-muted)', fontSize: 13, border: '1px dashed var(--border)', borderRadius: 8 }}>
          No plugins installed. Install one from a zip URL to get started.
        </div>
      ) : (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
          {plugins.map(p => (
            <div key={p.id} style={{ display: 'flex', alignItems: 'center', gap: 12, padding: '14px 16px', borderRadius: 10, background: 'var(--bg-surface)', border: '1px solid var(--border)', opacity: p.enabled ? 1 : 0.55 }}>
              <Blocks size={18} style={{ color: 'var(--accent-primary)', flexShrink: 0 }} />
              <div style={{ flex: 1, minWidth: 0 }}>
                <div style={{ fontSize: 14, fontWeight: 600 }}>{p.name}</div>
                <div style={{ fontSize: 12, color: 'var(--text-muted)' }}>
                  v{p.version}
                  {p.author && ` · ${p.author}`}
                  {p.nav_group && ` · ${p.nav_group}`}
                </div>
                {p.description && (
                  <div style={{ fontSize: 12, color: 'var(--text-secondary)', marginTop: 2 }}>{p.description}</div>
                )}
              </div>
              <a
                href={`/plugins/${p.id}`}
                style={{ display: 'flex', alignItems: 'center', gap: 4, fontSize: 12, color: 'var(--accent-primary)', textDecoration: 'none' }}
              >
                <ExternalLink size={13} /> Open
              </a>
              <button onClick={() => toggle(p)} title={p.enabled ? 'Disable' : 'Enable'} style={{ background: 'none', border: 'none', cursor: 'pointer', padding: 4, color: p.enabled ? 'var(--accent-success)' : 'var(--text-muted)' }}>
                {p.enabled ? <ToggleRight size={20} /> : <ToggleLeft size={20} />}
              </button>
              <button onClick={() => remove(p)} title="Uninstall" style={{ background: 'none', border: 'none', cursor: 'pointer', padding: 4, color: 'var(--text-muted)' }}>
                <Trash2 size={15} />
              </button>
            </div>
          ))}
        </div>
      )}

      <div style={{ padding: '14px 16px', borderRadius: 8, background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)' }}>
        <div style={{ fontSize: 12, fontWeight: 600, marginBottom: 8 }}>plugin.json format</div>
        <pre style={{ fontSize: 11, color: 'var(--text-secondary)', margin: 0, whiteSpace: 'pre-wrap', fontFamily: 'monospace' }}>{`{
  "id": "my-plugin",          // unique slug, no slashes
  "name": "My Plugin",
  "version": "1.0.0",
  "description": "What it does",
  "author": "Your Name",      // optional
  "entry": "index.html",      // default: index.html
  "icon": "LayoutDashboard",  // optional Lucide icon name
  "nav_group": "Tools"        // optional sidebar group label
}`}</pre>
      </div>

      {showInstall && (
        <InstallModal
          onClose={() => setShowInstall(false)}
          onInstalled={p => { setPlugins(prev => [...prev.filter(x => x.id !== p.id), p]); setShowInstall(false) }}
        />
      )}
    </div>
  )
}
