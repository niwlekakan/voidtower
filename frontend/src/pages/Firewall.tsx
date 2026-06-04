import { useEffect, useState } from 'react'
import { ShieldCheck, ShieldOff, Plus, Trash2, RefreshCw, X, AlertTriangle } from 'lucide-react'
import { notify } from '@/store/notifications'

async function apiFetch<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(path, { ...init, credentials: 'include', headers: { 'Content-Type': 'application/json', ...init?.headers } })
  if (!res.ok) throw new Error((await res.json().catch(() => ({}))).error?.message ?? res.statusText)
  return res.json()
}

interface FirewallRule { num: number; to: string; action: string; from: string; ipv6: boolean }
interface FirewallStatus { backend: string; enabled: boolean; rules: FirewallRule[]; logging: string | null; error: string | null }

const ACTION_COLOR: Record<string, string> = {
  ALLOW: 'var(--accent-success)', DENY: 'var(--accent-error)',
  LIMIT: 'var(--accent-warning)', REJECT: 'var(--accent-error)',
}

function AddRuleModal({ onClose, onSaved }: { onClose: () => void; onSaved: () => void }) {
  const [action, setAction] = useState('allow')
  const [port, setPort]     = useState('')
  const [proto, setProto]   = useState('tcp')
  const [from, setFrom]     = useState('')
  const [dir, setDir]       = useState('in')
  const [busy, setBusy]     = useState(false)

  const submit = async () => {
    if (!port && !from) return
    setBusy(true)
    try {
      await apiFetch('/api/firewall/rules', { method: 'POST', body: JSON.stringify({
        action, port: port || undefined, proto: proto !== 'any' ? proto : undefined,
        from: from || undefined, direction: dir,
      })})
      notify.success('Rule added')
      onSaved(); onClose()
    } catch (e: any) { notify.error(e.message ?? 'Failed to add rule') }
    finally { setBusy(false) }
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4" style={{ background: 'rgba(0,0,0,0.6)' }}>
      <div className="w-full max-w-sm rounded-lg p-5 space-y-4" style={{ background: 'var(--bg-panel)', border: '1px solid var(--border-subtle)' }}>
        <div className="flex items-center justify-between">
          <h2 className="font-semibold text-sm" style={{ color: 'var(--text-primary)' }}>Add Firewall Rule</h2>
          <button onClick={onClose}><X size={15} style={{ color: 'var(--text-muted)' }} /></button>
        </div>
        <div className="grid grid-cols-2 gap-3">
          {[
            { label: 'Action', el: <select value={action} onChange={e => setAction(e.target.value)} className="w-full rounded px-2 py-1.5 text-sm outline-none" style={{ background: 'var(--bg-surface)', border: '1px solid var(--border-subtle)', color: 'var(--text-primary)' }}><option>allow</option><option>deny</option><option>limit</option></select> },
            { label: 'Direction', el: <select value={dir} onChange={e => setDir(e.target.value)} className="w-full rounded px-2 py-1.5 text-sm outline-none" style={{ background: 'var(--bg-surface)', border: '1px solid var(--border-subtle)', color: 'var(--text-primary)' }}><option>in</option><option>out</option></select> },
            { label: 'Port', el: <input value={port} onChange={e => setPort(e.target.value)} placeholder="22, 80, 443" className="w-full rounded px-2 py-1.5 text-sm font-mono outline-none" style={{ background: 'var(--bg-surface)', border: '1px solid var(--border-subtle)', color: 'var(--text-primary)' }} /> },
            { label: 'Protocol', el: <select value={proto} onChange={e => setProto(e.target.value)} className="w-full rounded px-2 py-1.5 text-sm outline-none" style={{ background: 'var(--bg-surface)', border: '1px solid var(--border-subtle)', color: 'var(--text-primary)' }}><option>tcp</option><option>udp</option><option>any</option></select> },
          ].map(({ label, el }) => (
            <div key={label} className="space-y-1">
              <label className="text-xs" style={{ color: 'var(--text-muted)' }}>{label}</label>
              {el}
            </div>
          ))}
        </div>
        <div className="space-y-1">
          <label className="text-xs" style={{ color: 'var(--text-muted)' }}>From (IP/CIDR or leave blank for Anywhere)</label>
          <input value={from} onChange={e => setFrom(e.target.value)} placeholder="192.168.1.0/24" className="w-full rounded px-2 py-1.5 text-sm font-mono outline-none" style={{ background: 'var(--bg-surface)', border: '1px solid var(--border-subtle)', color: 'var(--text-primary)' }} />
        </div>
        <div className="flex gap-2 justify-end">
          <button onClick={onClose} className="px-3 py-1.5 rounded text-sm" style={{ background: 'var(--bg-surface)', color: 'var(--text-secondary)', border: '1px solid var(--border-subtle)' }}>Cancel</button>
          <button onClick={submit} disabled={busy || (!port && !from)} className="px-3 py-1.5 rounded text-sm disabled:opacity-50" style={{ background: 'var(--accent-primary)', color: '#fff' }}>
            {busy ? 'Adding…' : 'Add Rule'}
          </button>
        </div>
      </div>
    </div>
  )
}

