import { useEffect, useRef, useState } from 'react'
import { Pencil, Trash2, Plus, Download, Upload } from 'lucide-react'
import NativePanelShell, { NativeRow, StatusDot, IconBtn, EmptyState, LoadingState } from './NativePanelShell'
import { useAgents } from '@/hooks/useAgents'
import { useAgentStatusStream } from '@/hooks/useAgentStatusStream'
import type { AgentState, AgentWithStatus, ExportedAgent } from '@/api/types'

function downloadJson(filename: string, data: unknown) {
  const blob = new Blob([JSON.stringify(data, null, 2)], { type: 'application/json' })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = filename
  a.click()
  URL.revokeObjectURL(url)
}

type Modal = { type: 'new' } | { type: 'edit'; item: AgentWithStatus }
const empty = { name: '', source: '', icon: '', color: '' }

const STATE_COLOR: Record<AgentState, string> = {
  working: '#22c55e',
  idle: '#94a3b8',
  error: '#ef4444',
  offline: '#475569',
}

export default function NativeAgentsPanel() {
  const { agents, loading, create, update, remove, exportAgents, importAgents } = useAgents()
  const { statuses } = useAgentStatusStream()
  const [modal, setModal] = useState<Modal | null>(null)
  const [form, setForm] = useState(empty)
  const fileInputRef = useRef<HTMLInputElement>(null)

  async function handleExport() {
    const data = await exportAgents()
    downloadJson('voidtower-agents.json', data)
  }

  async function handleImportFile(e: React.ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0]
    e.target.value = ''
    if (!file) return
    const text = await file.text()
    const data = JSON.parse(text) as ExportedAgent[]
    await importAgents(data)
  }

  useEffect(() => {
    if (!modal) return
    const h = (e: KeyboardEvent) => { if (e.key === 'Escape') setModal(null) }
    window.addEventListener('keydown', h)
    return () => window.removeEventListener('keydown', h)
  }, [modal])

  async function submit() {
    if (modal?.type === 'edit') {
      await update(modal.item.id, { name: form.name, icon: form.icon || null, color: form.color || null })
    } else {
      await create({ name: form.name, source: form.source, icon: form.icon || null, color: form.color || null })
    }
    setModal(null)
  }

  return (
    <NativePanelShell actions={
      <>
        <IconBtn title="Export agents" onClick={handleExport}><Download size={12} /></IconBtn>
        <IconBtn title="Import agents" onClick={() => fileInputRef.current?.click()}><Upload size={12} /></IconBtn>
        <input ref={fileInputRef} type="file" accept="application/json" onChange={handleImportFile} style={{ display: 'none' }} />
        <IconBtn title="New agent" onClick={() => { setForm(empty); setModal({ type: 'new' }) }}><Plus size={12} /></IconBtn>
      </>
    }>
      {loading ? <LoadingState /> : agents.length === 0 ? <EmptyState text="No agents configured" /> :
        agents.map(a => {
          const live = statuses[a.id]
          const state = live?.state ?? a.state
          const activity = live?.activity ?? a.activity
          return (
            <NativeRow key={a.id}>
              <StatusDot color={a.enabled ? STATE_COLOR[state] : '#334155'} />
              <div style={{ flex: 1, minWidth: 0 }}>
                <div style={{ fontSize: 11, color: 'var(--text-primary)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{a.name}</div>
                <div style={{ fontSize: 9, color: 'var(--text-muted)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                  {activity ? activity : `${a.source} · ${state}`}
                </div>
              </div>
              <label style={{ display: 'flex', alignItems: 'center', gap: 4, cursor: 'pointer', marginRight: 4 }} title="Enabled">
                <input type="checkbox" checked={a.enabled} onChange={e => update(a.id, { enabled: e.target.checked })} />
              </label>
              <IconBtn title="Edit" onClick={() => { setForm({ name: a.name, source: a.source, icon: a.icon ?? '', color: a.color ?? '' }); setModal({ type: 'edit', item: a }) }}><Pencil size={11} /></IconBtn>
              <IconBtn title="Remove" onClick={() => remove(a.id)} danger><Trash2 size={11} /></IconBtn>
            </NativeRow>
          )
        })
      }
      {modal && (
        <div style={{ position: 'fixed', inset: 0, background: 'rgba(0,0,0,0.5)', zIndex: 50, display: 'flex', alignItems: 'center', justifyContent: 'center' }}
          onClick={() => setModal(null)}>
          <div style={{ background: 'var(--bg-panel)', borderRadius: 8, padding: 16, width: 300, border: '1px solid var(--border-subtle)' }}
            onClick={e => e.stopPropagation()}>
            <div style={{ fontSize: 11, fontWeight: 600, color: 'var(--text-primary)', marginBottom: 10 }}>{modal.type === 'new' ? 'New Agent' : 'Edit Agent'}</div>
            <div style={{ marginBottom: 8 }}>
              <div style={{ fontSize: 10, color: 'var(--text-muted)', marginBottom: 3 }}>Display Name</div>
              <input value={form.name} onChange={e => setForm(p => ({ ...p, name: e.target.value }))}
                style={{ width: '100%', boxSizing: 'border-box', background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)', borderRadius: 4, padding: '4px 6px', fontSize: 11, color: 'var(--text-primary)', outline: 'none' }} />
            </div>
            {modal.type === 'new' && (
              <div style={{ marginBottom: 8 }}>
                <div style={{ fontSize: 10, color: 'var(--text-muted)', marginBottom: 3 }}>Source</div>
                <input value={form.source} onChange={e => setForm(p => ({ ...p, source: e.target.value }))} placeholder="e.g. odysseus, mcp:my-server"
                  style={{ width: '100%', boxSizing: 'border-box', background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)', borderRadius: 4, padding: '4px 6px', fontSize: 11, color: 'var(--text-primary)', outline: 'none' }} />
              </div>
            )}
            <div style={{ marginBottom: 8 }}>
              <div style={{ fontSize: 10, color: 'var(--text-muted)', marginBottom: 3 }}>Color (optional)</div>
              <input value={form.color} onChange={e => setForm(p => ({ ...p, color: e.target.value }))} placeholder="#8b5cf6"
                style={{ width: '100%', boxSizing: 'border-box', background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)', borderRadius: 4, padding: '4px 6px', fontSize: 11, color: 'var(--text-primary)', outline: 'none' }} />
            </div>
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
