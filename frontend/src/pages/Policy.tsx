import { useState, useEffect, useCallback } from 'react'
import { ShieldAlert, Plus, Trash2, ToggleLeft, ToggleRight, FlaskConical, ChevronDown, ChevronUp } from 'lucide-react'
import { api } from '@/api/client'
import type { PolicyRule, PolicyCheckResult } from '@/api/types'

const ACTOR_LABELS: Record<string, string> = {
  'api_token': 'API Token',
  'automation': 'Automation',
  '*': 'Any actor',
}
const ACTION_OPTIONS = ['*', 'restart', 'start', 'stop', 'remove', 'deploy', 'run']
const RESOURCE_TYPE_OPTIONS = ['*', 'container', 'service', 'app', 'backup', 'vm']

function badge(text: string, color: string) {
  return (
    <span style={{
      display: 'inline-block', padding: '1px 8px', borderRadius: 4, fontSize: 11,
      background: color + '22', color, border: `1px solid ${color}55`, fontWeight: 500,
    }}>{text}</span>
  )
}

// ── Add rule modal ────────────────────────────────────────────────────────────

function AddRuleModal({ onClose, onSaved }: { onClose: () => void; onSaved: () => void }) {
  const [form, setForm] = useState({
    name: '', actor_type: 'api_token', action: '*', resource_type: '*',
    resource_tag: '', effect: 'deny', priority: 100,
  })
  const [saving, setSaving] = useState(false)
  const [err, setErr] = useState('')

  const save = async () => {
    if (!form.name.trim()) { setErr('Name is required'); return }
    setSaving(true); setErr('')
    try {
      await api.policy.create({
        ...form,
        resource_tag: form.resource_tag.trim() || null,
      })
      onSaved()
    } catch (e: any) { setErr(e.message || 'Failed to save') }
    finally { setSaving(false) }
  }

  const field = (label: string, children: React.ReactNode) => (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
      <label style={{ fontSize: 11, color: 'var(--text-muted)', fontWeight: 500 }}>{label}</label>
      {children}
    </div>
  )

  const sel = (value: string, onChange: (v: string) => void, options: string[], labelMap?: Record<string, string>) => (
    <select value={value} onChange={e => onChange(e.target.value)} style={{
      background: 'var(--bg-base)', border: '1px solid var(--border)', borderRadius: 6,
      color: 'var(--text-primary)', padding: '5px 8px', fontSize: 13,
    }}>
      {options.map(o => <option key={o} value={o}>{labelMap?.[o] ?? o}</option>)}
    </select>
  )

  return (
    <div style={{
      position: 'fixed', inset: 0, background: 'rgba(0,0,0,0.5)', display: 'flex',
      alignItems: 'center', justifyContent: 'center', zIndex: 1000,
    }} onClick={e => e.target === e.currentTarget && onClose()}>
      <div style={{
        background: 'var(--bg-surface)', border: '1px solid var(--border)', borderRadius: 12,
        padding: 24, width: 420, display: 'flex', flexDirection: 'column', gap: 16,
      }}>
        <div style={{ fontWeight: 600, fontSize: 15 }}>Add policy rule</div>

        {field('Rule name',
          <input value={form.name} onChange={e => setForm(f => ({ ...f, name: e.target.value }))}
            placeholder="e.g. Block AI from prod containers" style={{
              background: 'var(--bg-base)', border: '1px solid var(--border)', borderRadius: 6,
              color: 'var(--text-primary)', padding: '5px 8px', fontSize: 13,
            }} />
        )}

        <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 12 }}>
          {field('Actor', sel(form.actor_type, v => setForm(f => ({ ...f, actor_type: v })),
            ['api_token', 'automation', '*'], ACTOR_LABELS))}
          {field('Effect', sel(form.effect, v => setForm(f => ({ ...f, effect: v })), ['deny', 'allow']))}
          {field('Action', sel(form.action, v => setForm(f => ({ ...f, action: v })), ACTION_OPTIONS))}
          {field('Resource type', sel(form.resource_type, v => setForm(f => ({ ...f, resource_type: v })), RESOURCE_TYPE_OPTIONS))}
        </div>

        {field('Resource tag (optional — leave blank to match all)',
          <input value={form.resource_tag}
            onChange={e => setForm(f => ({ ...f, resource_tag: e.target.value }))}
            placeholder="e.g. critical, ai-no-touch, prod" style={{
              background: 'var(--bg-base)', border: '1px solid var(--border)', borderRadius: 6,
              color: 'var(--text-primary)', padding: '5px 8px', fontSize: 13,
            }} />
        )}

        {field('Priority (lower = evaluated first)',
          <input type="number" value={form.priority}
            onChange={e => setForm(f => ({ ...f, priority: +e.target.value }))} style={{
              background: 'var(--bg-base)', border: '1px solid var(--border)', borderRadius: 6,
              color: 'var(--text-primary)', padding: '5px 8px', fontSize: 13, width: 80,
            }} />
        )}

        {err && <div style={{ color: 'var(--accent-danger)', fontSize: 12 }}>{err}</div>}

        <div style={{ display: 'flex', gap: 8, justifyContent: 'flex-end' }}>
          <button onClick={onClose} style={{
            padding: '6px 14px', borderRadius: 6, border: '1px solid var(--border)',
            background: 'transparent', color: 'var(--text-primary)', cursor: 'pointer', fontSize: 13,
          }}>Cancel</button>
          <button onClick={save} disabled={saving} style={{
            padding: '6px 14px', borderRadius: 6, border: 'none',
            background: 'var(--accent-primary)', color: '#fff', cursor: 'pointer', fontSize: 13,
            opacity: saving ? 0.7 : 1,
          }}>{saving ? 'Saving…' : 'Save rule'}</button>
        </div>
      </div>
    </div>
  )
}

