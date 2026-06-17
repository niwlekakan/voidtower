import { useState } from 'react'
import { LayoutPanelTop, Plus, Trash2, ChevronUp, ChevronDown, Pencil } from 'lucide-react'
import { useCustomTabs } from '@/hooks/useCustomTabs'
import type { CustomTab, CustomTabKind } from '@/api/types'
import { ICON_REGISTRY, ICON_NAMES } from '@/components/ui/iconRegistry'

const emptyForm = { title: '', icon: '', kind: 'iframe' as CustomTabKind, url: '', content: '' }

function formFromTab(tab: CustomTab) {
  return {
    title: tab.title,
    icon: tab.icon ?? '',
    kind: tab.kind,
    url: typeof tab.config.url === 'string' ? tab.config.url : '',
    content: typeof tab.config.content === 'string' ? tab.config.content : '',
  }
}

export default function CustomizationTabs() {
  const { tabs, loading, create, update, remove, reorder } = useCustomTabs()
  const [editingId, setEditingId] = useState<string | null>(null)
  const [creating, setCreating] = useState(false)
  const [form, setForm] = useState(emptyForm)
  const [iconPickerOpen, setIconPickerOpen] = useState(false)

  const startCreate = () => { setForm(emptyForm); setCreating(true); setEditingId(null) }
  const startEdit = (tab: CustomTab) => { setForm(formFromTab(tab)); setEditingId(tab.id); setCreating(false) }
  const cancel = () => { setCreating(false); setEditingId(null); setIconPickerOpen(false) }

  const buildConfig = () => {
    if (form.kind === 'iframe') return { url: form.url, sandbox: 'allow-scripts allow-same-origin allow-forms' }
    if (form.kind === 'markdown') return { content: form.content }
    return {}
  }

  const submit = async () => {
    if (!form.title.trim()) return
    if (editingId) {
      await update(editingId, { title: form.title.trim(), icon: form.icon || null, config: buildConfig() })
    } else {
      await create({ title: form.title.trim(), icon: form.icon || null, kind: form.kind, config: buildConfig() })
    }
    cancel()
  }

  const move = (id: string, dir: -1 | 1) => {
    const idx = tabs.findIndex(t => t.id === id)
    const swapWith = idx + dir
    if (swapWith < 0 || swapWith >= tabs.length) return
    const ids = tabs.map(t => t.id)
    ;[ids[idx], ids[swapWith]] = [ids[swapWith], ids[idx]]
    reorder(ids)
  }

  const formOpen = creating || editingId !== null

  return (
    <div className="card space-y-4">
      <div className="flex items-center gap-2">
        <LayoutPanelTop size={14} style={{ color: 'var(--accent-primary)' }} />
        <h2 className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>My Tabs</h2>
      </div>
      <p className="text-xs" style={{ color: 'var(--text-muted)' }}>
        Personal nav entries that only you see — embed a URL in an iframe, or add static markdown notes.
      </p>

      {loading ? (
        <div className="text-xs" style={{ color: 'var(--text-muted)' }}>Loading…</div>
      ) : tabs.length === 0 && !formOpen ? (
        <div className="text-xs" style={{ color: 'var(--text-muted)' }}>No custom tabs yet.</div>
      ) : (
        <div className="space-y-1">
          {tabs.map((tab, i) => {
            const Icon = (tab.icon && ICON_REGISTRY[tab.icon]) || LayoutPanelTop
            return (
              <div key={tab.id} className="flex items-center gap-2 px-2 py-1.5 rounded"
                style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)' }}>
                <Icon size={14} style={{ color: 'var(--text-secondary)', flexShrink: 0 }} />
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div className="text-xs truncate" style={{ color: 'var(--text-primary)' }}>{tab.title}</div>
                  <div className="text-[10px] truncate" style={{ color: 'var(--text-muted)' }}>
                    {tab.kind}{typeof tab.config.url === 'string' ? ` · ${tab.config.url}` : ''}
                  </div>
                </div>
                <button onClick={() => move(tab.id, -1)} disabled={i === 0} title="Move up"
                  style={{ background: 'none', border: 'none', cursor: i === 0 ? 'default' : 'pointer', color: i === 0 ? 'var(--text-disabled)' : 'var(--text-muted)' }}>
                  <ChevronUp size={13} />
                </button>
                <button onClick={() => move(tab.id, 1)} disabled={i === tabs.length - 1} title="Move down"
                  style={{ background: 'none', border: 'none', cursor: i === tabs.length - 1 ? 'default' : 'pointer', color: i === tabs.length - 1 ? 'var(--text-disabled)' : 'var(--text-muted)' }}>
                  <ChevronDown size={13} />
                </button>
                <button onClick={() => startEdit(tab)} title="Edit"
                  style={{ background: 'none', border: 'none', cursor: 'pointer', color: 'var(--text-muted)' }}>
                  <Pencil size={13} />
                </button>
                <button onClick={() => remove(tab.id)} title="Delete"
                  style={{ background: 'none', border: 'none', cursor: 'pointer', color: 'var(--text-muted)' }}>
                  <Trash2 size={13} />
                </button>
              </div>
            )
          })}
        </div>
      )}

      {formOpen ? (
        <div className="space-y-2 p-3 rounded" style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)' }}>
          <div>
            <div className="text-xs mb-1" style={{ color: 'var(--text-secondary)' }}>Title</div>
            <input value={form.title} onChange={e => setForm(p => ({ ...p, title: e.target.value }))}
              className="w-full px-2 py-1.5 rounded text-xs outline-none"
              style={{ background: 'var(--bg-root)', border: '1px solid var(--border-subtle)', color: 'var(--text-primary)' }} />
          </div>

          <div style={{ position: 'relative' }}>
            <div className="text-xs mb-1" style={{ color: 'var(--text-secondary)' }}>Icon</div>
            <button onClick={() => setIconPickerOpen(o => !o)}
              className="flex items-center gap-2 px-2 py-1.5 rounded text-xs"
              style={{ background: 'var(--bg-root)', border: '1px solid var(--border-subtle)', color: 'var(--text-primary)' }}>
              {(() => { const PickedIcon = (form.icon && ICON_REGISTRY[form.icon]) || LayoutPanelTop; return <PickedIcon size={13} /> })()}
              {form.icon || 'Default'}
            </button>
            {iconPickerOpen && (
              <div style={{
                position: 'absolute', top: '100%', left: 0, zIndex: 20, marginTop: 4, padding: 6, borderRadius: 6,
                background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)', boxShadow: '0 4px 12px rgba(0,0,0,0.25)',
                display: 'grid', gridTemplateColumns: 'repeat(7, 1fr)', gap: 4, width: 196,
              }}>
                {ICON_NAMES.map(name => {
                  const OptIcon = ICON_REGISTRY[name]
                  return (
                    <button key={name} onClick={() => { setForm(p => ({ ...p, icon: name })); setIconPickerOpen(false) }} title={name}
                      style={{
                        display: 'flex', alignItems: 'center', justifyContent: 'center', padding: 4, borderRadius: 4, cursor: 'pointer',
                        background: form.icon === name ? 'var(--accent-primary-subtle)' : 'transparent',
                        border: `1px solid ${form.icon === name ? 'var(--accent-primary)' : 'transparent'}`,
                      }}>
                      <OptIcon size={13} style={{ color: 'var(--text-secondary)' }} />
                    </button>
                  )
                })}
              </div>
            )}
          </div>

          {creating && (
            <div>
              <div className="text-xs mb-1" style={{ color: 'var(--text-secondary)' }}>Kind</div>
              <select value={form.kind} onChange={e => setForm(p => ({ ...p, kind: e.target.value as CustomTabKind }))}
                className="w-full px-2 py-1.5 rounded text-xs outline-none"
                style={{ background: 'var(--bg-root)', border: '1px solid var(--border-subtle)', color: 'var(--text-primary)' }}>
                <option value="iframe">Iframe (embed a URL)</option>
                <option value="markdown">Markdown (static notes)</option>
              </select>
            </div>
          )}

          {form.kind === 'iframe' && (
            <div>
              <div className="text-xs mb-1" style={{ color: 'var(--text-secondary)' }}>URL</div>
              <input value={form.url} onChange={e => setForm(p => ({ ...p, url: e.target.value }))} placeholder="https://example.com"
                className="w-full px-2 py-1.5 rounded text-xs outline-none"
                style={{ background: 'var(--bg-root)', border: '1px solid var(--border-subtle)', color: 'var(--text-primary)' }} />
            </div>
          )}

          {form.kind === 'markdown' && (
            <div>
              <div className="text-xs mb-1" style={{ color: 'var(--text-secondary)' }}>Content</div>
              <textarea value={form.content} onChange={e => setForm(p => ({ ...p, content: e.target.value }))} rows={5}
                className="w-full px-2 py-1.5 rounded text-xs outline-none font-mono"
                style={{ background: 'var(--bg-root)', border: '1px solid var(--border-subtle)', color: 'var(--text-primary)', resize: 'vertical' }} />
            </div>
          )}

          <div className="flex gap-2 justify-end pt-1">
            <button onClick={cancel} className="px-3 py-1.5 rounded text-xs transition-colors hover:opacity-80"
              style={{ color: 'var(--text-muted)' }}>Cancel</button>
            <button onClick={submit} className="px-3 py-1.5 rounded text-xs font-medium transition-colors"
              style={{ background: 'var(--accent-primary)', color: '#fff' }}>Save</button>
          </div>
        </div>
      ) : (
        <button onClick={startCreate}
          className="px-3 py-1.5 rounded text-xs transition-colors hover:opacity-80 flex items-center gap-1.5"
          style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)', color: 'var(--text-secondary)' }}>
          <Plus size={13} /> New tab
        </button>
      )}
    </div>
  )
}
