import { useState, useEffect, useCallback } from 'react'
import { createPortal } from 'react-dom'
import { api } from '@/api/client'
import type { ApiToken, OdysseusConfig } from '@/api/types'
import { notify } from '@/store/notifications'
import {
  Key, Plus, Trash2, Copy, RefreshCw,
  ShieldAlert, ShieldCheck, PlugZap, Zap, BookOpen,
  AlertTriangle, CheckCircle, Clock, ChevronDown, ChevronUp,
} from 'lucide-react'

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function ts(unix: number): string {
  return new Date(unix * 1000).toLocaleString()
}

function relTime(unix: number): string {
  const diff = Date.now() / 1000 - unix
  if (diff < 60) return 'just now'
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`
  return `${Math.floor(diff / 86400)}d ago`
}

function riskColor(risk: string) {
  switch (risk) {
    case 'read-only': return 'text-green-400 bg-green-400/10'
    case 'low-risk': return 'text-blue-400 bg-blue-400/10'
    case 'medium-risk': return 'text-yellow-400 bg-yellow-400/10'
    case 'high-risk': return 'text-orange-400 bg-orange-400/10'
    case 'destructive': return 'text-red-400 bg-red-400/10'
    default: return 'text-zinc-400 bg-zinc-700'
  }
}

const ALL_SCOPE_GROUPS: Record<string, string[]> = {
  'Metrics & Network': ['metrics:read', 'network:read'],
  'Services': ['services:read', 'services:restart'],
  'Containers': ['containers:read', 'containers:restart', 'containers:logs'],
  'Apps & Storage': ['apps:read', 'apps:deploy', 'storage:read', 'files:read'],
  'Backups': ['backups:read', 'backups:run'],
  'Alerts': ['alerts:read', 'alerts:ack'],
  'Automation': ['automation:read', 'automation:run'],
  'Audit': ['timeline:read'],
}

// ---------------------------------------------------------------------------
// Token creation modal
// ---------------------------------------------------------------------------

interface CreateTokenModalProps {
  scopes: { name: string; description: string }[]
  onClose: () => void
  onCreated: (token: ApiToken, raw: string) => void
}

function CreateTokenModal({ scopes, onClose, onCreated }: CreateTokenModalProps) {
  const [name, setName] = useState('')
  const [selectedScopes, setSelectedScopes] = useState<string[]>([])
  const [expiresDays, setExpiresDays] = useState<string>('90')
  const [loading, setLoading] = useState(false)

  const scopeDesc = Object.fromEntries(scopes.map(s => [s.name, s.description]))

  const toggle = (scope: string) =>
    setSelectedScopes(p => p.includes(scope) ? p.filter(s => s !== scope) : [...p, scope])

  const toggleGroup = (group: string[]) => {
    const allOn = group.every(s => selectedScopes.includes(s))
    if (allOn) setSelectedScopes(p => p.filter(s => !group.includes(s)))
    else setSelectedScopes(p => [...new Set([...p, ...group])])
  }

  const handleCreate = async () => {
    if (!name.trim()) { notify.warning('Token name required'); return }
    if (selectedScopes.length === 0) { notify.warning('Select at least one scope'); return }
    setLoading(true)
    try {
      const days = parseInt(expiresDays)
      const resp = await api.integrations.createToken(name.trim(), selectedScopes, days > 0 ? days : undefined)
      onCreated({ id: resp.id, name: resp.name, scopes: resp.scopes, last_used_at: null, expires_at: null, created_at: resp.created_at }, resp.token)
    } catch (e: any) {
      notify.error('Failed to create token', e.message)
    } finally {
      setLoading(false)
    }
  }

  return createPortal(
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60" onClick={onClose}>
      <div className="bg-zinc-900 border border-zinc-700 rounded-xl p-6 w-full max-w-lg mx-4 max-h-[90vh] overflow-y-auto" onClick={e => e.stopPropagation()}>
        <h3 className="text-lg font-semibold mb-4">Create API token</h3>

        <div className="space-y-4">
          <div>
            <label className="text-xs text-zinc-400 mb-1 block">Token name</label>
            <input
              className="w-full bg-zinc-800 border border-zinc-700 rounded px-3 py-2 text-sm focus:outline-none focus:border-violet-500"
              placeholder="e.g. Odysseus Agent"
              value={name}
              onChange={e => setName(e.target.value)}
            />
          </div>

          <div>
            <label className="text-xs text-zinc-400 mb-1 block">Expires in (days, 0 = never)</label>
            <input
              className="w-full bg-zinc-800 border border-zinc-700 rounded px-3 py-2 text-sm focus:outline-none focus:border-violet-500"
              type="number" min="0" value={expiresDays}
              onChange={e => setExpiresDays(e.target.value)}
            />
          </div>

          <div>
            <label className="text-xs text-zinc-400 mb-2 block">Scopes</label>
            <div className="space-y-3">
              {Object.entries(ALL_SCOPE_GROUPS).map(([group, groupScopes]) => {
                const available = groupScopes.filter(s => scopes.some(sc => sc.name === s))
                if (available.length === 0) return null
                const allOn = available.every(s => selectedScopes.includes(s))
                return (
                  <div key={group} className="border border-zinc-700 rounded-lg p-3">
                    <div className="flex items-center justify-between mb-2">
                      <span className="text-xs font-medium text-zinc-300">{group}</span>
                      <button
                        className="text-xs text-violet-400 hover:text-violet-300"
                        onClick={() => toggleGroup(available)}
                      >
                        {allOn ? 'Deselect all' : 'Select all'}
                      </button>
                    </div>
                    <div className="space-y-1">
                      {available.map(scope => (
                        <label key={scope} className="flex items-start gap-2 cursor-pointer group">
                          <input
                            type="checkbox"
                            className="mt-0.5 accent-violet-500"
                            checked={selectedScopes.includes(scope)}
                            onChange={() => toggle(scope)}
                          />
                          <div>
                            <div className="text-xs font-mono text-zinc-200">{scope}</div>
                            <div className="text-xs text-zinc-500">{scopeDesc[scope] ?? ''}</div>
                          </div>
                        </label>
                      ))}
                    </div>
                  </div>
                )
              })}
            </div>
          </div>
        </div>

        <div className="flex gap-3 mt-6">
          <button
            className="flex-1 bg-violet-600 hover:bg-violet-500 text-white rounded-lg py-2 text-sm font-medium disabled:opacity-50"
            onClick={handleCreate}
            disabled={loading}
          >
            {loading ? 'Creating…' : 'Create token'}
          </button>
          <button
            className="px-4 bg-zinc-800 hover:bg-zinc-700 rounded-lg py-2 text-sm"
            onClick={onClose}
          >
            Cancel
          </button>
        </div>
      </div>
    </div>,
    document.body,
  )
}

// ---------------------------------------------------------------------------
// Reveal-once modal shown after token creation
// ---------------------------------------------------------------------------

function RevealModal({ token, onClose, label, note }: { token: string; onClose: () => void; label?: string; note?: string }) {
  const [copied, setCopied] = useState(false)

  const copy = () => {
    navigator.clipboard.writeText(token)
    setCopied(true)
    setTimeout(() => setCopied(false), 2000)
  }

  return createPortal(
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/70">
      <div className="bg-zinc-900 border border-amber-500/40 rounded-xl p-6 w-full max-w-lg mx-4">
        <div className="flex items-center gap-2 text-amber-400 mb-3">
          <AlertTriangle size={18} />
          <h3 className="font-semibold">{label ?? 'Copy your token now'}</h3>
        </div>
        <p className="text-sm text-zinc-400 mb-4">
          {note ?? 'This token will not be shown again. Store it securely — treat it like a password.'}
        </p>
        <div className="bg-zinc-800 rounded-lg p-3 font-mono text-xs break-all text-green-400 mb-4 select-all">
          {token}
        </div>
        <div className="flex gap-3">
          <button
            className="flex-1 flex items-center justify-center gap-2 bg-amber-600 hover:bg-amber-500 text-white rounded-lg py-2 text-sm font-medium"
            onClick={copy}
          >
            <Copy size={14} />
            {copied ? 'Copied!' : 'Copy to clipboard'}
          </button>
          <button
            className="px-4 bg-zinc-700 hover:bg-zinc-600 rounded-lg py-2 text-sm"
            onClick={onClose}
          >
            Done
          </button>
        </div>
      </div>
    </div>,
    document.body,
  )
}

// ---------------------------------------------------------------------------
// Token list section
// ---------------------------------------------------------------------------

function TokensSection() {
  const [tokens, setTokens] = useState<ApiToken[]>([])
  const [scopes, setScopes] = useState<{ name: string; description: string }[]>([])
  const [loading, setLoading] = useState(true)
  const [showCreate, setShowCreate] = useState(false)
  const [revealToken, setRevealToken] = useState<string | null>(null)

  const load = useCallback(async () => {
    try {
      const [tr, sr] = await Promise.all([api.integrations.listTokens(), api.integrations.scopes()])
      setTokens(tr.tokens)
      setScopes(sr.scopes)
    } catch {}
    setLoading(false)
  }, [])

  useEffect(() => { load() }, [load])

  const revoke = async (id: string, name: string) => {
    if (!confirm(`Revoke token "${name}"? This cannot be undone.`)) return
    try {
      await api.integrations.revokeToken(id)
      setTokens(p => p.filter(t => t.id !== id))
      notify.success('Token revoked')
    } catch (e: any) {
      notify.error('Failed to revoke', e.message)
    }
  }

  const onCreated = (token: ApiToken, raw: string) => {
    setShowCreate(false)
    setTokens(p => [token, ...p])
    setRevealToken(raw)
  }

  return (
    <section className="card p-5">
      <div className="flex items-center justify-between mb-4">
        <div className="flex items-center gap-2">
          <Key size={16} className="text-violet-400" />
          <h2 className="font-semibold">API Tokens</h2>
        </div>
        <button
          className="flex items-center gap-1.5 text-sm bg-violet-600 hover:bg-violet-500 text-white px-3 py-1.5 rounded-lg"
          onClick={() => setShowCreate(true)}
        >
          <Plus size={14} />
          New token
        </button>
      </div>

      {loading ? (
        <div className="text-sm text-zinc-500">Loading…</div>
      ) : tokens.length === 0 ? (
        <div className="text-sm text-zinc-500 py-4 text-center">
          No tokens yet. Create one to connect Odysseus.
        </div>
      ) : (
        <div className="space-y-3">
          {tokens.map(t => (
            <div key={t.id} className="bg-zinc-800/60 rounded-lg p-3 flex items-start justify-between gap-3">
              <div className="min-w-0">
                <div className="font-medium text-sm">{t.name}</div>
                <div className="flex flex-wrap gap-1 mt-1.5">
                  {t.scopes.map(s => (
                    <span key={s} className="text-xs font-mono bg-zinc-700 text-zinc-300 px-1.5 py-0.5 rounded">
                      {s}
                    </span>
                  ))}
                </div>
                <div className="flex gap-3 mt-1.5 text-xs text-zinc-500">
                  <span>Created {ts(t.created_at)}</span>
                  {t.last_used_at && <span>Last used {relTime(t.last_used_at)}</span>}
                  {t.expires_at && <span className={t.expires_at < Date.now() / 1000 ? 'text-red-400' : ''}>
                    Expires {ts(t.expires_at)}
                  </span>}
                </div>
              </div>
              <button
                className="text-zinc-500 hover:text-red-400 shrink-0"
                onClick={() => revoke(t.id, t.name)}
                title="Revoke token"
              >
                <Trash2 size={15} />
              </button>
            </div>
          ))}
        </div>
      )}

      {showCreate && (
        <CreateTokenModal
          scopes={scopes}
          onClose={() => setShowCreate(false)}
          onCreated={onCreated}
        />
      )}
      {revealToken && (
        <RevealModal token={revealToken} onClose={() => setRevealToken(null)} />
      )}
    </section>
  )
}

// ---------------------------------------------------------------------------
// Odysseus config section
// ---------------------------------------------------------------------------

function OdysseusSection() {
  const [cfg, setCfg] = useState<OdysseusConfig | null>(null)
  const [allowedUrl, setAllowedUrl] = useState('')
  const [saving, setSaving] = useState(false)
  const [revealSecret, setRevealSecret] = useState<string | null>(null)

  const load = useCallback(async () => {
    try {
      const c = await api.integrations.getOdysseusConfig()
      setCfg(c)
      setAllowedUrl(c.allowed_url)
    } catch {}
  }, [])

  useEffect(() => { load() }, [load])

  const save = async (patch: Parameters<typeof api.integrations.saveOdysseusConfig>[0]) => {
    setSaving(true)
    try {
      const resp = await api.integrations.saveOdysseusConfig(patch)
      if (resp.webhook_secret) setRevealSecret(resp.webhook_secret)
      await load()
      notify.success('Saved')
    } catch (e: any) {
      notify.error('Failed to save', e.message)
    } finally {
      setSaving(false)
    }
  }

  if (!cfg) return <div className="card p-5 text-sm text-zinc-500">Loading…</div>

  return (
    <>
    <section className="card p-5 space-y-5">
      <div className="flex items-center gap-2">
        <PlugZap size={16} className="text-violet-400" />
        <h2 className="font-semibold">Odysseus Integration</h2>
        {cfg.emergency_disabled && (
          <span className="ml-2 text-xs bg-red-500/20 text-red-400 px-2 py-0.5 rounded-full flex items-center gap-1">
            <ShieldAlert size={11} /> Emergency disabled
          </span>
        )}
      </div>

      {/* Enable/disable */}
      <div className="flex items-center justify-between">
        <div>
          <div className="text-sm font-medium">Enable integration</div>
          <div className="text-xs text-zinc-500">Allow API tokens and event stream for Odysseus</div>
        </div>
        <button
          onClick={() => save({ enabled: !cfg.enabled })}
          className={`relative w-11 h-6 rounded-full transition-colors ${cfg.enabled ? 'bg-violet-600' : 'bg-zinc-700'}`}
        >
          <span className={`absolute top-1 w-4 h-4 rounded-full bg-white transition-transform ${cfg.enabled ? 'translate-x-6' : 'translate-x-1'}`} />
        </button>
      </div>

      {/* MCP server (placeholder) */}
      <div className="flex items-center justify-between">
        <div>
          <div className="text-sm font-medium">MCP server</div>
          <div className="text-xs text-zinc-500">Expose VoidTower tools via Model Context Protocol (requires integration enabled)</div>
        </div>
        <button
          onClick={() => save({ mcp_enabled: !cfg.mcp_enabled })}
          disabled={!cfg.enabled}
          className={`relative w-11 h-6 rounded-full transition-colors ${cfg.mcp_enabled && cfg.enabled ? 'bg-violet-600' : 'bg-zinc-700'} disabled:opacity-40`}
        >
          <span className={`absolute top-1 w-4 h-4 rounded-full bg-white transition-transform ${cfg.mcp_enabled && cfg.enabled ? 'translate-x-6' : 'translate-x-1'}`} />
        </button>
      </div>

      {/* Odysseus base URL */}
      <div>
        <label className="text-xs text-zinc-400 mb-1 block">Odysseus base URL (optional, for CORS validation)</label>
        <div className="flex gap-2">
          <input
            className="flex-1 bg-zinc-800 border border-zinc-700 rounded px-3 py-2 text-sm focus:outline-none focus:border-violet-500"
            placeholder="http://localhost:8080"
            value={allowedUrl}
            onChange={e => setAllowedUrl(e.target.value)}
          />
          <button
            className="px-3 bg-zinc-700 hover:bg-zinc-600 rounded text-sm disabled:opacity-50"
            onClick={() => save({ allowed_url: allowedUrl })}
            disabled={saving}
          >
            Save
          </button>
        </div>
      </div>

      {/* Webhook secret */}
      <div>
        <div className="flex items-center justify-between mb-1">
          <label className="text-xs text-zinc-400">Webhook secret</label>
          <button
            className="text-xs text-violet-400 hover:text-violet-300 flex items-center gap-1"
            onClick={() => { if (confirm('Regenerate webhook secret? Existing Odysseus webhook config will need updating.')) save({ regenerate_webhook_secret: true }) }}
          >
            <RefreshCw size={11} /> Regenerate
          </button>
        </div>
        <div className="bg-zinc-800 border border-zinc-700 rounded px-3 py-2 text-sm font-mono text-zinc-400">
          {cfg.webhook_secret_hint ? cfg.webhook_secret_hint : <span className="italic text-zinc-600">Not set — click Regenerate</span>}
        </div>
        <p className="text-xs text-zinc-500 mt-1">
          Send as <code className="bg-zinc-800 px-1 rounded">Authorization: Bearer &lt;secret&gt;</code> when calling <code className="bg-zinc-800 px-1 rounded">POST /api/integrations/webhooks</code>
        </p>
      </div>

      {/* Emergency disable */}
      <div className={`border rounded-lg p-4 ${cfg.emergency_disabled ? 'border-red-500/40 bg-red-500/5' : 'border-zinc-700'}`}>
        <div className="flex items-start justify-between gap-4">
          <div>
            <div className="text-sm font-medium flex items-center gap-2">
              {cfg.emergency_disabled ? <ShieldAlert size={14} className="text-red-400" /> : <ShieldCheck size={14} className="text-green-400" />}
              {cfg.emergency_disabled ? 'AI access is disabled' : 'Emergency disable'}
            </div>
            <div className="text-xs text-zinc-500 mt-1">
              {cfg.emergency_disabled
                ? 'All API token access and event streams are blocked until re-enabled.'
                : 'Instantly block all API token and event stream access without deleting tokens.'}
            </div>
          </div>
          <button
            className={`shrink-0 px-3 py-1.5 rounded text-sm font-medium ${cfg.emergency_disabled ? 'bg-green-700 hover:bg-green-600' : 'bg-red-700 hover:bg-red-600'}`}
            onClick={() => save({ emergency_disable: !cfg.emergency_disabled })}
          >
            {cfg.emergency_disabled ? 'Re-enable' : 'Disable all AI access'}
          </button>
        </div>
      </div>
    </section>
    {revealSecret && createPortal(
      <RevealModal
        token={revealSecret}
        label="Copy your webhook secret"
        note="This secret will not be shown again. Use it as the Bearer token in the Authorization header when calling POST /api/integrations/webhooks."
        onClose={() => setRevealSecret(null)}
      />,
      document.body,
    )}
    </>
  )
}

// ---------------------------------------------------------------------------
// Tool manifest section
// ---------------------------------------------------------------------------

function ManifestSection() {
  const [tools, setTools] = useState<any[]>([])
  const [open, setOpen] = useState(false)
  const [loading, setLoading] = useState(false)

  const load = async () => {
    if (tools.length > 0) { setOpen(p => !p); return }
    setLoading(true)
    try {
      const m = await api.integrations.manifest()
      setTools(m.tools ?? [])
      setOpen(true)
    } catch {}
    setLoading(false)
  }

  return (
    <section className="card p-5">
      <button className="w-full flex items-center justify-between" onClick={load}>
        <div className="flex items-center gap-2">
          <BookOpen size={16} className="text-violet-400" />
          <h2 className="font-semibold">Tool manifest</h2>
          <span className="text-xs text-zinc-500">— tools available to Odysseus agents</span>
        </div>
        {open ? <ChevronUp size={16} className="text-zinc-500" /> : <ChevronDown size={16} className="text-zinc-500" />}
      </button>

      {loading && <div className="mt-3 text-sm text-zinc-500">Loading…</div>}

      {open && tools.length > 0 && (
        <div className="mt-4 space-y-2">
          {tools.map((t: any) => (
            <div key={t.name} className="bg-zinc-800/60 rounded-lg p-3 flex items-start justify-between gap-3">
              <div>
                <div className="text-sm font-mono font-medium">{t.name}</div>
                <div className="text-xs text-zinc-400 mt-0.5">{t.description}</div>
                <div className="flex items-center gap-2 mt-1.5">
                  <span className="text-xs font-mono bg-zinc-700 text-zinc-300 px-1.5 py-0.5 rounded">{t.required_scope}</span>
                  {t.requires_confirmation && (
                    <span className="text-xs text-amber-400">confirmation required</span>
                  )}
                  {t.destructive && (
                    <span className="text-xs text-red-400">destructive</span>
                  )}
                </div>
              </div>
              <span className={`shrink-0 text-xs px-2 py-0.5 rounded-full font-medium ${riskColor(t.risk)}`}>
                {t.risk}
              </span>
            </div>
          ))}
        </div>
      )}

      {open && tools.length === 0 && !loading && (
        <div className="mt-3 text-sm text-zinc-500">Integration is disabled — no tools exposed.</div>
      )}
    </section>
  )
}

// ---------------------------------------------------------------------------
// Setup instructions
// ---------------------------------------------------------------------------

function SetupSection() {
  const [copied, setCopied] = useState<string | null>(null)
  const copy = (key: string, text: string) => {
    navigator.clipboard.writeText(text)
    setCopied(key)
    setTimeout(() => setCopied(null), 2000)
  }

  const snippets = [
    {
      key: 'curl',
      label: 'Test with curl',
      code: `curl -H "Authorization: Bearer <your-token>" \\\n  http://localhost:8743/api/metrics/current`,
    },
    {
      key: 'sse',
      label: 'Subscribe to event stream',
      code: `curl -H "Accept: text/event-stream" \\\n  "http://localhost:8743/api/integrations/events?token=<your-token>"`,
    },
    {
      key: 'webhook',
      label: 'Trigger automation via webhook',
      code: `curl -X POST \\\n  -H "Authorization: Bearer <webhook-secret>" \\\n  -H "Content-Type: application/json" \\\n  -d '{"automation_id":"<id>"}' \\\n  http://localhost:8743/api/integrations/webhooks`,
    },
    {
      key: 'manifest',
      label: 'Fetch tool manifest',
      code: `curl http://localhost:8743/api/integrations/odysseus/manifest`,
    },
  ]

  return (
    <section className="card p-5">
      <div className="flex items-center gap-2 mb-4">
        <Zap size={16} className="text-violet-400" />
        <h2 className="font-semibold">Setup & quick-start</h2>
      </div>
      <ol className="text-sm text-zinc-300 space-y-2 mb-5 list-decimal list-inside">
        <li>Enable the integration above.</li>
        <li>Create an API token with the scopes Odysseus needs.</li>
        <li>In Odysseus settings, add VoidTower as a tool server with the token.</li>
        <li>Optionally configure the webhook secret so Odysseus can trigger automations.</li>
        <li>Subscribe to the event stream for real-time alerts.</li>
      </ol>
      <div className="space-y-3">
        {snippets.map(s => (
          <div key={s.key} className="bg-zinc-800 rounded-lg overflow-hidden">
            <div className="flex items-center justify-between px-3 py-2 border-b border-zinc-700">
              <span className="text-xs text-zinc-400">{s.label}</span>
              <button
                className="text-xs text-zinc-500 hover:text-zinc-300 flex items-center gap-1"
                onClick={() => copy(s.key, s.code.replace(/\\\n\s+/g, ' '))}
              >
                <Copy size={11} />
                {copied === s.key ? 'Copied' : 'Copy'}
              </button>
            </div>
            <pre className="text-xs text-green-400 px-3 py-2 overflow-x-auto whitespace-pre-wrap">{s.code}</pre>
          </div>
        ))}
      </div>
    </section>
  )
}

