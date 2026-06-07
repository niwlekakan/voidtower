import { useEffect, useState } from 'react'
import { Plus, Trash2, Eye, EyeOff, Copy, Check, X, RefreshCw } from 'lucide-react'
import { api } from '@/api/client'
import type { SecretMeta } from '@/api/types'
import { notify } from '@/store/notifications'

function fmtDate(ts: number) {
  return new Date(ts * 1000).toLocaleDateString(undefined, { year: 'numeric', month: 'short', day: 'numeric' })
}

function AddModal({ onClose, onCreated }: { onClose: () => void; onCreated: () => void }) {
  const [name, setName] = useState('')
  const [desc, setDesc] = useState('')
  const [value, setValue] = useState('')
  const [show, setShow] = useState(false)
  const [busy, setBusy] = useState(false)

  const submit = async () => {
    if (!name.trim() || !value) return
    setBusy(true)
    try {
      await api.secrets.create(name.trim(), desc, value)
      notify.success(`Secret "${name}" created`)
      onCreated()
      onClose()
    } catch (e: any) {
      notify.error(e.message ?? 'Failed to create secret')
    } finally { setBusy(false) }
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4" style={{ background: 'rgba(0,0,0,0.6)' }}>
      <div className="w-full max-w-md rounded-lg p-5 space-y-4" style={{ background: 'var(--bg-panel)', border: '1px solid var(--border-subtle)' }}>
        <div className="flex items-center justify-between">
          <h2 className="font-semibold text-sm" style={{ color: 'var(--text-primary)' }}>New Secret</h2>
          <button onClick={onClose} style={{ color: 'var(--text-muted)' }}><X size={16} /></button>
        </div>
        {[
          { label: 'Name', value: name, set: setName, placeholder: 'e.g. restic-password', type: 'text' },
          { label: 'Description', value: desc, set: setDesc, placeholder: 'Optional note', type: 'text' },
        ].map(({ label, value: v, set, placeholder, type }) => (
          <div key={label} className="space-y-1">
            <label className="text-xs" style={{ color: 'var(--text-muted)' }}>{label}</label>
            <input
              type={type} value={v} onChange={e => set(e.target.value)} placeholder={placeholder}
              className="w-full rounded px-3 py-2 text-sm outline-none"
              style={{ background: 'var(--bg-surface)', border: '1px solid var(--border-subtle)', color: 'var(--text-primary)' }}
            />
          </div>
        ))}
        <div className="space-y-1">
          <label className="text-xs" style={{ color: 'var(--text-muted)' }}>Value</label>
          <div className="relative">
            <input
              type={show ? 'text' : 'password'} value={value} onChange={e => setValue(e.target.value)}
              placeholder="Secret value"
              className="w-full rounded px-3 py-2 pr-9 text-sm font-mono outline-none"
              style={{ background: 'var(--bg-surface)', border: '1px solid var(--border-subtle)', color: 'var(--text-primary)' }}
            />
            <button onClick={() => setShow(s => !s)} className="absolute right-2.5 top-1/2 -translate-y-1/2" style={{ color: 'var(--text-muted)' }}>
              {show ? <EyeOff size={14} /> : <Eye size={14} />}
            </button>
          </div>
        </div>
        <div className="flex gap-2 justify-end pt-1">
          <button onClick={onClose} className="px-3 py-1.5 rounded text-sm" style={{ background: 'var(--bg-surface)', color: 'var(--text-secondary)', border: '1px solid var(--border-subtle)' }}>Cancel</button>
          <button onClick={submit} disabled={busy || !name.trim() || !value} className="px-3 py-1.5 rounded text-sm disabled:opacity-50" style={{ background: 'var(--accent-primary)', color: '#fff' }}>
            {busy ? 'Saving…' : 'Save'}
          </button>
        </div>
      </div>
    </div>
  )
}

