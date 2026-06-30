import { useState, useEffect } from 'react'
import { api } from '@/api/client'
import type {
  AiProviderConfig,
  AiProviderKind,
  CreateAiProviderReq,
  UpdateAiProviderReq,
} from '@/api/types'
import { notify } from '@/store/notifications'
import {
  BrainCircuit, Plus, Trash2, RefreshCw, CheckCircle,
  XCircle, ChevronDown, ChevronUp, Edit2, Save, X,
} from 'lucide-react'

// ── Constants ─────────────────────────────────────────────────────────────────

const KIND_LABELS: Record<AiProviderKind, string> = {
  odysseus: 'Odysseus',
  openai: 'OpenAI',
  anthropic: 'Anthropic (Claude)',
  local: 'Local LLM (Ollama / llama.cpp)',
}

const KIND_COLORS: Record<AiProviderKind, string> = {
  odysseus: '#8b5cf6',
  openai: '#22c55e',
  anthropic: '#f59e0b',
  local: '#38bdf8',
}

const KIND_DEFAULTS: Record<AiProviderKind, Partial<CreateAiProviderReq>> = {
  odysseus: { base_url: 'http://localhost:7000', model: undefined },
  openai:   { base_url: 'https://api.openai.com', model: 'gpt-4o' },
  anthropic: { model: 'claude-sonnet-4-6' },
  local:    { base_url: 'http://localhost:11434', model: 'llama3' },
}

// ── Helpers ───────────────────────────────────────────────────────────────────

function Badge({ color, children }: { color: string; children: React.ReactNode }) {
  return (
    <span style={{
      padding: '2px 8px', borderRadius: 5, fontSize: 10, fontWeight: 600,
      background: color + '22', color,
      border: `1px solid ${color}44`,
    }}>{children}</span>
  )
}

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
      <label style={{ fontSize: 11, color: 'var(--text-muted)', fontWeight: 600 }}>{label}</label>
      {children}
    </div>
  )
}

function Input({ value, onChange, placeholder, type = 'text' }: {
  value: string; onChange: (v: string) => void; placeholder?: string; type?: string
}) {
  return (
    <input
      type={type}
      value={value}
      onChange={e => onChange(e.target.value)}
      placeholder={placeholder}
      style={{
        background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)',
        borderRadius: 6, padding: '5px 10px', fontSize: 12, color: 'var(--text-primary)',
        outline: 'none', width: '100%', boxSizing: 'border-box',
      }}
    />
  )
}

// ── Add Provider Form ─────────────────────────────────────────────────────────

