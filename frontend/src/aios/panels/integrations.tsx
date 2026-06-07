import { useEffect, useState } from 'react'
import { Trash2, Plus, AlertTriangle } from 'lucide-react'
import NativePanelShell, { NativeRow, StatusDot, IconBtn, EmptyState, LoadingState } from './NativePanelShell'

interface Token { id: string; name: string; scope?: string; last_used?: string; expires_at?: string }
interface OdyConfig { enabled: boolean; mcp_enabled: boolean; allowed_url?: string; emergency_disabled?: boolean }

function rel(ts?: string) {
  if (!ts) return 'never'
  const d = Date.now() - new Date(ts).getTime()
  if (d < 3600000) return `${Math.floor(d / 60000)}m ago`
  if (d < 86400000) return `${Math.floor(d / 3600000)}h ago`
  return `${Math.floor(d / 86400000)}d ago`
}

const TABS = [{ id: 'tokens', label: 'Tokens' }, { id: 'odysseus', label: 'Odysseus' }]

export default function NativeIntegrationsPanel() {
  const [tab, setTab] = useState('tokens')
  const [tokens, setTokens] = useState<Token[]>([])
  const [ody, setOdy] = useState<OdyConfig | null>(null)
  const [loading, setLoading] = useState(true)
  const [modal, setModal] = useState(false)
  const [form, setForm] = useState({ name: '', scope: '' })

  async function loadTokens() {
    const r = await fetch('/api/integrations/tokens', { credentials: 'include' })
    if (r.ok) { const d = await r.json(); setTokens(d.tokens ?? []) }
  }
  async function loadOdy() {
    const r = await fetch('/api/integrations/odysseus/config', { credentials: 'include' })
    if (r.ok) { const d = await r.json(); setOdy(d) }
  }
  async function load() {
    await Promise.all([loadTokens(), loadOdy()])
    setLoading(false)
  }
  async function revoke(id: string) {
    await fetch(`/api/integrations/tokens/${id}`, { method: 'DELETE', credentials: 'include' })
    loadTokens()
  }
  async function submitToken() {
    const body: Record<string, string> = { name: form.name }
    if (form.scope) body.scope = form.scope
    await fetch('/api/integrations/tokens', { method: 'POST', credentials: 'include', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) })
    setModal(false); loadTokens()
  }
  async function patchOdy(patch: Partial<OdyConfig>) {
    await fetch('/api/integrations/odysseus/config', { method: 'PATCH', credentials: 'include', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(patch) })
    loadOdy()
  }
  async function emergencyDisable() {
    await fetch('/api/integrations/odysseus/config', { method: 'POST', credentials: 'include', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ emergency_disable: true }) })
    loadOdy()
  }

  useEffect(() => { load() }, [])
  useEffect(() => {
    if (!modal) return
    const h = (e: KeyboardEvent) => { if (e.key === 'Escape') setModal(false) }
    window.addEventListener('keydown', h)
    return () => window.removeEventListener('keydown', h)
  }, [modal])

  function isExpired(t: Token) { return t.expires_at ? new Date(t.expires_at) < new Date() : false }

  return (
    <NativePanelShell tabs={TABS} activeTab={tab} onTabChange={setTab} actions={
      tab === 'tokens'
        ? <IconBtn title="New token" onClick={() => { setForm({ name: '', scope: '' }); setModal(true) }}><Plus size={12} /></IconBtn>
        : undefined
    }>
      {loading ? <LoadingState /> : tab === 'tokens' ? (
        tokens.length === 0 ? <EmptyState text="No API tokens" /> :
          tokens.map(t => (
            <NativeRow key={t.id}>
              <StatusDot color={isExpired(t) ? '#ef4444' : '#22c55e'} />
              <div style={{ flex: 1, minWidth: 0 }}>
                <div style={{ fontSize: 11, color: 'var(--text-primary)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{t.name}</div>
                <div style={{ fontSize: 9, color: 'var(--text-muted)' }}>
                  {t.scope ?? 'full'} · expires {t.expires_at ? rel(t.expires_at) : 'never'}{t.last_used ? ` · used ${rel(t.last_used)}` : ''}
                  {isExpired(t) ? ' · expired' : ''}
                </div>
              </div>
              <IconBtn title="Revoke token" onClick={() => revoke(t.id)} danger><Trash2 size={11} /></IconBtn>
            </NativeRow>
          ))
      ) : (
        ody == null ? <EmptyState text="Odysseus config unavailable" /> : <>
          <NativeRow>
            <StatusDot color={ody.enabled ? '#22c55e' : '#94a3b8'} />
            <div style={{ flex: 1, fontSize: 11, color: 'var(--text-primary)' }}>Odysseus {ody.enabled ? 'Enabled' : 'Disabled'}</div>
            <button onClick={() => patchOdy({ enabled: !ody.enabled })} style={{ fontSize: 10, padding: '2px 8px', borderRadius: 4, border: '1px solid var(--border-subtle)', background: 'none', color: 'var(--text-muted)', cursor: 'pointer' }}>
              {ody.enabled ? 'Disable' : 'Enable'}
            </button>
          </NativeRow>
          <NativeRow>
            <StatusDot color={ody.mcp_enabled ? '#22c55e' : '#94a3b8'} />
            <div style={{ flex: 1, fontSize: 11, color: 'var(--text-primary)' }}>MCP {ody.mcp_enabled ? 'Enabled' : 'Disabled'}</div>
            <button onClick={() => patchOdy({ mcp_enabled: !ody.mcp_enabled })} style={{ fontSize: 10, padding: '2px 8px', borderRadius: 4, border: '1px solid var(--border-subtle)', background: 'none', color: 'var(--text-muted)', cursor: 'pointer' }}>
              {ody.mcp_enabled ? 'Disable' : 'Enable'}
            </button>
          </NativeRow>
          {ody.allowed_url && (
            <NativeRow>
              <div style={{ flex: 1, minWidth: 0 }}>
                <div style={{ fontSize: 10, color: 'var(--text-muted)', marginBottom: 1 }}>URL</div>
                <div style={{ fontSize: 9, color: 'var(--text-secondary)', fontFamily: 'monospace', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{ody.allowed_url}</div>
              </div>
            </NativeRow>
          )}
          <NativeRow>
            <IconBtn title="Emergency disable Odysseus" onClick={emergencyDisable} danger><AlertTriangle size={11} /></IconBtn>
            <span style={{ fontSize: 10, color: 'var(--text-muted)' }}>Emergency disable</span>
          </NativeRow>
        </>
      )}
      {modal && (
        <div style={{ position: 'fixed', inset: 0, background: 'rgba(0,0,0,0.5)', zIndex: 50, display: 'flex', alignItems: 'center', justifyContent: 'center' }}
          onClick={() => setModal(false)}>
          <div style={{ background: 'var(--bg-panel)', borderRadius: 8, padding: 16, width: 300, border: '1px solid var(--border-subtle)' }}
            onClick={e => e.stopPropagation()}>
            <div style={{ fontSize: 11, fontWeight: 600, color: 'var(--text-primary)', marginBottom: 10 }}>New Token</div>
            {([{ label: 'Name', key: 'name' }, { label: 'Scope (optional)', key: 'scope' }] as { label: string; key: 'name' | 'scope' }[]).map(({ label, key }) => (
              <div key={key} style={{ marginBottom: 8 }}>
                <div style={{ fontSize: 10, color: 'var(--text-muted)', marginBottom: 3 }}>{label}</div>
                <input value={form[key]} onChange={e => setForm(p => ({ ...p, [key]: e.target.value }))}
                  style={{ width: '100%', boxSizing: 'border-box', background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)', borderRadius: 4, padding: '4px 6px', fontSize: 11, color: 'var(--text-primary)', outline: 'none' }} />
              </div>
            ))}
            <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 6, marginTop: 12 }}>
              <button onClick={() => setModal(false)} style={{ fontSize: 11, padding: '4px 10px', borderRadius: 4, border: '1px solid var(--border-subtle)', background: 'none', color: 'var(--text-muted)', cursor: 'pointer' }}>Cancel</button>
              <button onClick={submitToken} style={{ fontSize: 11, padding: '4px 10px', borderRadius: 4, border: 'none', background: 'var(--accent-primary)', color: '#fff', cursor: 'pointer' }}>Create</button>
            </div>
          </div>
        </div>
      )}
    </NativePanelShell>
  )
}