function RevealModal({ secret, onClose }: { secret: SecretMeta; onClose: () => void }) {
  const [value, setValue] = useState<string | null>(null)
  const [copied, setCopied] = useState(false)

  useEffect(() => {
    api.secrets.reveal(secret.id)
      .then(r => setValue(r.value))
      .catch(e => { notify.error(e.message ?? 'Failed to reveal'); onClose() })
  }, [secret.id])

  const copy = () => {
    if (!value) return
    navigator.clipboard.writeText(value).then(() => { setCopied(true); setTimeout(() => setCopied(false), 2000) })
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4" style={{ background: 'rgba(0,0,0,0.6)' }}>
      <div className="w-full max-w-md rounded-lg p-5 space-y-3" style={{ background: 'var(--bg-panel)', border: '1px solid var(--border-subtle)' }}>
        <div className="flex items-center justify-between">
          <h2 className="font-semibold text-sm" style={{ color: 'var(--text-primary)' }}>{secret.name}</h2>
          <button onClick={onClose} style={{ color: 'var(--text-muted)' }}><X size={16} /></button>
        </div>
        <div className="relative rounded p-3 font-mono text-xs break-all min-h-[3rem]" style={{ background: 'var(--bg-surface)', border: '1px solid var(--border-subtle)', color: 'var(--text-primary)' }}>
          {value ?? '…'}
          {value && (
            <button onClick={copy} className="absolute top-2 right-2" style={{ color: copied ? 'var(--accent-success)' : 'var(--text-muted)' }}>
              {copied ? <Check size={13} /> : <Copy size={13} />}
            </button>
          )}
        </div>
        <p className="text-xs" style={{ color: 'var(--text-muted)' }}>This reveal is recorded in the audit log.</p>
        <div className="flex justify-end">
          <button onClick={onClose} className="px-3 py-1.5 rounded text-sm" style={{ background: 'var(--bg-surface)', color: 'var(--text-secondary)', border: '1px solid var(--border-subtle)' }}>Close</button>
        </div>
      </div>
    </div>
  )
}

