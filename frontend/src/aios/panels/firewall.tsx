import { useEffect, useState } from 'react'
import { Trash2, Play, Plus, ShieldCheck, ShieldOff } from 'lucide-react'
import NativePanelShell, { NativeRow, StatusDot, IconBtn, EmptyState, LoadingState } from './NativePanelShell'

interface FirewallRule { id: string; chain: string; protocol: string; port?: string; action: string; enabled: boolean; from?: string }
interface FwStatus { status: 'active' | 'inactive' | 'unknown'; rules: FirewallRule[] }
const emptyForm = { direction: 'in', action: 'allow', protocol: 'tcp', port: '' }

export default function NativeFirewallPanel() {
  const [status, setStatus] = useState<FwStatus | null>(null)
  const [loading, setLoading] = useState(true)
  const [modal, setModal] = useState(false)
  const [form, setForm] = useState(emptyForm)

  async function load() {
    const r = await fetch('/api/firewall', { credentials: 'include' })
    if (r.ok) { const d = await r.json(); setStatus(d) }
    setLoading(false)
  }
  async function deleteRule(id: string) {
    await fetch('/api/firewall/rules/delete', { method: 'POST', credentials: 'include', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ id }) })
    load()
  }
  async function runAction(action: string) {
    await fetch('/api/firewall/action', { method: 'POST', credentials: 'include', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ action }) })
    load()
  }
  async function submit() {
    const body: Record<string, string> = { direction: form.direction, action: form.action, protocol: form.protocol }
    if (form.port) body.port = form.port
    await fetch('/api/firewall', { method: 'POST', credentials: 'include', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) })
    setModal(false); load()
  }

  useEffect(() => { load() }, [])
  useEffect(() => {
    if (!modal) return
    const h = (e: KeyboardEvent) => { if (e.key === 'Escape') setModal(false) }
    window.addEventListener('keydown', h)
    return () => window.removeEventListener('keydown', h)
  }, [modal])

  const active = status?.status === 'active'
  const rules = status?.rules ?? []

  return (
    <NativePanelShell actions={<>
      <IconBtn title="Reload firewall" onClick={() => runAction('reload')}><Play size={11} /></IconBtn>
      <IconBtn title="New rule" onClick={() => { setForm(emptyForm); setModal(true) }}><Plus size={12} /></IconBtn>
    </>}>
      {loading ? <LoadingState /> : <>
        <NativeRow style={{ background: 'var(--bg-elevated)' }}>
          <StatusDot color={active ? '#22c55e' : status?.status === 'inactive' ? '#ef4444' : '#94a3b8'} />
          <div style={{ flex: 1, fontSize: 11, color: 'var(--text-primary)' }}>UFW {active ? 'Active' : 'Inactive'}</div>
          <IconBtn title={active ? 'Disable firewall' : 'Enable firewall'} onClick={() => runAction(active ? 'disable' : 'enable')}>
            {active ? <ShieldCheck size={13} /> : <ShieldOff size={13} />}
          </IconBtn>
        </NativeRow>
        {rules.length === 0 ? <EmptyState text="No rules" /> :
          rules.map(rule => (
            <NativeRow key={rule.id}>
              <StatusDot color={rule.action === 'ACCEPT' ? '#22c55e' : rule.action === 'DROP' ? '#ef4444' : '#f59e0b'} />
              <div style={{ flex: 1, minWidth: 0 }}>
                <div style={{ fontSize: 11, color: 'var(--text-primary)' }}>{rule.chain} {rule.protocol}{rule.port ? `:${rule.port}` : ''} · {rule.action}</div>
                {rule.from && <div style={{ fontSize: 9, color: 'var(--text-muted)' }}>from {rule.from}</div>}
              </div>
              <IconBtn title="Delete rule" onClick={() => deleteRule(rule.id)} danger><Trash2 size={11} /></IconBtn>
            </NativeRow>
          ))
        }
      </>}
      {modal && (
        <div style={{ position: 'fixed', inset: 0, background: 'rgba(0,0,0,0.5)', zIndex: 50, display: 'flex', alignItems: 'center', justifyContent: 'center' }}
          onClick={() => setModal(false)}>
          <div style={{ background: 'var(--bg-panel)', borderRadius: 8, padding: 16, width: 300, border: '1px solid var(--border-subtle)' }}
            onClick={e => e.stopPropagation()}>
            <div style={{ fontSize: 11, fontWeight: 600, color: 'var(--text-primary)', marginBottom: 10 }}>New Firewall Rule</div>
            {([
              { label: 'Direction', key: 'direction', opts: ['in', 'out', 'fwd'] },
              { label: 'Action', key: 'action', opts: ['allow', 'deny'] },
              { label: 'Protocol', key: 'protocol', opts: ['tcp', 'udp', 'any'] },
            ] as { label: string; key: keyof typeof form; opts: string[] }[]).map(({ label, key, opts }) => (
              <div key={key} style={{ marginBottom: 8 }}>
                <div style={{ fontSize: 10, color: 'var(--text-muted)', marginBottom: 3 }}>{label}</div>
                <select value={form[key]} onChange={e => setForm(p => ({ ...p, [key]: e.target.value }))}
                  style={{ width: '100%', background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)', borderRadius: 4, padding: '4px 6px', fontSize: 11, color: 'var(--text-primary)', outline: 'none' }}>
                  {opts.map(o => <option key={o} value={o}>{o}</option>)}
                </select>
              </div>
            ))}
            <div style={{ marginBottom: 8 }}>
              <div style={{ fontSize: 10, color: 'var(--text-muted)', marginBottom: 3 }}>Port (optional)</div>
              <input value={form.port} onChange={e => setForm(p => ({ ...p, port: e.target.value }))} placeholder="e.g. 8080"
                style={{ width: '100%', boxSizing: 'border-box', background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)', borderRadius: 4, padding: '4px 6px', fontSize: 11, color: 'var(--text-primary)', outline: 'none' }} />
            </div>
            <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 6, marginTop: 12 }}>
              <button onClick={() => setModal(false)} style={{ fontSize: 11, padding: '4px 10px', borderRadius: 4, border: '1px solid var(--border-subtle)', background: 'none', color: 'var(--text-muted)', cursor: 'pointer' }}>Cancel</button>
              <button onClick={submit} style={{ fontSize: 11, padding: '4px 10px', borderRadius: 4, border: 'none', background: 'var(--accent-primary)', color: '#fff', cursor: 'pointer' }}>Add</button>
            </div>
          </div>
        </div>
      )}
    </NativePanelShell>
  )
}
