import { useEffect, useRef, useState } from 'react'
import { Pencil, Trash2, Plus, ChevronUp, ChevronDown, ExternalLink, Download, Upload } from 'lucide-react'
import NativePanelShell, { NativeRow, IconBtn, EmptyState, LoadingState } from './NativePanelShell'
import { useCustomTabs } from '@/hooks/useCustomTabs'
import type { CustomTab, CustomTabKind, ExportedTab } from '@/api/types'

function downloadJson(filename: string, data: unknown) {
  const blob = new Blob([JSON.stringify(data, null, 2)], { type: 'application/json' })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = filename
  a.click()
  URL.revokeObjectURL(url)
}

type Modal = { type: 'new' } | { type: 'edit'; item: CustomTab }
const empty = { title: '', icon: '', kind: 'iframe' as CustomTabKind, url: '', content: '' }

function configToForm(tab: CustomTab) {
  return {
    title: tab.title,
    icon: tab.icon ?? '',
    kind: tab.kind,
    url: typeof tab.config.url === 'string' ? tab.config.url : '',
    content: typeof tab.config.content === 'string' ? tab.config.content : '',
  }
}

export default function NativeTabsPanel() {
  const { tabs, loading, create, update, remove, reorder, exportTabs, importTabs } = useCustomTabs()
  const [modal, setModal] = useState<Modal | null>(null)
  const [form, setForm] = useState(empty)
  const [expanded, setExpanded] = useState<string | null>(null)
  const fileInputRef = useRef<HTMLInputElement>(null)

  async function handleExport() {
    const data = await exportTabs()
    downloadJson('voidtower-tabs.json', data)
  }

  async function handleImportFile(e: React.ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0]
    e.target.value = ''
    if (!file) return
    const text = await file.text()
    const data = JSON.parse(text) as ExportedTab[]
    await importTabs(data)
  }

  useEffect(() => {
    if (!modal) return
    const h = (e: KeyboardEvent) => { if (e.key === 'Escape') setModal(null) }
    window.addEventListener('keydown', h)
    return () => window.removeEventListener('keydown', h)
  }, [modal])

  function buildConfig() {
    if (form.kind === 'iframe') return { url: form.url, sandbox: 'allow-scripts allow-same-origin allow-forms' }
    if (form.kind === 'markdown') return { content: form.content }
    return {}
  }

  async function submit() {
    if (modal?.type === 'edit') {
      await update(modal.item.id, { title: form.title, icon: form.icon || null, config: buildConfig() })
    } else {
      await create({ title: form.title, icon: form.icon || null, kind: form.kind, config: buildConfig() })
    }
    setModal(null)
  }

  function move(id: string, dir: -1 | 1) {
    const idx = tabs.findIndex(t => t.id === id)
    const swapWith = idx + dir
    if (swapWith < 0 || swapWith >= tabs.length) return
    const ids = tabs.map(t => t.id)
    ;[ids[idx], ids[swapWith]] = [ids[swapWith], ids[idx]]
    reorder(ids)
  }

  return (
    <NativePanelShell actions={
      <>
        <IconBtn title="Export tabs" onClick={handleExport}><Download size={12} /></IconBtn>
        <IconBtn title="Import tabs" onClick={() => fileInputRef.current?.click()}><Upload size={12} /></IconBtn>
        <input ref={fileInputRef} type="file" accept="application/json" onChange={handleImportFile} style={{ display: 'none' }} />
        <IconBtn title="New tab" onClick={() => { setForm(empty); setModal({ type: 'new' }) }}><Plus size={12} /></IconBtn>
      </>
    }>
      {loading ? <LoadingState /> : tabs.length === 0 ? <EmptyState text="No custom tabs" /> :
        tabs.map((t, i) => (
          <div key={t.id}>
            <NativeRow>
              <div style={{ flex: 1, minWidth: 0 }}>
                <div style={{ fontSize: 11, color: 'var(--text-primary)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{t.title}</div>
                <div style={{ fontSize: 9, color: 'var(--text-muted)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                  {t.kind}{typeof t.config.url === 'string' ? ` · ${t.config.url}` : ''}
                </div>
              </div>
              <IconBtn title="Move up" onClick={() => move(t.id, -1)} disabled={i === 0}><ChevronUp size={11} /></IconBtn>
              <IconBtn title="Move down" onClick={() => move(t.id, 1)} disabled={i === tabs.length - 1}><ChevronDown size={11} /></IconBtn>
              {t.kind === 'iframe' && (
                <IconBtn title="Preview" onClick={() => setExpanded(expanded === t.id ? null : t.id)}><ExternalLink size={11} /></IconBtn>
              )}
              <IconBtn title="Edit" onClick={() => { setForm(configToForm(t)); setModal({ type: 'edit', item: t }) }}><Pencil size={11} /></IconBtn>
              <IconBtn title="Remove" onClick={() => remove(t.id)} danger><Trash2 size={11} /></IconBtn>
            </NativeRow>
            {expanded === t.id && t.kind === 'iframe' && typeof t.config.url === 'string' && (
              <div style={{ height: 360, borderBottom: '1px solid var(--border-subtle)' }}>
                <iframe
                  src={t.config.url}
                  title={t.title}
                  sandbox="allow-scripts allow-same-origin allow-forms"
                  style={{ width: '100%', height: '100%', border: 'none' }}
                />
              </div>
            )}
            {expanded === t.id && t.kind === 'markdown' && (
              <div style={{ padding: '8px 12px', fontSize: 11, color: 'var(--text-primary)', whiteSpace: 'pre-wrap', borderBottom: '1px solid var(--border-subtle)' }}>
                {typeof t.config.content === 'string' ? t.config.content : ''}
              </div>
            )}
          </div>
        ))
      }
      {modal && (
        <div style={{ position: 'fixed', inset: 0, background: 'rgba(0,0,0,0.5)', zIndex: 50, display: 'flex', alignItems: 'center', justifyContent: 'center' }}
          onClick={() => setModal(null)}>
          <div style={{ background: 'var(--bg-panel)', borderRadius: 8, padding: 16, width: 340, border: '1px solid var(--border-subtle)' }}
            onClick={e => e.stopPropagation()}>
            <div style={{ fontSize: 11, fontWeight: 600, color: 'var(--text-primary)', marginBottom: 10 }}>{modal.type === 'new' ? 'New Tab' : 'Edit Tab'}</div>
            <div style={{ marginBottom: 8 }}>
              <div style={{ fontSize: 10, color: 'var(--text-muted)', marginBottom: 3 }}>Title</div>
              <input value={form.title} onChange={e => setForm(p => ({ ...p, title: e.target.value }))}
                style={{ width: '100%', boxSizing: 'border-box', background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)', borderRadius: 4, padding: '4px 6px', fontSize: 11, color: 'var(--text-primary)', outline: 'none' }} />
            </div>
            {modal.type === 'new' && (
              <div style={{ marginBottom: 8 }}>
                <div style={{ fontSize: 10, color: 'var(--text-muted)', marginBottom: 3 }}>Kind</div>
                <select value={form.kind} onChange={e => setForm(p => ({ ...p, kind: e.target.value as CustomTabKind }))}
                  style={{ width: '100%', boxSizing: 'border-box', background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)', borderRadius: 4, padding: '4px 6px', fontSize: 11, color: 'var(--text-primary)', outline: 'none' }}>
                  <option value="iframe">Iframe (embed a URL)</option>
                  <option value="markdown">Markdown (static notes)</option>
                </select>
              </div>
            )}
            {form.kind === 'iframe' && (
              <div style={{ marginBottom: 8 }}>
                <div style={{ fontSize: 10, color: 'var(--text-muted)', marginBottom: 3 }}>URL</div>
                <input value={form.url} onChange={e => setForm(p => ({ ...p, url: e.target.value }))} placeholder="https://example.com"
                  style={{ width: '100%', boxSizing: 'border-box', background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)', borderRadius: 4, padding: '4px 6px', fontSize: 11, color: 'var(--text-primary)', outline: 'none' }} />
              </div>
            )}
            {form.kind === 'markdown' && (
              <div style={{ marginBottom: 8 }}>
                <div style={{ fontSize: 10, color: 'var(--text-muted)', marginBottom: 3 }}>Content</div>
                <textarea value={form.content} onChange={e => setForm(p => ({ ...p, content: e.target.value }))} rows={5}
                  style={{ width: '100%', boxSizing: 'border-box', background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)', borderRadius: 4, padding: '4px 6px', fontSize: 11, color: 'var(--text-primary)', outline: 'none', resize: 'vertical', fontFamily: 'monospace' }} />
              </div>
            )}
            <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 6, marginTop: 12 }}>
              <button onClick={() => setModal(null)} style={{ fontSize: 11, padding: '4px 10px', borderRadius: 4, border: '1px solid var(--border-subtle)', background: 'none', color: 'var(--text-muted)', cursor: 'pointer' }}>Cancel</button>
              <button onClick={submit} style={{ fontSize: 11, padding: '4px 10px', borderRadius: 4, border: 'none', background: 'var(--accent-primary)', color: '#fff', cursor: 'pointer' }}>Save</button>
            </div>
          </div>
        </div>
      )}
    </NativePanelShell>
  )
}