// ── Test panel ────────────────────────────────────────────────────────────────

function TestPanel() {
  const [open, setOpen] = useState(false)
  const [form, setForm] = useState({ actor_type: 'api_token', action: 'restart', resource_type: 'container', resource_id: '' })
  const [result, setResult] = useState<PolicyCheckResult | null>(null)
  const [loading, setLoading] = useState(false)

  const run = async () => {
    if (!form.resource_id.trim()) return
    setLoading(true); setResult(null)
    try { setResult(await api.policy.check(form)) }
    catch { setResult({ verdict: 'deny', reason: 'Request failed' }) }
    finally { setLoading(false) }
  }

  const sel = (key: keyof typeof form, options: string[]) => (
    <select value={form[key]} onChange={e => setForm(f => ({ ...f, [key]: e.target.value }))} style={{
      background: 'var(--bg-base)', border: '1px solid var(--border)', borderRadius: 6,
      color: 'var(--text-primary)', padding: '4px 7px', fontSize: 12,
    }}>
      {options.map(o => <option key={o}>{o}</option>)}
    </select>
  )

  return (
    <div style={{ border: '1px solid var(--border)', borderRadius: 8, overflow: 'hidden' }}>
      <button onClick={() => setOpen(o => !o)} style={{
        width: '100%', display: 'flex', alignItems: 'center', gap: 8, padding: '10px 14px',
        background: 'var(--bg-elevated)', border: 'none', cursor: 'pointer',
        color: 'var(--text-primary)', fontSize: 13, fontWeight: 500,
      }}>
        <FlaskConical size={14} style={{ color: 'var(--text-muted)' }} />
        Test policy
        <span style={{ marginLeft: 'auto' }}>{open ? <ChevronUp size={14} /> : <ChevronDown size={14} />}</span>
      </button>
      {open && (
        <div style={{ padding: 14, display: 'flex', flexDirection: 'column', gap: 10 }}>
          <div style={{ display: 'flex', flexWrap: 'wrap', gap: 8, alignItems: 'center' }}>
            <span style={{ fontSize: 12, color: 'var(--text-muted)' }}>If</span>
            {sel('actor_type', ['api_token', 'automation', '*'])}
            <span style={{ fontSize: 12, color: 'var(--text-muted)' }}>tries to</span>
            {sel('action', ACTION_OPTIONS)}
            <span style={{ fontSize: 12, color: 'var(--text-muted)' }}>a</span>
            {sel('resource_type', RESOURCE_TYPE_OPTIONS)}
            <span style={{ fontSize: 12, color: 'var(--text-muted)' }}>with ID</span>
            <input value={form.resource_id} onChange={e => setForm(f => ({ ...f, resource_id: e.target.value }))}
              placeholder="resource ID" style={{
                background: 'var(--bg-base)', border: '1px solid var(--border)', borderRadius: 6,
                color: 'var(--text-primary)', padding: '4px 7px', fontSize: 12, width: 140,
              }} />
            <button onClick={run} disabled={loading || !form.resource_id.trim()} style={{
              padding: '4px 12px', borderRadius: 6, border: 'none',
              background: 'var(--accent-primary)', color: '#fff', cursor: 'pointer', fontSize: 12,
              opacity: loading ? 0.7 : 1,
            }}>Check</button>
          </div>
          {result && (
            <div style={{
              display: 'flex', alignItems: 'center', gap: 8, padding: '8px 12px',
              borderRadius: 6, fontSize: 12,
              background: result.verdict === 'allow' ? 'var(--accent-success)18' : 'var(--accent-danger)18',
              border: `1px solid ${result.verdict === 'allow' ? 'var(--accent-success)' : 'var(--accent-danger)'}44`,
              color: result.verdict === 'allow' ? 'var(--accent-success)' : 'var(--accent-danger)',
            }}>
              <strong>{result.verdict === 'allow' ? '✓ Allowed' : '✗ Denied'}</strong>
              {result.reason && <span style={{ color: 'var(--text-secondary)' }}>— {result.reason}</span>}
            </div>
          )}
          <p style={{ fontSize: 11, color: 'var(--text-muted)', margin: 0 }}>
            Resource ID must exist in VoidTower for tag-based rules to fire. Blank resource_tag rules match any resource.
          </p>
        </div>
      )}
    </div>
  )
}