function AddProviderForm({ onCreated }: { onCreated: () => void }) {
  const [open, setOpen] = useState(false)
  const [kind, setKind] = useState<AiProviderKind>('odysseus')
  const [name, setName] = useState('')
  const [baseUrl, setBaseUrl] = useState(KIND_DEFAULTS.odysseus.base_url ?? '')
  const [model, setModel] = useState(KIND_DEFAULTS.odysseus.model ?? '')
  const [apiKeyRef, setApiKeyRef] = useState('')
  const [apiKeyValue, setApiKeyValue] = useState('')
  const [priority, setPriority] = useState('50')
  const [saving, setSaving] = useState(false)

  function handleKindChange(k: AiProviderKind) {
    setKind(k)
    const d = KIND_DEFAULTS[k]
    setBaseUrl(d.base_url ?? '')
    setModel(d.model ?? '')
    if (!name) setName(KIND_LABELS[k])
  }

  async function handleSave() {
    if (!name.trim()) { notify.error('Name is required'); return }
    setSaving(true)
    try {
      const req: CreateAiProviderReq = {
        kind,
        name: name.trim(),
        base_url: baseUrl.trim() || undefined,
        model: model.trim() || undefined,
        api_key_ref: apiKeyRef.trim() || undefined,
        api_key_value: apiKeyValue.trim() || undefined,
        priority: parseInt(priority, 10) || 50,
      }
      await api.aiProviders.create(req)
      notify.success('Provider added')
      setOpen(false)
      setName(''); setBaseUrl(''); setModel(''); setApiKeyRef(''); setApiKeyValue('')
      onCreated()
    } catch {
      notify.error('Failed to add provider')
    } finally {
      setSaving(false)
    }
  }

  const needsKey = kind === 'openai' || kind === 'anthropic'

  return (
    <div style={{ borderTop: '1px solid var(--border-subtle)', paddingTop: 12 }}>
      {!open ? (
        <button
          onClick={() => setOpen(true)}
          style={{
            display: 'flex', alignItems: 'center', gap: 6, padding: '6px 14px',
            background: 'var(--accent-primary-subtle)', color: 'var(--accent-primary)',
            border: '1px solid rgba(139,92,246,0.3)', borderRadius: 7,
            fontSize: 12, fontWeight: 600, cursor: 'pointer',
          }}
        >
          <Plus size={13} /> Add Provider
        </button>
      ) : (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 12, padding: 12, background: 'var(--bg-elevated)', borderRadius: 8, border: '1px solid var(--border-subtle)' }}>
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
            <span style={{ fontSize: 12, fontWeight: 600, color: 'var(--text-primary)' }}>New AI Provider</span>
            <button onClick={() => setOpen(false)} style={{ background: 'none', border: 'none', cursor: 'pointer', color: 'var(--text-muted)' }}>
              <X size={14} />
            </button>
          </div>

          <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 10 }}>
            <Row label="Provider type">
              <select
                value={kind}
                onChange={e => handleKindChange(e.target.value as AiProviderKind)}
                style={{
                  background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)',
                  borderRadius: 6, padding: '5px 10px', fontSize: 12, color: 'var(--text-primary)',
                }}
              >
                {(Object.keys(KIND_LABELS) as AiProviderKind[]).map(k => (
                  <option key={k} value={k}>{KIND_LABELS[k]}</option>
                ))}
              </select>
            </Row>

            <Row label="Display name">
              <Input value={name} onChange={setName} placeholder={KIND_LABELS[kind]} />
            </Row>

            {kind !== 'anthropic' && (
              <Row label="Base URL">
                <Input value={baseUrl} onChange={setBaseUrl} placeholder="http://..." />
              </Row>
            )}

            {kind !== 'odysseus' && (
              <Row label="Model">
                <Input value={model} onChange={setModel} placeholder={KIND_DEFAULTS[kind].model ?? 'default'} />
              </Row>
            )}

            {needsKey && (
              <>
                <Row label="API key settings key">
                  <Input value={apiKeyRef} onChange={setApiKeyRef} placeholder={`ai.${kind}.key`} />
                </Row>
                <Row label="API key value">
                  <Input value={apiKeyValue} onChange={setApiKeyValue} placeholder="sk-..." type="password" />
                </Row>
              </>
            )}

            <Row label="Priority (lower = preferred)">
              <Input value={priority} onChange={setPriority} placeholder="50" />
            </Row>
          </div>

          <div style={{ display: 'flex', gap: 8, justifyContent: 'flex-end' }}>
            <button onClick={() => setOpen(false)} style={{ padding: '5px 12px', borderRadius: 6, border: '1px solid var(--border-subtle)', background: 'none', color: 'var(--text-secondary)', fontSize: 12, cursor: 'pointer' }}>
              Cancel
            </button>
            <button
              onClick={handleSave}
              disabled={saving}
              style={{
                padding: '5px 14px', borderRadius: 6, border: 'none',
                background: 'var(--accent-primary)', color: '#fff',
                fontSize: 12, fontWeight: 600, cursor: saving ? 'not-allowed' : 'pointer',
                opacity: saving ? 0.7 : 1,
              }}
            >
              {saving ? 'Saving…' : 'Add'}
            </button>
          </div>
        </div>
      )}
    </div>
  )
}

// ── Provider Card ─────────────────────────────────────────────────────────────