// ---------------------------------------------------------------------------
// Recent AI actions
// ---------------------------------------------------------------------------

function RecentActionsSection() {
  const [actions, setActions] = useState<any[]>([])
  const [open, setOpen] = useState(false)
  const [loading, setLoading] = useState(false)

  const load = async () => {
    if (open) { setOpen(false); return }
    setLoading(true)
    try {
      const r = await api.integrations.recentActions()
      setActions(r.actions)
      setOpen(true)
    } catch {}
    setLoading(false)
  }

  return (
    <section className="card p-5">
      <button className="w-full flex items-center justify-between" onClick={load}>
        <div className="flex items-center gap-2">
          <Clock size={16} className="text-violet-400" />
          <h2 className="font-semibold">Recent AI-triggered actions</h2>
        </div>
        {open ? <ChevronUp size={16} className="text-zinc-500" /> : <ChevronDown size={16} className="text-zinc-500" />}
      </button>

      {loading && <div className="mt-3 text-sm text-zinc-500">Loading…</div>}

      {open && (
        <div className="mt-4">
          {actions.length === 0 ? (
            <div className="text-sm text-zinc-500">No agent-triggered actions recorded yet.</div>
          ) : (
            <div className="space-y-2">
              {actions.map(a => (
                <div key={a.id} className="flex items-start gap-3 text-sm">
                  {a.outcome === 'success'
                    ? <CheckCircle size={14} className="text-green-400 mt-0.5 shrink-0" />
                    : <AlertTriangle size={14} className="text-red-400 mt-0.5 shrink-0" />}
                  <div className="min-w-0">
                    <div className="font-mono text-xs text-zinc-200">{a.action}</div>
                    {a.resource_type && (
                      <div className="text-xs text-zinc-500">{a.resource_type}{a.resource_id ? ` / ${a.resource_id}` : ''}</div>
                    )}
                    {a.details && <div className="text-xs text-zinc-600">{a.details}</div>}
                  </div>
                  <div className="shrink-0 text-xs text-zinc-600 ml-auto">{relTime(a.timestamp)}</div>
                </div>
              ))}
            </div>
          )}
        </div>
      )}
    </section>
  )
}

// ---------------------------------------------------------------------------
// Page
// ---------------------------------------------------------------------------

export default function IntegrationsPage() {
  return (
    <div className="max-w-3xl mx-auto px-4 py-6 space-y-5">
      <div>
        <h1 className="text-2xl font-bold">Integrations</h1>
        <p className="text-sm text-zinc-400 mt-1">
          Connect Odysseus AI workspace to VoidTower infrastructure via scoped API tokens, event stream, and webhooks.
        </p>
      </div>

      <OdysseusSection />
      <TokensSection />
      <ManifestSection />
      <SetupSection />
      <RecentActionsSection />
    </div>
  )
}