function RotateModal({ secret, onClose, onRotated }: { secret: SecretMeta; onClose: () => void; onRotated: () => void }) {
  const [confirmed, setConfirmed] = useState(false)
  const [newValue, setNewValue] = useState('')
  const [show, setShow] = useState(false)
  const [busy, setBusy] = useState(false)
  const [result, setResult] = useState<{ version: number } | null>(null)

  const doRotate = async () => {
    setBusy(true)
    try {
      const r = await api.secrets.rotate(secret.id, newValue || undefined)
      setResult({ version: r.version })
      onRotated()
    } catch (e: any) {
      notify.error(e.message ?? 'Rotation failed')
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4" style={{ background: 'rgba(0,0,0,0.6)' }}>
      <div className="w-full max-w-md rounded-lg p-5 space-y-4" style={{ background: 'var(--bg-panel)', border: '1px solid var(--border-subtle)' }}>
        <div className="flex items-center justify-between">
          <h2 className="font-semibold text-sm" style={{ color: 'var(--text-primary)' }}>Rotate Secret — {secret.name}</h2>
          <button onClick={onClose} style={{ color: 'var(--text-muted)' }}><X size={16} /></button>
        </div>

        {result ? (
          <>
            <p className="text-sm" style={{ color: 'var(--accent-success)' }}>
              Secret rotated — version {result.version}
            </p>
            <div className="flex justify-end">
              <button onClick={onClose} className="px-3 py-1.5 rounded text-sm" style={{ background: 'var(--bg-surface)', color: 'var(--text-secondary)', border: '1px solid var(--border-subtle)' }}>Close</button>
            </div>
          </>
        ) : !confirmed ? (
          <>
            <p className="text-sm" style={{ color: 'var(--text-secondary)' }}>
              This will replace the encrypted value and increment the version counter. The old value will be permanently overwritten.
            </p>
            <div className="flex gap-2 justify-end pt-1">
              <button onClick={onClose} className="px-3 py-1.5 rounded text-sm" style={{ background: 'var(--bg-surface)', color: 'var(--text-secondary)', border: '1px solid var(--border-subtle)' }}>Cancel</button>
              <button onClick={() => setConfirmed(true)} className="px-3 py-1.5 rounded text-sm" style={{ background: 'var(--accent-primary)', color: '#fff' }}>Continue</button>
            </div>
          </>
        ) : (
          <>
            <div className="space-y-1">
              <label className="text-xs" style={{ color: 'var(--text-muted)' }}>New value (leave blank to generate a random value)</label>
              <div className="relative">
                <input
                  type={show ? 'text' : 'password'} value={newValue} onChange={e => setNewValue(e.target.value)}
                  placeholder="Leave blank to auto-generate"
                  className="w-full rounded px-3 py-2 pr-9 text-sm font-mono outline-none"
                  style={{ background: 'var(--bg-surface)', border: '1px solid var(--border-subtle)', color: 'var(--text-primary)' }}
                />
                <button onClick={() => setShow(s => !s)} className="absolute right-2.5 top-1/2 -translate-y-1/2" style={{ color: 'var(--text-muted)' }}>
                  {show ? <EyeOff size={14} /> : <Eye size={14} />}
                </button>
              </div>
            </div>
            <div className="flex gap-2 justify-end pt-1">
              <button onClick={onClose} className="px-3 py-1.5 rounded text-sm" style={{ background: 'var(--bg-surface)', color: 'var(--text-secondary)', border: '1px solid var(--border-subtle)' }}>Cancel</button>
              <button onClick={doRotate} disabled={busy} className="px-3 py-1.5 rounded text-sm disabled:opacity-50 flex items-center gap-1.5" style={{ background: 'var(--accent-primary)', color: '#fff' }}>
                <RefreshCw size={13} className={busy ? 'animate-spin' : ''} />
                {busy ? 'Rotating…' : 'Rotate'}
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  )
}

export default function SecretsPage() {
  const [secrets, setSecrets] = useState<SecretMeta[]>([])
  const [adding, setAdding] = useState(false)
  const [revealing, setRevealing] = useState<SecretMeta | null>(null)
  const [rotating, setRotating] = useState<SecretMeta | null>(null)

  const load = () => api.secrets.list().then(r => setSecrets(r.secrets)).catch(() => {})

  useEffect(() => { load() }, [])

  const del = async (s: SecretMeta) => {
    if (!confirm(`Delete secret "${s.name}"?`)) return
    await api.secrets.delete(s.id)
    notify.success(`Deleted "${s.name}"`)
    load()
  }

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>Secrets</h1>
          <p className="text-xs mt-0.5" style={{ color: 'var(--text-muted)' }}>Encrypted at rest with AES-256-GCM. Values never logged.</p>
        </div>
        <button onClick={() => setAdding(true)} className="flex items-center gap-1.5 px-3 py-1.5 rounded text-sm" style={{ background: 'var(--accent-primary)', color: '#fff' }}>
          <Plus size={14} /> New Secret
        </button>
      </div>

      <div className="panel overflow-hidden">
        {secrets.length === 0 ? (
          <div className="px-4 py-10 text-center text-sm" style={{ color: 'var(--text-muted)' }}>No secrets stored yet.</div>
        ) : (
          <table className="w-full text-xs">
            <thead>
              <tr style={{ borderBottom: '1px solid var(--border-subtle)' }}>
                {['Name', 'Description', 'Ver', 'Created', 'Last revealed', ''].map(h => (
                  <th key={h} className="px-4 py-2.5 text-left uppercase tracking-wider" style={{ color: 'var(--text-muted)' }}>{h}</th>
                ))}
              </tr>
            </thead>
            <tbody>
              {secrets.map(s => (
                <tr key={s.id} style={{ borderBottom: '1px solid var(--border-subtle)' }}>
                  <td className="px-4 py-3 font-medium font-mono" style={{ color: 'var(--text-primary)' }}>{s.name}</td>
                  <td className="px-4 py-3" style={{ color: 'var(--text-secondary)' }}>{s.description ?? <span style={{ color: 'var(--text-disabled)' }}>—</span>}</td>
                  <td className="px-4 py-3 tabular-nums" style={{ color: 'var(--text-muted)' }}>{s.version ?? 0}</td>
                  <td className="px-4 py-3 tabular-nums" style={{ color: 'var(--text-muted)' }}>{fmtDate(s.created_at)}</td>
                  <td className="px-4 py-3 tabular-nums" style={{ color: 'var(--text-muted)' }}>{s.last_used_at ? fmtDate(s.last_used_at) : <span style={{ color: 'var(--text-disabled)' }}>never</span>}</td>
                  <td className="px-4 py-3">
                    <div className="flex items-center gap-2 justify-end">
                      <button onClick={() => setRevealing(s)} className="flex items-center gap-1 px-2 py-1 rounded text-xs hover:opacity-80" style={{ background: 'var(--bg-elevated)', color: 'var(--text-secondary)', border: '1px solid var(--border-subtle)' }}>
                        <Eye size={11} /> Reveal
                      </button>
                      <button onClick={() => setRotating(s)} className="flex items-center gap-1 px-2 py-1 rounded text-xs hover:opacity-80" style={{ background: 'var(--bg-elevated)', color: 'var(--text-secondary)', border: '1px solid var(--border-subtle)' }} title="Rotate secret">
                        <RefreshCw size={11} /> Rotate
                      </button>
                      <button onClick={() => del(s)} className="p-1 rounded hover:opacity-80" style={{ color: 'var(--accent-error)' }} title="Delete">
                        <Trash2 size={13} />
                      </button>
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>

      {adding && <AddModal onClose={() => setAdding(false)} onCreated={load} />}
      {revealing && <RevealModal secret={revealing} onClose={() => setRevealing(null)} />}
      {rotating && <RotateModal secret={rotating} onClose={() => setRotating(null)} onRotated={load} />}
    </div>
  )
}