function ProviderCard({
  provider,
  onDeleted,
  onUpdated,
}: {
  provider: AiProviderConfig
  onDeleted: () => void
  onUpdated: () => void
}) {
  const [expanded, setExpanded] = useState(false)
  const [health, setHealth] = useState<{ ok: boolean; error: string | null } | null>(null)
  const [checking, setChecking] = useState(false)
  const [deleting, setDeleting] = useState(false)
  const [editing, setEditing] = useState(false)
  const [editName, setEditName] = useState(provider.name)
  const [editEnabled, setEditEnabled] = useState(provider.enabled)
  const [editPriority, setEditPriority] = useState(String(provider.priority))
  const [editModel, setEditModel] = useState(provider.model ?? '')
  const [editBaseUrl, setEditBaseUrl] = useState(provider.base_url ?? '')
  const [editApiKey, setEditApiKey] = useState('')
  const [saving, setSaving] = useState(false)

  const color = KIND_COLORS[provider.kind as AiProviderKind] ?? '#888'

  async function handleHealth() {
    setChecking(true)
    setHealth(null)
    try {
      const r = await api.aiProviders.health(provider.id)
      setHealth({ ok: r.ok, error: r.error })
    } catch {
      setHealth({ ok: false, error: 'Request failed' })
    } finally {
      setChecking(false)
    }
  }

  async function handleDelete() {
    if (!confirm(`Remove provider "${provider.name}"?`)) return
    setDeleting(true)
    try {
      await api.aiProviders.delete(provider.id)
      notify.success('Provider removed')
      onDeleted()
    } catch {
      notify.error('Failed to remove provider')
      setDeleting(false)
    }
  }

  async function handleSave() {
    setSaving(true)
    try {
      const req: UpdateAiProviderReq = {
        name: editName || undefined,
        enabled: editEnabled,
        priority: parseInt(editPriority, 10) || 50,
        model: editModel || undefined,
        base_url: editBaseUrl || undefined,
        api_key_ref: editApiKey ? provider.api_key_ref ?? undefined : undefined,
        api_key_value: editApiKey || undefined,
      }
      await api.aiProviders.update(provider.id, req)
      notify.success('Provider updated')
      setEditing(false)
      onUpdated()
    } catch {
      notify.error('Failed to update provider')
    } finally {
      setSaving(false)
    }
  }

  return (
    <div style={{
      border: `1px solid ${provider.enabled ? color + '33' : 'var(--border-subtle)'}`,
      borderRadius: 8, overflow: 'hidden',
      opacity: provider.enabled ? 1 : 0.55,
    }}>
      {/* Header row */}
      <div
        style={{
          display: 'flex', alignItems: 'center', gap: 10, padding: '8px 12px',
          background: provider.enabled ? color + '11' : 'var(--bg-elevated)',
          cursor: 'pointer',
        }}
        onClick={() => setExpanded(e => !e)}
      >
        <BrainCircuit size={14} style={{ color }} />
        <span style={{ fontSize: 13, fontWeight: 600, color: 'var(--text-primary)', flex: 1 }}>
          {provider.name}
        </span>
        <Badge color={color}>{KIND_LABELS[provider.kind as AiProviderKind] ?? provider.kind}</Badge>
        {provider.enabled
          ? <Badge color="#22c55e">enabled</Badge>
          : <Badge color="#888">disabled</Badge>}
        <span style={{ fontSize: 10, color: 'var(--text-muted)' }}>priority {provider.priority}</span>
        {expanded ? <ChevronUp size={13} style={{ color: 'var(--text-muted)' }} /> : <ChevronDown size={13} style={{ color: 'var(--text-muted)' }} />}
      </div>

      {/* Expanded body */}
      {expanded && (
        <div style={{ padding: 12, display: 'flex', flexDirection: 'column', gap: 10, borderTop: '1px solid var(--border-subtle)' }}>
          {!editing ? (
            <>
              {provider.base_url && (
                <div style={{ fontSize: 11, color: 'var(--text-muted)' }}>
                  URL: <span style={{ color: 'var(--text-secondary)' }}>{provider.base_url}</span>
                </div>
              )}
              {provider.model && (
                <div style={{ fontSize: 11, color: 'var(--text-muted)' }}>
                  Model: <span style={{ color: 'var(--text-secondary)' }}>{provider.model}</span>
                </div>
              )}
              {provider.api_key_ref && (
                <div style={{ fontSize: 11, color: 'var(--text-muted)' }}>
                  API key ref: <span style={{ color: 'var(--text-secondary)' }}>{provider.api_key_ref}</span>
                </div>
              )}

              {/* Health check result */}
              {health && (
                <div style={{ display: 'flex', alignItems: 'center', gap: 6, fontSize: 11 }}>
                  {health.ok
                    ? <><CheckCircle size={12} style={{ color: '#22c55e' }} /><span style={{ color: '#22c55e' }}>Reachable</span></>
                    : <><XCircle size={12} style={{ color: '#ef4444' }} /><span style={{ color: '#ef4444' }}>{health.error ?? 'Unreachable'}</span></>}
                </div>
              )}

              <div style={{ display: 'flex', gap: 8 }}>
                <button onClick={handleHealth} disabled={checking} style={btnStyle('#38bdf8')}>
                  <RefreshCw size={11} style={{ animation: checking ? 'spin 1s linear infinite' : undefined }} />
                  {checking ? 'Checking…' : 'Test'}
                </button>
                <button onClick={() => setEditing(true)} style={btnStyle(color)}>
                  <Edit2 size={11} /> Edit
                </button>
                <button onClick={handleDelete} disabled={deleting} style={{ ...btnStyle('#ef4444'), marginLeft: 'auto' }}>
                  <Trash2 size={11} /> {deleting ? 'Removing…' : 'Remove'}
                </button>
              </div>
            </>
          ) : (
            <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 10 }}>
              <Row label="Name"><Input value={editName} onChange={setEditName} /></Row>
              <Row label="Priority"><Input value={editPriority} onChange={setEditPriority} /></Row>
              {provider.base_url !== null && <Row label="Base URL"><Input value={editBaseUrl} onChange={setEditBaseUrl} /></Row>}
              {provider.model !== null && <Row label="Model"><Input value={editModel} onChange={setEditModel} /></Row>}
              {provider.api_key_ref && (
                <Row label={`API key (${provider.api_key_ref})`}>
                  <Input value={editApiKey} onChange={setEditApiKey} type="password" placeholder="Leave blank to keep current" />
                </Row>
              )}
              <div style={{ gridColumn: '1 / -1', display: 'flex', alignItems: 'center', gap: 8 }}>
                <label style={{ display: 'flex', alignItems: 'center', gap: 6, fontSize: 12, color: 'var(--text-secondary)', cursor: 'pointer' }}>
                  <input type="checkbox" checked={editEnabled} onChange={e => setEditEnabled(e.target.checked)} />
                  Enabled
                </label>
                <div style={{ marginLeft: 'auto', display: 'flex', gap: 8 }}>
                  <button onClick={() => setEditing(false)} style={btnStyle('#888')}>
                    <X size={11} /> Cancel
                  </button>
                  <button onClick={handleSave} disabled={saving} style={btnStyle('#22c55e')}>
                    <Save size={11} /> {saving ? 'Saving…' : 'Save'}
                  </button>
                </div>
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  )
}

function btnStyle(color: string) {
  return {
    display: 'flex' as const, alignItems: 'center' as const, gap: 5,
    padding: '4px 10px', borderRadius: 6, fontSize: 11, fontWeight: 600,
    background: color + '22', color, border: `1px solid ${color}44`,
    cursor: 'pointer',
  }
}

// ── Page ──────────────────────────────────────────────────────────────────────

export default function AiProvidersPage() {
  const [providers, setProviders] = useState<AiProviderConfig[]>([])
  const [loading, setLoading] = useState(true)

  async function reload() {
    setLoading(true)
    try {
      setProviders(await api.aiProviders.list())
    } catch {
      notify.error('Failed to load AI providers')
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => { reload() }, [])

  return (
    <div style={{ padding: 24, maxWidth: 720 }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 10, marginBottom: 20 }}>
        <BrainCircuit size={20} style={{ color: 'var(--accent-primary)' }} />
        <div>
          <h1 style={{ fontSize: 16, fontWeight: 700, color: 'var(--text-primary)', margin: 0 }}>AI Providers</h1>
          <p style={{ fontSize: 11, color: 'var(--text-muted)', margin: 0 }}>
            VoidTower routes AI requests through the highest-priority enabled provider.
          </p>
        </div>
      </div>

      {loading ? (
        <div style={{ color: 'var(--text-muted)', fontSize: 13 }}>Loading…</div>
      ) : (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
          {providers.length === 0 && (
            <div style={{
              padding: 20, textAlign: 'center', borderRadius: 8,
              border: '1px dashed var(--border-subtle)', color: 'var(--text-muted)', fontSize: 12,
            }}>
              No providers configured. Add one below to enable the AI assistant.
            </div>
          )}
          {providers.map(p => (
            <ProviderCard key={p.id} provider={p} onDeleted={reload} onUpdated={reload} />
          ))}
          <AddProviderForm onCreated={reload} />
        </div>
      )}

      <div style={{ marginTop: 24, padding: 12, borderRadius: 8, background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)' }}>
        <div style={{ fontSize: 11, fontWeight: 600, color: 'var(--text-muted)', marginBottom: 6 }}>How routing works</div>
        <div style={{ fontSize: 11, color: 'var(--text-secondary)', lineHeight: 1.8 }}>
          • Requests go to the <strong>lowest-priority</strong> enabled provider<br />
          • The ask popup lets users pin a specific provider per conversation<br />
          • Existing Odysseus integrations keep working as a built-in fallback<br />
          • API key values are stored in the settings table under the key reference name
        </div>
      </div>
    </div>
  )
}