// ── Main page ─────────────────────────────────────────────────────────────────

export default function PolicyPage() {
  const [rules, setRules] = useState<PolicyRule[]>([])
  const [loading, setLoading] = useState(true)
  const [showAdd, setShowAdd] = useState(false)

  const load = useCallback(async () => {
    try { setRules(await api.policy.list()) }
    catch { /* handled below */ }
    finally { setLoading(false) }
  }, [])

  useEffect(() => { load() }, [load])

  const toggle = async (rule: PolicyRule) => {
    await api.policy.update(rule.id, { enabled: !rule.enabled })
    load()
  }

  const del = async (id: string) => {
    if (!confirm('Delete this policy rule?')) return
    await api.policy.delete(id)
    load()
  }

  return (
    <div style={{ padding: '24px 28px', maxWidth: 900, display: 'flex', flexDirection: 'column', gap: 20 }}>
      {/* Header */}
      <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
        <ShieldAlert size={20} style={{ color: 'var(--accent-primary)' }} />
        <div>
          <h1 style={{ fontSize: 18, fontWeight: 600, margin: 0 }}>Policy</h1>
          <p style={{ fontSize: 12, color: 'var(--text-muted)', margin: 0 }}>
            Rules that govern what automated actors (API tokens, automations) can do. Evaluated in priority order — first match wins.
          </p>
        </div>
        <button onClick={() => setShowAdd(true)} style={{
          marginLeft: 'auto', display: 'flex', alignItems: 'center', gap: 6,
          padding: '6px 14px', borderRadius: 7, border: 'none',
          background: 'var(--accent-primary)', color: '#fff', cursor: 'pointer', fontSize: 13,
        }}>
          <Plus size={14} /> Add rule
        </button>
      </div>

      {/* Info callout */}
      <div style={{
        padding: '10px 14px', borderRadius: 8, fontSize: 12, lineHeight: 1.5,
        background: 'var(--accent-primary)11', border: '1px solid var(--accent-primary)33',
        color: 'var(--text-secondary)',
      }}>
        Policy rules layer on top of API token scopes. Scopes control <em>what types</em> of operations are allowed;
        policy rules control <em>which specific resources</em> can be acted on and by whom.
        Human sessions are not policy-gated — only API token and automation actors are checked.
        Rules with <strong>deny</strong> effect block the action; <strong>allow</strong> can be used to create exceptions.
      </div>

      {/* Rule list */}
      {loading ? (
        <div style={{ color: 'var(--text-muted)', fontSize: 13 }}>Loading…</div>
      ) : rules.length === 0 ? (
        <div style={{
          textAlign: 'center', padding: 40, color: 'var(--text-muted)', fontSize: 13,
          border: '1px dashed var(--border)', borderRadius: 8,
        }}>
          No policy rules defined. Add a rule to start restricting automated access.
        </div>
      ) : (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 1 }}>
          <div style={{
            display: 'grid', gridTemplateColumns: '2fr 1fr 1fr 1fr 1fr 60px 80px',
            gap: 8, padding: '6px 12px',
            fontSize: 11, color: 'var(--text-muted)', fontWeight: 500,
          }}>
            <span>RULE</span><span>ACTOR</span><span>ACTION</span><span>RESOURCE</span><span>TAG</span><span>EFFECT</span><span></span>
          </div>
          {rules.map(rule => (
            <div key={rule.id} style={{
              display: 'grid', gridTemplateColumns: '2fr 1fr 1fr 1fr 1fr 60px 80px',
              gap: 8, padding: '10px 12px', borderRadius: 8, alignItems: 'center',
              background: 'var(--bg-surface)', border: '1px solid var(--border)',
              opacity: rule.enabled ? 1 : 0.5,
            }}>
              <div>
                <div style={{ fontSize: 13, fontWeight: 500 }}>{rule.name}</div>
                <div style={{ fontSize: 11, color: 'var(--text-muted)' }}>Priority {rule.priority}</div>
              </div>
              <span style={{ fontSize: 12 }}>{ACTOR_LABELS[rule.actor_type] ?? rule.actor_type}</span>
              <span style={{ fontSize: 12 }}>{rule.action}</span>
              <span style={{ fontSize: 12 }}>{rule.resource_type}</span>
              <span style={{ fontSize: 12 }}>{rule.resource_tag ?? <span style={{ color: 'var(--text-muted)' }}>any</span>}</span>
              {badge(rule.effect, rule.effect === 'deny' ? 'var(--accent-danger)' : 'var(--accent-success)')}
              <div style={{ display: 'flex', gap: 4, justifyContent: 'flex-end' }}>
                <button onClick={() => toggle(rule)} title={rule.enabled ? 'Disable' : 'Enable'} style={{
                  background: 'none', border: 'none', cursor: 'pointer', padding: 4,
                  color: rule.enabled ? 'var(--accent-success)' : 'var(--text-muted)',
                }}>
                  {rule.enabled ? <ToggleRight size={18} /> : <ToggleLeft size={18} />}
                </button>
                <button onClick={() => del(rule.id)} title="Delete" style={{
                  background: 'none', border: 'none', cursor: 'pointer', padding: 4,
                  color: 'var(--text-muted)',
                }}>
                  <Trash2 size={14} />
                </button>
              </div>
            </div>
          ))}
        </div>
      )}

      {/* Test panel */}
      <TestPanel />

      {showAdd && <AddRuleModal onClose={() => setShowAdd(false)} onSaved={() => { setShowAdd(false); load() }} />}
    </div>
  )
}