export default function FirewallPage() {
  const [status, setStatus] = useState<FirewallStatus | null>(null)
  const [loading, setLoading] = useState(true)
  const [adding, setAdding] = useState(false)
  const [toggling, setToggling] = useState(false)

  const load = () => {
    setLoading(true)
    apiFetch<FirewallStatus>('/api/firewall')
      .then(setStatus).catch(() => notify.error('Failed to load firewall status'))
      .finally(() => setLoading(false))
  }

  useEffect(() => { load() }, [])

  const toggle = async () => {
    if (!status) return
    const action = status.enabled ? 'disable' : 'enable'
    if (!confirm(`${action === 'disable' ? 'Disable' : 'Enable'} the firewall?`)) return
    setToggling(true)
    try {
      await apiFetch('/api/firewall/action', { method: 'POST', body: JSON.stringify({ action }) })
      notify.success(`Firewall ${action}d`)
      load()
    } catch (e: any) { notify.error(e.message ?? 'Action failed') }
    finally { setToggling(false) }
  }

  const deleteRule = async (num: number) => {
    if (!confirm(`Delete rule #${num}? Rule numbers will shift after deletion.`)) return
    try {
      await apiFetch('/api/firewall/rules/delete', { method: 'POST', body: JSON.stringify({ num }) })
      notify.success(`Rule #${num} deleted`)
      load()
    } catch (e: any) { notify.error(e.message ?? 'Delete failed') }
  }

  const ipv4Rules = status?.rules.filter(r => !r.ipv6) ?? []
  const ipv6Rules = status?.rules.filter(r => r.ipv6) ?? []

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between flex-wrap gap-2">
        <div>
          <h1 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>Firewall</h1>
          {status && <p className="text-xs mt-0.5" style={{ color: 'var(--text-muted)' }}>Backend: {status.backend}{status.logging ? ` · Logging: ${status.logging}` : ''}</p>}
        </div>
        <div className="flex gap-2">
          <button onClick={load} disabled={loading} className="flex items-center gap-1.5 px-3 py-1.5 rounded text-sm hover:opacity-80 disabled:opacity-50" style={{ background: 'var(--bg-panel)', border: '1px solid var(--border-subtle)', color: 'var(--text-secondary)' }}>
            <RefreshCw size={13} className={loading ? 'animate-spin' : ''} /> Refresh
          </button>
          {status && !status.error && (
            <>
              <button onClick={() => setAdding(true)} className="flex items-center gap-1.5 px-3 py-1.5 rounded text-sm" style={{ background: 'var(--accent-primary)', color: '#fff' }}>
                <Plus size={13} /> Add Rule
              </button>
              <button onClick={toggle} disabled={toggling} className="flex items-center gap-1.5 px-3 py-1.5 rounded text-sm hover:opacity-80 disabled:opacity-50"
                style={{ background: status.enabled ? 'color-mix(in srgb, var(--accent-error) 15%, transparent)' : 'color-mix(in srgb, var(--accent-success) 15%, transparent)',
                         border: `1px solid ${status.enabled ? 'color-mix(in srgb, var(--accent-error) 40%, transparent)' : 'color-mix(in srgb, var(--accent-success) 40%, transparent)'}`,
                         color: status.enabled ? 'var(--accent-error)' : 'var(--accent-success)' }}>
                {status.enabled ? <><ShieldOff size={13} /> Disable</> : <><ShieldCheck size={13} /> Enable</>}
              </button>
            </>
          )}
        </div>
      </div>

      {/* Status banner */}
      {status && (
        <div className="flex items-center gap-3 rounded p-3" style={{
          background: status.error ? 'color-mix(in srgb, var(--accent-warning) 10%, transparent)'
            : status.enabled ? 'color-mix(in srgb, var(--accent-success) 10%, transparent)'
            : 'color-mix(in srgb, var(--accent-error) 10%, transparent)',
          border: `1px solid ${status.error ? 'color-mix(in srgb, var(--accent-warning) 30%, transparent)'
            : status.enabled ? 'color-mix(in srgb, var(--accent-success) 30%, transparent)'
            : 'color-mix(in srgb, var(--accent-error) 30%, transparent)'}`,
        }}>
          {status.error
            ? <AlertTriangle size={15} style={{ color: 'var(--accent-warning)', flexShrink: 0 }} />
            : status.enabled
            ? <ShieldCheck size={15} style={{ color: 'var(--accent-success)', flexShrink: 0 }} />
            : <ShieldOff size={15} style={{ color: 'var(--accent-error)', flexShrink: 0 }} />}
          <span className="text-sm" style={{ color: 'var(--text-primary)' }}>
            {status.error ?? (status.enabled ? 'Firewall is active' : 'Firewall is inactive — traffic is not filtered')}
          </span>
        </div>
      )}

      {/* Rules table */}
      {status && !status.error && (
        <div className="space-y-4">
          {[{ label: 'IPv4 Rules', rules: ipv4Rules }, { label: 'IPv6 Rules', rules: ipv6Rules }]
            .filter(g => g.rules.length > 0)
            .map(({ label, rules }) => (
              <div key={label}>
                <h2 className="text-xs font-semibold uppercase tracking-wider mb-2 px-0.5" style={{ color: 'var(--text-muted)' }}>{label}</h2>
                <div className="panel overflow-hidden">
                  <table className="w-full text-xs">
                    <thead>
                      <tr style={{ borderBottom: '1px solid var(--border-subtle)' }}>
                        {['#', 'To', 'Action', 'From', ''].map(h => (
                          <th key={h} className="px-4 py-2.5 text-left uppercase tracking-wider" style={{ color: 'var(--text-muted)' }}>{h}</th>
                        ))}
                      </tr>
                    </thead>
                    <tbody>
                      {rules.map(r => (
                        <tr key={r.num} style={{ borderBottom: '1px solid var(--border-subtle)' }}>
                          <td className="px-4 py-2.5 tabular-nums" style={{ color: 'var(--text-muted)' }}>{r.num}</td>
                          <td className="px-4 py-2.5 font-mono" style={{ color: 'var(--text-primary)' }}>{r.to || 'Any'}</td>
                          <td className="px-4 py-2.5 font-medium" style={{ color: ACTION_COLOR[r.action.toUpperCase().split(' ')[0]] ?? 'var(--text-secondary)' }}>
                            {r.action}
                          </td>
                          <td className="px-4 py-2.5 font-mono" style={{ color: 'var(--text-secondary)' }}>{r.from || 'Anywhere'}</td>
                          <td className="px-4 py-2.5">
                            <button onClick={() => deleteRule(r.num)} className="p-1 rounded hover:opacity-80" title="Delete rule" style={{ color: 'var(--accent-error)' }}>
                              <Trash2 size={12} />
                            </button>
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              </div>
            ))
          }
          {ipv4Rules.length === 0 && ipv6Rules.length === 0 && (
            <div className="py-10 text-center text-sm" style={{ color: 'var(--text-muted)' }}>No rules configured.</div>
          )}
        </div>
      )}

      {adding && <AddRuleModal onClose={() => setAdding(false)} onSaved={load} />}
    </div>
  )
}
